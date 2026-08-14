//! Deterministic review-capsule generation.
//!
//! Capsules are bounded, inspectable model inputs. This module never
//! calls a model and treats source and requirement prose only as data.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::DocSpec;
use crate::coverage::COVERAGE_SCHEMA;
use crate::docs::{Requirement, parse_doc};
use crate::impact::{
    IMPACT_SCHEMA, Impact, ImpactArtifact, ImpactClass, git_file, named_function_excerpt,
    source_excerpt,
};
use crate::scan::{EnforcementScopeKind, SourceRange, scan};

pub const CAPSULE_SCHEMA: &str = "shallguard.requirement-review-capsule/v2";
const LEGACY_CAPSULE_SCHEMA: &str = "shallguard.requirement-review-capsule/v1";
pub const MANIFEST_SCHEMA: &str = "shallguard.requirement-review-manifest/v1";
pub const REVIEW_PROTOCOL: &str = "requirement-review/v2";
const MAX_ENFORCEMENT_SOURCE_LINES_PER_SITE: usize = 240;
const MAX_ENFORCEMENT_SOURCE_LINES_PER_CAPSULE: usize = 960;

pub(crate) fn is_supported_capsule_schema(schema: &str) -> bool {
    matches!(schema, CAPSULE_SCHEMA | LEGACY_CAPSULE_SCHEMA)
}
/// Inputs for deterministic capsule generation.
pub struct BundleOptions<'a> {
    /// Previously generated impact artifact.
    pub impact_file: &'a Path,
    /// Optional executable-coverage artifact to attach by requirement ID.
    pub coverage_file: Option<&'a Path>,
    /// New directory to create. Existing paths are refused.
    pub output_dir: &'a Path,
}

/// Result of creating a review bundle.
pub struct BundleResult {
    pub output_dir: PathBuf,
    pub capsules: usize,
}

#[derive(Debug, Deserialize)]
struct CleanManifest {
    schema: String,
}

/// Removes a configured generated bundle.
///
/// The path must be a real directory containing a manifest with the expected
/// ShallGuard bundle schema. Missing output is treated as an idempotent no-op.
///
/// # Errors
///
/// Returns an error when the path is a symlink, is not a directory, cannot be
/// inspected, does not contain a valid ShallGuard bundle manifest, or cannot be
/// removed.
pub fn clean_bundle(root: &Path, relative_output_dir: &Path) -> Result<Option<PathBuf>> {
    let output_dir = root.join(relative_output_dir);
    let metadata = match std::fs::symlink_metadata(&output_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("inspecting bundle {}", output_dir.display()));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "refusing to clean {} because it is not a real directory",
            output_dir.display()
        );
    }
    let manifest_path = output_dir.join("manifest.json");
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading bundle manifest {}", manifest_path.display()))?;
    let manifest: CleanManifest = serde_json::from_str(&manifest_text)
        .with_context(|| format!("parsing bundle manifest {}", manifest_path.display()))?;
    if manifest.schema != MANIFEST_SCHEMA {
        bail!(
            "refusing to clean {} because manifest schema {:?} is not {MANIFEST_SCHEMA:?}",
            output_dir.display(),
            manifest.schema
        );
    }
    std::fs::remove_dir_all(&output_dir)
        .with_context(|| format!("removing generated bundle {}", output_dir.display()))?;
    Ok(Some(output_dir))
}

/// Generates one capsule per impacted requirement plus a manifest and
/// Markdown summary.
///
/// # Errors
///
/// Returns an error for an incompatible impact schema, missing
/// requirement, unreadable source, serialization failure, or existing
/// output path.
pub fn generate(
    root: &Path,
    docs: &[DocSpec],
    options: &BundleOptions<'_>,
) -> Result<BundleResult> {
    let impact = read_impact(options.impact_file)?;
    let coverage = options
        .coverage_file
        .map(|path| read_coverage(path, &impact))
        .transpose()?;
    let requirements = load_requirements(root, docs)?;
    let selected_requirements = impact
        .requirements
        .iter()
        .map(|requirement| requirement.id.clone())
        .collect::<BTreeSet<_>>();
    let mut enforcement = enforcement_contexts(root, docs, &selected_requirements)?;
    let mut capsules = Vec::with_capacity(impact.requirements.len());

    for impacted in &impact.requirements {
        let requirement = requirements.get(impacted.id.as_str()).with_context(|| {
            format!(
                "impact artifact references requirement {} missing from head documents",
                impacted.id
            )
        })?;
        capsules.push(build_capsule(
            root,
            &impact,
            impacted.impact.as_slice(),
            requirement,
            CapsuleBuildContext {
                enforcement: enforcement.remove(&requirement.id).unwrap_or_default(),
                coverage: coverage
                    .as_ref()
                    .and_then(|coverage| coverage.get(&requirement.id))
                    .cloned(),
                coverage_requested: coverage.is_some(),
            },
        )?);
    }

    create_parent(options.output_dir)?;
    std::fs::create_dir(options.output_dir).with_context(|| {
        format!(
            "creating review output {} (refusing to replace an existing path)",
            options.output_dir.display()
        )
    })?;

    let mut entries = Vec::with_capacity(capsules.len());
    for capsule in &capsules {
        let file = format!("{}.json", capsule.requirement.id);
        write_json(&options.output_dir.join(&file), capsule)?;
        entries.push(ManifestEntry {
            requirement: capsule.requirement.id.clone(),
            description: concise_requirement_description(&capsule.requirement.statement),
            file,
            digest: capsule.provenance.digest.clone(),
            context_complete: capsule.implementation.context_complete,
        });
    }
    let manifest = BundleManifest {
        schema: MANIFEST_SCHEMA,
        repository: impact.repository.clone(),
        base_commit: impact.base_commit.clone(),
        head_commit: impact.head_commit.clone(),
        impact_schema: impact.schema.clone(),
        protocol: REVIEW_PROTOCOL,
        capsules: entries,
    };
    write_json(&options.output_dir.join("manifest.json"), &manifest)?;
    std::fs::write(
        options.output_dir.join("summary.md"),
        render_summary(&manifest),
    )
    .context("writing review bundle summary")?;

    Ok(BundleResult {
        output_dir: options.output_dir.to_path_buf(),
        capsules: capsules.len(),
    })
}

fn create_parent(path: &Path) -> Result<()> {
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(());
    };
    std::fs::create_dir_all(parent)
        .with_context(|| format!("creating output parent {}", parent.display()))
}

#[derive(Debug, Serialize, Deserialize)]
struct ReviewCapsule {
    schema: String,
    repository: String,
    base_commit: String,
    head_commit: String,
    requirement: CapsuleRequirement,
    impact: Vec<Impact>,
    implementation: ImplementationContext,
    evidence: EvidenceContext,
    provenance: CapsuleProvenance,
}

#[derive(Debug, Serialize, Deserialize)]
struct LegacyReviewCapsule {
    schema: String,
    repository: String,
    base_commit: String,
    head_commit: String,
    requirement: CapsuleRequirement,
    impact: Vec<Impact>,
    implementation: LegacyImplementationContext,
    evidence: EvidenceContext,
    provenance: CapsuleProvenance,
}

#[derive(Debug, Serialize, Deserialize)]
struct LegacyImplementationContext {
    changes: Vec<SourceContext>,
    context_complete: bool,
    limitations: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CapsuleSchemaProbe {
    schema: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CapsuleRequirement {
    id: String,
    area: String,
    document: String,
    line: usize,
    statement: String,
    clauses: Vec<NormativeClause>,
    enforced: String,
    verified: String,
    related: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NormativeClause {
    id: String,
    keyword: String,
    text: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ImplementationContext {
    changes: Vec<SourceContext>,
    #[serde(default)]
    enforcement: Vec<EnforcementContext>,
    context_complete: bool,
    limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EnforcementContext {
    file: String,
    anchor_line: usize,
    scope_kind: EnforcementScopeKind,
    scope: Option<SourceRange>,
    head: Option<SourceExcerpt>,
    limitation: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SourceContext {
    change_id: String,
    file: String,
    symbol: Option<String>,
    reported_line: usize,
    base: Option<SourceExcerpt>,
    head: Option<SourceExcerpt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceExcerpt {
    start_line: usize,
    end_line: usize,
    source: String,
}

#[derive(Default)]
struct EnforcementResolution {
    sites: Vec<EnforcementContext>,
    limitations: Vec<String>,
}

struct CapsuleBuildContext {
    enforcement: EnforcementResolution,
    coverage: Option<serde_json::Value>,
    coverage_requested: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct EvidenceContext {
    tests: Vec<TestContext>,
    static_findings: Vec<serde_json::Value>,
    coverage: Option<serde_json::Value>,
    mutations: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestContext {
    file: String,
    test_function: Option<String>,
    source_change_id: Option<String>,
    head: Option<SourceExcerpt>,
    limitation: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CapsuleProvenance {
    generator: String,
    protocol: String,
    impact_schema: String,
    digest: String,
}

#[derive(Debug, Serialize)]
struct BundleManifest {
    schema: &'static str,
    repository: String,
    base_commit: String,
    head_commit: String,
    impact_schema: String,
    protocol: &'static str,
    capsules: Vec<ManifestEntry>,
}

#[derive(Debug, Serialize)]
struct ManifestEntry {
    requirement: String,
    description: String,
    file: String,
    digest: String,
    context_complete: bool,
}

fn read_impact(path: &Path) -> Result<ImpactArtifact> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading impact artifact {}", path.display()))?;
    let impact: ImpactArtifact = serde_json::from_str(&text)
        .with_context(|| format!("parsing impact artifact {}", path.display()))?;
    if impact.schema != IMPACT_SCHEMA {
        bail!(
            "unsupported impact schema {:?}; expected {IMPACT_SCHEMA:?}",
            impact.schema
        );
    }
    Ok(impact)
}

fn read_coverage(
    path: &Path,
    impact: &ImpactArtifact,
) -> Result<BTreeMap<String, serde_json::Value>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading coverage artifact {}", path.display()))?;
    let artifact: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("parsing coverage artifact {}", path.display()))?;
    coverage_by_requirement(&artifact, &impact.head_commit)
}

fn coverage_by_requirement(
    artifact: &serde_json::Value,
    expected_head: &str,
) -> Result<BTreeMap<String, serde_json::Value>> {
    let schema = artifact
        .get("schema")
        .and_then(serde_json::Value::as_str)
        .context("coverage artifact has no string schema")?;
    if schema != COVERAGE_SCHEMA {
        bail!("unsupported coverage schema {schema:?}; expected {COVERAGE_SCHEMA:?}");
    }
    let head_commit = artifact
        .get("head_commit")
        .and_then(serde_json::Value::as_str)
        .context("coverage artifact has no string head_commit")?;
    if head_commit != expected_head {
        bail!("coverage head {head_commit:?} does not match impact head {expected_head:?}");
    }
    let requirements = artifact
        .get("requirements")
        .and_then(serde_json::Value::as_array)
        .context("coverage artifact has no requirements array")?;
    let mut by_requirement = BTreeMap::new();
    for requirement in requirements {
        let id = requirement
            .get("id")
            .and_then(serde_json::Value::as_str)
            .context("coverage requirement has no string id")?;
        let attachment = serde_json::json!({
            "schema": schema,
            "head_commit": head_commit,
            "rust_toolchain": artifact.get("rust_toolchain"),
            "coverage_tool": artifact.get("coverage_tool"),
            "configuration": artifact.get("configuration"),
            "requirement": requirement,
            "infrastructure_findings": artifact.get("infrastructure_findings"),
        });
        if by_requirement.insert(id.to_string(), attachment).is_some() {
            bail!("coverage artifact contains duplicate requirement {id}");
        }
    }
    Ok(by_requirement)
}

fn load_requirements(root: &Path, docs: &[DocSpec]) -> Result<BTreeMap<String, Requirement>> {
    let mut requirements = BTreeMap::new();
    for spec in docs {
        for requirement in parse_doc(root, spec)?.requirements {
            requirements.insert(requirement.id.clone(), requirement);
        }
    }
    Ok(requirements)
}

fn build_capsule(
    root: &Path,
    artifact: &ImpactArtifact,
    impacts: &[Impact],
    requirement: &Requirement,
    context: CapsuleBuildContext,
) -> Result<ReviewCapsule> {
    let statement = clean_statement(&requirement.statement);
    let (changes, mut limitations) = implementation_context(root, artifact, impacts)?;
    limitations.extend(context.enforcement.limitations);
    if context.enforcement.sites.is_empty() && !requirement.not_implemented && !requirement.retired
    {
        limitations.push(format!(
            "no enforcement anchor source resolved for {}",
            requirement.id
        ));
    }
    let tests = evidence_context(root, requirement, &changes, &mut limitations)?;
    if context.coverage_requested && context.coverage.is_none() {
        limitations.push(format!(
            "the supplied coverage artifact contains no result for {}",
            requirement.id
        ));
    }
    let context_complete = limitations.is_empty();
    let mut capsule = ReviewCapsule {
        schema: CAPSULE_SCHEMA.to_string(),
        repository: artifact.repository.clone(),
        base_commit: artifact.base_commit.clone(),
        head_commit: artifact.head_commit.clone(),
        requirement: CapsuleRequirement {
            id: requirement.id.clone(),
            area: requirement.area.clone(),
            document: requirement.doc.clone(),
            line: requirement.line,
            clauses: extract_clauses(&requirement.id, &statement),
            related: related_requirements(requirement),
            statement,
            enforced: requirement.enforced_text.clone(),
            verified: requirement.verified_text.clone(),
        },
        impact: impacts.to_vec(),
        implementation: ImplementationContext {
            changes,
            enforcement: context.enforcement.sites,
            context_complete,
            limitations,
        },
        evidence: EvidenceContext {
            tests,
            static_findings: Vec::new(),
            coverage: context.coverage,
            mutations: None,
        },
        provenance: CapsuleProvenance {
            generator: format!("cargo-shallguard {}", env!("CARGO_PKG_VERSION")),
            protocol: REVIEW_PROTOCOL.to_string(),
            impact_schema: artifact.schema.clone(),
            digest: String::new(),
        },
    };
    capsule.provenance.digest = capsule_digest(&capsule)?;
    Ok(capsule)
}

fn clean_statement(statement: &str) -> String {
    statement
        .split_once('\u{2014}')
        .map_or(statement, |(_, text)| text)
        .trim()
        .to_string()
}

const REQUIREMENT_DESCRIPTION_MAX_CHARS: usize = 64;

pub(crate) fn concise_requirement_description(statement: &str) -> String {
    let normalized = statement
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("**", "");
    let first_sentence = normalized
        .split_once(". ")
        .map_or(normalized.as_str(), |(sentence, _)| sentence)
        .trim_end_matches('.');
    if first_sentence.chars().count() <= REQUIREMENT_DESCRIPTION_MAX_CHARS {
        return first_sentence.to_string();
    }

    let byte_limit = first_sentence
        .char_indices()
        .nth(REQUIREMENT_DESCRIPTION_MAX_CHARS - 3)
        .map_or(first_sentence.len(), |(index, _)| index);
    let prefix = &first_sentence[..byte_limit];
    let word_boundary = prefix.rfind(char::is_whitespace).unwrap_or(byte_limit);
    format!("{}...", prefix[..word_boundary].trim_end())
}

pub(crate) fn capsule_description(text: &str, expected_requirement: &str) -> Result<String> {
    let capsule: ReviewCapsule =
        serde_json::from_str(text).context("parsing review capsule description")?;
    if capsule.requirement.id != expected_requirement {
        bail!(
            "capsule requirement {:?} does not match manifest requirement {:?}",
            capsule.requirement.id,
            expected_requirement
        );
    }
    Ok(concise_requirement_description(
        &capsule.requirement.statement,
    ))
}

fn extract_clauses(requirement: &str, statement: &str) -> Vec<NormativeClause> {
    let keyword_re = Regex::new(r"\b(SHALL NOT|MUST NOT|SHOULD NOT|SHALL|MUST|SHOULD|MAY)\b")
        .expect("BUG: invalid normative-keyword regex");
    let segments = clause_segments(statement);
    let mut clauses = Vec::new();
    for segment in segments {
        for keyword in keyword_re.find_iter(segment) {
            clauses.push(NormativeClause {
                id: format!("{requirement}/C{}", clauses.len() + 1),
                keyword: keyword.as_str().to_string(),
                text: segment.trim().to_string(),
            });
        }
    }
    if clauses.is_empty() {
        clauses.push(NormativeClause {
            id: format!("{requirement}/C1"),
            keyword: "UNSPECIFIED".to_string(),
            text: statement.to_string(),
        });
    }
    clauses
}

fn clause_segments(statement: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut start = 0usize;
    for (index, character) in statement.char_indices() {
        if matches!(character, ';' | '.') {
            let end = index + character.len_utf8();
            if !statement[start..end].trim().is_empty() {
                segments.push(&statement[start..end]);
            }
            start = end;
        }
    }
    if !statement[start..].trim().is_empty() {
        segments.push(&statement[start..]);
    }
    segments
}

fn related_requirements(requirement: &Requirement) -> Vec<String> {
    let id_re = Regex::new(r"REQ-[A-Z]{2,}-\d{3}").expect("BUG: invalid requirement ID regex");
    let mut related = BTreeSet::new();
    for text in [
        requirement.statement.as_str(),
        requirement.enforced_text.as_str(),
        requirement.verified_text.as_str(),
    ] {
        related.extend(
            id_re
                .find_iter(text)
                .map(|found| found.as_str().to_string())
                .filter(|id| id != &requirement.id),
        );
    }
    related.into_iter().collect()
}

fn enforcement_contexts(
    root: &Path,
    docs: &[DocSpec],
    selected: &BTreeSet<String>,
) -> Result<BTreeMap<String, EnforcementResolution>> {
    let scan_roots = docs
        .iter()
        .flat_map(DocSpec::scan_roots)
        .collect::<BTreeSet<_>>();
    let anchors = scan(
        root,
        &scan_roots.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    let mut grouped = selected
        .iter()
        .map(|id| (id.clone(), BTreeMap::new()))
        .collect::<BTreeMap<String, BTreeMap<_, usize>>>();
    for anchor in anchors.enforcement {
        let key = (anchor.file, anchor.scope_kind, anchor.scope);
        for id in anchor.ids.into_iter().filter(|id| selected.contains(id)) {
            grouped
                .get_mut(&id)
                .expect("BUG: selected enforcement requirement is missing")
                .entry(key.clone())
                .and_modify(|line| *line = (*line).min(anchor.line))
                .or_insert(anchor.line);
        }
    }

    let mut source_cache = BTreeMap::<PathBuf, String>::new();
    let mut resolved = BTreeMap::new();
    for (id, sites) in grouped {
        let mut resolution = EnforcementResolution::default();
        let mut remaining_lines = MAX_ENFORCEMENT_SOURCE_LINES_PER_CAPSULE;
        for ((file, scope_kind, scope), anchor_line) in sites {
            if !source_cache.contains_key(&file) {
                let text = std::fs::read_to_string(root.join(&file)).with_context(|| {
                    format!("reading enforcement source {}", root.join(&file).display())
                })?;
                source_cache.insert(file.clone(), text);
            }
            let text = source_cache
                .get(&file)
                .expect("BUG: enforcement source was inserted above");
            let excerpt = enforcement_excerpt(text, anchor_line, scope, remaining_lines);
            remaining_lines = remaining_lines.saturating_sub(excerpt.included_lines);
            let file_name = file.to_string_lossy().replace('\\', "/");
            if let Some(limitation) = &excerpt.limitation {
                resolution
                    .limitations
                    .push(format!("{file_name}:{anchor_line}: {limitation}"));
            }
            resolution.sites.push(EnforcementContext {
                file: file_name,
                anchor_line,
                scope_kind,
                scope,
                head: excerpt.head,
                limitation: excerpt.limitation,
            });
        }
        resolved.insert(id, resolution);
    }
    Ok(resolved)
}

struct BoundedEnforcementExcerpt {
    head: Option<SourceExcerpt>,
    included_lines: usize,
    limitation: Option<String>,
}

fn enforcement_excerpt(
    text: &str,
    anchor_line: usize,
    scope: Option<SourceRange>,
    remaining_lines: usize,
) -> BoundedEnforcementExcerpt {
    let (scope_start, scope_end) = scope.map_or((anchor_line, anchor_line), |scope| {
        (
            scope.start_line.min(anchor_line),
            scope.end_line.max(anchor_line),
        )
    });
    let available_lines = remaining_lines.min(MAX_ENFORCEMENT_SOURCE_LINES_PER_SITE);
    if available_lines == 0 {
        return BoundedEnforcementExcerpt {
            head: None,
            included_lines: 0,
            limitation: Some(format!(
                "source omitted because the per-capsule enforcement budget is {} lines",
                MAX_ENFORCEMENT_SOURCE_LINES_PER_CAPSULE
            )),
        };
    }
    let (start_line, end_line) =
        bounded_line_range(scope_start, scope_end, anchor_line, available_lines);
    let source = source_line_range(text, start_line, end_line);
    let included_lines = source.as_ref().map_or(0, |_| end_line - start_line + 1);
    let limitation = if source.is_none() {
        Some(format!(
            "source range {start_line}-{end_line} is outside the current file"
        ))
    } else if scope.is_none() {
        Some("anchor has no recoverable syntactic scope; only its source line is supplied".into())
    } else if start_line != scope_start || end_line != scope_end {
        Some(format!(
            "scope {scope_start}-{scope_end} is truncated to {start_line}-{end_line} by the bounded source budget"
        ))
    } else {
        None
    };
    BoundedEnforcementExcerpt {
        head: source.map(|source| SourceExcerpt {
            start_line,
            end_line,
            source,
        }),
        included_lines,
        limitation,
    }
}

fn bounded_line_range(
    scope_start: usize,
    scope_end: usize,
    anchor_line: usize,
    limit: usize,
) -> (usize, usize) {
    let scope_len = scope_end.saturating_sub(scope_start).saturating_add(1);
    if scope_len <= limit {
        return (scope_start, scope_end);
    }
    let anchor_line = anchor_line.clamp(scope_start, scope_end);
    let mut start = anchor_line.saturating_sub(limit / 3).max(scope_start);
    let end = start.saturating_add(limit - 1).min(scope_end);
    start = end.saturating_add(1).saturating_sub(limit).max(scope_start);
    (start, end)
}

fn source_line_range(text: &str, start_line: usize, end_line: usize) -> Option<String> {
    if start_line == 0 || start_line > end_line {
        return None;
    }
    let count = end_line - start_line + 1;
    let lines = text
        .lines()
        .skip(start_line - 1)
        .take(count)
        .collect::<Vec<_>>();
    (lines.len() == count).then(|| lines.join("\n"))
}

fn implementation_context(
    root: &Path,
    artifact: &ImpactArtifact,
    impacts: &[Impact],
) -> Result<(Vec<SourceContext>, Vec<String>)> {
    let mut changes = Vec::new();
    let mut limitations = Vec::new();
    let mut seen = BTreeSet::new();
    for impact in impacts {
        if impact.class == ImpactClass::Specification
            || Path::new(&impact.site.file)
                .extension()
                .is_none_or(|ext| ext != "rs")
        {
            continue;
        }
        let key = (
            impact.change_id.as_str(),
            impact.site.file.as_str(),
            impact.site.symbol.as_deref(),
        );
        if !seen.insert(key) {
            continue;
        }
        let path = Path::new(&impact.site.file);
        let base_text = git_file(root, &artifact.base_commit, path)?;
        let head_text = read_optional(root.join(path))?;
        let base = excerpt(
            base_text.as_deref(),
            path,
            impact.site.symbol.as_deref(),
            impact.site.line,
        )?;
        let head = excerpt(
            head_text.as_deref(),
            path,
            impact.site.symbol.as_deref(),
            impact.site.line,
        )?;
        if base.is_none() && head.is_none() {
            limitations.push(format!(
                "no source excerpt resolved for {} at {}:{}",
                impact.change_id, impact.site.file, impact.site.line
            ));
        }
        changes.push(SourceContext {
            change_id: impact.change_id.clone(),
            file: impact.site.file.clone(),
            symbol: impact.site.symbol.clone(),
            reported_line: impact.site.line,
            base,
            head,
        });
    }
    Ok((changes, limitations))
}

fn evidence_context(
    root: &Path,
    requirement: &Requirement,
    changes: &[SourceContext],
    limitations: &mut Vec<String>,
) -> Result<Vec<TestContext>> {
    let mut tests = Vec::with_capacity(requirement.evidence.len());
    for evidence in &requirement.evidence {
        let source_change_id = evidence.test_fn.as_deref().and_then(|function| {
            let suffix = format!("::fn:{function}");
            changes
                .iter()
                .find(|change| {
                    change.file == evidence.file.to_string_lossy()
                        && change
                            .symbol
                            .as_ref()
                            .is_some_and(|symbol| symbol.ends_with(&suffix))
                })
                .map(|change| change.change_id.clone())
        });
        let text = read_optional(root.join(&evidence.file))?;
        let head = match (
            source_change_id.is_none(),
            text.as_deref(),
            evidence.test_fn.as_deref(),
        ) {
            (true, Some(text), Some(function)) => {
                named_function_excerpt(text, &evidence.file, function)?.map(tuple_excerpt)
            }
            _ => None,
        };
        let limitation = if evidence.test_fn.is_none() {
            Some("evidence citation names no test function".to_string())
        } else if text.is_none() {
            Some("cited test file is unavailable".to_string())
        } else if head.is_none() && source_change_id.is_none() {
            Some("cited test function did not resolve in head source".to_string())
        } else {
            None
        };
        if let Some(limitation) = &limitation {
            limitations.push(format!("{}: {limitation}", evidence.file.to_string_lossy()));
        }
        tests.push(TestContext {
            file: evidence.file.to_string_lossy().into_owned(),
            test_function: evidence.test_fn.clone(),
            source_change_id,
            head,
            limitation,
        });
    }
    Ok(tests)
}

fn excerpt(
    text: Option<&str>,
    path: &Path,
    symbol: Option<&str>,
    line: usize,
) -> Result<Option<SourceExcerpt>> {
    text.map(|text| {
        source_excerpt(text, path, symbol, line).map(|excerpt| excerpt.map(tuple_excerpt))
    })
    .transpose()
    .map(Option::flatten)
}

fn tuple_excerpt((start_line, end_line, source): (usize, usize, String)) -> SourceExcerpt {
    SourceExcerpt {
        start_line,
        end_line,
        source,
    }
}

fn read_optional(path: PathBuf) -> Result<Option<String>> {
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("reading {}", path.display())),
    }
}

fn capsule_digest(capsule: &impl Serialize) -> Result<String> {
    let encoded = serde_json::to_vec(capsule).context("serializing capsule for digest")?;
    let digest = Sha256::digest(encoded);
    Ok(format!("sha256:{digest:x}"))
}

/// Verifies that a serialized capsule still matches its manifest digest.
pub(crate) fn verify_capsule_digest(text: &str, manifest_digest: &str) -> Result<()> {
    let schema: CapsuleSchemaProbe =
        serde_json::from_str(text).context("parsing capsule schema for digest verification")?;
    let computed = match schema.schema.as_str() {
        CAPSULE_SCHEMA => {
            let mut capsule: ReviewCapsule =
                serde_json::from_str(text).context("parsing capsule for digest verification")?;
            validate_claimed_digest(&capsule.provenance.digest, manifest_digest)?;
            capsule.provenance.digest.clear();
            capsule_digest(&capsule)?
        }
        LEGACY_CAPSULE_SCHEMA => {
            let mut capsule: LegacyReviewCapsule = serde_json::from_str(text)
                .context("parsing legacy capsule for digest verification")?;
            validate_claimed_digest(&capsule.provenance.digest, manifest_digest)?;
            capsule.provenance.digest.clear();
            capsule_digest(&capsule)?
        }
        unsupported => {
            bail!(
                "unsupported capsule schema {unsupported:?}; expected {CAPSULE_SCHEMA:?} or legacy {LEGACY_CAPSULE_SCHEMA:?}"
            )
        }
    };
    if computed != manifest_digest {
        bail!(
            "capsule content digest {computed:?} does not match recorded digest \
             {manifest_digest:?}"
        );
    }
    Ok(())
}

fn validate_claimed_digest(claimed: &str, manifest_digest: &str) -> Result<()> {
    if claimed != manifest_digest {
        bail!("capsule provenance digest does not match bundle manifest");
    }
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut json = serde_json::to_string_pretty(value)
        .with_context(|| format!("serializing {}", path.display()))?;
    json.push('\n');
    std::fs::write(path, json).with_context(|| format!("writing {}", path.display()))
}

fn render_summary(manifest: &BundleManifest) -> String {
    let mut out = String::new();
    let incomplete = manifest
        .capsules
        .iter()
        .filter(|capsule| !capsule.context_complete)
        .count();
    let _ = writeln!(out, "# Requirement review bundle\n");
    let _ = writeln!(out, "- Base: `{}`", manifest.base_commit);
    let _ = writeln!(out, "- Head: `{}`", manifest.head_commit);
    let _ = writeln!(out, "- Capsules: {}", manifest.capsules.len());
    let _ = writeln!(out, "- Incomplete contexts: {incomplete}");
    if !manifest.capsules.is_empty() {
        let _ = writeln!(out, "\n## Capsules\n");
        for capsule in &manifest.capsules {
            let status = if capsule.context_complete {
                "complete"
            } else {
                "incomplete"
            };
            let _ = writeln!(
                out,
                "- [`{}`]({}) - {status} - `{}`",
                capsule.requirement, capsule.file, capsule.digest
            );
        }
    }
    out
}

#[cfg(test)]
pub(crate) fn review_test_capsule_with_coverage() -> (String, String) {
    let mut capsule = ReviewCapsule {
        schema: CAPSULE_SCHEMA.to_string(),
        repository: "repo".to_string(),
        base_commit: "base".to_string(),
        head_commit: "head".to_string(),
        requirement: CapsuleRequirement {
            id: "REQ-ZZ-001".to_string(),
            area: "ZZ".to_string(),
            document: "docs/requirements.md".to_string(),
            line: 7,
            statement: "The system SHALL retain evidence.".to_string(),
            clauses: vec![NormativeClause {
                id: "REQ-ZZ-001/C1".to_string(),
                keyword: "SHALL".to_string(),
                text: "retain evidence".to_string(),
            }],
            enforced: "src/lib.rs".to_string(),
            verified: "src/lib.rs::tests::retains_evidence".to_string(),
            related: Vec::new(),
        },
        impact: Vec::new(),
        implementation: ImplementationContext {
            changes: Vec::new(),
            enforcement: vec![EnforcementContext {
                file: "src/lib.rs".to_string(),
                anchor_line: 10,
                scope_kind: EnforcementScopeKind::FunctionBody,
                scope: Some(SourceRange {
                    start_line: 10,
                    start_column: 1,
                    end_line: 20,
                    end_column: 2,
                }),
                head: Some(SourceExcerpt {
                    start_line: 10,
                    end_line: 20,
                    source: "fn retain_evidence() {}".to_string(),
                }),
                limitation: None,
            }],
            context_complete: true,
            limitations: Vec::new(),
        },
        evidence: EvidenceContext {
            tests: Vec::new(),
            static_findings: Vec::new(),
            coverage: Some(serde_json::json!({
                "requirement": {
                    "sites": [{
                        "file": "src/lib.rs",
                        "anchor_line": 42,
                        "scope": { "start_line": 40, "end_line": 50 }
                    }]
                }
            })),
            mutations: None,
        },
        provenance: CapsuleProvenance {
            generator: "cargo-shallguard test".to_string(),
            protocol: REVIEW_PROTOCOL.to_string(),
            impact_schema: IMPACT_SCHEMA.to_string(),
            digest: String::new(),
        },
    };
    let digest = capsule_digest(&capsule).expect("test capsule digest succeeds");
    capsule.provenance.digest.clone_from(&digest);
    let text = serde_json::to_string(&capsule).expect("test capsule serializes");
    (text, digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_BUNDLE_DIR: &str = "target/requirement-review";

    #[test]
    fn extracts_normative_clauses_and_keeps_complete_segments() {
        let clauses = extract_clauses(
            "REQ-ZZ-001",
            "The service SHALL validate input; it MUST NOT mutate on failure.",
        );
        assert_eq!(clauses.len(), 2);
        assert_eq!(clauses[0].id, "REQ-ZZ-001/C1");
        assert_eq!(clauses[0].keyword, "SHALL");
        assert_eq!(clauses[0].text, "The service SHALL validate input;");
        assert_eq!(clauses[1].keyword, "MUST NOT");
    }

    #[test]
    fn requirement_description_is_concise_and_human_readable() {
        let description = concise_requirement_description(
            "Every controller pass SHALL emit at least the `other` sink split for **every** \
             configured domain. Additional rationale follows.",
        );

        assert_eq!(
            description,
            "Every controller pass SHALL emit at least the `other` sink..."
        );
        assert!(description.chars().count() <= REQUIREMENT_DESCRIPTION_MAX_CHARS);
    }

    #[test]
    fn capsule_description_checks_identity() {
        let text = serde_json::to_string(&sample_capsule(
            "The service SHALL preserve work. Additional rationale.",
        ))
        .expect("capsule serializes");

        assert_eq!(
            capsule_description(&text, "REQ-ZZ-001").expect("description parses"),
            "The service SHALL preserve work"
        );
        assert!(capsule_description(&text, "REQ-ZZ-002").is_err());
    }

    #[test]
    fn related_requirements_are_unique_and_exclude_self() {
        let requirement = Requirement {
            id: "REQ-ZZ-001".to_string(),
            area: "ZZ".to_string(),
            title: "test".to_string(),
            statement: "REQ-ZZ-001 composes REQ-AA-002 and REQ-AA-002".to_string(),
            enforced_text: "REQ-BB-003".to_string(),
            verified_text: "REQ-ZZ-001".to_string(),
            doc: "docs/requirements.md".to_string(),
            line: 1,
            enforced_paths: Vec::new(),
            not_implemented: false,
            retired: false,
            automated: false,
            evidence: Vec::new(),
            e2e: false,
            review_only: true,
            pending: false,
        };
        assert_eq!(
            related_requirements(&requirement),
            vec!["REQ-AA-002".to_string(), "REQ-BB-003".to_string()]
        );
    }

    #[test]
    fn capsule_includes_unchanged_anchored_enforcement_source() {
        let root = tempfile::tempdir().expect("temporary workspace");
        let source_path = root.path().join("crate/src/lib.rs");
        std::fs::create_dir_all(source_path.parent().expect("source has parent"))
            .expect("source directory creates");
        std::fs::write(
            &source_path,
            "#[enforces(\"REQ-ZZ-001\")]\nfn retain_state(input: bool) -> bool {\n    if input {\n        true\n    } else {\n        false\n    }\n}\n",
        )
        .expect("source fixture writes");
        let docs = vec![DocSpec::new(
            "crate/docs/requirements.md",
            "crate",
            BTreeMap::new(),
        )];
        let selected = BTreeSet::from(["REQ-ZZ-001".to_string()]);
        let mut enforcement =
            enforcement_contexts(root.path(), &docs, &selected).expect("anchors resolve");
        let resolution = enforcement
            .remove("REQ-ZZ-001")
            .expect("requirement resolution exists");
        let requirement = Requirement {
            id: "REQ-ZZ-001".to_string(),
            area: "ZZ".to_string(),
            title: "Retain state".to_string(),
            statement: "- **REQ-ZZ-001** — The service SHALL retain state.".to_string(),
            enforced_text: "`src/lib.rs` (`retain_state`) ·".to_string(),
            verified_text: "👁 code review only".to_string(),
            doc: "crate/docs/requirements.md".to_string(),
            line: 1,
            enforced_paths: vec![PathBuf::from("crate/src/lib.rs")],
            not_implemented: false,
            retired: false,
            automated: false,
            evidence: Vec::new(),
            e2e: false,
            review_only: true,
            pending: false,
        };
        let artifact = ImpactArtifact {
            schema: IMPACT_SCHEMA.to_string(),
            repository: "repo".to_string(),
            base_commit: "base".to_string(),
            head_commit: "head".to_string(),
            head_source: "working-tree".to_string(),
            working_tree_dirty: true,
            configuration: crate::impact::ImpactConfiguration {
                features: Vec::new(),
                targets: vec!["workspace".to_string()],
                dependency_depth: 1,
                scope_precision: "rust-item+anchor-block".to_string(),
            },
            requirements: Vec::new(),
            unclaimed_changes: Vec::new(),
            findings: Vec::new(),
        };

        let capsule = build_capsule(
            root.path(),
            &artifact,
            &[],
            &requirement,
            CapsuleBuildContext {
                enforcement: resolution,
                coverage: None,
                coverage_requested: false,
            },
        )
        .expect("capsule builds");

        assert!(capsule.implementation.changes.is_empty());
        assert!(capsule.implementation.context_complete);
        assert_eq!(capsule.implementation.enforcement.len(), 1);
        let site = &capsule.implementation.enforcement[0];
        assert_eq!(site.file, "crate/src/lib.rs");
        assert_eq!(site.anchor_line, 1);
        assert_eq!(site.scope_kind, EnforcementScopeKind::FunctionBody);
        let head = site.head.as_ref().expect("enforcement source is supplied");
        assert_eq!(head.start_line, 1);
        assert!(head.source.contains("fn retain_state"));
        assert!(head.source.contains("false"));
    }

    #[test]
    fn oversized_enforcement_scope_is_bounded_and_marked_incomplete() {
        let text = (1..=400)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let excerpt = enforcement_excerpt(
            &text,
            200,
            Some(SourceRange {
                start_line: 1,
                start_column: 1,
                end_line: 400,
                end_column: 1,
            }),
            MAX_ENFORCEMENT_SOURCE_LINES_PER_CAPSULE,
        );

        let head = excerpt.head.expect("bounded excerpt exists");
        assert_eq!(
            excerpt.included_lines,
            MAX_ENFORCEMENT_SOURCE_LINES_PER_SITE
        );
        assert!(head.start_line <= 200 && head.end_line >= 200);
        assert!(
            excerpt
                .limitation
                .as_deref()
                .is_some_and(|limitation| limitation.contains("truncated"))
        );
    }

    #[test]
    fn digest_is_stable_and_content_sensitive() {
        let capsule = sample_capsule("statement one");
        let same = sample_capsule("statement one");
        let changed = sample_capsule("statement two");
        assert_eq!(
            capsule_digest(&capsule).expect("digest succeeds"),
            capsule_digest(&same).expect("digest succeeds")
        );
        assert_ne!(
            capsule_digest(&capsule).expect("digest succeeds"),
            capsule_digest(&changed).expect("digest succeeds")
        );
    }

    #[test]
    fn verifies_serialized_capsule_content_against_manifest_digest() {
        let mut capsule = sample_capsule("statement one");
        let digest = capsule_digest(&capsule).expect("digest succeeds");
        capsule.provenance.digest.clone_from(&digest);
        let text = serde_json::to_string(&capsule).expect("capsule serializes");
        verify_capsule_digest(&text, &digest).expect("matching capsule verifies");

        let tampered = text.replace("statement one", "statement changed");
        assert!(verify_capsule_digest(&tampered, &digest).is_err());

        let legacy_source = serde_json::to_string(&sample_capsule("legacy statement"))
            .expect("legacy source serializes");
        let mut legacy: LegacyReviewCapsule =
            serde_json::from_str(&legacy_source).expect("legacy capsule converts");
        legacy.schema = LEGACY_CAPSULE_SCHEMA.to_string();
        let legacy_digest = capsule_digest(&legacy).expect("legacy digest succeeds");
        legacy.provenance.digest.clone_from(&legacy_digest);
        let legacy_text = serde_json::to_string(&legacy).expect("legacy capsule serializes");
        verify_capsule_digest(&legacy_text, &legacy_digest)
            .expect("legacy capsule remains replayable");
    }

    #[test]
    fn selects_requirement_coverage_and_checks_head_identity() {
        let artifact = serde_json::json!({
            "schema": COVERAGE_SCHEMA,
            "head_commit": "head",
            "rust_toolchain": "rustc test",
            "coverage_tool": "cargo-llvm-cov test",
            "configuration": {},
            "requirements": [{ "id": "REQ-ZZ-001", "status": "covered" }],
            "infrastructure_findings": []
        });
        let coverage =
            coverage_by_requirement(&artifact, "head").expect("coverage artifact validates");
        assert_eq!(coverage["REQ-ZZ-001"]["requirement"]["status"], "covered");
        assert!(coverage_by_requirement(&artifact, "other-head").is_err());
    }

    #[test]
    fn clean_removes_only_a_valid_default_bundle_and_is_idempotent() {
        let root = tempfile::tempdir().expect("temporary workspace");
        let output = root.path().join(TEST_BUNDLE_DIR);
        std::fs::create_dir_all(&output).expect("bundle directory is created");
        std::fs::write(
            output.join("manifest.json"),
            serde_json::json!({ "schema": MANIFEST_SCHEMA }).to_string(),
        )
        .expect("bundle manifest is written");
        std::fs::write(output.join("REQ-ZZ-001.json"), "{}\n")
            .expect("capsule placeholder is written");

        assert_eq!(
            clean_bundle(root.path(), Path::new(TEST_BUNDLE_DIR)).expect("valid bundle cleans"),
            Some(output.clone())
        );
        assert!(!output.exists());
        assert_eq!(
            clean_bundle(root.path(), Path::new(TEST_BUNDLE_DIR))
                .expect("missing bundle is a no-op"),
            None
        );
    }

    #[test]
    fn clean_preserves_a_directory_without_a_shallguard_manifest() {
        let root = tempfile::tempdir().expect("temporary workspace");
        let output = root.path().join(TEST_BUNDLE_DIR);
        std::fs::create_dir_all(&output).expect("bundle directory is created");
        std::fs::write(
            output.join("manifest.json"),
            serde_json::json!({ "schema": "unrelated-data/v1" }).to_string(),
        )
        .expect("unrelated manifest is written");

        let error = clean_bundle(root.path(), Path::new(TEST_BUNDLE_DIR))
            .expect_err("unrelated data is rejected");
        assert!(error.to_string().contains("refusing to clean"));
        assert!(output.exists());
    }

    fn sample_capsule(statement: &str) -> ReviewCapsule {
        ReviewCapsule {
            schema: CAPSULE_SCHEMA.to_string(),
            repository: "repo".to_string(),
            base_commit: "base".to_string(),
            head_commit: "head".to_string(),
            requirement: CapsuleRequirement {
                id: "REQ-ZZ-001".to_string(),
                area: "ZZ".to_string(),
                document: "docs/requirements.md".to_string(),
                line: 1,
                statement: statement.to_string(),
                clauses: Vec::new(),
                enforced: String::new(),
                verified: String::new(),
                related: Vec::new(),
            },
            impact: Vec::new(),
            implementation: ImplementationContext {
                changes: Vec::new(),
                enforcement: Vec::new(),
                context_complete: true,
                limitations: Vec::new(),
            },
            evidence: EvidenceContext {
                tests: Vec::new(),
                static_findings: Vec::new(),
                coverage: None,
                mutations: None,
            },
            provenance: CapsuleProvenance {
                generator: "cargo-shallguard test".to_string(),
                protocol: REVIEW_PROTOCOL.to_string(),
                impact_schema: IMPACT_SCHEMA.to_string(),
                digest: String::new(),
            },
        }
    }
}
