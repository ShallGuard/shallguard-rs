//! Syntax-derived, one-hop dependency propagation for impact analysis.
//! This is a conservative `syn` view, so propagated relations have
//! `possible` confidence rather than compiler-resolved certainty.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};

use anyhow::{Context, Result, bail};
use quote::ToTokens;
use regex::Regex;
use syn::spanned::Spanned as _;
use syn::visit::Visit;

use crate::scan::collect_rs_files;

/// A declaration that a changed source scope provides.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Definition {
    module: Vec<String>,
    owner: Option<String>,
    name: String,
    kind: DefinitionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DefinitionKind {
    Callable,
    Type,
    Value,
}

impl Definition {
    fn crate_name(&self) -> &str {
        self.module
            .first()
            .map(String::as_str)
            .expect("BUG: definition module has no crate component")
    }

    fn canonical_path(&self) -> Vec<&str> {
        self.module
            .iter()
            .map(String::as_str)
            .chain(self.owner.as_deref())
            .chain(std::iter::once(self.name.as_str()))
            .collect()
    }
}

/// One behavior-bearing source change eligible for reverse propagation.
#[derive(Debug)]
pub(crate) struct ChangedDefinition {
    pub change_id: String,
    pub file: String,
    pub symbol: String,
    pub line: usize,
    pub definitions: BTreeSet<Definition>,
    pub associated_requirements: BTreeSet<String>,
}

/// A requirement reached through one syntax-derived reverse dependency.
#[derive(Debug)]
pub(crate) struct DependencyImpact {
    pub class: DependencyClass,
    pub requirement: String,
    pub change_id: String,
    pub file: String,
    pub symbol: String,
    pub line: usize,
    pub reason: String,
}

/// Public impact classification selected from the changed declaration kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DependencyClass {
    Structural,
    Transitive,
}

/// A source file omitted from the dependency index after a parse failure.
#[derive(Debug)]
pub(crate) struct DependencyWarning {
    pub file: String,
    pub message: String,
}

/// Complete result of one-hop dependency propagation.
#[derive(Debug, Default)]
pub(crate) struct DependencyAnalysis {
    pub impacts: Vec<DependencyImpact>,
    pub claimed_changes: BTreeSet<String>,
    pub warnings: Vec<DependencyWarning>,
}

/// Returns the definition provided by a free Rust item, when it has one.
pub(crate) fn definition_for_item(item: &syn::Item, module: &str) -> Option<Definition> {
    let (name, kind) = match item {
        syn::Item::Const(item) => (item.ident.to_string(), DefinitionKind::Value),
        syn::Item::Enum(item) => (item.ident.to_string(), DefinitionKind::Type),
        syn::Item::Fn(item) => (item.sig.ident.to_string(), DefinitionKind::Callable),
        syn::Item::Static(item) => (item.ident.to_string(), DefinitionKind::Value),
        syn::Item::Struct(item) => (item.ident.to_string(), DefinitionKind::Type),
        syn::Item::Trait(item) => (item.ident.to_string(), DefinitionKind::Type),
        syn::Item::TraitAlias(item) => (item.ident.to_string(), DefinitionKind::Type),
        syn::Item::Type(item) => (item.ident.to_string(), DefinitionKind::Type),
        syn::Item::Union(item) => (item.ident.to_string(), DefinitionKind::Type),
        _ => return None,
    };
    Some(Definition {
        module: module_components(module),
        owner: None,
        name,
        kind,
    })
}

/// Returns the definition provided by an implementation member.
pub(crate) fn definition_for_impl_item(
    item: &syn::ImplItem,
    module: &str,
    self_ty: &syn::Type,
) -> Option<Definition> {
    let owner = simple_type_name(self_ty)?;
    let (name, kind) = match item {
        syn::ImplItem::Const(item) => (item.ident.to_string(), DefinitionKind::Value),
        syn::ImplItem::Fn(item) => (item.sig.ident.to_string(), DefinitionKind::Callable),
        syn::ImplItem::Type(item) => (item.ident.to_string(), DefinitionKind::Type),
        _ => return None,
    };
    Some(Definition {
        module: module_components(module),
        owner: Some(owner),
        name,
        kind,
    })
}

/// Returns the definition provided by a trait member.
pub(crate) fn definition_for_trait_item(
    item: &syn::TraitItem,
    module: &str,
    trait_name: &str,
) -> Option<Definition> {
    let (name, kind) = match item {
        syn::TraitItem::Const(item) => (item.ident.to_string(), DefinitionKind::Value),
        syn::TraitItem::Fn(item) => (item.sig.ident.to_string(), DefinitionKind::Callable),
        syn::TraitItem::Type(item) => (item.ident.to_string(), DefinitionKind::Type),
        _ => return None,
    };
    Some(Definition {
        module: module_components(module),
        owner: Some(trait_name.to_string()),
        name,
        kind,
    })
}

/// Finds requirements whose anchored source scopes depend directly on a
/// changed definition in either the base or head syntax graph.
#[shallguard_macros::enforces("REQ-IMP-006", "REQ-PORT-004")]
pub(crate) fn analyze(
    root: &Path,
    base_commit: &str,
    source_roots: &BTreeSet<String>,
    changes: &[ChangedDefinition],
) -> Result<DependencyAnalysis> {
    if changes.is_empty() {
        return Ok(DependencyAnalysis::default());
    }

    let base = index_base(root, base_commit, source_roots)?;
    let head = index_head(root, source_roots)?;
    Ok(propagate(changes, [base, head]))
}

#[derive(Debug, Default)]
struct RevisionIndex {
    callers: Vec<DependencyScope>,
    warnings: Vec<DependencyWarning>,
}

#[derive(Debug)]
struct DependencyScope {
    module: Vec<String>,
    owner: Option<String>,
    file: String,
    symbol: String,
    line: usize,
    enforcement: Vec<EnforcementSite>,
    references: Vec<DependencyReference>,
}

impl DependencyScope {
    fn crate_name(&self) -> &str {
        self.module
            .first()
            .map(String::as_str)
            .expect("BUG: dependency scope module has no crate component")
    }
}

#[derive(Debug)]
struct EnforcementSite {
    id: String,
    start_line: usize,
    end_line: usize,
}

#[derive(Debug)]
struct DependencyReference {
    segments: Vec<String>,
    line: usize,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PropagationKey {
    change_id: String,
    requirement: String,
    caller_file: String,
    caller_symbol: String,
}

fn propagate(changes: &[ChangedDefinition], revisions: [RevisionIndex; 2]) -> DependencyAnalysis {
    let mut changed_by_definition = BTreeMap::<Definition, Vec<usize>>::new();
    for (index, change) in changes.iter().enumerate() {
        for definition in &change.definitions {
            changed_by_definition
                .entry(definition.clone())
                .or_default()
                .push(index);
        }
    }
    let changed_definitions = changed_by_definition
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();

    let mut propagated = BTreeMap::<PropagationKey, DependencyImpact>::new();
    let mut warnings = Vec::new();
    for revision in revisions {
        warnings.extend(revision.warnings);
        for caller in &revision.callers {
            for site in &caller.enforcement {
                for reference in caller.references.iter().filter(|reference| {
                    site.start_line <= reference.line && reference.line <= site.end_line
                }) {
                    for definition in resolve_reference(reference, caller, &changed_definitions) {
                        let Some(change_indexes) = changed_by_definition.get(definition) else {
                            continue;
                        };
                        for change_index in change_indexes {
                            let change = &changes[*change_index];
                            if change.associated_requirements.contains(&site.id)
                                || (change.file == caller.file && change.symbol == caller.symbol)
                            {
                                continue;
                            }
                            let key = PropagationKey {
                                change_id: change.change_id.clone(),
                                requirement: site.id.clone(),
                                caller_file: caller.file.clone(),
                                caller_symbol: caller.symbol.clone(),
                            };
                            propagated.entry(key).or_insert_with(|| DependencyImpact {
                                class: match definition.kind {
                                    DefinitionKind::Callable => DependencyClass::Transitive,
                                    DefinitionKind::Type | DefinitionKind::Value => {
                                        DependencyClass::Structural
                                    }
                                },
                                requirement: site.id.clone(),
                                change_id: change.change_id.clone(),
                                file: change.file.clone(),
                                symbol: change.symbol.clone(),
                                line: change.line,
                                reason: dependency_reason(definition, caller),
                            });
                        }
                    }
                }
            }
        }
    }

    warnings.sort_by(|a, b| a.file.cmp(&b.file).then(a.message.cmp(&b.message)));
    warnings.dedup_by(|a, b| a.file == b.file && a.message == b.message);
    let impacts = propagated.into_values().collect::<Vec<_>>();
    let claimed_changes = impacts
        .iter()
        .map(|impact| impact.change_id.clone())
        .collect();
    DependencyAnalysis {
        impacts,
        claimed_changes,
        warnings,
    }
}

fn dependency_reason(definition: &Definition, caller: &DependencyScope) -> String {
    let changed = match definition.kind {
        DefinitionKind::Callable => "changed callable",
        DefinitionKind::Type => "changed type",
        DefinitionKind::Value => "changed constant or static",
    };
    format!(
        "{changed} is referenced by anchored scope {} at {}:{}",
        caller.symbol, caller.file, caller.line
    )
}

fn resolve_reference<'a>(
    reference: &DependencyReference,
    caller: &DependencyScope,
    definitions: &'a BTreeSet<Definition>,
) -> Vec<&'a Definition> {
    let same_crate = definitions
        .iter()
        .filter(|definition| definition.crate_name() == caller.crate_name())
        .collect::<Vec<_>>();
    let candidates = reference_candidates(reference, caller);
    same_crate
        .into_iter()
        .filter(|definition| {
            candidates.iter().any(|candidate| {
                candidate
                    .iter()
                    .map(String::as_str)
                    .eq(definition.canonical_path())
                    || (definition.kind == DefinitionKind::Type
                        && candidate.len() == definition.canonical_path().len() + 1
                        && candidate[..candidate.len() - 1]
                            .iter()
                            .map(String::as_str)
                            .eq(definition.canonical_path()))
            })
        })
        .collect()
}

fn reference_candidates(
    reference: &DependencyReference,
    caller: &DependencyScope,
) -> Vec<Vec<String>> {
    let segments = &reference.segments;
    let Some(first) = segments.first().map(String::as_str) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    match first {
        "crate" => {
            let mut path = vec![caller.crate_name().to_string()];
            path.extend(segments.iter().skip(1).cloned());
            candidates.push(path);
        }
        "self" => {
            let mut path = caller.module.clone();
            path.extend(segments.iter().skip(1).cloned());
            candidates.push(path);
        }
        "super" => {
            let mut path = caller.module.clone();
            for segment in segments
                .iter()
                .take_while(|segment| segment.as_str() == "super")
            {
                let _ = segment;
                if path.len() > 1 {
                    path.pop();
                }
            }
            path.extend(
                segments
                    .iter()
                    .skip_while(|segment| segment.as_str() == "super")
                    .cloned(),
            );
            candidates.push(path);
        }
        "Self" => {
            if let Some(owner) = &caller.owner {
                let mut path = caller.module.clone();
                path.push(owner.clone());
                path.extend(segments.iter().skip(1).cloned());
                candidates.push(path);
            }
        }
        _ => {
            let mut local = caller.module.clone();
            local.extend(segments.iter().cloned());
            candidates.push(local);
            if segments.len() > 1 {
                let mut from_crate = vec![caller.crate_name().to_string()];
                from_crate.extend(segments.iter().cloned());
                candidates.push(from_crate);
            }
        }
    }
    candidates
}

fn index_head(root: &Path, source_roots: &BTreeSet<String>) -> Result<RevisionIndex> {
    let mut files = Vec::new();
    for source_root in source_roots {
        collect_rs_files(&root.join(source_root), &mut files)?;
    }
    files.sort();

    let mut index = RevisionIndex::default();
    for file in files {
        let relative = file
            .strip_prefix(root)
            .expect("BUG: collected source file outside workspace root");
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("reading dependency source {}", file.display()))?;
        index_text(&text, relative, &mut index);
    }
    Ok(index)
}

fn index_base(
    root: &Path,
    revision: &str,
    source_roots: &BTreeSet<String>,
) -> Result<RevisionIndex> {
    let mut command = ProcessCommand::new("git");
    command.args(["ls-tree", "-r", "-z", revision, "--"]);
    command.args(source_roots);
    let output = command
        .current_dir(root)
        .output()
        .context("listing base source files for dependency analysis")?;
    if !output.status.success() {
        bail!(
            "cannot list base source files for dependency analysis: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let mut blobs = Vec::new();
    for raw in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let record = std::str::from_utf8(raw).context("base tree record is not UTF-8")?;
        let (metadata, path) = record
            .split_once('\t')
            .with_context(|| format!("invalid base tree record {record:?}"))?;
        if !path.ends_with(".rs") {
            continue;
        }
        let mut fields = metadata.split_whitespace();
        let _mode = fields.next().context("base tree record has no mode")?;
        let kind = fields
            .next()
            .context("base tree record has no object kind")?;
        let object = fields.next().context("base tree record has no object ID")?;
        if kind != "blob" || fields.next().is_some() {
            bail!("invalid base tree metadata {metadata:?}");
        }
        let path = PathBuf::from(path);
        if path.is_absolute()
            || path
                .components()
                .any(|component| component.as_os_str() == "..")
        {
            bail!("unsafe base source path {}", path.display());
        }
        blobs.push((object.to_string(), path));
    }

    let mut child = ProcessCommand::new("git")
        .args(["cat-file", "--batch"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("starting batched base source reader")?;
    {
        let mut stdin = child
            .stdin
            .take()
            .context("batched base source reader has no stdin")?;
        for (object, _) in &blobs {
            writeln!(stdin, "{object}").context("requesting base source blob")?;
        }
    }
    let batch = child
        .wait_with_output()
        .context("waiting for batched base source reader")?;
    if !batch.status.success() {
        bail!(
            "cannot read base sources for dependency analysis: {}",
            String::from_utf8_lossy(&batch.stderr).trim()
        );
    }

    let mut index = RevisionIndex::default();
    let mut offset = 0usize;
    for (expected_object, path) in blobs {
        let header_end = batch.stdout[offset..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|position| offset + position)
            .with_context(|| format!("missing batch header for {}", path.display()))?;
        let header = std::str::from_utf8(&batch.stdout[offset..header_end])
            .with_context(|| format!("batch header for {} is not UTF-8", path.display()))?;
        let mut fields = header.split_whitespace();
        let object = fields.next().context("batch header has no object ID")?;
        let kind = fields.next().context("batch header has no object kind")?;
        let size = fields
            .next()
            .context("batch header has no blob size")?
            .parse::<usize>()
            .with_context(|| format!("invalid blob size in batch header {header:?}"))?;
        if object != expected_object || kind != "blob" || fields.next().is_some() {
            bail!("unexpected batch header {header:?} for {}", path.display());
        }
        let content_start = header_end + 1;
        let content_end = content_start
            .checked_add(size)
            .filter(|end| *end <= batch.stdout.len())
            .with_context(|| format!("truncated base source blob for {}", path.display()))?;
        let text = std::str::from_utf8(&batch.stdout[content_start..content_end])
            .with_context(|| format!("base dependency source {} is not UTF-8", path.display()))?;
        index_text(text, &path, &mut index);
        if batch.stdout.get(content_end) != Some(&b'\n') {
            bail!("missing batch separator after {}", path.display());
        }
        offset = content_end + 1;
    }
    if offset != batch.stdout.len() {
        bail!("unexpected trailing output from batched base source reader");
    }
    Ok(index)
}

fn index_text(text: &str, path: &Path, index: &mut RevisionIndex) {
    if !text.contains("enforces") {
        return;
    }
    let syntax = match syn::parse_file(text) {
        Ok(syntax) => syntax,
        Err(error) => {
            index.warnings.push(DependencyWarning {
                file: path.to_string_lossy().into_owned(),
                message: format!(
                    "omitted from one-hop dependency index after parse error: {error}"
                ),
            });
            return;
        }
    };
    let id_re = requirement_id_regex();
    let module = module_root(path);
    collect_items(&syntax.items, &module, path, &id_re, index);
}

fn collect_items(
    items: &[syn::Item],
    module: &str,
    path: &Path,
    id_re: &Regex,
    index: &mut RevisionIndex,
) {
    for item in items {
        match item {
            syn::Item::Mod(item_mod) if item_mod.content.is_some() => {
                let nested = join_symbol(module, &item_mod.ident.to_string());
                let (_, nested_items) = item_mod.content.as_ref().expect("module content checked");
                collect_items(nested_items, &nested, path, id_re, index);
            }
            syn::Item::Impl(item_impl) => {
                collect_impl_items(item_impl, module, path, id_re, index);
            }
            syn::Item::Trait(item_trait) => {
                collect_trait_items(item_trait, module, path, id_re, index);
            }
            _ => index_item(item, module, None, path, id_re, index),
        }
    }
}

fn collect_impl_items(
    item_impl: &syn::ItemImpl,
    module: &str,
    path: &Path,
    id_re: &Regex,
    index: &mut RevisionIndex,
) {
    let owner = simple_type_name(&item_impl.self_ty);
    for item in &item_impl.items {
        let Some(name) = impl_item_name(item) else {
            continue;
        };
        index_scope(item, module, owner.as_deref(), name, path, id_re, index);
    }
}

fn collect_trait_items(
    item_trait: &syn::ItemTrait,
    module: &str,
    path: &Path,
    id_re: &Regex,
    index: &mut RevisionIndex,
) {
    let owner = item_trait.ident.to_string();
    for item in &item_trait.items {
        let Some(name) = trait_item_name(item) else {
            continue;
        };
        index_scope(item, module, Some(&owner), name, path, id_re, index);
    }
}

fn index_item(
    item: &syn::Item,
    module: &str,
    owner: Option<&str>,
    path: &Path,
    id_re: &Regex,
    index: &mut RevisionIndex,
) {
    let definition = definition_for_item(item, module);
    let name = definition.as_ref().map_or_else(
        || "anonymous".to_string(),
        |definition| definition.name.clone(),
    );
    index_scope(item, module, owner, name, path, id_re, index);
}

fn index_scope<T: ToTokens + for<'ast> VisitTarget<'ast>>(
    item: &T,
    module: &str,
    owner: Option<&str>,
    name: String,
    path: &Path,
    id_re: &Regex,
    index: &mut RevisionIndex,
) {
    let mut collector = ScopeCollector::new(id_re, item.target_span());
    item.visit_with(&mut collector);
    if collector.enforcement.is_empty() {
        return;
    }
    let symbol = owner.map_or_else(
        || join_symbol(module, &name),
        |owner| join_symbol(module, &format!("{owner}::{name}")),
    );
    index.callers.push(DependencyScope {
        module: module_components(module),
        owner: owner.map(str::to_string),
        file: path.to_string_lossy().into_owned(),
        symbol,
        line: item.target_span().start().line,
        enforcement: collector.enforcement,
        references: collector.references,
    });
}

trait VisitTarget<'ast> {
    fn visit_with(&'ast self, visitor: &mut ScopeCollector<'_>);
    fn target_span(&self) -> proc_macro2::Span;
}

macro_rules! impl_visit_target {
    ($type:ty, $method:ident) => {
        impl<'ast> VisitTarget<'ast> for $type {
            fn visit_with(&'ast self, visitor: &mut ScopeCollector<'_>) {
                visitor.$method(self);
            }

            fn target_span(&self) -> proc_macro2::Span {
                self.span()
            }
        }
    };
}

impl_visit_target!(syn::Item, visit_item);
impl_visit_target!(syn::ImplItem, visit_impl_item);
impl_visit_target!(syn::TraitItem, visit_trait_item);

struct ScopeCollector<'a> {
    id_re: &'a Regex,
    ranges: Vec<(usize, usize)>,
    enforcement: Vec<EnforcementSite>,
    references: Vec<DependencyReference>,
}

impl<'a> ScopeCollector<'a> {
    fn new(id_re: &'a Regex, span: proc_macro2::Span) -> Self {
        Self {
            id_re,
            ranges: vec![(span.start().line, span.end().line)],
            enforcement: Vec::new(),
            references: Vec::new(),
        }
    }

    fn collect_enforcement(&mut self, tokens: impl ToTokens) {
        let (start_line, end_line) = self
            .ranges
            .last()
            .copied()
            .expect("BUG: dependency collector without owning range");
        self.enforcement.extend(
            self.id_re
                .find_iter(&tokens.to_token_stream().to_string())
                .map(|found| EnforcementSite {
                    id: found.as_str().to_string(),
                    start_line,
                    end_line,
                }),
        );
    }

    fn collect_path(&mut self, path: &syn::Path, line: usize) {
        let segments = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>();
        if !segments.is_empty() {
            self.references.push(DependencyReference { segments, line });
        }
    }

    fn within(&mut self, span: proc_macro2::Span, visit: impl FnOnce(&mut Self)) {
        self.ranges.push((span.start().line, span.end().line));
        visit(self);
        self.ranges.pop().expect("BUG: pushed range disappeared");
    }
}

impl<'ast> Visit<'ast> for ScopeCollector<'_> {
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if path_ends_with(attribute.path(), "enforces") {
            self.collect_enforcement(&attribute.meta);
        }
        syn::visit::visit_attribute(self, attribute);
    }

    fn visit_macro(&mut self, item_macro: &'ast syn::Macro) {
        if path_ends_with(&item_macro.path, "enforces_here") {
            self.collect_enforcement(&item_macro.tokens);
        }
        syn::visit::visit_macro(self, item_macro);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = call.func.as_ref() {
            self.collect_path(&path.path, path.span().start().line);
        }
        syn::visit::visit_expr_call(self, call);
    }

    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        let terminal = path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string());
        if path.path.segments.len() > 1
            || terminal
                .as_deref()
                .is_some_and(|name| name.chars().all(|character| !character.is_lowercase()))
        {
            self.collect_path(&path.path, path.span().start().line);
        }
        syn::visit::visit_expr_path(self, path);
    }

    fn visit_expr_struct(&mut self, value: &'ast syn::ExprStruct) {
        self.collect_path(&value.path, value.path.span().start().line);
        syn::visit::visit_expr_struct(self, value);
    }

    fn visit_pat_struct(&mut self, value: &'ast syn::PatStruct) {
        self.collect_path(&value.path, value.path.span().start().line);
        syn::visit::visit_pat_struct(self, value);
    }

    fn visit_pat_tuple_struct(&mut self, value: &'ast syn::PatTupleStruct) {
        self.collect_path(&value.path, value.path.span().start().line);
        syn::visit::visit_pat_tuple_struct(self, value);
    }

    fn visit_type_path(&mut self, value: &'ast syn::TypePath) {
        self.collect_path(&value.path, value.path.span().start().line);
        syn::visit::visit_type_path(self, value);
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.within(block.span(), |collector| {
            syn::visit::visit_block(collector, block);
        });
    }

    fn visit_field(&mut self, field: &'ast syn::Field) {
        self.within(field.span(), |collector| {
            syn::visit::visit_field(collector, field);
        });
    }

    fn visit_variant(&mut self, variant: &'ast syn::Variant) {
        self.within(variant.span(), |collector| {
            syn::visit::visit_variant(collector, variant);
        });
    }

    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        self.within(arm.span(), |collector| {
            syn::visit::visit_arm(collector, arm);
        });
    }
}

fn impl_item_name(item: &syn::ImplItem) -> Option<String> {
    match item {
        syn::ImplItem::Const(item) => Some(item.ident.to_string()),
        syn::ImplItem::Fn(item) => Some(item.sig.ident.to_string()),
        syn::ImplItem::Type(item) => Some(item.ident.to_string()),
        _ => None,
    }
}

fn trait_item_name(item: &syn::TraitItem) -> Option<String> {
    match item {
        syn::TraitItem::Const(item) => Some(item.ident.to_string()),
        syn::TraitItem::Fn(item) => Some(item.sig.ident.to_string()),
        syn::TraitItem::Type(item) => Some(item.ident.to_string()),
        _ => None,
    }
}

fn simple_type_name(value: &syn::Type) -> Option<String> {
    let syn::Type::Path(path) = value else {
        return None;
    };
    path.path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn path_ends_with(path: &syn::Path, expected: &str) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == expected)
}

fn requirement_id_regex() -> Regex {
    Regex::new(r"REQ-[A-Z]{2,}-\d{3}").expect("BUG: invalid requirement ID regex")
}

fn module_components(module: &str) -> Vec<String> {
    module.split("::").map(str::to_string).collect()
}

fn module_root(path: &Path) -> String {
    let components = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    let source_index = components
        .iter()
        .position(|component| *component == "src" || *component == "tests");
    let crate_name = source_index
        .and_then(|index| index.checked_sub(1))
        .and_then(|index| components.get(index).copied())
        .unwrap_or("workspace")
        .replace('-', "_");
    let mut parts = vec![crate_name];
    if let Some(source_index) = source_index {
        parts.extend(
            components[source_index + 1..]
                .iter()
                .map(|component| component.trim_end_matches(".rs"))
                .filter(|component| {
                    !component.is_empty()
                        && *component != "lib"
                        && *component != "main"
                        && *component != "mod"
                })
                .map(|component| component.replace('-', "_")),
        );
    }
    parts.join("::")
}

fn join_symbol(prefix: &str, suffix: &str) -> String {
    if prefix.is_empty() {
        suffix.to_string()
    } else if suffix.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}::{suffix}")
    }
}

#[cfg(test)]
#[path = "impact_dependency_tests.rs"]
mod tests;
