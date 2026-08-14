//! Deterministic base/head requirement impact analysis.
//!
//! The implementation compares normalized requirement clauses and Rust
//! item syntax, then performs one conservative reverse-dependency hop
//! from changed declarations to anchored enforcement scopes.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use quote::ToTokens;
use regex::Regex;
use serde::{Deserialize, Serialize};
use syn::spanned::Spanned as _;
use syn::visit::Visit;

use crate::DocSpec;
use crate::baseline::{BASELINE_PATH, Baseline, GapKey};
use crate::docs::{Requirement, parse_text};
use crate::impact_dependency::{
    self, ChangedDefinition, Definition, DependencyClass, definition_for_impl_item,
    definition_for_item, definition_for_trait_item,
};

/// Version of the public impact artifact.
pub const IMPACT_SCHEMA: &str = "shallguard.requirement-impact/v1";

/// Inputs controlling one impact analysis.
pub struct ImpactOptions<'a> {
    /// How the comparison base is selected.
    pub base: BaseSelection<'a>,
}

/// Git base selection for local, MR, and branch pipelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseSelection<'a> {
    /// Use this exact revision as the base.
    Revision(&'a str),
    /// Compute `git merge-base HEAD <target>` and use that commit.
    MergeBaseWith(&'a str),
}

/// Complete, deterministic impact artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactArtifact {
    pub schema: String,
    pub repository: String,
    pub base_commit: String,
    pub head_commit: String,
    pub head_source: String,
    pub working_tree_dirty: bool,
    pub configuration: ImpactConfiguration,
    pub requirements: Vec<ImpactedRequirement>,
    pub unclaimed_changes: Vec<UnclaimedChange>,
    pub findings: Vec<ImpactFinding>,
}

/// Analysis configuration recorded for reproducibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactConfiguration {
    pub features: Vec<String>,
    pub targets: Vec<String>,
    pub dependency_depth: usize,
    pub scope_precision: String,
}

impl ImpactArtifact {
    /// Returns true when deterministic MR policy should reject the
    /// analyzed change after still publishing the artifact.
    pub fn has_policy_errors(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity == FindingSeverity::Error)
    }

    /// Serializes the artifact as stable, pretty-printed JSON.
    pub fn to_json(&self) -> Result<String> {
        let mut json = serde_json::to_string_pretty(self).context("serializing impact artifact")?;
        json.push('\n');
        Ok(json)
    }

    /// Renders a concise human-readable companion report.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# Requirement impact\n");
        let _ = writeln!(out, "- Base: `{}`", self.base_commit);
        let _ = writeln!(out, "- Head: `{}` ({})", self.head_commit, self.head_source);
        let _ = writeln!(out, "- Impacted requirements: {}", self.requirements.len());
        let _ = writeln!(out, "- Unclaimed changes: {}", self.unclaimed_changes.len());
        let _ = writeln!(out, "- Policy findings: {}", self.findings.len());

        if !self.findings.is_empty() {
            let _ = writeln!(out, "\n## Findings\n");
            for finding in &self.findings {
                let requirement = finding
                    .requirement
                    .as_ref()
                    .map_or(String::new(), |id| format!(" {id}"));
                let _ = writeln!(
                    out,
                    "- `{:?}` `{}`{}: {}",
                    finding.severity, finding.code, requirement, finding.message
                );
            }
        }

        if !self.requirements.is_empty() {
            let _ = writeln!(out, "\n## Impacted requirements\n");
            for requirement in &self.requirements {
                let classes = requirement
                    .impact
                    .iter()
                    .map(|impact| format!("{:?}", impact.class).to_lowercase())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = writeln!(out, "- `{}` ({classes})", requirement.id);
            }
        }

        if !self.unclaimed_changes.is_empty() {
            let _ = writeln!(out, "\n## Unclaimed runtime changes\n");
            for change in &self.unclaimed_changes {
                let _ = writeln!(
                    out,
                    "- `{}`: `{}` at {}:{}",
                    change.change_id, change.symbol, change.file, change.line
                );
            }
        }
        out
    }
}

/// One requirement selected for review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactedRequirement {
    pub id: String,
    pub area: String,
    pub impact: Vec<Impact>,
}

/// One reason a requirement is impacted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Impact {
    pub class: ImpactClass,
    pub confidence: Confidence,
    pub change_id: String,
    pub reason: String,
    pub site: ImpactSite,
}

/// Primary impact classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactClass {
    Specification,
    Direct,
    Evidence,
    Anchor,
    Structural,
    Transitive,
    FileFallback,
}

/// Confidence in a deterministic association.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Certain,
    Possible,
}

/// Workspace source location for an impact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactSite {
    pub file: String,
    pub symbol: Option<String>,
    pub line: usize,
}

/// Semantic runtime item that has no direct requirement association.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnclaimedChange {
    pub change_id: String,
    pub file: String,
    pub symbol: String,
    pub line: usize,
    pub reason: String,
}

/// Deterministic policy or analysis finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactFinding {
    pub code: String,
    pub severity: FindingSeverity,
    pub requirement: Option<String>,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub message: String,
}

/// Severity used by impact policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChangedFile {
    status: FileStatus,
    base_path: Option<PathBuf>,
    head_path: Option<PathBuf>,
}

impl ChangedFile {
    fn display_path(&self) -> &Path {
        self.head_path
            .as_deref()
            .or(self.base_path.as_deref())
            .expect("BUG: changed file has neither base nor head path")
    }
}

struct AnalysisState {
    requirements: BTreeMap<String, RequirementImpactBuilder>,
    unclaimed: Vec<UnclaimedChange>,
    findings: Vec<ImpactFinding>,
    dependency_changes: Vec<ChangedDefinition>,
    next_change: usize,
}

impl AnalysisState {
    fn new() -> Self {
        Self {
            requirements: BTreeMap::new(),
            unclaimed: Vec::new(),
            findings: Vec::new(),
            dependency_changes: Vec::new(),
            next_change: 1,
        }
    }

    fn change_id(&mut self) -> String {
        let id = format!("change-{:04}", self.next_change);
        self.next_change += 1;
        id
    }

    fn add_impact(&mut self, id: &str, area: &str, impact: Impact) {
        self.requirements
            .entry(id.to_string())
            .or_insert_with(|| RequirementImpactBuilder {
                area: area.to_string(),
                impacts: Vec::new(),
            })
            .impacts
            .push(impact);
    }
}

struct RequirementImpactBuilder {
    area: String,
    impacts: Vec<Impact>,
}

/// Analyzes the working tree against a Git base revision.
///
/// # Errors
///
/// Returns an error when the revision, Git diff, requirement documents,
/// baseline, or changed source cannot be read reliably.
pub fn analyze(
    root: &Path,
    docs: &[DocSpec],
    options: &ImpactOptions<'_>,
) -> Result<ImpactArtifact> {
    let base_commit = match options.base {
        BaseSelection::Revision(revision) => resolve_revision(root, revision)?,
        BaseSelection::MergeBaseWith(target) => merge_base(root, target)?,
    };
    let head_commit = resolve_revision(root, "HEAD")?;
    let changed_files = changed_files(root, &base_commit)?;
    let baseline = Baseline::load(root)?;
    let mut state = AnalysisState::new();

    let (base_requirements, head_requirements) = load_requirements(root, docs, &base_commit)?;
    compare_requirement_documents(
        &base_requirements,
        &head_requirements,
        &baseline,
        &mut state,
    );
    compare_baseline(root, &base_commit, &baseline, &mut state)?;
    compare_rust_files(
        root,
        &base_commit,
        &changed_files,
        &base_requirements,
        &head_requirements,
        &mut state,
    )?;
    apply_dependency_impacts(
        root,
        &base_commit,
        &base_requirements,
        &head_requirements,
        &mut state,
    )?;

    let requirements = state
        .requirements
        .into_iter()
        .map(|(id, mut builder)| {
            builder.impacts.sort_by(|a, b| {
                a.change_id
                    .cmp(&b.change_id)
                    .then(a.class.cmp(&b.class))
                    .then(a.site.file.cmp(&b.site.file))
                    .then(a.site.line.cmp(&b.site.line))
            });
            ImpactedRequirement {
                id,
                area: builder.area,
                impact: builder.impacts,
            }
        })
        .collect();
    state.unclaimed.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.symbol.cmp(&b.symbol))
    });
    state.findings.sort_by(|a, b| {
        a.code
            .cmp(&b.code)
            .then(a.requirement.cmp(&b.requirement))
            .then(a.file.cmp(&b.file))
            .then(a.line.cmp(&b.line))
    });

    Ok(ImpactArtifact {
        schema: IMPACT_SCHEMA.to_string(),
        repository: "workspace".to_string(),
        base_commit,
        head_commit,
        head_source: "working-tree".to_string(),
        working_tree_dirty: working_tree_dirty(root)?,
        configuration: ImpactConfiguration {
            features: Vec::new(),
            targets: vec!["workspace".to_string()],
            dependency_depth: 1,
            scope_precision: "rust-item+anchor-block".to_string(),
        },
        requirements,
        unclaimed_changes: state.unclaimed,
        findings: state.findings,
    })
}

fn git_output(root: &Path, args: &[&str]) -> Result<std::process::Output> {
    ProcessCommand::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))
}

fn resolve_revision(root: &Path, revision: &str) -> Result<String> {
    let expression = format!("{revision}^{{commit}}");
    let output = git_output(root, &["rev-parse", "--verify", &expression])?;
    if !output.status.success() {
        bail!(
            "cannot resolve Git revision {revision:?}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout)
        .context("Git revision is not UTF-8")
        .map(|value| value.trim().to_string())
}

fn merge_base(root: &Path, target: &str) -> Result<String> {
    resolve_revision(root, target)?;
    let output = git_output(root, &["merge-base", "HEAD", target])?;
    if !output.status.success() {
        bail!(
            "cannot compute merge base between HEAD and {target:?}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout)
        .context("Git merge base is not UTF-8")
        .map(|value| value.trim().to_string())
}

fn working_tree_dirty(root: &Path) -> Result<bool> {
    let output = git_output(root, &["status", "--porcelain=v1"])?;
    if !output.status.success() {
        bail!(
            "cannot inspect working-tree status: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(!output.stdout.is_empty())
}

fn changed_files(root: &Path, base: &str) -> Result<Vec<ChangedFile>> {
    let output = git_output(
        root,
        &["diff", "--name-status", "-z", "--find-renames", base, "--"],
    )?;
    if !output.status.success() {
        bail!(
            "cannot read Git diff from {base}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let mut changed = parse_name_status(&output.stdout)?;
    let tracked_head_paths: HashSet<PathBuf> = changed
        .iter()
        .filter_map(|file| file.head_path.clone())
        .collect();
    let untracked = git_output(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    if !untracked.status.success() {
        bail!(
            "cannot list untracked files: {}",
            String::from_utf8_lossy(&untracked.stderr).trim()
        );
    }
    for raw in nul_fields(&untracked.stdout) {
        let path = path_from_git(raw)?;
        if !tracked_head_paths.contains(&path) {
            changed.push(ChangedFile {
                status: FileStatus::Added,
                base_path: None,
                head_path: Some(path),
            });
        }
    }
    changed.sort_by(|a, b| a.display_path().cmp(b.display_path()));
    Ok(changed)
}

fn parse_name_status(raw: &[u8]) -> Result<Vec<ChangedFile>> {
    let fields = nul_fields(raw);
    let mut changed = Vec::new();
    let mut index = 0usize;
    while index < fields.len() {
        let status = std::str::from_utf8(fields[index]).context("Git status is not UTF-8")?;
        index += 1;
        let kind = status
            .as_bytes()
            .first()
            .copied()
            .context("empty Git name-status field")?;
        let file = match kind {
            b'A' => ChangedFile {
                status: FileStatus::Added,
                base_path: None,
                head_path: Some(next_git_path(&fields, &mut index)?),
            },
            b'D' => ChangedFile {
                status: FileStatus::Deleted,
                base_path: Some(next_git_path(&fields, &mut index)?),
                head_path: None,
            },
            b'M' | b'T' => {
                let path = next_git_path(&fields, &mut index)?;
                ChangedFile {
                    status: FileStatus::Modified,
                    base_path: Some(path.clone()),
                    head_path: Some(path),
                }
            }
            b'R' | b'C' => ChangedFile {
                status: FileStatus::Renamed,
                base_path: Some(next_git_path(&fields, &mut index)?),
                head_path: Some(next_git_path(&fields, &mut index)?),
            },
            other => bail!("unsupported Git name-status code {}", char::from(other)),
        };
        changed.push(file);
    }
    Ok(changed)
}

fn nul_fields(raw: &[u8]) -> Vec<&[u8]> {
    raw.split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .collect()
}

fn next_git_path(fields: &[&[u8]], index: &mut usize) -> Result<PathBuf> {
    let raw = fields
        .get(*index)
        .copied()
        .context("truncated Git name-status output")?;
    *index += 1;
    path_from_git(raw)
}

fn path_from_git(raw: &[u8]) -> Result<PathBuf> {
    let path = std::str::from_utf8(raw).context("Git path is not UTF-8")?;
    if Path::new(path).is_absolute() || path.split('/').any(|part| part == "..") {
        bail!("unsafe workspace-relative Git path {path:?}");
    }
    Ok(PathBuf::from(path))
}

pub(crate) fn git_file(root: &Path, revision: &str, path: &Path) -> Result<Option<String>> {
    let path = path
        .to_str()
        .with_context(|| format!("non-UTF-8 workspace path {}", path.display()))?;
    let object = format!("{revision}:{path}");
    let output = git_output(root, &["show", &object])?;
    if !output.status.success() {
        return Ok(None);
    }
    String::from_utf8(output.stdout)
        .context("Git blob is not UTF-8")
        .map(Some)
}

fn working_tree_file(root: &Path, path: &Path) -> Result<Option<String>> {
    let full = root.join(path);
    match std::fs::read_to_string(&full) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("reading {}", full.display())),
    }
}

fn load_requirements(
    root: &Path,
    docs: &[DocSpec],
    base: &str,
) -> Result<(BTreeMap<String, Requirement>, BTreeMap<String, Requirement>)> {
    let mut base_requirements = BTreeMap::new();
    let mut head_requirements = BTreeMap::new();
    for spec in docs {
        if let Some(text) = git_file(root, base, Path::new(&spec.path))? {
            for requirement in parse_text(&text, spec).requirements {
                base_requirements.insert(requirement.id.clone(), requirement);
            }
        }
        let text = working_tree_file(root, Path::new(&spec.path))?
            .with_context(|| format!("head requirement document {} is missing", spec.path))?;
        for requirement in parse_text(&text, spec).requirements {
            head_requirements.insert(requirement.id.clone(), requirement);
        }
    }
    Ok((base_requirements, head_requirements))
}

fn compare_requirement_documents(
    base: &BTreeMap<String, Requirement>,
    head: &BTreeMap<String, Requirement>,
    baseline: &Baseline,
    state: &mut AnalysisState,
) {
    let baseline_by_requirement = baseline.gaps.iter().fold(
        BTreeMap::<&str, Vec<GapKey>>::new(),
        |mut grouped, entry| {
            grouped
                .entry(entry.requirement.as_str())
                .or_default()
                .push(entry.key());
            grouped
        },
    );
    let ids = base
        .keys()
        .chain(head.keys())
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    for id in ids {
        let before = base.get(id);
        let after = head.get(id);
        let reasons = requirement_change_reasons(before, after);
        if reasons.is_empty() {
            continue;
        }
        let current = after
            .or(before)
            .expect("BUG: requirement ID without revision");
        let change_id = state.change_id();
        state.add_impact(
            id,
            &current.area,
            Impact {
                class: ImpactClass::Specification,
                confidence: Confidence::Certain,
                change_id,
                reason: reasons.join(", "),
                site: ImpactSite {
                    file: current.doc.clone(),
                    symbol: None,
                    line: current.line,
                },
            },
        );

        if let Some(gaps) = baseline_by_requirement.get(id) {
            let kinds = gaps
                .iter()
                .map(|key| key.kind.to_string())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ");
            state.findings.push(ImpactFinding {
                code: "changed-baselined-requirement".to_string(),
                severity: FindingSeverity::Error,
                requirement: Some(id.to_string()),
                file: Some(current.doc.clone()),
                line: Some(current.line),
                message: format!(
                    "requirement changed while it still has inherited baseline debt: {kinds}"
                ),
            });
        }
    }
}

fn requirement_change_reasons(
    before: Option<&Requirement>,
    after: Option<&Requirement>,
) -> Vec<&'static str> {
    let (Some(before), Some(after)) = (before, after) else {
        return vec![if before.is_none() {
            "requirement added"
        } else {
            "requirement removed"
        }];
    };
    let mut reasons = Vec::new();
    if before.statement != after.statement {
        reasons.push("normative statement changed");
    }
    if before.enforced_text != after.enforced_text {
        reasons.push("enforcement references changed");
    }
    if before.verified_text != after.verified_text {
        reasons.push("verification evidence changed");
    }
    if before.retired != after.retired {
        reasons.push("retirement state changed");
    }
    reasons
}

fn compare_baseline(
    root: &Path,
    base: &str,
    head: &Baseline,
    state: &mut AnalysisState,
) -> Result<()> {
    let Some(text) = git_file(root, base, Path::new(BASELINE_PATH))? else {
        if !head.gaps.is_empty() {
            state.findings.push(ImpactFinding {
                code: "baseline-initialized".to_string(),
                severity: FindingSeverity::Warning,
                requirement: None,
                file: Some(BASELINE_PATH.to_string()),
                line: Some(1),
                message: format!(
                    "initial baseline introduces {} historical gap exceptions; review the \
                     complete debt inventory",
                    head.gaps.len()
                ),
            });
        }
        return Ok(());
    };
    let before = Baseline::parse(&text, &format!("{base}:{BASELINE_PATH}"))?;
    let old: BTreeSet<GapKey> = before.gaps.iter().map(|entry| entry.key()).collect();
    for added in head
        .gaps
        .iter()
        .map(|entry| entry.key())
        .filter(|key| !old.contains(key))
    {
        state.findings.push(ImpactFinding {
            code: "baseline-entry-added".to_string(),
            severity: FindingSeverity::Error,
            requirement: Some(added.requirement.clone()),
            file: Some(BASELINE_PATH.to_string()),
            line: Some(1),
            message: format!(
                "MR adds a {} exception; baseline maintenance is removal-only",
                added.kind
            ),
        });
    }
    Ok(())
}

#[derive(Debug)]
struct SourceScope {
    symbol: String,
    kind: &'static str,
    line: usize,
    end_line: usize,
    tokens: String,
    behavior_tokens: String,
    definition: Option<Definition>,
    enforcement: Vec<EnforcementScope>,
    verification: BTreeSet<String>,
    is_test: bool,
}

#[derive(Debug)]
struct EnforcementScope {
    id: String,
    start_line: usize,
    end_line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LineRange {
    start: usize,
    end: usize,
}

impl LineRange {
    fn from_start_count(start: usize, count: usize) -> Option<Self> {
        (count > 0).then_some(Self {
            start,
            end: start + count.saturating_sub(1),
        })
    }

    fn intersects(self, start: usize, end: usize) -> bool {
        self.start <= end && start <= self.end
    }
}

#[derive(Debug, Default)]
struct ChangedLines {
    base: Vec<LineRange>,
    head: Vec<LineRange>,
}

fn compare_rust_files(
    root: &Path,
    base: &str,
    changed_files: &[ChangedFile],
    base_requirements: &BTreeMap<String, Requirement>,
    head_requirements: &BTreeMap<String, Requirement>,
    state: &mut AnalysisState,
) -> Result<()> {
    for file in changed_files
        .iter()
        .filter(|file| relevant_rust_path(file.display_path()))
    {
        let base_text = match &file.base_path {
            Some(path) => git_file(root, base, path)?,
            None => None,
        };
        let head_text = match &file.head_path {
            Some(path) => working_tree_file(root, path)?,
            None => None,
        };
        let changed_lines =
            changed_line_ranges(root, base, file, base_text.as_deref(), head_text.as_deref())?;
        let base_index = base_text
            .as_deref()
            .map(|text| index_source(text, file.base_path.as_deref().expect("base path exists")));
        let head_index = head_text
            .as_deref()
            .map(|text| index_source(text, file.head_path.as_deref().expect("head path exists")));

        let parse_error = base_index
            .as_ref()
            .and_then(|result| result.as_ref().err())
            .map(ToString::to_string)
            .or_else(|| {
                head_index
                    .as_ref()
                    .and_then(|result| result.as_ref().err())
                    .map(ToString::to_string)
            });
        if let Some(error) = parse_error {
            file_fallback(
                file,
                base_text.as_deref(),
                head_text.as_deref(),
                &error,
                base_requirements,
                head_requirements,
                state,
            );
            continue;
        }

        let empty = BTreeMap::new();
        let before = base_index
            .as_ref()
            .map(|result| result.as_ref().expect("parse error handled above"))
            .unwrap_or(&empty);
        let after = head_index
            .as_ref()
            .map(|result| result.as_ref().expect("parse error handled above"))
            .unwrap_or(&empty);
        compare_scopes(
            file,
            before,
            after,
            &changed_lines,
            base_requirements,
            head_requirements,
            state,
        );
    }
    Ok(())
}

fn apply_dependency_impacts(
    root: &Path,
    base_commit: &str,
    base_requirements: &BTreeMap<String, Requirement>,
    head_requirements: &BTreeMap<String, Requirement>,
    state: &mut AnalysisState,
) -> Result<()> {
    let analysis = impact_dependency::analyze(root, base_commit, &state.dependency_changes)?;
    for impact in analysis.impacts {
        let area = requirement_area(&impact.requirement, base_requirements, head_requirements);
        let class = match impact.class {
            DependencyClass::Structural => ImpactClass::Structural,
            DependencyClass::Transitive => ImpactClass::Transitive,
        };
        state.add_impact(
            &impact.requirement,
            area,
            Impact {
                class,
                confidence: Confidence::Possible,
                change_id: impact.change_id,
                reason: impact.reason,
                site: ImpactSite {
                    file: impact.file,
                    symbol: Some(impact.symbol),
                    line: impact.line,
                },
            },
        );
    }
    state
        .unclaimed
        .retain(|change| !analysis.claimed_changes.contains(&change.change_id));
    state
        .findings
        .extend(analysis.warnings.into_iter().map(|warning| ImpactFinding {
            code: "dependency-index-parse".to_string(),
            severity: FindingSeverity::Warning,
            requirement: None,
            file: Some(warning.file),
            line: Some(1),
            message: warning.message,
        }));
    Ok(())
}

fn relevant_rust_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    path.extension().is_some_and(|extension| extension == "rs")
        && (text.starts_with("example-app/src/")
            || text.starts_with("example-app/tests/")
            || text.starts_with("example-core/src/")
            || text.starts_with("example-core/tests/"))
}

fn changed_line_ranges(
    root: &Path,
    base: &str,
    file: &ChangedFile,
    base_text: Option<&str>,
    head_text: Option<&str>,
) -> Result<ChangedLines> {
    if base_text.is_none() {
        return Ok(ChangedLines {
            base: Vec::new(),
            head: full_file_range(head_text),
        });
    }
    if head_text.is_none() {
        return Ok(ChangedLines {
            base: full_file_range(base_text),
            head: Vec::new(),
        });
    }

    let mut args = vec!["diff", "--unified=0", "--no-color", base, "--"];
    let mut paths = Vec::new();
    if let Some(path) = &file.base_path {
        paths.push(
            path.to_str()
                .with_context(|| format!("non-UTF-8 changed path {}", path.display()))?,
        );
    }
    if let Some(path) = &file.head_path
        && file.base_path.as_ref() != Some(path)
    {
        paths.push(
            path.to_str()
                .with_context(|| format!("non-UTF-8 changed path {}", path.display()))?,
        );
    }
    args.extend(paths);
    let output = git_output(root, &args)?;
    if !output.status.success() {
        bail!(
            "cannot read zero-context diff for {}: {}",
            file.display_path().display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8(output.stdout).context("Git diff is not UTF-8")?;
    let mut changed = parse_hunk_ranges(&text)?;
    if changed.base.is_empty() && changed.head.is_empty() && file.status != FileStatus::Renamed {
        changed.base = full_file_range(base_text);
        changed.head = full_file_range(head_text);
    }
    Ok(changed)
}

fn full_file_range(text: Option<&str>) -> Vec<LineRange> {
    text.and_then(|text| LineRange::from_start_count(1, text.lines().count()))
        .into_iter()
        .collect()
}

fn parse_hunk_ranges(diff: &str) -> Result<ChangedLines> {
    let hunk_re = Regex::new(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@")
        .expect("BUG: invalid Git hunk regex");
    let mut changed = ChangedLines::default();
    for line in diff.lines().filter(|line| line.starts_with("@@ ")) {
        let captures = hunk_re
            .captures(line)
            .with_context(|| format!("unsupported Git hunk header {line:?}"))?;
        let number = |index: usize, default: usize| -> Result<usize> {
            captures.get(index).map_or(Ok(default), |value| {
                value
                    .as_str()
                    .parse()
                    .with_context(|| format!("invalid line number in Git hunk {line:?}"))
            })
        };
        let base_start = number(1, 0)?;
        let base_count = number(2, 1)?;
        let head_start = number(3, 0)?;
        let head_count = number(4, 1)?;
        changed
            .base
            .extend(LineRange::from_start_count(base_start, base_count));
        changed
            .head
            .extend(LineRange::from_start_count(head_start, head_count));
    }
    Ok(changed)
}

fn compare_scopes(
    file: &ChangedFile,
    before: &BTreeMap<String, SourceScope>,
    after: &BTreeMap<String, SourceScope>,
    changed_lines: &ChangedLines,
    base_requirements: &BTreeMap<String, Requirement>,
    head_requirements: &BTreeMap<String, Requirement>,
    state: &mut AnalysisState,
) {
    let identities = before
        .keys()
        .chain(after.keys())
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for identity in identities {
        let old = before.get(identity);
        let new = after.get(identity);
        let syntax_unchanged = old
            .zip(new)
            .is_some_and(|(old, new)| old.tokens == new.tokens);
        let behavior_unchanged = old
            .zip(new)
            .is_some_and(|(old, new)| old.behavior_tokens == new.behavior_tokens);
        let path_moved = file.status == FileStatus::Renamed && file.base_path != file.head_path;
        if syntax_unchanged && !path_moved {
            continue;
        }
        let scope = new.or(old).expect("BUG: scope absent in both revisions");
        let change_id = state.change_id();
        let enforcement = affected_enforcement(old, new, changed_lines, path_moved);
        let verification = affected_verification(old, new, changed_lines, path_moved);
        let mut requirements = enforcement.clone();
        requirements.extend(verification.iter().cloned());
        let runtime_change = report_as_unclaimed(file, old, new);

        if runtime_change && !behavior_unchanged {
            let definitions = old
                .into_iter()
                .chain(new)
                .filter_map(|scope| scope.definition.clone())
                .collect::<BTreeSet<_>>();
            if !definitions.is_empty() {
                state.dependency_changes.push(ChangedDefinition {
                    change_id: change_id.clone(),
                    file: impact_path(file, new.is_some()),
                    symbol: scope.symbol.clone(),
                    line: scope.line,
                    definitions,
                    associated_requirements: requirements.clone(),
                });
            }
        }

        if requirements.is_empty() {
            if !syntax_unchanged && runtime_change {
                state.unclaimed.push(UnclaimedChange {
                    change_id,
                    file: impact_path(file, new.is_some()),
                    symbol: scope.symbol.clone(),
                    line: scope.line,
                    reason: format!("changed {} has no direct requirement anchor", scope.kind),
                });
            }
            continue;
        }

        for requirement in requirements {
            let class = if path_moved || anchor_membership_changed(&requirement, old, new) {
                ImpactClass::Anchor
            } else if verification.contains(&requirement) {
                ImpactClass::Evidence
            } else {
                ImpactClass::Direct
            };
            let area = requirement_area(&requirement, base_requirements, head_requirements);
            state.add_impact(
                &requirement,
                area,
                Impact {
                    class,
                    confidence: Confidence::Certain,
                    change_id: change_id.clone(),
                    reason: impact_reason(class, file.status).to_string(),
                    site: ImpactSite {
                        file: impact_path(file, new.is_some()),
                        symbol: Some(scope.symbol.clone()),
                        line: scope.line,
                    },
                },
            );
        }
    }
}

fn affected_enforcement(
    before: Option<&SourceScope>,
    after: Option<&SourceScope>,
    changed: &ChangedLines,
    path_moved: bool,
) -> BTreeSet<String> {
    before
        .into_iter()
        .flat_map(|scope| {
            scope.enforcement.iter().filter(|site| {
                path_moved
                    || changed
                        .base
                        .iter()
                        .any(|range| range.intersects(site.start_line, site.end_line))
            })
        })
        .chain(after.into_iter().flat_map(|scope| {
            scope.enforcement.iter().filter(|site| {
                path_moved
                    || changed
                        .head
                        .iter()
                        .any(|range| range.intersects(site.start_line, site.end_line))
            })
        }))
        .map(|site| site.id.clone())
        .collect()
}

fn affected_verification(
    before: Option<&SourceScope>,
    after: Option<&SourceScope>,
    changed: &ChangedLines,
    path_moved: bool,
) -> BTreeSet<String> {
    before
        .into_iter()
        .filter(|scope| {
            path_moved
                || changed
                    .base
                    .iter()
                    .any(|range| range.intersects(scope.line, scope.end_line))
        })
        .chain(after.into_iter().filter(|scope| {
            path_moved
                || changed
                    .head
                    .iter()
                    .any(|range| range.intersects(scope.line, scope.end_line))
        }))
        .flat_map(|scope| scope.verification.iter().cloned())
        .collect()
}

fn anchor_membership_changed(
    requirement: &str,
    before: Option<&SourceScope>,
    after: Option<&SourceScope>,
) -> bool {
    let membership = |scope: Option<&SourceScope>| {
        scope.is_some_and(|scope| {
            scope.enforcement.iter().any(|site| site.id == requirement)
                || scope.verification.contains(requirement)
        })
    };
    membership(before) != membership(after)
        || before.zip(after).is_some_and(|(before, after)| {
            before.enforcement.iter().any(|site| site.id == requirement)
                != after.enforcement.iter().any(|site| site.id == requirement)
                || before.verification.contains(requirement)
                    != after.verification.contains(requirement)
        })
}

fn impact_reason(class: ImpactClass, status: FileStatus) -> &'static str {
    match class {
        ImpactClass::Specification => "requirement specification changed",
        ImpactClass::Direct => "changed Rust item carries an enforcement anchor",
        ImpactClass::Evidence => "changed Rust test carries a verification anchor",
        ImpactClass::Anchor => match status {
            FileStatus::Added => "anchored Rust item was added",
            FileStatus::Deleted => "anchored Rust item was deleted",
            FileStatus::Renamed => "anchor or anchored item moved or changed",
            FileStatus::Modified => "anchor membership or anchored item changed",
        },
        ImpactClass::Structural => "changed type or value is referenced by an enforcement scope",
        ImpactClass::Transitive => "changed Rust item is referenced by an enforcement scope",
        ImpactClass::FileFallback => "file-level association after Rust parse failure",
    }
}

fn report_as_unclaimed(
    file: &ChangedFile,
    before: Option<&SourceScope>,
    after: Option<&SourceScope>,
) -> bool {
    let test_path = test_fixture_path(file.display_path());
    let test_scope = before.into_iter().chain(after).any(|scope| {
        scope.is_test || scope.symbol.contains("::tests::") || scope.symbol.contains("::test::")
    });
    let behavior_bearing = before.into_iter().chain(after).any(|scope| {
        !matches!(
            scope.kind,
            "use declaration" | "module declaration" | "item"
        )
    });
    !test_path && !test_scope && behavior_bearing
}

fn test_fixture_path(path: &Path) -> bool {
    if path
        .components()
        .any(|component| component.as_os_str() == "tests")
    {
        return true;
    }
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };
    stem == "test"
        || stem == "tests"
        || stem.starts_with("test_")
        || stem.ends_with("_test")
        || stem.ends_with("_tests")
}

fn impact_path(file: &ChangedFile, use_head: bool) -> String {
    let path = if use_head {
        file.head_path.as_deref().or(file.base_path.as_deref())
    } else {
        file.base_path.as_deref().or(file.head_path.as_deref())
    }
    .expect("BUG: changed file without path");
    path.to_string_lossy().into_owned()
}

fn requirement_area<'a>(
    id: &str,
    base: &'a BTreeMap<String, Requirement>,
    head: &'a BTreeMap<String, Requirement>,
) -> &'a str {
    head.get(id)
        .or_else(|| base.get(id))
        .map_or("UNKNOWN", |requirement| requirement.area.as_str())
}

fn file_fallback(
    file: &ChangedFile,
    base_text: Option<&str>,
    head_text: Option<&str>,
    error: &str,
    base_requirements: &BTreeMap<String, Requirement>,
    head_requirements: &BTreeMap<String, Requirement>,
    state: &mut AnalysisState,
) {
    let id_re = requirement_id_regex();
    let ids = base_text
        .into_iter()
        .chain(head_text)
        .flat_map(|text| {
            id_re
                .find_iter(text)
                .map(|found| found.as_str().to_string())
        })
        .collect::<BTreeSet<_>>();
    let change_id = state.change_id();
    let path = impact_path(file, head_text.is_some());
    for id in ids {
        let area = requirement_area(&id, base_requirements, head_requirements);
        state.add_impact(
            &id,
            area,
            Impact {
                class: ImpactClass::FileFallback,
                confidence: Confidence::Possible,
                change_id: change_id.clone(),
                reason: impact_reason(ImpactClass::FileFallback, file.status).to_string(),
                site: ImpactSite {
                    file: path.clone(),
                    symbol: None,
                    line: 1,
                },
            },
        );
    }
    state.findings.push(ImpactFinding {
        code: "rust-parse-fallback".to_string(),
        severity: FindingSeverity::Warning,
        requirement: None,
        file: Some(path),
        line: Some(1),
        message: format!("Rust parse failed; used conservative file-level impact: {error}"),
    });
}

fn index_source(text: &str, path: &Path) -> Result<BTreeMap<String, SourceScope>, syn::Error> {
    let syntax = syn::parse_file(text)?;
    let id_re = requirement_id_regex();
    let root = module_root(path);
    let mut scopes = BTreeMap::new();
    let mut ordinal = 0usize;
    collect_items(&syntax.items, "", &root, &id_re, &mut ordinal, &mut scopes);
    Ok(scopes)
}

/// Extracts the complete enclosing Rust item for a locatable impact or
/// evidence site. Symbols are preferred; line containment is a
/// conservative fallback for moved items.
pub(crate) fn source_excerpt(
    text: &str,
    path: &Path,
    symbol: Option<&str>,
    line: usize,
) -> Result<Option<(usize, usize, String)>> {
    let scopes = index_source(text, path).context("parsing source for review context")?;
    let scope = symbol
        .and_then(|symbol| scopes.values().find(|scope| scope.symbol == symbol))
        .or_else(|| {
            scopes
                .values()
                .filter(|scope| scope.line <= line && line <= scope.end_line)
                .min_by_key(|scope| scope.end_line.saturating_sub(scope.line))
        });
    let Some(scope) = scope else {
        return Ok(None);
    };
    let lines = text.lines().collect::<Vec<_>>();
    let start = scope.line.saturating_sub(1).min(lines.len());
    let end = scope.end_line.min(lines.len());
    Ok(Some((
        scope.line,
        scope.end_line,
        lines[start..end].join("\n"),
    )))
}

/// Extracts a named Rust function or method from source content.
pub(crate) fn named_function_excerpt(
    text: &str,
    path: &Path,
    function: &str,
) -> Result<Option<(usize, usize, String)>> {
    let scopes = index_source(text, path).context("parsing test source for review context")?;
    let suffix = format!("::fn:{function}");
    let scope = scopes
        .values()
        .find(|scope| scope.symbol.ends_with(&suffix));
    let Some(scope) = scope else {
        return Ok(None);
    };
    let lines = text.lines().collect::<Vec<_>>();
    let start = scope.line.saturating_sub(1).min(lines.len());
    let end = scope.end_line.min(lines.len());
    Ok(Some((
        scope.line,
        scope.end_line,
        lines[start..end].join("\n"),
    )))
}

fn collect_items(
    items: &[syn::Item],
    local_module: &str,
    symbol_root: &str,
    id_re: &Regex,
    ordinal: &mut usize,
    scopes: &mut BTreeMap<String, SourceScope>,
) {
    for item in items {
        match item {
            syn::Item::Mod(module) if module.content.is_some() => {
                let nested = join_symbol(local_module, &module.ident.to_string());
                let (_, items) = module.content.as_ref().expect("module content checked");
                collect_items(items, &nested, symbol_root, id_re, ordinal, scopes);
            }
            syn::Item::Impl(item_impl) => {
                collect_impl_items(item_impl, local_module, symbol_root, id_re, ordinal, scopes);
            }
            syn::Item::Trait(item_trait) => {
                collect_trait_items(
                    item_trait,
                    local_module,
                    symbol_root,
                    id_re,
                    ordinal,
                    scopes,
                );
            }
            item => {
                *ordinal += 1;
                let identity = join_symbol(local_module, &item_identity(item, *ordinal));
                let module = join_symbol(symbol_root, local_module);
                let scope = scope_from_item(item, identity.clone(), symbol_root, &module, id_re);
                scopes.insert(identity, scope);
            }
        }
    }
}

fn collect_impl_items(
    item_impl: &syn::ItemImpl,
    local_module: &str,
    symbol_root: &str,
    id_re: &Regex,
    ordinal: &mut usize,
    scopes: &mut BTreeMap<String, SourceScope>,
) {
    let self_type = item_impl.self_ty.to_token_stream().to_string();
    let trait_name = item_impl
        .trait_
        .as_ref()
        .map(|(_, path, _)| path.to_token_stream().to_string());
    let owner = trait_name.map_or_else(
        || format!("impl:{self_type}"),
        |trait_name| format!("impl:{trait_name} for {self_type}"),
    );
    for item in &item_impl.items {
        *ordinal += 1;
        let member = impl_item_identity(item, *ordinal);
        let identity = join_symbol(local_module, &format!("{owner}::{member}"));
        let module = join_symbol(symbol_root, local_module);
        let scope = scope_from_impl_item(
            item,
            identity.clone(),
            symbol_root,
            &module,
            &item_impl.self_ty,
            id_re,
        );
        scopes.insert(identity, scope);
    }
}

fn collect_trait_items(
    item_trait: &syn::ItemTrait,
    local_module: &str,
    symbol_root: &str,
    id_re: &Regex,
    ordinal: &mut usize,
    scopes: &mut BTreeMap<String, SourceScope>,
) {
    let owner = format!("trait:{}", item_trait.ident);
    for item in &item_trait.items {
        *ordinal += 1;
        let member = trait_item_identity(item, *ordinal);
        let identity = join_symbol(local_module, &format!("{owner}::{member}"));
        let module = join_symbol(symbol_root, local_module);
        let scope = scope_from_trait_item(
            item,
            identity.clone(),
            symbol_root,
            &module,
            &item_trait.ident.to_string(),
            id_re,
        );
        scopes.insert(identity, scope);
    }
}

fn item_identity(item: &syn::Item, ordinal: usize) -> String {
    match item {
        syn::Item::Const(item) => format!("const:{}", item.ident),
        syn::Item::Enum(item) => format!("enum:{}", item.ident),
        syn::Item::ExternCrate(item) => format!("extern-crate:{}", item.ident),
        syn::Item::Fn(item) => format!("fn:{}", item.sig.ident),
        syn::Item::ForeignMod(_) => format!("foreign-mod:{ordinal}"),
        syn::Item::Macro(item) => item.ident.as_ref().map_or_else(
            || {
                format!(
                    "macro:{}:{}",
                    item.mac.path.to_token_stream(),
                    item.mac.tokens
                )
            },
            |ident| format!("macro:{ident}"),
        ),
        syn::Item::Mod(item) => format!("mod:{}", item.ident),
        syn::Item::Static(item) => format!("static:{}", item.ident),
        syn::Item::Struct(item) => format!("struct:{}", item.ident),
        syn::Item::TraitAlias(item) => format!("trait-alias:{}", item.ident),
        syn::Item::Type(item) => format!("type:{}", item.ident),
        syn::Item::Union(item) => format!("union:{}", item.ident),
        syn::Item::Use(item) => format!("use:{}", item.tree.to_token_stream()),
        syn::Item::Verbatim(_) => format!("verbatim:{ordinal}"),
        syn::Item::Impl(_) | syn::Item::Trait(_) => {
            format!("container:{ordinal}")
        }
        _ => format!("item:{ordinal}"),
    }
}

fn impl_item_identity(item: &syn::ImplItem, ordinal: usize) -> String {
    match item {
        syn::ImplItem::Const(item) => format!("const:{}", item.ident),
        syn::ImplItem::Fn(item) => format!("fn:{}", item.sig.ident),
        syn::ImplItem::Type(item) => format!("type:{}", item.ident),
        syn::ImplItem::Macro(item) => {
            format!("macro:{}:{ordinal}", item.mac.path.to_token_stream())
        }
        syn::ImplItem::Verbatim(_) => format!("verbatim:{ordinal}"),
        _ => format!("member:{ordinal}"),
    }
}

fn trait_item_identity(item: &syn::TraitItem, ordinal: usize) -> String {
    match item {
        syn::TraitItem::Const(item) => format!("const:{}", item.ident),
        syn::TraitItem::Fn(item) => format!("fn:{}", item.sig.ident),
        syn::TraitItem::Type(item) => format!("type:{}", item.ident),
        syn::TraitItem::Macro(item) => {
            format!("macro:{}:{ordinal}", item.mac.path.to_token_stream())
        }
        syn::TraitItem::Verbatim(_) => format!("verbatim:{ordinal}"),
        _ => format!("member:{ordinal}"),
    }
}

fn scope_from_item(
    item: &syn::Item,
    identity: String,
    symbol_root: &str,
    module: &str,
    id_re: &Regex,
) -> SourceScope {
    let mut collector = EnforcementCollector::new(id_re, item.span());
    collector.visit_item(item);
    let tokens = normalized_item_tokens(item);
    let (verification, is_test) = match item {
        syn::Item::Fn(function) => verification_ids(&function.attrs, id_re),
        _ => (BTreeSet::new(), false),
    };
    SourceScope {
        symbol: join_symbol(symbol_root, &identity),
        kind: item_kind(item),
        line: item.span().start().line,
        end_line: item.span().end().line,
        behavior_tokens: normalized_behavior_tokens(&tokens),
        tokens,
        definition: definition_for_item(item, module),
        enforcement: collector.sites,
        verification,
        is_test,
    }
}

fn scope_from_impl_item(
    item: &syn::ImplItem,
    identity: String,
    symbol_root: &str,
    module: &str,
    self_ty: &syn::Type,
    id_re: &Regex,
) -> SourceScope {
    let mut collector = EnforcementCollector::new(id_re, item.span());
    collector.visit_impl_item(item);
    let tokens = normalized_impl_item_tokens(item);
    let (verification, is_test) = match item {
        syn::ImplItem::Fn(function) => verification_ids(&function.attrs, id_re),
        _ => (BTreeSet::new(), false),
    };
    SourceScope {
        symbol: join_symbol(symbol_root, &identity),
        kind: impl_item_kind(item),
        line: item.span().start().line,
        end_line: item.span().end().line,
        behavior_tokens: normalized_behavior_tokens(&tokens),
        tokens,
        definition: definition_for_impl_item(item, module, self_ty),
        enforcement: collector.sites,
        verification,
        is_test,
    }
}

fn scope_from_trait_item(
    item: &syn::TraitItem,
    identity: String,
    symbol_root: &str,
    module: &str,
    trait_name: &str,
    id_re: &Regex,
) -> SourceScope {
    let mut collector = EnforcementCollector::new(id_re, item.span());
    collector.visit_trait_item(item);
    let tokens = normalized_trait_item_tokens(item);
    let (verification, is_test) = match item {
        syn::TraitItem::Fn(function) => verification_ids(&function.attrs, id_re),
        _ => (BTreeSet::new(), false),
    };
    SourceScope {
        symbol: join_symbol(symbol_root, &identity),
        kind: trait_item_kind(item),
        line: item.span().start().line,
        end_line: item.span().end().line,
        behavior_tokens: normalized_behavior_tokens(&tokens),
        tokens,
        definition: definition_for_trait_item(item, module, trait_name),
        enforcement: collector.sites,
        verification,
        is_test,
    }
}

fn item_kind(item: &syn::Item) -> &'static str {
    match item {
        syn::Item::Const(_) => "constant",
        syn::Item::Enum(_) => "enum",
        syn::Item::Fn(_) => "function",
        syn::Item::Macro(_) => "macro item",
        syn::Item::Mod(_) => "module declaration",
        syn::Item::Static(_) => "static",
        syn::Item::Struct(_) => "struct",
        syn::Item::TraitAlias(_) => "trait alias",
        syn::Item::Type(_) => "type alias",
        syn::Item::Union(_) => "union",
        syn::Item::Use(_) => "use declaration",
        syn::Item::ExternCrate(_)
        | syn::Item::ForeignMod(_)
        | syn::Item::Verbatim(_)
        | syn::Item::Impl(_)
        | syn::Item::Trait(_) => "item",
        _ => "item",
    }
}

fn impl_item_kind(item: &syn::ImplItem) -> &'static str {
    match item {
        syn::ImplItem::Const(_) => "associated constant",
        syn::ImplItem::Fn(_) => "method",
        syn::ImplItem::Type(_) => "associated type",
        syn::ImplItem::Macro(_) => "impl macro",
        syn::ImplItem::Verbatim(_) => "impl item",
        _ => "impl item",
    }
}

fn trait_item_kind(item: &syn::TraitItem) -> &'static str {
    match item {
        syn::TraitItem::Const(_) => "trait constant",
        syn::TraitItem::Fn(_) => "trait method",
        syn::TraitItem::Type(_) => "trait type",
        syn::TraitItem::Macro(_) => "trait macro",
        syn::TraitItem::Verbatim(_) => "trait item",
        _ => "trait item",
    }
}

struct EnforcementCollector<'a> {
    id_re: &'a Regex,
    scopes: Vec<(usize, usize)>,
    sites: Vec<EnforcementScope>,
}

impl<'a> EnforcementCollector<'a> {
    fn new(id_re: &'a Regex, span: proc_macro2::Span) -> Self {
        Self {
            id_re,
            scopes: vec![(span.start().line, span.end().line)],
            sites: Vec::new(),
        }
    }

    fn collect(&mut self, tokens: impl ToTokens) {
        let (start_line, end_line) = self
            .scopes
            .last()
            .copied()
            .expect("BUG: enforcement collector without owning scope");
        self.sites.extend(
            self.id_re
                .find_iter(&tokens.to_token_stream().to_string())
                .map(|found| EnforcementScope {
                    id: found.as_str().to_string(),
                    start_line,
                    end_line,
                }),
        );
    }

    fn within(&mut self, span: proc_macro2::Span, visit: impl FnOnce(&mut Self)) {
        self.scopes.push((span.start().line, span.end().line));
        visit(self);
        self.scopes.pop().expect("BUG: pushed scope disappeared");
    }
}

impl<'ast> Visit<'ast> for EnforcementCollector<'_> {
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if path_ends_with(attribute.path(), "enforces") {
            self.collect(&attribute.meta);
        }
        syn::visit::visit_attribute(self, attribute);
    }

    fn visit_macro(&mut self, item_macro: &'ast syn::Macro) {
        if path_ends_with(&item_macro.path, "enforces_here") {
            self.collect(&item_macro.tokens);
        }
        syn::visit::visit_macro(self, item_macro);
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

fn verification_ids(attrs: &[syn::Attribute], id_re: &Regex) -> (BTreeSet<String>, bool) {
    let is_test = attrs.iter().any(|attr| path_ends_with(attr.path(), "test"));
    let ignored = attrs
        .iter()
        .any(|attr| path_ends_with(attr.path(), "ignore"));
    if !is_test || ignored {
        return (BTreeSet::new(), is_test);
    }
    let ids = attrs
        .iter()
        .filter(|attr| path_ends_with(attr.path(), "verifies"))
        .flat_map(|attr| {
            let tokens = attr.meta.to_token_stream().to_string();
            id_re
                .find_iter(&tokens)
                .map(|found| found.as_str().to_string())
                .collect::<Vec<_>>()
        })
        .collect();
    (ids, true)
}

fn path_ends_with(path: &syn::Path, expected: &str) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == expected)
}

fn requirement_id_regex() -> Regex {
    Regex::new(r"REQ-[A-Z]{2,}-\d{3}").expect("BUG: invalid requirement ID regex")
}

fn normalized_item_tokens(item: &syn::Item) -> String {
    let mut item = item.clone();
    strip_item_docs(&mut item);
    item.to_token_stream().to_string()
}

fn normalized_impl_item_tokens(item: &syn::ImplItem) -> String {
    let mut item = item.clone();
    strip_impl_item_docs(&mut item);
    item.to_token_stream().to_string()
}

fn normalized_trait_item_tokens(item: &syn::TraitItem) -> String {
    let mut item = item.clone();
    strip_trait_item_docs(&mut item);
    item.to_token_stream().to_string()
}

fn normalized_behavior_tokens(tokens: &str) -> String {
    static ATTRIBUTE_RE: OnceLock<Regex> = OnceLock::new();
    static STATEMENT_RE: OnceLock<Regex> = OnceLock::new();
    static EXPRESSION_RE: OnceLock<Regex> = OnceLock::new();
    let attribute_re = ATTRIBUTE_RE.get_or_init(|| {
        Regex::new(
            r#"#\s*\[\s*(?:[A-Za-z_][A-Za-z0-9_]*\s*::\s*)*(?:enforces|verifies)(?:\s*\([^\]]*\))?\s*\]"#,
        )
        .expect("BUG: invalid trace attribute regex")
    });
    let statement_re = STATEMENT_RE.get_or_init(|| {
        Regex::new(r#"(?:[A-Za-z_][A-Za-z0-9_]*\s*::\s*)*enforces_here\s*!\s*\([^\)]*\)\s*;"#)
            .expect("BUG: invalid branch anchor regex")
    });
    let expression_re = EXPRESSION_RE.get_or_init(|| {
        Regex::new(r#"(?:[A-Za-z_][A-Za-z0-9_]*\s*::\s*)*enforces_here\s*!\s*\([^\)]*\)"#)
            .expect("BUG: invalid expression anchor regex")
    });
    let without_attributes = attribute_re.replace_all(tokens, " ");
    let without_statements = statement_re.replace_all(&without_attributes, " ");
    expression_re
        .replace_all(&without_statements, "()")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_doc_attrs(attrs: &mut Vec<syn::Attribute>) {
    attrs.retain(|attr| !attr.path().is_ident("doc"));
}

fn strip_field_docs(fields: &mut syn::Fields) {
    for field in fields.iter_mut() {
        strip_doc_attrs(&mut field.attrs);
    }
}

fn strip_item_docs(item: &mut syn::Item) {
    match item {
        syn::Item::Const(item) => strip_doc_attrs(&mut item.attrs),
        syn::Item::Enum(item) => {
            strip_doc_attrs(&mut item.attrs);
            for variant in &mut item.variants {
                strip_doc_attrs(&mut variant.attrs);
                strip_field_docs(&mut variant.fields);
            }
        }
        syn::Item::ExternCrate(item) => strip_doc_attrs(&mut item.attrs),
        syn::Item::Fn(item) => strip_doc_attrs(&mut item.attrs),
        syn::Item::ForeignMod(item) => strip_doc_attrs(&mut item.attrs),
        syn::Item::Impl(item) => strip_doc_attrs(&mut item.attrs),
        syn::Item::Macro(item) => strip_doc_attrs(&mut item.attrs),
        syn::Item::Mod(item) => strip_doc_attrs(&mut item.attrs),
        syn::Item::Static(item) => strip_doc_attrs(&mut item.attrs),
        syn::Item::Struct(item) => {
            strip_doc_attrs(&mut item.attrs);
            strip_field_docs(&mut item.fields);
        }
        syn::Item::Trait(item) => strip_doc_attrs(&mut item.attrs),
        syn::Item::TraitAlias(item) => strip_doc_attrs(&mut item.attrs),
        syn::Item::Type(item) => strip_doc_attrs(&mut item.attrs),
        syn::Item::Union(item) => {
            strip_doc_attrs(&mut item.attrs);
            for field in &mut item.fields.named {
                strip_doc_attrs(&mut field.attrs);
            }
        }
        syn::Item::Use(item) => strip_doc_attrs(&mut item.attrs),
        syn::Item::Verbatim(_) => {}
        _ => {}
    }
}

fn strip_impl_item_docs(item: &mut syn::ImplItem) {
    match item {
        syn::ImplItem::Const(item) => strip_doc_attrs(&mut item.attrs),
        syn::ImplItem::Fn(item) => strip_doc_attrs(&mut item.attrs),
        syn::ImplItem::Type(item) => strip_doc_attrs(&mut item.attrs),
        syn::ImplItem::Macro(item) => strip_doc_attrs(&mut item.attrs),
        syn::ImplItem::Verbatim(_) => {}
        _ => {}
    }
}

fn strip_trait_item_docs(item: &mut syn::TraitItem) {
    match item {
        syn::TraitItem::Const(item) => strip_doc_attrs(&mut item.attrs),
        syn::TraitItem::Fn(item) => strip_doc_attrs(&mut item.attrs),
        syn::TraitItem::Type(item) => strip_doc_attrs(&mut item.attrs),
        syn::TraitItem::Macro(item) => strip_doc_attrs(&mut item.attrs),
        syn::TraitItem::Verbatim(_) => {}
        _ => {}
    }
}

fn module_root(path: &Path) -> String {
    let components = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    let crate_name = components
        .first()
        .map_or("workspace", |name| *name)
        .replace('-', "_");
    let source_index = components
        .iter()
        .position(|component| *component == "src" || *component == "tests");
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
mod tests {
    use super::*;
    use crate::baseline::{BaselineEntry, GapKind};

    fn requirement(id: &str) -> Requirement {
        Requirement {
            id: id.to_string(),
            area: "ZZ".to_string(),
            title: "Test requirement".to_string(),
            statement: format!("**{id}** Test SHALL hold."),
            enforced_text: "`src/lib.rs`".to_string(),
            verified_text: "review".to_string(),
            doc: "crate/docs/requirements.md".to_string(),
            line: 10,
            enforced_paths: Vec::new(),
            not_implemented: false,
            retired: false,
            automated: false,
            evidence: Vec::new(),
            e2e: false,
            review_only: true,
            pending: false,
        }
    }

    #[test]
    fn parses_nul_terminated_name_status_with_rename() {
        let raw = b"M\0src/a.rs\0R097\0src/old.rs\0src/new.rs\0A\0src/added.rs\0";
        let files = parse_name_status(raw).expect("name status parses");
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].status, FileStatus::Modified);
        assert_eq!(files[1].status, FileStatus::Renamed);
        assert_eq!(files[1].base_path, Some(PathBuf::from("src/old.rs")));
        assert_eq!(files[1].head_path, Some(PathBuf::from("src/new.rs")));
        assert_eq!(files[2].status, FileStatus::Added);
    }

    #[test]
    fn source_index_ignores_comments_but_finds_typed_anchors() {
        let before = r#"
            /// old documentation
            #[enforces("REQ-ZZ-001")]
            fn apply() { enforces_here!("REQ-ZZ-002"); work(); }

            #[verifies("REQ-ZZ-001")]
            #[test]
            fn proves_it() { assert!(true); }
        "#;
        let after = before.replace("old documentation", "new documentation");
        let path = Path::new("crate/src/lib.rs");
        let before = index_source(before, path).expect("base source parses");
        let after = index_source(&after, path).expect("head source parses");
        assert_eq!(before["fn:apply"].tokens, after["fn:apply"].tokens);
        assert_eq!(
            before["fn:apply"]
                .enforcement
                .iter()
                .map(|site| site.id.clone())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["REQ-ZZ-001".to_string(), "REQ-ZZ-002".to_string()])
        );
        assert_eq!(
            before["fn:proves_it"].verification,
            BTreeSet::from(["REQ-ZZ-001".to_string()])
        );
    }

    #[test]
    fn behavior_tokens_exclude_trace_metadata() {
        let before = index_source("fn apply() { work(); }", Path::new("crate/src/lib.rs"))
            .expect("base source parses");
        let after = index_source(
            "#[shallguard_macros::enforces(\"REQ-ZZ-001\")]\n\
             fn apply() {\n\
                 shallguard_macros::enforces_here!(\"REQ-ZZ-002\");\n\
                 work();\n\
             }",
            Path::new("crate/src/lib.rs"),
        )
        .expect("head source parses");
        assert_ne!(before["fn:apply"].tokens, after["fn:apply"].tokens);
        assert_eq!(
            before["fn:apply"].behavior_tokens,
            after["fn:apply"].behavior_tokens
        );

        let before = index_source(
            "fn select(value: u8) { match value { 0 => (), _ => work(), } }",
            Path::new("crate/src/lib.rs"),
        )
        .expect("base expression source parses");
        let after = index_source(
            "fn select(value: u8) {\n\
                 match value {\n\
                     0 => enforces_here!(\"REQ-ZZ-001\"),\n\
                     _ => work(),\n\
                 }\n\
             }",
            Path::new("crate/src/lib.rs"),
        )
        .expect("head expression source parses");
        assert_eq!(
            before["fn:select"].behavior_tokens,
            after["fn:select"].behavior_tokens
        );
    }

    #[test]
    fn changed_anchored_function_is_direct_impact() {
        let base = index_source(
            "#[enforces(\"REQ-ZZ-001\")] fn apply() { old(); }",
            Path::new("crate/src/lib.rs"),
        )
        .expect("base parses");
        let head = index_source(
            "#[enforces(\"REQ-ZZ-001\")] fn apply() { new(); }",
            Path::new("crate/src/lib.rs"),
        )
        .expect("head parses");
        let file = ChangedFile {
            status: FileStatus::Modified,
            base_path: Some(PathBuf::from("crate/src/lib.rs")),
            head_path: Some(PathBuf::from("crate/src/lib.rs")),
        };
        let requirements = BTreeMap::from([("REQ-ZZ-001".to_string(), requirement("REQ-ZZ-001"))]);
        let mut state = AnalysisState::new();
        compare_scopes(
            &file,
            &base,
            &head,
            &ChangedLines {
                base: vec![LineRange { start: 1, end: 1 }],
                head: vec![LineRange { start: 1, end: 1 }],
            },
            &requirements,
            &requirements,
            &mut state,
        );
        assert_eq!(
            state.requirements["REQ-ZZ-001"].impacts[0].class,
            ImpactClass::Direct
        );
        assert!(state.unclaimed.is_empty());
    }

    #[test]
    fn changed_unanchored_function_records_dependency_candidate() {
        let base = index_source("fn helper() { old(); }", Path::new("crate/src/lib.rs"))
            .expect("base parses");
        let head = index_source("fn helper() { new(); }", Path::new("crate/src/lib.rs"))
            .expect("head parses");
        let file = ChangedFile {
            status: FileStatus::Modified,
            base_path: Some(PathBuf::from("crate/src/lib.rs")),
            head_path: Some(PathBuf::from("crate/src/lib.rs")),
        };
        let mut state = AnalysisState::new();
        compare_scopes(
            &file,
            &base,
            &head,
            &ChangedLines {
                base: vec![LineRange { start: 1, end: 1 }],
                head: vec![LineRange { start: 1, end: 1 }],
            },
            &BTreeMap::new(),
            &BTreeMap::new(),
            &mut state,
        );
        assert_eq!(state.dependency_changes.len(), 1);
        assert_eq!(state.unclaimed.len(), 1);
        assert_eq!(
            state.dependency_changes[0].change_id,
            state.unclaimed[0].change_id
        );
    }

    #[test]
    fn renamed_file_reports_anchored_item_move_only() {
        let scopes = index_source(
            "#[enforces(\"REQ-ZZ-001\")] fn apply() {} fn helper() {}",
            Path::new("crate/src/old.rs"),
        )
        .expect("source parses");
        let file = ChangedFile {
            status: FileStatus::Renamed,
            base_path: Some(PathBuf::from("crate/src/old.rs")),
            head_path: Some(PathBuf::from("crate/src/new.rs")),
        };
        let requirements = BTreeMap::from([("REQ-ZZ-001".to_string(), requirement("REQ-ZZ-001"))]);
        let mut state = AnalysisState::new();
        compare_scopes(
            &file,
            &scopes,
            &scopes,
            &ChangedLines::default(),
            &requirements,
            &requirements,
            &mut state,
        );
        assert_eq!(
            state.requirements["REQ-ZZ-001"].impacts[0].class,
            ImpactClass::Anchor
        );
        assert!(state.unclaimed.is_empty());
    }

    #[test]
    fn branch_anchor_only_owns_its_enclosing_block() {
        let base = index_source(
            "fn apply(flag: bool) {\n\
                 if flag {\n\
                     enforces_here!(\"REQ-ZZ-001\");\n\
                     guarded_old();\n\
                 }\n\
                 unrelated_old();\n\
             }",
            Path::new("crate/src/lib.rs"),
        )
        .expect("base parses");
        let head = index_source(
            "fn apply(flag: bool) {\n\
                 if flag {\n\
                     enforces_here!(\"REQ-ZZ-001\");\n\
                     guarded_old();\n\
                 }\n\
                 unrelated_new();\n\
             }",
            Path::new("crate/src/lib.rs"),
        )
        .expect("head parses");
        let file = ChangedFile {
            status: FileStatus::Modified,
            base_path: Some(PathBuf::from("crate/src/lib.rs")),
            head_path: Some(PathBuf::from("crate/src/lib.rs")),
        };
        let requirements = BTreeMap::from([("REQ-ZZ-001".to_string(), requirement("REQ-ZZ-001"))]);
        let mut state = AnalysisState::new();
        compare_scopes(
            &file,
            &base,
            &head,
            &ChangedLines {
                base: vec![LineRange { start: 6, end: 6 }],
                head: vec![LineRange { start: 6, end: 6 }],
            },
            &requirements,
            &requirements,
            &mut state,
        );
        assert!(state.requirements.is_empty());
        assert_eq!(state.unclaimed.len(), 1);
    }

    #[test]
    fn branch_anchor_without_braces_owns_its_match_arm() {
        let source = "fn apply(value: u8) {\n\
             match value {\n\
                 1 => enforces_here!(\"REQ-ZZ-001\"),\n\
                 _ => other(),\n\
             }\n\
         }";
        let scopes = index_source(source, Path::new("crate/src/lib.rs")).expect("source parses");
        let site = &scopes["fn:apply"].enforcement[0];
        assert_eq!((site.start_line, site.end_line), (3, 3));
    }

    #[test]
    fn parses_zero_context_hunk_ranges() {
        let diff = "@@ -10,2 +10,0 @@ old\n@@ -20 +18,3 @@ old\n";
        let changed = parse_hunk_ranges(diff).expect("hunks parse");
        assert_eq!(
            changed.base,
            vec![
                LineRange { start: 10, end: 11 },
                LineRange { start: 20, end: 20 }
            ]
        );
        assert_eq!(changed.head, vec![LineRange { start: 18, end: 20 }]);
    }

    #[test]
    fn recognizes_test_fixture_source_paths() {
        assert!(test_fixture_path(Path::new("crate/tests/basic.rs")));
        assert!(test_fixture_path(Path::new(
            "crate/src/state/test_mocks.rs"
        )));
        assert!(test_fixture_path(Path::new("crate/src/router_tests.rs")));
        assert!(!test_fixture_path(Path::new("crate/src/router.rs")));
    }

    #[test]
    fn changed_requirement_with_baseline_debt_is_policy_error() {
        let before = requirement("REQ-ZZ-001");
        let mut after = requirement("REQ-ZZ-001");
        after.statement.push_str(" It SHALL remain safe.");
        let baseline = Baseline::from_entries(vec![BaselineEntry {
            requirement: "REQ-ZZ-001".to_string(),
            kind: GapKind::EnforcementAnchor,
        }]);
        let mut state = AnalysisState::new();
        compare_requirement_documents(
            &BTreeMap::from([("REQ-ZZ-001".to_string(), before)]),
            &BTreeMap::from([("REQ-ZZ-001".to_string(), after)]),
            &baseline,
            &mut state,
        );
        assert_eq!(state.findings.len(), 1);
        assert_eq!(state.findings[0].code, "changed-baselined-requirement");
        assert_eq!(state.findings[0].severity, FindingSeverity::Error);
    }

    #[test]
    fn requirement_change_classifies_each_segment() {
        let before = requirement("REQ-ZZ-001");
        let mut after = requirement("REQ-ZZ-001");
        after.enforced_text = "`src/other.rs`".to_string();
        after.verified_text = "automated test".to_string();
        assert_eq!(
            requirement_change_reasons(Some(&before), Some(&after)),
            vec![
                "enforcement references changed",
                "verification evidence changed"
            ]
        );
    }

    #[test]
    fn json_artifact_uses_versioned_schema_and_configuration() {
        let artifact = ImpactArtifact {
            schema: IMPACT_SCHEMA.to_string(),
            repository: "workspace".to_string(),
            base_commit: "base".to_string(),
            head_commit: "head".to_string(),
            head_source: "working-tree".to_string(),
            working_tree_dirty: false,
            configuration: ImpactConfiguration {
                features: Vec::new(),
                targets: vec!["workspace".to_string()],
                dependency_depth: 1,
                scope_precision: "rust-item+anchor-block".to_string(),
            },
            requirements: Vec::new(),
            unclaimed_changes: Vec::new(),
            findings: Vec::new(),
        };
        let value = serde_json::to_value(&artifact).expect("artifact serializes");
        assert_eq!(value["schema"], IMPACT_SCHEMA);
        assert_eq!(
            value["configuration"]["scope_precision"],
            "rust-item+anchor-block"
        );
        assert_eq!(value["configuration"]["dependency_depth"], 1);
    }
}
