//! Scanning of Rust sources for requirement anchors.
//!
//! Every anchor is found syntactically via `syn` — comments are never
//! anchors and never participate in checking. Three concepts are kept
//! apart:
//!
//! - **references** — every requirement ID cited by some anchor. Used
//!   only for unknown/retired-ID checking.
//! - **enforcement anchors** — `#[enforces(...)]` attributes on items,
//!   struct fields, and enum variants, and statement-position
//!   `enforces_here!("REQ-...")` macro invocations inside function
//!   bodies. Commented-out anchors or anchor-like text in strings are
//!   invisible.
//! - **verification anchors** — `#[verifies(...)]` attributes attached
//!   to functions that carry a recognized test attribute (`#[test]`,
//!   `#[tokio::test]`, or any attribute path ending in `test`) and are
//!   not `#[ignore]`d. There is no statement form: a Rust test is always
//!   an item and carries the attribute.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use syn::spanned::Spanned as _;
use syn::visit::Visit;

/// An `#[enforces]` attribute or `enforces_here!` invocation.
pub struct EnforcementAnchor {
    /// Workspace-relative Rust source file.
    pub file: PathBuf,
    /// 1-based line.
    pub line: usize,
    /// Requirement IDs carried by this anchor.
    pub ids: Vec<String>,
    /// Syntactic class used to decide whether runtime coverage applies.
    pub scope_kind: EnforcementScopeKind,
    /// Complete syntactic enforcement scope, when one can be identified.
    pub scope: Option<SourceRange>,
}

/// Coverage-relevant class of an enforcement anchor's owning syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementScopeKind {
    /// Body of an annotated free function or method.
    FunctionBody,
    /// Smallest enclosing block of an `enforces_here!` invocation.
    Block,
    /// Annotated constant initializer; executable only if LLVM emits regions.
    ConstInitializer,
    /// Annotated static initializer; executable only if LLVM emits regions.
    StaticInitializer,
    /// Declaration-only syntax such as a field, variant, type, or trait.
    Structural,
    /// An anchor inside opaque generated syntax with no recoverable block.
    Unmapped,
}

impl EnforcementScopeKind {
    /// Whether this syntax can reasonably carry LLVM executable regions.
    pub fn is_potentially_executable(self) -> bool {
        matches!(
            self,
            Self::FunctionBody | Self::Block | Self::ConstInitializer | Self::StaticInitializer
        )
    }
}

/// One-based, half-open source range compatible with LLVM coverage coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceRange {
    /// First line in the range.
    pub start_line: usize,
    /// First column in the range.
    pub start_column: usize,
    /// Last line in the range.
    pub end_line: usize,
    /// Column immediately after the range.
    pub end_column: usize,
}

/// A `#[verifies]` attribute on a real, enabled test function.
pub struct VerificationAnchor {
    pub file: PathBuf,
    /// 1-based line.
    pub line: usize,
    /// The test function's name.
    pub test_fn: String,
    /// Inline module path inside the source file.
    pub inline_modules: Vec<String>,
    pub ids: Vec<String>,
}

/// A structurally invalid anchor (e.g. `#[verifies]` off a test).
pub struct InvalidAnchor {
    pub file: PathBuf,
    pub line: usize,
    pub message: String,
}

/// Everything the scanner found.
pub struct Anchors {
    /// Every requirement ID cited by an anchor: id -> (file, line).
    pub references: HashMap<String, Vec<(PathBuf, usize)>>,
    pub enforcement: Vec<EnforcementAnchor>,
    pub verification: Vec<VerificationAnchor>,
    pub invalid: Vec<InvalidAnchor>,
}

impl Anchors {
    /// IDs claimed by at least one valid verification anchor.
    pub fn verified_ids(&self) -> impl Iterator<Item = &str> {
        self.verification
            .iter()
            .flat_map(|a| a.ids.iter().map(String::as_str))
    }
}

pub fn scan(root: &Path, roots: &[&str]) -> Result<Anchors> {
    let id_re = Regex::new(r"REQ-[A-Z]{2,}-\d{3}").expect("BUG: invalid ID regex");

    let mut files = Vec::new();
    for scan_root in roots {
        collect_rs_files(&root.join(scan_root), &mut files)
            .with_context(|| format!("scanning {scan_root}"))?;
    }
    files.sort();

    let mut anchors = Anchors {
        references: HashMap::new(),
        enforcement: Vec::new(),
        verification: Vec::new(),
        invalid: Vec::new(),
    };

    for file in files {
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("reading {}", file.display()))?;
        let rel = file
            .strip_prefix(root)
            .expect("BUG: scanned file outside workspace root")
            .to_path_buf();

        let ast = syn::parse_file(&text).with_context(|| format!("parsing {}", file.display()))?;
        walk_items(&ast.items, &rel, &[], &id_re, &mut anchors);

        let mut macro_visitor = MacroVisitor {
            file: &rel,
            id_re: &id_re,
            anchors: &mut anchors,
            blocks: Vec::new(),
        };
        macro_visitor.visit_file(&ast);
    }

    // References are anchor-derived: every ID an anchor cites, with the
    // anchor's location. Comments and other free text never count.
    let cited = anchors
        .enforcement
        .iter()
        .map(|a| (&a.ids, &a.file, a.line))
        .chain(
            anchors
                .verification
                .iter()
                .map(|a| (&a.ids, &a.file, a.line)),
        )
        .flat_map(|(ids, file, line)| ids.iter().map(move |id| (id.clone(), file.clone(), line)))
        .collect::<Vec<_>>();
    for (id, file, line) in cited {
        anchors.references.entry(id).or_default().push((file, line));
    }

    Ok(anchors)
}

/// Collects `enforces_here!("REQ-...")` macro invocations anywhere in a
/// file — statement position in function bodies, or item position.
struct MacroVisitor<'a> {
    file: &'a Path,
    id_re: &'a Regex,
    anchors: &'a mut Anchors,
    blocks: Vec<SourceRange>,
}

impl<'ast> Visit<'ast> for MacroVisitor<'_> {
    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.blocks.push(source_range(block.span()));
        syn::visit::visit_block(self, block);
        self.blocks.pop();
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let is_anchor = mac
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "enforces_here");
        if is_anchor {
            let line = mac
                .path
                .segments
                .first()
                .map_or(1, |s| s.ident.span().start().line);
            self.record(line, &mac.tokens, self.blocks.last().copied());
        } else {
            // Another macro's body (tokio::select!, etc.) is an opaque
            // token stream to syn — an `enforces_here!` nested inside it
            // still expands at compile time, so find it at token level.
            self.scan_tokens(mac.tokens.clone(), self.blocks.last().copied());
        }
        syn::visit::visit_macro(self, mac);
    }
}

impl MacroVisitor<'_> {
    /// Records one `enforces_here!` anchor whose argument tokens are
    /// `args`; an empty ID list is a structural error.
    fn record(&mut self, line: usize, args: &proc_macro2::TokenStream, scope: Option<SourceRange>) {
        let ids: Vec<String> = self
            .id_re
            .find_iter(&args.to_string())
            .map(|m| m.as_str().to_string())
            .collect();
        if ids.is_empty() {
            self.anchors.invalid.push(InvalidAnchor {
                file: self.file.to_path_buf(),
                line,
                message: "enforces_here! without a requirement ID".to_string(),
            });
        } else {
            self.anchors.enforcement.push(EnforcementAnchor {
                file: self.file.to_path_buf(),
                line,
                ids,
                scope_kind: scope.map_or(EnforcementScopeKind::Unmapped, |_| {
                    EnforcementScopeKind::Block
                }),
                scope,
            });
        }
    }

    /// Walks a raw token stream looking for the `enforces_here ! ( ... )`
    /// sequence, recursing into every group so arbitrarily nested macro
    /// bodies are covered.
    fn scan_tokens(
        &mut self,
        tokens: proc_macro2::TokenStream,
        enclosing_block: Option<SourceRange>,
    ) {
        use proc_macro2::{Delimiter, TokenTree};
        let trees: Vec<TokenTree> = tokens.into_iter().collect();
        let mut i = 0;
        while i < trees.len() {
            match &trees[i] {
                TokenTree::Ident(ident) if *ident == "enforces_here" => {
                    if let (Some(TokenTree::Punct(p)), Some(TokenTree::Group(g))) =
                        (trees.get(i + 1), trees.get(i + 2))
                        && p.as_char() == '!'
                    {
                        self.record(ident.span().start().line, &g.stream(), enclosing_block);
                        i += 3;
                        continue;
                    }
                    i += 1;
                }
                TokenTree::Group(g) => {
                    let nested_block = if g.delimiter() == Delimiter::Brace {
                        Some(source_range(g.span()))
                    } else {
                        enclosing_block
                    };
                    self.scan_tokens(g.stream(), nested_block);
                    i += 1;
                }
                _ => i += 1,
            }
        }
    }
}

/// Recursively walks items (modules, impls) collecting anchor attributes.
fn walk_items(
    items: &[syn::Item],
    file: &Path,
    inline_modules: &[String],
    id_re: &Regex,
    anchors: &mut Anchors,
) {
    for item in items {
        match item {
            syn::Item::Mod(m) => {
                collect_item_attrs(
                    &m.attrs,
                    EnforcementScopeKind::Structural,
                    source_range(m.span()),
                    file,
                    id_re,
                    anchors,
                );
                if let Some((_, items)) = &m.content {
                    let mut nested = Vec::with_capacity(inline_modules.len() + 1);
                    nested.extend_from_slice(inline_modules);
                    nested.push(m.ident.to_string());
                    walk_items(items, file, &nested, id_re, anchors);
                }
            }
            syn::Item::Impl(imp) => {
                collect_item_attrs(
                    &imp.attrs,
                    EnforcementScopeKind::Structural,
                    source_range(imp.span()),
                    file,
                    id_re,
                    anchors,
                );
                for ii in &imp.items {
                    if let syn::ImplItem::Fn(f) = ii {
                        collect_fn_attrs(
                            &f.attrs,
                            &f.sig.ident,
                            source_range(f.block.span()),
                            file,
                            inline_modules,
                            id_re,
                            anchors,
                        );
                    }
                }
            }
            syn::Item::Fn(f) => {
                collect_fn_attrs(
                    &f.attrs,
                    &f.sig.ident,
                    source_range(f.block.span()),
                    file,
                    inline_modules,
                    id_re,
                    anchors,
                );
            }
            syn::Item::Struct(s) => {
                collect_item_attrs(
                    &s.attrs,
                    EnforcementScopeKind::Structural,
                    source_range(s.span()),
                    file,
                    id_re,
                    anchors,
                );
                for field in &s.fields {
                    collect_item_attrs(
                        &field.attrs,
                        EnforcementScopeKind::Structural,
                        source_range(field.span()),
                        file,
                        id_re,
                        anchors,
                    );
                }
            }
            syn::Item::Enum(e) => {
                collect_item_attrs(
                    &e.attrs,
                    EnforcementScopeKind::Structural,
                    source_range(e.span()),
                    file,
                    id_re,
                    anchors,
                );
                for variant in &e.variants {
                    collect_item_attrs(
                        &variant.attrs,
                        EnforcementScopeKind::Structural,
                        source_range(variant.span()),
                        file,
                        id_re,
                        anchors,
                    );
                    for field in &variant.fields {
                        collect_item_attrs(
                            &field.attrs,
                            EnforcementScopeKind::Structural,
                            source_range(field.span()),
                            file,
                            id_re,
                            anchors,
                        );
                    }
                }
            }
            syn::Item::Const(c) => collect_item_attrs(
                &c.attrs,
                EnforcementScopeKind::ConstInitializer,
                source_range(c.expr.span()),
                file,
                id_re,
                anchors,
            ),
            syn::Item::Static(s) => collect_item_attrs(
                &s.attrs,
                EnforcementScopeKind::StaticInitializer,
                source_range(s.expr.span()),
                file,
                id_re,
                anchors,
            ),
            syn::Item::Trait(t) => collect_item_attrs(
                &t.attrs,
                EnforcementScopeKind::Structural,
                source_range(t.span()),
                file,
                id_re,
                anchors,
            ),
            syn::Item::Type(t) => collect_item_attrs(
                &t.attrs,
                EnforcementScopeKind::Structural,
                source_range(t.span()),
                file,
                id_re,
                anchors,
            ),
            _ => {}
        }
    }
}

/// Handles attributes on a non-function item: `#[enforces]` is an
/// enforcement anchor, `#[verifies]` is invalid here.
fn collect_item_attrs(
    attrs: &[syn::Attribute],
    scope_kind: EnforcementScopeKind,
    scope: SourceRange,
    file: &Path,
    id_re: &Regex,
    anchors: &mut Anchors,
) {
    for attr in attrs {
        match anchor_kind(attr) {
            Some(AnchorKind::Enforces) => {
                if let Some((line, ids)) = attr_ids(attr, id_re) {
                    anchors.enforcement.push(EnforcementAnchor {
                        file: file.to_path_buf(),
                        line,
                        ids,
                        scope_kind,
                        scope: Some(scope),
                    });
                }
            }
            Some(AnchorKind::Verifies) => anchors.invalid.push(InvalidAnchor {
                file: file.to_path_buf(),
                line: attr_line(attr),
                message: "#[verifies] on a non-test item is not evidence".to_string(),
            }),
            None => {}
        }
    }
}

/// Handles attributes on a function (free or impl): `#[verifies]` is a
/// verification anchor only when the function is a real, enabled test.
fn collect_fn_attrs(
    attrs: &[syn::Attribute],
    ident: &syn::Ident,
    scope: SourceRange,
    file: &Path,
    inline_modules: &[String],
    id_re: &Regex,
    anchors: &mut Anchors,
) {
    let is_test = attrs.iter().any(|a| {
        a.path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "test")
    });
    let is_ignored = attrs.iter().any(|a| a.path().is_ident("ignore"));

    for attr in attrs {
        match anchor_kind(attr) {
            Some(AnchorKind::Enforces) => {
                if let Some((line, ids)) = attr_ids(attr, id_re) {
                    anchors.enforcement.push(EnforcementAnchor {
                        file: file.to_path_buf(),
                        line,
                        ids,
                        scope_kind: EnforcementScopeKind::FunctionBody,
                        scope: Some(scope),
                    });
                }
            }
            Some(AnchorKind::Verifies) => {
                if !is_test {
                    anchors.invalid.push(InvalidAnchor {
                        file: file.to_path_buf(),
                        line: attr_line(attr),
                        message: format!(
                            "#[verifies] on non-test function `{ident}` is not evidence"
                        ),
                    });
                } else if is_ignored {
                    anchors.invalid.push(InvalidAnchor {
                        file: file.to_path_buf(),
                        line: attr_line(attr),
                        message: format!(
                            "#[verifies] on #[ignore]d test `{ident}` is not evidence"
                        ),
                    });
                } else if let Some((line, ids)) = attr_ids(attr, id_re) {
                    anchors.verification.push(VerificationAnchor {
                        file: file.to_path_buf(),
                        line,
                        test_fn: ident.to_string(),
                        inline_modules: inline_modules.to_vec(),
                        ids,
                    });
                }
            }
            None => {}
        }
    }
}

enum AnchorKind {
    Enforces,
    Verifies,
}

fn anchor_kind(attr: &syn::Attribute) -> Option<AnchorKind> {
    match attr.path().segments.last().map(|s| s.ident.to_string()) {
        Some(name) if name == "enforces" => Some(AnchorKind::Enforces),
        Some(name) if name == "verifies" => Some(AnchorKind::Verifies),
        _ => None,
    }
}

fn attr_line(attr: &syn::Attribute) -> usize {
    attr.path()
        .segments
        .first()
        .map_or(1, |s| s.ident.span().start().line)
}

fn source_range(span: proc_macro2::Span) -> SourceRange {
    let start = span.start();
    let end = span.end();
    SourceRange {
        start_line: start.line,
        start_column: start.column + 1,
        end_line: end.line,
        end_column: end.column + 1,
    }
}

fn attr_ids(attr: &syn::Attribute, id_re: &Regex) -> Option<(usize, Vec<String>)> {
    let syn::Meta::List(list) = &attr.meta else {
        return None;
    };
    let ids: Vec<String> = id_re
        .find_iter(&list.tokens.to_string())
        .map(|m| m.as_str().to_string())
        .collect();
    if ids.is_empty() {
        None
    } else {
        Some((attr_line(attr), ids))
    }
}

pub fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).with_context(|| format!("listing {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect_rs_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_text(text: &str) -> Anchors {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-trace-scan-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let sub = dir.join("src");
        std::fs::create_dir_all(&sub).expect("BUG: temp dir creation failed");
        std::fs::write(sub.join("sample.rs"), text).expect("BUG: temp write failed");
        let anchors = scan(&dir, &["src"]).expect("BUG: scan failed");
        std::fs::remove_dir_all(&dir).ok();
        anchors
    }

    #[test]
    fn comments_are_never_anchors() {
        let anchors = scan_text(
            "\
// Enforces: REQ-HRS-003 - comments no longer anchor anything.
fn a() {}

// REQ-HRS-004: bare comment, not even a reference.
fn b() {}
",
        );
        assert!(anchors.enforcement.is_empty());
        assert!(anchors.references.is_empty());
        assert!(anchors.verification.is_empty());
        assert!(anchors.invalid.is_empty());
    }

    #[test]
    fn enforces_here_macro_in_statement_and_item_position() {
        let anchors = scan_text(
            "\
use shallguard_macros::enforces_here;

enforces_here!(\"REQ-DYN-016\");

fn a(configured: usize) -> usize {
    if configured == 0 {
        enforces_here!(\"REQ-CM-038\", \"REQ-SAFE-004\");
        return 1;
    }
    match configured {
        1 => {
            enforces_here!(\"REQ-CM-039\");
            1
        }
        n => n,
    }
}
",
        );
        let all: Vec<&[String]> = anchors
            .enforcement
            .iter()
            .map(|a| a.ids.as_slice())
            .collect();
        assert_eq!(anchors.enforcement.len(), 3, "{all:?}");
        assert!(
            anchors
                .enforcement
                .iter()
                .any(|a| a.ids == vec!["REQ-CM-038", "REQ-SAFE-004"])
        );
        assert!(
            anchors
                .enforcement
                .iter()
                .any(|a| a.ids == vec!["REQ-CM-039"])
        );
        assert!(
            anchors
                .enforcement
                .iter()
                .any(|a| a.ids == vec!["REQ-DYN-016"])
        );
        let branch = anchors
            .enforcement
            .iter()
            .find(|anchor| anchor.ids.contains(&"REQ-CM-038".to_string()))
            .expect("branch anchor exists");
        assert_eq!(branch.scope_kind, EnforcementScopeKind::Block);
        assert!(branch.scope.is_some());
        let item = anchors
            .enforcement
            .iter()
            .find(|anchor| anchor.ids == ["REQ-DYN-016"])
            .expect("item anchor exists");
        assert_eq!(item.scope_kind, EnforcementScopeKind::Unmapped);
        assert!(item.scope.is_none());
        assert!(anchors.references.contains_key("REQ-CM-038"));
    }

    #[test]
    fn attribute_anchors_record_executable_and_structural_scopes() {
        let anchors = scan_text(
            "\
#[enforces(\"REQ-CM-001\")]
fn executable() {
    do_work();
}

struct Config {
    #[enforces(\"REQ-CF-001\")]
    value: usize,
}

#[enforces(\"REQ-CF-002\")]
const DEFAULT: usize = 1;
",
        );

        let function = anchors
            .enforcement
            .iter()
            .find(|anchor| anchor.ids == ["REQ-CM-001"])
            .expect("function anchor exists");
        assert_eq!(function.scope_kind, EnforcementScopeKind::FunctionBody);
        assert_eq!(function.scope.expect("function has scope").start_line, 2);

        let field = anchors
            .enforcement
            .iter()
            .find(|anchor| anchor.ids == ["REQ-CF-001"])
            .expect("field anchor exists");
        assert_eq!(field.scope_kind, EnforcementScopeKind::Structural);

        let constant = anchors
            .enforcement
            .iter()
            .find(|anchor| anchor.ids == ["REQ-CF-002"])
            .expect("constant anchor exists");
        assert_eq!(constant.scope_kind, EnforcementScopeKind::ConstInitializer);
    }

    #[test]
    fn enforces_here_nested_in_another_macro_body_is_found() {
        let anchors = scan_text(
            "\
async fn actor_loop() {
    tokio::select! {
        event = rx.recv() => {
            match event {
                Command::Create => {
                    enforces_here!(\"REQ-CM-037\");
                }
                _ => {}
            }
        }
        _ = tick.tick() => {
            enforces_here!(\"REQ-OP-048\", \"REQ-OP-046\");
        }
    }
}
",
        );
        assert_eq!(anchors.enforcement.len(), 2);
        assert!(
            anchors
                .enforcement
                .iter()
                .any(|a| a.ids == vec!["REQ-CM-037"])
        );
        assert!(
            anchors
                .enforcement
                .iter()
                .any(|a| a.ids == vec!["REQ-OP-048", "REQ-OP-046"])
        );
    }

    #[test]
    fn enforces_here_without_id_is_invalid() {
        let anchors = scan_text(
            "\
fn a() {
    enforces_here!();
}
",
        );
        assert!(anchors.enforcement.is_empty());
        assert_eq!(anchors.invalid.len(), 1);
        assert!(
            anchors.invalid[0]
                .message
                .contains("without a requirement ID")
        );
    }

    #[test]
    fn field_and_variant_attributes_are_anchors() {
        let anchors = scan_text(
            "\
#[enforces]
struct Config {
    #[enforces(\"REQ-CM-036\")]
    max_creating_providers: usize,
    other: u32,
}

#[enforces(\"REQ-CM-048\")]
enum Input {
    #[enforces(\"REQ-CM-049\")]
    Evict {
        #[enforces(\"REQ-CM-050\")]
        id: u32,
    },
}
",
        );
        let all: Vec<&str> = anchors
            .enforcement
            .iter()
            .flat_map(|a| a.ids.iter().map(String::as_str))
            .collect();
        assert!(all.contains(&"REQ-CM-036"));
        assert!(all.contains(&"REQ-CM-048"));
        assert!(all.contains(&"REQ-CM-049"));
        assert!(all.contains(&"REQ-CM-050"));
    }

    #[test]
    fn anchor_text_inside_strings_is_invisible() {
        let anchors = scan_text(
            r##"
fn a() {
    let _s = "enforces_here!(\"REQ-HRS-001\") - not an anchor";
    let _r = r#"#[enforces("REQ-HRS-001")] - not an anchor"#;
}
/* enforces_here!("REQ-HRS-001"); - inside a block comment, invisible */
"##,
        );
        assert!(anchors.enforcement.is_empty());
        assert!(anchors.references.is_empty());
    }

    #[test]
    fn verifies_attribute_needs_an_enabled_test() {
        let anchors = scan_text(
            "\
#[verifies(\"REQ-RD-006\")]
#[test]
fn valid_test() {}

#[verifies(\"REQ-RD-007\")]
fn not_a_test() {}

#[verifies(\"REQ-RD-008\")]
#[test]
#[ignore]
fn ignored_test() {}
",
        );
        let ids: Vec<&str> = anchors.verified_ids().collect();
        assert_eq!(ids, vec!["REQ-RD-006"]);
        assert_eq!(anchors.verification[0].test_fn, "valid_test");
        assert_eq!(anchors.invalid.len(), 2);
    }

    #[test]
    fn enforces_attribute_on_items_and_impl_fns() {
        let anchors = scan_text(
            "\
#[enforces(\"REQ-HRS-002\", \"REQ-SAFE-001\")]
fn site() {}

struct S;
impl S {
    #[enforces(\"REQ-RD-007\")]
    fn method(&self) {}
}

mod inner {
    #[enforces(\"REQ-DYN-016\")]
    pub struct Gate;
}
",
        );
        let all: Vec<&str> = anchors
            .enforcement
            .iter()
            .flat_map(|a| a.ids.iter().map(String::as_str))
            .collect();
        assert!(all.contains(&"REQ-HRS-002"));
        assert!(all.contains(&"REQ-SAFE-001"));
        assert!(all.contains(&"REQ-RD-007"));
        assert!(all.contains(&"REQ-DYN-016"));
        // Attributes never count as verification evidence.
        assert!(anchors.verification.is_empty());
    }

    #[test]
    fn qualified_and_wrapped_attributes_are_found_with_lines() {
        let anchors = scan_text(
            "\
mod outer {
mod tests {
#[shallguard_macros::verifies(
    \"REQ-RD-007\",
    \"REQ-RD-008\",
)]
#[tokio::test]
async fn t2() {}
}
}
",
        );
        assert_eq!(anchors.verification.len(), 1);
        assert_eq!(anchors.verification[0].ids.len(), 2);
        assert_eq!(anchors.verification[0].line, 3);
        assert_eq!(anchors.verification[0].inline_modules, ["outer", "tests"]);
    }
}
