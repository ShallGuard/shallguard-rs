//! Implementation of ShallGuard's requirement-traceability anchor macros.
//!
//! A repository keeps numbered system requirements (`REQ-<AREA>-<NNN>`,
//! e.g. `REQ-HRS-002`) in its configured requirements documents. These
//! attributes anchor code and tests to those requirements so the link
//! between document and implementation is machine-checkable instead of
//! living only in developers' heads:
//!
//! Consumers use these macros through the `shallguard` crate's public
//! namespace; they do not need to depend on this implementation crate.
//!
//! - [`macro@enforces`] marks an item as an enforcement site of one or
//!   more requirements — "this code exists because this contract exists".
//! - [`macro@verifies`] marks a test as verification evidence for one or
//!   more requirements — it backs a `[test]` entry in a requirement's
//!   *Verified:* line.
//!
//! All anchor forms are compile-time no-ops: the annotated item is
//! emitted unchanged and nothing is added at runtime. What the macros do
//! check, at compile time, is the anchor itself — a malformed or
//! duplicated requirement ID is a build error, so typos cannot survive
//! as dead anchors. Whether a well-formed ID actually exists in the
//! requirements documents is deliberately out of scope here (a macro
//! cannot detect *missing* anchors either); that is the job of the
//! external ShallGuard checker, which scans anchors syntactically.
//! Comments are never anchors.
//!
//! Placement conventions:
//!
//! - `#[shallguard::enforces]` attaches to items — and, inside a struct or enum
//!   that itself carries `#[shallguard::enforces]` (with IDs, or bare as a
//!   container marker), to individual fields and variants: the container
//!   attribute validates and strips the field-level anchors before the compiler
//!   or any derive sees them.
//! - A contract living on a specific branch, match arm, or statement is
//!   anchored with the statement-position `shallguard::enforces_here!` macro,
//!   which expands to nothing.
//! - `#[shallguard::verifies]` goes on test functions only, above the `#[test]` /
//!   `#[tokio::test]` attribute; it rejects non-test placements and
//!   `#[ignore]`d tests at compile time.
//! - The relation is many-to-many: one item may enforce several
//!   requirements, and one requirement may be enforced or verified at
//!   several sites.
//!
//! ```
//! # use shallguard_macros as shallguard;
//!
//! #[shallguard::enforces("REQ-RD-006", "REQ-RD-007")]
//! fn resolve_identity_claim() -> &'static str {
//!     "worker"
//! }
//!
//! #[shallguard::verifies("REQ-RD-006")]
//! #[test]
//! fn req_rd_006_exact_worker_claim_wins() {
//!     assert_eq!(resolve_identity_claim(), "worker");
//! }
//! ```
//!
//! Malformed IDs and invalid placements fail the build:
//!
//! ```compile_fail
//! # use shallguard_macros as shallguard;
//!
//! #[shallguard::enforces("REQ-hrs-2")]
//! fn broken() {}
//! ```
//!
//! ```compile_fail
//! # use shallguard_macros as shallguard;
//!
//! #[shallguard::verifies()]
//! fn missing_id() {}
//! ```
//!
//! ```compile_fail
//! # use shallguard_macros as shallguard;
//!
//! // Not a test function.
//! #[shallguard::verifies("REQ-RD-006")]
//! fn not_a_test() {}
//! ```
//!
//! ```compile_fail
//! # use shallguard_macros as shallguard;
//!
//! // Not a function at all.
//! #[shallguard::verifies("REQ-RD-006")]
//! struct NotAFunction;
//! ```
//!
//! ```compile_fail
//! # use shallguard_macros as shallguard;
//!
//! // Ignored tests are not evidence.
//! #[shallguard::verifies("REQ-RD-006")]
//! #[test]
//! #[ignore]
//! fn ignored_test() {}
//! ```
//!
//! ```compile_fail
//! # use shallguard_macros as shallguard;
//!
//! // A body with no failure path cannot be evidence.
//! #[shallguard::verifies("REQ-RD-006")]
//! #[test]
//! fn cannot_fail() {
//!     let _ = 1 + 1;
//! }
//! ```
//!
//! ```compile_fail
//! # use shallguard_macros as shallguard;
//!
//! // Constant assertions are not a failure path.
//! #[shallguard::verifies("REQ-RD-006")]
//! #[test]
//! fn constant_assertion() {
//!     assert!(true);
//! }
//! ```
//!
//! ```compile_fail
//! # use shallguard_macros as shallguard;
//!
//! // The oracle opt-out is a closed set: panic, compile, external.
//! #[shallguard::verifies("REQ-RD-006", oracle = "vibes")]
//! #[test]
//! fn unknown_oracle_class() {
//!     run_scenario();
//! }
//! # fn run_scenario() {}
//! ```

// The crate-level doctest shows #[shallguard::verifies] above #[test] — the real
// usage — which trips clippy's test_attr_in_doctest; the example is
// illustrative, not an executed unit test.
#![allow(clippy::test_attr_in_doctest)]

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{LitStr, Token};

/// Marks an item as an enforcement site of the listed requirements.
///
/// Takes one or more requirement IDs as string literals. Expands to the
/// item unchanged; the IDs are validated for form (`REQ-<AREA>-<NNN>`)
/// and uniqueness at compile time.
///
/// On a struct or enum, individual fields and variants may carry their
/// own `#[shallguard::enforces("REQ-...")]` anchors; this container attribute
/// validates them and strips them from the emitted item (so derives and
/// the compiler never see them). In that case the container attribute
/// itself may be bare — `#[shallguard::enforces]` — acting purely as the marker
/// that enables field-level anchors.
#[proc_macro_attribute]
pub fn enforces(args: TokenStream, item: TokenStream) -> TokenStream {
    match syn::parse::<syn::Item>(item.clone()) {
        Ok(syn::Item::Struct(mut s)) => {
            let mut errors = proc_macro2::TokenStream::new();
            let field_anchors = strip_field_anchors(s.fields.iter_mut(), &mut errors);
            validate_container_args(args, field_anchors, &mut errors);
            quote! { #errors #s }.into()
        }
        Ok(syn::Item::Enum(mut e)) => {
            let mut errors = proc_macro2::TokenStream::new();
            let mut field_anchors = 0;
            for variant in &mut e.variants {
                field_anchors += strip_anchor_attrs(&mut variant.attrs, &mut errors);
                field_anchors += strip_field_anchors(variant.fields.iter_mut(), &mut errors);
            }
            validate_container_args(args, field_anchors, &mut errors);
            quote! { #errors #e }.into()
        }
        _ => anchor("enforces", args, item),
    }
}

/// Statement-position enforcement anchor: marks the surrounding branch,
/// match arm, or statement sequence as an enforcement site of the listed
/// requirements. Expands to nothing; the IDs are validated for form and
/// uniqueness at compile time exactly like [`macro@enforces`].
///
/// ```
/// # use shallguard_macros as shallguard;
///
/// fn floor(configured: usize) -> usize {
///     shallguard::enforces_here!("REQ-HRS-002");
///     configured.max(1)
/// }
/// ```
#[proc_macro]
pub fn enforces_here(input: TokenStream) -> TokenStream {
    let mut errors = proc_macro2::TokenStream::new();
    validate_id_list("enforces_here", input, false, &mut errors);
    errors.into()
}

/// Validates the ID arguments of a container-level `#[shallguard::enforces]`:
/// bare is allowed only when field-level anchors exist, IDs are validated as
/// usual.
fn validate_container_args(
    args: TokenStream,
    field_anchors: usize,
    errors: &mut proc_macro2::TokenStream,
) {
    validate_id_list("enforces", args, field_anchors > 0, errors);
}

/// Strips `#[shallguard::enforces(...)]` anchors off every field in the
/// iterator, validating their IDs; returns how many anchors were stripped.
fn strip_field_anchors<'a>(
    fields: impl Iterator<Item = &'a mut syn::Field>,
    errors: &mut proc_macro2::TokenStream,
) -> usize {
    fields
        .map(|field| strip_anchor_attrs(&mut field.attrs, errors))
        .sum()
}

/// Strips `#[shallguard::enforces(...)]` attributes from an attribute list,
/// validating each stripped anchor's IDs; `#[shallguard::verifies]` here is an
/// error.
/// Returns the number of stripped enforcement anchors.
fn strip_anchor_attrs(
    attrs: &mut Vec<syn::Attribute>,
    errors: &mut proc_macro2::TokenStream,
) -> usize {
    let mut stripped = 0;
    attrs.retain(|attr| {
        let name = attr.path().segments.last().map(|s| s.ident.to_string());
        match name.as_deref() {
            Some("enforces") => {
                match &attr.meta {
                    syn::Meta::List(list) => {
                        validate_id_list("enforces", list.tokens.clone().into(), false, errors);
                    }
                    _ => errors.extend(
                        syn::Error::new_spanned(
                            attr,
                            "field-level #[shallguard::enforces] needs at least one requirement ID",
                        )
                        .to_compile_error(),
                    ),
                }
                stripped += 1;
                false
            }
            Some("verifies") => {
                errors.extend(
                    syn::Error::new_spanned(
                        attr,
                        "#[shallguard::verifies] applies only to test functions; \
                         use #[shallguard::enforces] for enforcement sites",
                    )
                    .to_compile_error(),
                );
                false
            }
            _ => true,
        }
    });
    stripped
}

/// Parses a comma-separated list of string-literal requirement IDs and
/// validates form and uniqueness. An empty list is an error unless
/// `allow_empty`.
fn validate_id_list(
    name: &str,
    args: TokenStream,
    allow_empty: bool,
    errors: &mut proc_macro2::TokenStream,
) {
    match Punctuated::<LitStr, Token![,]>::parse_terminated.parse(args) {
        Ok(ids) => {
            let ids: Vec<LitStr> = ids.into_iter().collect();
            validate_ids(name, &ids, allow_empty, errors);
        }
        Err(err) => errors.extend(err.to_compile_error()),
    }
}

/// Validates already-parsed requirement ID literals for presence, form,
/// and uniqueness.
fn validate_ids(
    name: &str,
    ids: &[LitStr],
    allow_empty: bool,
    errors: &mut proc_macro2::TokenStream,
) {
    if ids.is_empty() {
        if !allow_empty {
            let msg = format!(
                "{name} needs at least one requirement ID, \
                 e.g. {name}(\"REQ-HRS-002\")"
            );
            errors.extend(syn::Error::new(Span::call_site(), msg).to_compile_error());
        }
        return;
    }
    let mut seen = std::collections::HashSet::new();
    for lit in ids {
        let id = lit.value();
        if let Err(msg) = validate_req_id(&id) {
            errors.extend(syn::Error::new(lit.span(), msg).to_compile_error());
        } else if !seen.insert(id) {
            let msg = format!("duplicate requirement ID {:?}", lit.value());
            errors.extend(syn::Error::new(lit.span(), msg).to_compile_error());
        }
    }
}

/// Marks a test as verification evidence for the listed requirements.
///
/// Takes one or more requirement IDs as string literals, optionally
/// followed by an `oracle = "<class>"` opt-out. Expands to the item
/// unchanged. At compile time it validates the IDs for form
/// (`REQ-<AREA>-<NNN>`) and uniqueness, and validates the placement:
/// the item must be a function carrying a recognized test attribute
/// (`#[test]`, `#[tokio::test]`, or any attribute whose path ends in
/// `test`) and must not be `#[ignore]`d — evidence that does not run is
/// not evidence.
///
/// As an early front line for the deterministic vacuity check, the macro
/// also rejects test bodies that definitely cannot fail: bodies with no
/// failure-path candidate tokens at all, or whose only assertions are
/// literal-constant (`assert!(true)`, `assert_eq!(1, 1)`) or compare a
/// value with itself. Anything ambiguous compiles — `cargo shallguard
/// check` remains authoritative. Tests whose oracle genuinely lives
/// outside the body opt out visibly with `oracle = "panic"`,
/// `oracle = "compile"`, or `oracle = "external"`; suppression is counted
/// and listed by the checker, never silent.
#[proc_macro_attribute]
pub fn verifies(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut errors = proc_macro2::TokenStream::new();
    let parsed = parse_verifies_args(args, &mut errors);
    if let Some(parsed) = &parsed {
        validate_ids("#[verifies]", &parsed.ids, false, &mut errors);
        if let Some(oracle) = &parsed.oracle
            && !ORACLE_CLASSES.contains(&oracle.value().as_str())
        {
            errors.extend(
                syn::Error::new(
                    oracle.span(),
                    format!(
                        "unknown oracle class {:?}; accepted values: \
                         \"panic\", \"compile\", \"external\"",
                        oracle.value()
                    ),
                )
                .to_compile_error(),
            );
        }
    }
    match syn::parse::<syn::Item>(item.clone()) {
        Ok(syn::Item::Fn(fun)) => {
            let is_test = fun.attrs.iter().any(|attr| {
                attr.path()
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == "test")
            });
            let is_ignored = fun.attrs.iter().any(|attr| attr.path().is_ident("ignore"));
            if !is_test {
                errors.extend(
                    syn::Error::new_spanned(
                        &fun.sig.ident,
                        "#[shallguard::verifies] requires a test function: add #[test] / \
                         #[tokio::test] below the anchor, or remove the anchor",
                    )
                    .to_compile_error(),
                );
            }
            if is_ignored {
                errors.extend(
                    syn::Error::new_spanned(
                        &fun.sig.ident,
                        "#[shallguard::verifies] must not be placed on an #[ignore]d test: \
                         evidence that does not run is not evidence",
                    )
                    .to_compile_error(),
                );
            }
            if let Some(parsed) = &parsed
                && is_test
                && parsed.oracle.is_none()
                && !has_should_panic(&fun.attrs)
            {
                reject_definitely_vacuous(&fun, &parsed.ids, &mut errors);
            }
        }
        Ok(other) => {
            errors.extend(
                syn::Error::new_spanned(
                    &other,
                    "#[shallguard::verifies] applies only to test functions; \
                     use #[shallguard::enforces] for enforcement sites",
                )
                .to_compile_error(),
            );
        }
        Err(err) => errors.extend(err.to_compile_error()),
    }
    let item = proc_macro2::TokenStream::from(item);
    quote! { #errors #item }.into()
}

/// The closed set of oracle opt-out classes.
const ORACLE_CLASSES: &[&str] = &["panic", "compile", "external"];

struct VerifiesArgs {
    ids: Vec<LitStr>,
    oracle: Option<LitStr>,
}

/// Parses `#[verifies]` arguments: requirement ID string literals plus at
/// most one `oracle = "<class>"`. Returns `None` after a parse error.
fn parse_verifies_args(
    args: TokenStream,
    errors: &mut proc_macro2::TokenStream,
) -> Option<VerifiesArgs> {
    let parser = |input: syn::parse::ParseStream<'_>| -> syn::Result<VerifiesArgs> {
        let mut ids = Vec::new();
        let mut oracle: Option<LitStr> = None;
        while !input.is_empty() {
            if input.peek(syn::Ident) {
                let name: syn::Ident = input.parse()?;
                if name != "oracle" {
                    return Err(syn::Error::new(
                        name.span(),
                        format!(
                            "unknown #[shallguard::verifies] argument `{name}`; expected \
                             requirement ID string literals and at most one \
                             oracle = \"<class>\""
                        ),
                    ));
                }
                input.parse::<Token![=]>()?;
                let value: LitStr = input.parse()?;
                if oracle.is_some() {
                    return Err(syn::Error::new(value.span(), "duplicate oracle argument"));
                }
                oracle = Some(value);
            } else {
                ids.push(input.parse::<LitStr>()?);
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(VerifiesArgs { ids, oracle })
    };
    match parser.parse(args) {
        Ok(parsed) => Some(parsed),
        Err(err) => {
            errors.extend(err.to_compile_error());
            None
        }
    }
}

fn has_should_panic(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.path().is_ident("should_panic"))
}

/// Compile-time front line for the deterministic vacuity check: rejects
/// only the zero-false-positive subset — a body whose tokens contain no
/// failure-path candidate at all, or only trivial assertions. Anything
/// ambiguous compiles; the external checker is authoritative.
fn reject_definitely_vacuous(
    fun: &syn::ItemFn,
    ids: &[LitStr],
    errors: &mut proc_macro2::TokenStream,
) {
    let mut scan = FrontLineScan {
        candidate: false,
        trivial_assertion: false,
    };
    scan_body_tokens(quote::ToTokens::to_token_stream(&fun.block), &mut scan);
    if scan.candidate {
        return;
    }
    let detail = if scan.trivial_assertion {
        "its only assertions are constant or self-identical"
    } else {
        "the body contains no assertion, panic, unwrap/expect, or `?`"
    };
    let cited = ids.iter().map(LitStr::value).collect::<Vec<_>>().join(", ");
    errors.extend(
        syn::Error::new_spanned(
            &fun.sig.ident,
            format!(
                "#[shallguard::verifies({cited})] cites a test that cannot fail: {detail}. \
                 Evidence must offer a real failure path (see SKILL.md, evidence \
                 honesty): assert an actual output of the enforcement site, or opt \
                 out visibly with oracle = \"panic\" | \"compile\" | \"external\""
            ),
        )
        .to_compile_error(),
    );
}

struct FrontLineScan {
    /// Something that can fail — or that we cannot rule out — was seen.
    candidate: bool,
    /// A constant-foldable or self-identical assertion was seen.
    trivial_assertion: bool,
}

/// Token-level failure-path scan. Deliberately conservative: any macro
/// invocation other than a trivial `assert*` counts as a candidate, as
/// do `unwrap`/`expect` idents and the `?` operator anywhere.
fn scan_body_tokens(tokens: proc_macro2::TokenStream, scan: &mut FrontLineScan) {
    use proc_macro2::TokenTree;
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    let mut i = 0;
    while i < trees.len() {
        match &trees[i] {
            TokenTree::Ident(ident) => {
                let name = ident.to_string();
                let bang = matches!(
                    trees.get(i + 1),
                    Some(TokenTree::Punct(p)) if p.as_char() == '!'
                );
                if bang {
                    if matches!(name.as_str(), "assert" | "assert_eq" | "assert_ne")
                        && let Some(TokenTree::Group(group)) = trees.get(i + 2)
                    {
                        if assert_args_trivial(&name, group.stream()) {
                            scan.trivial_assertion = true;
                        } else {
                            scan.candidate = true;
                        }
                        i += 3;
                        continue;
                    }
                    // Any other macro may assert internally. Conservative.
                    scan.candidate = true;
                    i += 2;
                    continue;
                }
                if matches!(
                    name.as_str(),
                    "unwrap" | "expect" | "unwrap_err" | "expect_err"
                ) {
                    scan.candidate = true;
                }
                i += 1;
            }
            TokenTree::Punct(punct) if punct.as_char() == '?' => {
                scan.candidate = true;
                i += 1;
            }
            TokenTree::Group(group) => {
                scan_body_tokens(group.stream(), scan);
                i += 1;
            }
            _ => i += 1,
        }
    }
}

/// Whether an `assert*` invocation's significant arguments are all
/// literals, or the two compared sides are token-identical.
fn assert_args_trivial(name: &str, tokens: proc_macro2::TokenStream) -> bool {
    use proc_macro2::TokenTree;
    let mut args: Vec<Vec<TokenTree>> = vec![Vec::new()];
    for tree in tokens {
        match &tree {
            TokenTree::Punct(p) if p.as_char() == ',' => args.push(Vec::new()),
            _ => args
                .last_mut()
                .expect("BUG: argument accumulator is never empty")
                .push(tree),
        }
    }
    args.retain(|arg| !arg.is_empty());
    if args.is_empty() {
        return false;
    }
    let render = |arg: &[TokenTree]| {
        arg.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    };
    let literal_only = |arg: &[TokenTree]| {
        arg.iter().all(|tree| match tree {
            TokenTree::Literal(_) => true,
            TokenTree::Ident(ident) => ident == "true" || ident == "false",
            _ => false,
        })
    };
    match name {
        "assert" => literal_only(&args[0]),
        _ => {
            if args.len() < 2 {
                return false;
            }
            render(&args[0]) == render(&args[1])
                || (literal_only(&args[0]) && literal_only(&args[1]))
        }
    }
}

fn anchor(name: &str, args: TokenStream, item: TokenStream) -> TokenStream {
    let mut errors = proc_macro2::TokenStream::new();
    validate_id_list(&format!("#[{name}]"), args, false, &mut errors);
    let item = proc_macro2::TokenStream::from(item);
    quote! { #errors #item }.into()
}

/// Checks the `REQ-<AREA>-<NNN>` form: `AREA` is two or more uppercase
/// ASCII letters, `NNN` is exactly three ASCII digits.
///
/// Only the form is checked here. Existence in the requirements documents
/// is the external checker's job — validating it in the macro would couple
/// every `cargo build` to the doc files.
fn validate_req_id(id: &str) -> Result<(), String> {
    let bad = || {
        format!(
            "{id:?} is not a valid requirement ID \
             (expected REQ-<AREA>-<NNN>, e.g. \"REQ-HRS-002\")"
        )
    };
    let rest = id.strip_prefix("REQ-").ok_or_else(bad)?;
    let (area, num) = rest.split_once('-').ok_or_else(bad)?;
    if area.len() >= 2
        && area.bytes().all(|b| b.is_ascii_uppercase())
        && num.len() == 3
        && num.bytes().all(|b| b.is_ascii_digit())
    {
        Ok(())
    } else {
        Err(bad())
    }
}

#[cfg(test)]
mod tests {
    use super::validate_req_id;

    #[test]
    fn accepts_known_area_shapes() {
        for id in [
            "REQ-RD-001",
            "REQ-HRS-002",
            "REQ-SAFE-010",
            "REQ-PERF-025",
            "REQ-AUTH-029",
        ] {
            assert!(validate_req_id(id).is_ok(), "{id} should be valid");
        }
    }

    #[test]
    fn rejects_malformed_ids() {
        for id in [
            "",
            "REQ-",
            "REQ-HRS",
            "REQ-HRS-",
            "REQ-HRS-2",    // number must be three digits
            "REQ-HRS-0002", // number must be three digits
            "REQ-hrs-002",  // area must be uppercase
            "REQ-H-002",    // area must be at least two letters
            "REQ-HRS-00X",  // number must be digits
            "req-HRS-002",  // prefix is case-sensitive
            "REQHRS-002",
            "HRS-002",
            "REQ-HRS-002 ", // no surrounding whitespace
        ] {
            assert!(validate_req_id(id).is_err(), "{id:?} should be rejected");
        }
    }
}
