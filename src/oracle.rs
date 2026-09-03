//! Syntactic oracle classification of `#[shallguard::verifies]` test bodies.
//!
//! The classifier answers one question per anchored test: does the body
//! contain at least one failure path, so the test *can* fail? It never
//! executes tested code and it is deliberately conservative: any
//! construct it does not fully understand (an unknown macro, generated
//! syntax) classifies as [`OracleClass::Present`]. False negatives are
//! acceptable; false positives are adoption poison.

use proc_macro2::{TokenStream, TokenTree};
use syn::visit::Visit;

/// Classification of one `#[shallguard::verifies]` test body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OracleClass {
    /// At least one failure path was found (or could not be ruled out).
    Present,
    /// The only failure paths found are structurally weak.
    Weak(Vec<WeakReason>),
    /// No failure path: the test cannot fail.
    Vacuous(VacuityReason),
    /// Explicitly opted out via `oracle = "<class>"`; never silent.
    Suppressed(String),
}

/// Why evidence is weak rather than absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeakReason {
    /// `#[should_panic]` without an `expected` message and no other
    /// failure path in the body.
    BareShouldPanic,
}

impl WeakReason {
    pub fn describe(self) -> &'static str {
        match self {
            Self::BareShouldPanic => "relies on bare #[should_panic] without an expected message",
        }
    }
}

/// Why a test body cannot fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VacuityReason {
    /// No assertion, panic, unwrap/expect, or `?` failure path at all.
    NoFailurePath,
    /// The only assertions are constant-foldable or compare a value
    /// with itself.
    TrivialFailurePathsOnly,
}

impl VacuityReason {
    pub fn describe(self) -> &'static str {
        match self {
            Self::NoFailurePath => "contains no failure path",
            Self::TrivialFailurePathsOnly => "contains only constant or self-identical assertions",
        }
    }
}

/// Classifies one test function's oracle from its syntax alone.
#[shallguard::enforces("REQ-TRACE-009", "REQ-TRACE-012", "REQ-TRACE-015")]
pub fn classify(
    attrs: &[syn::Attribute],
    sig: &syn::Signature,
    block: &syn::Block,
    suppression: Option<&str>,
) -> OracleClass {
    if let Some(class) = suppression {
        return OracleClass::Suppressed(class.to_string());
    }

    let mut visitor = BodyVisitor {
        returns_result: returns_result_like(sig),
        real_failure_path: false,
        trivial_assertion: false,
        opaque_construct: false,
    };
    visitor.visit_block(block);

    // Conservatism (REQ-TRACE-015): anything not fully understood counts
    // as a potential failure path.
    if visitor.real_failure_path || visitor.opaque_construct {
        return OracleClass::Present;
    }

    match should_panic(attrs) {
        ShouldPanic::WithExpected => OracleClass::Present,
        ShouldPanic::Bare => OracleClass::Weak(vec![WeakReason::BareShouldPanic]),
        ShouldPanic::Absent => {
            if visitor.trivial_assertion {
                OracleClass::Vacuous(VacuityReason::TrivialFailurePathsOnly)
            } else {
                OracleClass::Vacuous(VacuityReason::NoFailurePath)
            }
        }
    }
}

enum ShouldPanic {
    Absent,
    Bare,
    WithExpected,
}

fn should_panic(attrs: &[syn::Attribute]) -> ShouldPanic {
    for attr in attrs {
        if !attr.path().is_ident("should_panic") {
            continue;
        }
        return match &attr.meta {
            syn::Meta::Path(_) => ShouldPanic::Bare,
            _ => ShouldPanic::WithExpected,
        };
    }
    ShouldPanic::Absent
}

/// Whether `?` in this signature is a genuine failure path.
fn returns_result_like(sig: &syn::Signature) -> bool {
    match &sig.output {
        syn::ReturnType::Default => false,
        syn::ReturnType::Type(_, ty) => {
            let rendered = quote::ToTokens::to_token_stream(ty).to_string();
            rendered.contains("Result") || rendered.contains("Termination")
        }
    }
}

/// Macros that are known not to constitute a failure path, so they do
/// not trigger the conservative opaque-construct rule.
const INNOCUOUS_MACROS: &[&str] = &[
    "println",
    "print",
    "eprintln",
    "eprint",
    "format",
    "write",
    "writeln",
    "vec",
    "dbg",
    "matches",
    "concat",
    "stringify",
    "line",
    "file",
    "column",
    "env",
];

/// Macros whose invocation always offers a failure path.
const PANICKING_MACROS: &[&str] = &["panic", "todo", "unreachable", "unimplemented"];

struct BodyVisitor {
    returns_result: bool,
    real_failure_path: bool,
    trivial_assertion: bool,
    opaque_construct: bool,
}

impl<'ast> Visit<'ast> for BodyVisitor {
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let Some(name) = mac.path.segments.last().map(|s| s.ident.to_string()) else {
            self.opaque_construct = true;
            return;
        };
        if name.starts_with("assert") {
            if assertion_is_trivial(&name, mac.tokens.clone()) {
                self.trivial_assertion = true;
            } else {
                self.real_failure_path = true;
            }
        } else if PANICKING_MACROS.contains(&name.as_str()) {
            self.real_failure_path = true;
        } else if !INNOCUOUS_MACROS.contains(&name.as_str()) {
            // Unknown macro: it may assert internally. Conservative.
            self.opaque_construct = true;
        }
        syn::visit::visit_macro(self, mac);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let name = call.method.to_string();
        if matches!(
            name.as_str(),
            "unwrap" | "expect" | "unwrap_err" | "expect_err"
        ) {
            self.real_failure_path = true;
        }
        syn::visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_try(&mut self, expr: &'ast syn::ExprTry) {
        if self.returns_result {
            self.real_failure_path = true;
        }
        syn::visit::visit_expr_try(self, expr);
    }
}

/// Whether an `assert*`-family invocation cannot fail on non-constant
/// input: all significant arguments are literals, or the two compared
/// sides of `assert_eq!`/`assert_ne!` are token-identical.
#[shallguard::enforces("REQ-TRACE-010", "REQ-TRACE-011")]
fn assertion_is_trivial(name: &str, tokens: TokenStream) -> bool {
    let args = split_top_level_commas(tokens);
    if args.is_empty() {
        // `assert!()` does not compile; treat as not trivial and let
        // rustc report it.
        return false;
    }
    // Significant arguments: the condition for `assert!`-style macros,
    // the two compared sides for `assert_eq!`/`assert_ne!`. Trailing
    // format-message arguments are ignored. Unknown assert-like macros
    // are trivial only when every argument is literal.
    let significant: &[Vec<TokenTree>] = match name {
        "assert" => &args[..1],
        "assert_eq" | "assert_ne" => {
            if args.len() < 2 {
                return false;
            }
            if normalized(&args[0]) == normalized(&args[1]) {
                return true;
            }
            &args[..2]
        }
        _ => &args[..],
    };
    significant.iter().all(|arg| arg_is_literal_only(arg))
}

fn split_top_level_commas(tokens: TokenStream) -> Vec<Vec<TokenTree>> {
    let mut args = Vec::new();
    let mut current = Vec::new();
    for tree in tokens {
        match &tree {
            TokenTree::Punct(p) if p.as_char() == ',' => {
                args.push(std::mem::take(&mut current));
            }
            _ => current.push(tree),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

fn arg_is_literal_only(arg: &[TokenTree]) -> bool {
    !arg.is_empty()
        && arg.iter().all(|tree| match tree {
            TokenTree::Literal(_) => true,
            TokenTree::Ident(ident) => ident == "true" || ident == "false",
            _ => false,
        })
}

fn normalized(arg: &[TokenTree]) -> String {
    arg.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify_fn(source: &str) -> OracleClass {
        let item: syn::ItemFn = syn::parse_str(source).expect("BUG: test source parses");
        classify(&item.attrs, &item.sig, &item.block, None)
    }

    #[shallguard::verifies("REQ-TRACE-009")]
    #[test]
    fn empty_and_assertion_free_bodies_are_vacuous() {
        assert_eq!(
            classify_fn("fn t() {}"),
            OracleClass::Vacuous(VacuityReason::NoFailurePath)
        );
        assert_eq!(
            classify_fn("fn t() { let _ = f(); }"),
            OracleClass::Vacuous(VacuityReason::NoFailurePath)
        );
        assert_eq!(
            classify_fn("fn t() { println!(\"ran {}\", f()); }"),
            OracleClass::Vacuous(VacuityReason::NoFailurePath)
        );
    }

    #[shallguard::verifies("REQ-TRACE-009")]
    #[test]
    fn real_failure_paths_classify_as_present() {
        assert_eq!(
            classify_fn("fn t() { assert!(f()); }"),
            OracleClass::Present
        );
        assert_eq!(
            classify_fn("fn t() { assert_eq!(floor(0), 1); }"),
            OracleClass::Present
        );
        assert_eq!(
            classify_fn("fn t() { f().unwrap(); }"),
            OracleClass::Present
        );
        assert_eq!(
            classify_fn("fn t() { f().expect_err(\"must fail\"); }"),
            OracleClass::Present
        );
        assert_eq!(
            classify_fn("fn t() { panic!(\"boom\"); }"),
            OracleClass::Present
        );
        assert_eq!(
            classify_fn("fn t() -> anyhow::Result<()> { f()?; Ok(()) }"),
            OracleClass::Present
        );
    }

    #[shallguard::verifies("REQ-TRACE-009")]
    #[test]
    fn question_mark_without_result_return_is_not_a_failure_path() {
        // `?` only fails the test when the signature can propagate it.
        assert_eq!(
            classify_fn("fn t() -> anyhow::Result<()> { f()?; Ok(()) }"),
            OracleClass::Present
        );
        assert_eq!(
            classify_fn("fn t() { let _ = || -> Option<u8> { f()?; None }; }"),
            OracleClass::Vacuous(VacuityReason::NoFailurePath)
        );
    }

    #[shallguard::verifies("REQ-TRACE-010")]
    #[test]
    fn literal_only_assertions_are_trivial() {
        assert_eq!(
            classify_fn("fn t() { assert!(true); }"),
            OracleClass::Vacuous(VacuityReason::TrivialFailurePathsOnly)
        );
        assert_eq!(
            classify_fn("fn t() { assert_eq!(1, 1); }"),
            OracleClass::Vacuous(VacuityReason::TrivialFailurePathsOnly)
        );
        assert_eq!(
            classify_fn("fn t() { assert_ne!(0, 1, \"context {}\", 2); }"),
            OracleClass::Vacuous(VacuityReason::TrivialFailurePathsOnly)
        );
        // A literal condition with a non-literal message is still trivial:
        // the message never fires.
        assert_eq!(
            classify_fn("fn t() { assert!(true, \"saw {}\", value()); }"),
            OracleClass::Vacuous(VacuityReason::TrivialFailurePathsOnly)
        );
    }

    #[shallguard::verifies("REQ-TRACE-011")]
    #[test]
    fn token_identical_sides_are_trivial() {
        assert_eq!(
            classify_fn("fn t() { assert_eq!(x, x); }"),
            OracleClass::Vacuous(VacuityReason::TrivialFailurePathsOnly)
        );
        assert_eq!(
            classify_fn("fn t() { assert_eq!(floor(0), floor(0)); }"),
            OracleClass::Vacuous(VacuityReason::TrivialFailurePathsOnly)
        );
        // Different sides are a real failure path.
        assert_eq!(
            classify_fn("fn t() { assert_eq!(floor(0), floor(1)); }"),
            OracleClass::Present
        );
    }

    #[shallguard::verifies("REQ-TRACE-012")]
    #[test]
    fn bare_should_panic_is_weak_and_expected_is_present() {
        assert_eq!(
            classify_fn("#[should_panic]\nfn t() { f(); }"),
            OracleClass::Weak(vec![WeakReason::BareShouldPanic])
        );
        assert_eq!(
            classify_fn("#[should_panic(expected = \"zero floor\")]\nfn t() { f(); }"),
            OracleClass::Present
        );
        // A real assertion outranks the bare attribute.
        assert_eq!(
            classify_fn("#[should_panic]\nfn t() { assert!(f()); }"),
            OracleClass::Present
        );
    }

    #[shallguard::verifies("REQ-TRACE-015")]
    #[test]
    fn unknown_constructs_classify_as_present() {
        // Unknown macros may assert internally; never flag them.
        assert_eq!(
            classify_fn("fn t() { my_custom_check!(f()); }"),
            OracleClass::Present
        );
        assert_eq!(
            classify_fn("fn t() { include!(\"generated_assertions.rs\"); }"),
            OracleClass::Present
        );
    }

    // Anchored to REQ-TRACE-014 once the compile-time opt-out lands.
    #[test]
    fn suppression_is_recorded_not_silent() {
        let item: syn::ItemFn = syn::parse_str("fn t() {}").expect("BUG: parses");
        assert_eq!(
            classify(&item.attrs, &item.sig, &item.block, Some("compile")),
            OracleClass::Suppressed("compile".to_string())
        );
    }
}
