//! Read-only loading and validation of retained local-review artifacts.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::state::{RUN_STATE_SCHEMA, validate_result_summary};
use super::validation::{capsule_metadata, validate_response};
use super::{
    BundleEntry, FindingCategory, FindingSeverity, REVIEW_RESULT_SCHEMA, REVIEW_RUN_SCHEMA,
    ReviewEntry, ReviewFailureKind, ReviewOrigin, ReviewProvider, ReviewResult, ReviewStatus,
    ReviewVerdict, ReviewVerdictCounts, digest, validate_bundle_file, validate_requirement_id,
};
use crate::bundle::{MANIFEST_SCHEMA, REVIEW_PROTOCOL};

/// Completion state recorded by a stored review manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredReviewRunStatus {
    /// The run may contain processed and pending requirements.
    Running,
    /// Every selected requirement has a stored attempt.
    Completed,
}

impl StoredReviewRunStatus {
    /// Returns the stable command-line label for this state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
        }
    }
}

/// Advisory verdict retained for a successfully validated requirement review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredReviewVerdict {
    /// Supplied context supports every normative clause.
    Satisfied,
    /// Supplied context contains a concrete counterexample.
    Violated,
    /// Supplied context cannot support a conclusion.
    InsufficientEvidence,
    /// The requirement is unrelated to the reviewed change.
    NotImpacted,
}

impl StoredReviewVerdict {
    /// Returns the stable artifact label for this verdict.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Satisfied => "satisfied",
            Self::Violated => "violated",
            Self::InsufficientEvidence => "insufficient_evidence",
            Self::NotImpacted => "not_impacted",
        }
    }
}

/// Processing state for one selected requirement in a stored run.
#[derive(Debug, Clone, PartialEq)]
pub enum StoredRequirementStatus {
    /// No attempt has been selected by the run manifest yet.
    Pending,
    /// A result passed artifact and semantic-response validation.
    Completed {
        /// Advisory semantic verdict.
        verdict: StoredReviewVerdict,
        /// Provider confidence in the range zero through one.
        confidence: f64,
    },
    /// The provider did not return a usable response.
    Unavailable {
        /// Stable failure classification retained by the run.
        kind: String,
        /// Human-readable provider or timeout diagnostic.
        error: String,
    },
    /// A returned response failed identity, citation, or schema validation.
    Invalid {
        /// Stable validation-failure classification retained by the run.
        kind: String,
        /// Human-readable validation diagnostic.
        error: String,
    },
}

/// One source citation retained in semantic review details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCitation {
    /// Repository-relative cited file.
    pub file: String,
    /// One-based cited source line.
    pub line: usize,
}

/// Per-clause semantic assessment retained in a validated result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredClauseReview {
    /// Stable normative clause identifier.
    pub clause_id: String,
    /// Advisory verdict for this clause.
    pub verdict: StoredReviewVerdict,
    /// Provider explanation bounded to supplied context.
    pub reason: String,
    /// Citations into the supplied capsule excerpts.
    pub citations: Vec<StoredCitation>,
    /// Concrete counterexample considered by the provider.
    pub counterexample: String,
    /// Assessment of the supplied automated or static evidence.
    pub evidence_assessment: String,
}

/// One retained semantic finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFinding {
    /// Stable severity label.
    pub severity: String,
    /// Normative clause associated with the finding.
    pub clause_id: String,
    /// Stable finding-category label.
    pub category: String,
    /// Concise finding title.
    pub title: String,
    /// Explanation of the observed problem.
    pub explanation: String,
    /// Scenario demonstrating the problem.
    pub scenario: String,
    /// Citations into the supplied capsule excerpts.
    pub citations: Vec<StoredCitation>,
    /// Product outcome affected by the finding.
    pub affected_outcome: String,
    /// Suggested additional verification.
    pub suggested_verification: String,
}

/// Detailed content of one validated semantic-review result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredReviewDetails {
    /// Per-clause assessments.
    pub clause_reviews: Vec<StoredClauseReview>,
    /// Concrete semantic findings.
    pub findings: Vec<StoredFinding>,
    /// Evidence the provider needed but did not receive.
    pub missing_evidence: Vec<String>,
    /// Limits on conclusions imposed by the supplied context.
    pub context_limitations: Vec<String>,
}

/// Validated stored state for one selected requirement.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredRequirementReview {
    /// Stable requirement identifier.
    pub requirement_id: String,
    /// Processing or semantic result state.
    pub status: StoredRequirementStatus,
    /// Manifest-selected attempt number, absent while pending.
    pub attempt: Option<u32>,
    /// Whether the selected attempt was fresh, resumed, or cached.
    pub origin: Option<String>,
    /// Provider duration for the selected attempt in milliseconds.
    pub duration_ms: Option<u64>,
    /// Validated retained result path, present only for completed reviews.
    pub result_path: Option<PathBuf>,
    /// Validated details, present only for completed reviews.
    pub details: Option<StoredReviewDetails>,
}

/// Validated, read-only view of one retained local-review run.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredReview {
    /// Review artifact directory that was inspected.
    pub output_dir: PathBuf,
    /// Whether the manifest represents a running or completed run.
    pub status: StoredReviewRunStatus,
    /// Configured local provider name.
    pub provider: String,
    /// Configured provider model or the stable default label.
    pub model: String,
    /// Total number of requirements frozen in the run identity.
    pub selected_requirements: usize,
    /// Number of requirements with a manifest-selected attempt.
    pub processed_responses: usize,
    /// Counts of validated advisory semantic verdicts.
    pub verdicts: ReviewVerdictCounts,
    /// Number of manifest-selected unavailable or invalid attempts.
    pub unavailable_or_invalid: usize,
    /// Selected requirements, optionally narrowed by the caller's filter.
    pub requirements: Vec<StoredRequirementReview>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredManifest {
    schema: String,
    protocol: String,
    status: StoredReviewRunStatus,
    selected_requirements: usize,
    provider: ReviewProvider,
    model: String,
    local_provider: Option<String>,
    cli_version: String,
    bundle_schema: String,
    repository: String,
    base_commit: String,
    head_commit: String,
    response_schema_digest: String,
    started_unix_seconds: u64,
    duration_ms: u64,
    reviews: Vec<ReviewEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRunState {
    schema: String,
    identity: StoredRunIdentity,
    started_unix_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRunIdentity {
    protocol: String,
    bundle_manifest_digest: String,
    provider: ReviewProvider,
    model: String,
    local_provider: Option<String>,
    cli_version: String,
    units: Vec<StoredRunUnit>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRunUnit {
    requirement: String,
    capsule_digest: String,
}

/// Reads and validates a retained local-review run without changing it.
///
/// An empty `requirements` set returns every requirement frozen in `run.json`.
/// A non-empty set narrows the returned requirement details after the complete
/// stored artifact has been validated.
///
/// # Errors
///
/// Returns an error when the artifact is absent or unreadable, a schema or
/// identity is unsupported, a digest or contained path is invalid, or a
/// requested requirement is not selected by the stored run.
#[shallguard::enforces("REQ-CLI-007", "REQ-CLI-009", "REQ-CLI-010", "REQ-CLI-011")]
pub fn inspect_stored_review(
    output_dir: &Path,
    requirements: &BTreeSet<String>,
) -> Result<StoredReview> {
    for requirement in requirements {
        validate_requirement_id(requirement)?;
    }
    let manifest_path = output_dir.join("manifest.json");
    let manifest: StoredManifest = read_json(&manifest_path, "review manifest")?;
    validate_manifest(&manifest)?;

    let run_path = output_dir.join("run.json");
    let run: StoredRunState = read_json(&run_path, "review run state")?;
    validate_run_identity(&run, &manifest)?;

    let unit_ids = run
        .identity
        .units
        .iter()
        .map(|unit| unit.requirement.as_str())
        .collect::<BTreeSet<_>>();
    let missing = requirements
        .iter()
        .filter(|requirement| !unit_ids.contains(requirement.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "requested requirement(s) absent from stored review: {}",
            missing.join(", ")
        );
    }

    let mut entries = BTreeMap::new();
    for review in &manifest.reviews {
        if entries
            .insert(review.requirement_id.as_str(), review)
            .is_some()
        {
            bail!(
                "review manifest selects requirement {:?} more than once",
                review.requirement_id
            );
        }
    }

    let mut verdicts = ReviewVerdictCounts::default();
    let mut unavailable_or_invalid = 0usize;
    let mut stored_requirements = Vec::with_capacity(run.identity.units.len());
    for unit in &run.identity.units {
        let review = match entries.get(unit.requirement.as_str()) {
            Some(review) => {
                let stored = validate_review_entry(output_dir, unit, review)?;
                match &stored.status {
                    StoredRequirementStatus::Completed { verdict, .. } => {
                        increment_verdict(&mut verdicts, *verdict);
                    }
                    StoredRequirementStatus::Unavailable { .. }
                    | StoredRequirementStatus::Invalid { .. } => {
                        unavailable_or_invalid += 1;
                    }
                    StoredRequirementStatus::Pending => {
                        unreachable!("a manifest-selected attempt cannot be pending")
                    }
                }
                stored
            }
            None => StoredRequirementReview {
                requirement_id: unit.requirement.clone(),
                status: StoredRequirementStatus::Pending,
                attempt: None,
                origin: None,
                duration_ms: None,
                result_path: None,
                details: None,
            },
        };
        if requirements.is_empty() || requirements.contains(&unit.requirement) {
            stored_requirements.push(review);
        }
    }

    Ok(StoredReview {
        output_dir: output_dir.to_path_buf(),
        status: manifest.status,
        provider: provider_label(manifest.provider).to_string(),
        model: manifest.model,
        selected_requirements: manifest.selected_requirements,
        processed_responses: manifest.reviews.len(),
        verdicts,
        unavailable_or_invalid,
        requirements: stored_requirements,
    })
}

fn validate_manifest(manifest: &StoredManifest) -> Result<()> {
    if manifest.schema != REVIEW_RUN_SCHEMA {
        bail!(
            "unsupported review manifest schema {:?}; expected {REVIEW_RUN_SCHEMA:?}",
            manifest.schema
        );
    }
    if manifest.protocol != REVIEW_PROTOCOL {
        bail!(
            "unsupported review protocol {:?}; expected {REVIEW_PROTOCOL:?}",
            manifest.protocol
        );
    }
    if manifest.bundle_schema != MANIFEST_SCHEMA {
        bail!(
            "unsupported bundle schema {:?}; expected {MANIFEST_SCHEMA:?}",
            manifest.bundle_schema
        );
    }
    let expected_schema_digest = digest(REVIEW_RESULT_SCHEMA.as_bytes());
    if manifest.response_schema_digest != expected_schema_digest {
        bail!("review response schema digest does not match this ShallGuard version");
    }
    validate_digest(&manifest.response_schema_digest, "response schema")?;
    if manifest.selected_requirements == 0 {
        bail!("review manifest selects no requirements");
    }
    if manifest.reviews.len() > manifest.selected_requirements {
        bail!("review manifest contains more responses than selected requirements");
    }
    if manifest.status == StoredReviewRunStatus::Completed
        && manifest.reviews.len() != manifest.selected_requirements
    {
        bail!("completed review manifest does not contain every selected requirement");
    }
    if manifest.repository.is_empty()
        || manifest.base_commit.is_empty()
        || manifest.head_commit.is_empty()
        || manifest.cli_version.is_empty()
    {
        bail!("review manifest contains incomplete run identity");
    }
    let _ = (manifest.started_unix_seconds, manifest.duration_ms);
    Ok(())
}

fn validate_run_identity(run: &StoredRunState, manifest: &StoredManifest) -> Result<()> {
    if run.schema != RUN_STATE_SCHEMA {
        bail!(
            "unsupported review run state schema {:?}; expected {RUN_STATE_SCHEMA:?}",
            run.schema
        );
    }
    if run.identity.protocol != manifest.protocol
        || run.identity.provider != manifest.provider
        || run.identity.model != manifest.model
        || run.identity.local_provider != manifest.local_provider
        || run.identity.cli_version != manifest.cli_version
    {
        bail!("review manifest identity does not match run.json");
    }
    validate_digest(
        &run.identity.bundle_manifest_digest,
        "bundle manifest identity",
    )?;
    if run.identity.units.len() != manifest.selected_requirements {
        bail!("review manifest selection count does not match run.json");
    }
    let mut ids = BTreeSet::new();
    for unit in &run.identity.units {
        validate_requirement_id(&unit.requirement)?;
        validate_digest(&unit.capsule_digest, "capsule")?;
        if !ids.insert(unit.requirement.as_str()) {
            bail!(
                "review run state selects requirement {:?} more than once",
                unit.requirement
            );
        }
    }
    let _ = run.started_unix_seconds;
    Ok(())
}

fn validate_review_entry(
    output_dir: &Path,
    unit: &StoredRunUnit,
    review: &ReviewEntry,
) -> Result<StoredRequirementReview> {
    validate_requirement_id(&review.requirement_id)?;
    validate_bundle_file(&review.capsule_file)?;
    validate_digest(&review.capsule_digest, "capsule")?;
    validate_digest(&review.prompt_digest, "prompt")?;
    if review.requirement_id != unit.requirement || review.capsule_digest != unit.capsule_digest {
        bail!(
            "review manifest entry for {:?} does not match run.json",
            review.requirement_id
        );
    }
    if review.attempt == 0 {
        bail!("review manifest entry has invalid attempt number zero");
    }
    let attempt_relative = format!(
        "units/{}/attempts/{:04}",
        review.requirement_id, review.attempt
    );
    let attempt_json = contained_path(output_dir, &format!("{attempt_relative}/attempt.json"))?;
    let selected: ReviewEntry = read_json(&attempt_json, "review attempt")?;
    if selected != *review {
        bail!(
            "manifest-selected attempt for {:?} does not match attempt.json",
            review.requirement_id
        );
    }
    let capsule_path = contained_path(output_dir, &format!("{attempt_relative}/capsule.json"))?;
    let capsule_text = std::fs::read_to_string(&capsule_path)
        .with_context(|| format!("reading review capsule {}", capsule_path.display()))?;
    let entry = BundleEntry {
        requirement: review.requirement_id.clone(),
        description: String::new(),
        file: review.capsule_file.clone(),
        digest: review.capsule_digest.clone(),
    };
    let metadata = capsule_metadata(&capsule_text, &entry)
        .with_context(|| format!("validating review capsule {}", capsule_path.display()))?;
    let prompt_path = contained_path(output_dir, &format!("{attempt_relative}/prompt.txt"))?;
    let prompt = std::fs::read(&prompt_path)
        .with_context(|| format!("reading review prompt {}", prompt_path.display()))?;
    if digest(&prompt) != review.prompt_digest {
        bail!("review prompt digest does not match prompt.txt");
    }

    let common = |status, result_path, details| StoredRequirementReview {
        requirement_id: review.requirement_id.clone(),
        status,
        attempt: Some(review.attempt),
        origin: Some(origin_label(review.origin).to_string()),
        duration_ms: Some(review.duration_ms),
        result_path,
        details,
    };
    match review.status {
        ReviewStatus::Completed => {
            let result_file = review
                .result_file
                .as_deref()
                .context("completed review has no result file")?;
            let expected = format!("{attempt_relative}/result.json");
            if result_file != expected {
                bail!("review result path does not match its requirement and current attempt");
            }
            let result_path = contained_path(output_dir, result_file)?;
            let result: ReviewResult = read_json(&result_path, "review result")?;
            let result = validate_response(result, &metadata)
                .map_err(anyhow::Error::from)
                .with_context(|| format!("validating review result {}", result_path.display()))?;
            validate_result_summary(review, &result)?;
            let verdict = review
                .verdict
                .context("completed review has no semantic verdict")?;
            let confidence = review
                .confidence
                .context("completed review has no confidence")?;
            Ok(common(
                StoredRequirementStatus::Completed {
                    verdict: stored_verdict(verdict),
                    confidence,
                },
                Some(result_path),
                Some(stored_details(result)),
            ))
        }
        ReviewStatus::Failed => {
            if review.response_digest.is_some()
                || review.verdict.is_some()
                || review.confidence.is_some()
                || review.result_file.is_some()
            {
                bail!("failed review entry contains completed-result fields");
            }
            let failure = review
                .failure_kind
                .context("failed review entry has no failure classification")?;
            let error = review
                .error
                .clone()
                .filter(|error| !error.is_empty())
                .context("failed review entry has no diagnostic")?;
            let kind = failure_label(failure).to_string();
            let status = match failure {
                ReviewFailureKind::ProviderTimeout | ReviewFailureKind::ProviderFailed => {
                    StoredRequirementStatus::Unavailable { kind, error }
                }
                ReviewFailureKind::IdentityInvalid
                | ReviewFailureKind::CitationInvalid
                | ReviewFailureKind::SchemaInvalid => {
                    StoredRequirementStatus::Invalid { kind, error }
                }
            };
            Ok(common(status, None, None))
        }
    }
}

fn stored_details(result: ReviewResult) -> StoredReviewDetails {
    StoredReviewDetails {
        clause_reviews: result
            .clause_reviews
            .into_iter()
            .map(|review| StoredClauseReview {
                clause_id: review.clause_id,
                verdict: stored_verdict(review.verdict),
                reason: review.reason,
                citations: stored_citations(review.citations),
                counterexample: review.counterexample,
                evidence_assessment: review.evidence_assessment,
            })
            .collect(),
        findings: result
            .findings
            .into_iter()
            .map(|finding| StoredFinding {
                severity: severity_label(finding.severity).to_string(),
                clause_id: finding.clause_id,
                category: category_label(finding.category).to_string(),
                title: finding.title,
                explanation: finding.explanation,
                scenario: finding.scenario,
                citations: stored_citations(finding.citations),
                affected_outcome: finding.affected_outcome,
                suggested_verification: finding.suggested_verification,
            })
            .collect(),
        missing_evidence: result.missing_evidence,
        context_limitations: result.context_limitations,
    }
}

fn stored_citations(citations: Vec<super::Citation>) -> Vec<StoredCitation> {
    citations
        .into_iter()
        .map(|citation| StoredCitation {
            file: citation.file,
            line: citation.line,
        })
        .collect()
}

fn increment_verdict(counts: &mut ReviewVerdictCounts, verdict: StoredReviewVerdict) {
    match verdict {
        StoredReviewVerdict::Satisfied => counts.satisfied += 1,
        StoredReviewVerdict::Violated => counts.violated += 1,
        StoredReviewVerdict::InsufficientEvidence => counts.insufficient_evidence += 1,
        StoredReviewVerdict::NotImpacted => counts.not_impacted += 1,
    }
}

fn stored_verdict(verdict: ReviewVerdict) -> StoredReviewVerdict {
    match verdict {
        ReviewVerdict::Satisfied => StoredReviewVerdict::Satisfied,
        ReviewVerdict::Violated => StoredReviewVerdict::Violated,
        ReviewVerdict::InsufficientEvidence => StoredReviewVerdict::InsufficientEvidence,
        ReviewVerdict::NotImpacted => StoredReviewVerdict::NotImpacted,
    }
}

fn provider_label(provider: ReviewProvider) -> &'static str {
    match provider {
        ReviewProvider::Codex => "codex",
        ReviewProvider::Claude => "claude",
        ReviewProvider::Copilot => "copilot",
    }
}

fn origin_label(origin: ReviewOrigin) -> &'static str {
    match origin {
        ReviewOrigin::Fresh => "fresh",
        ReviewOrigin::Resumed => "resumed",
        ReviewOrigin::Cache => "cache",
    }
}

fn failure_label(failure: ReviewFailureKind) -> &'static str {
    match failure {
        ReviewFailureKind::ProviderTimeout => "provider_timeout",
        ReviewFailureKind::ProviderFailed => "provider_failed",
        ReviewFailureKind::IdentityInvalid => "identity_invalid",
        ReviewFailureKind::CitationInvalid => "citation_invalid",
        ReviewFailureKind::SchemaInvalid => "schema_invalid",
    }
}

fn severity_label(severity: FindingSeverity) -> &'static str {
    match severity {
        FindingSeverity::Critical => "critical",
        FindingSeverity::High => "high",
        FindingSeverity::Medium => "medium",
        FindingSeverity::Low => "low",
        FindingSeverity::Note => "note",
    }
}

fn category_label(category: FindingCategory) -> &'static str {
    match category {
        FindingCategory::Behavior => "behavior",
        FindingCategory::Safety => "safety",
        FindingCategory::Compatibility => "compatibility",
        FindingCategory::Evidence => "evidence",
        FindingCategory::Ambiguity => "ambiguity",
        FindingCategory::Scope => "scope",
    }
}

fn validate_digest(value: &str, description: &str) -> Result<()> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        bail!("invalid {description} digest {value:?}");
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid {description} digest {value:?}");
    }
    Ok(())
}

fn contained_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("unsafe path in review artifact: {relative:?}");
    }
    let root = root
        .canonicalize()
        .with_context(|| format!("resolving review output {}", root.display()))?;
    let path = root.join(relative_path);
    let resolved = path
        .canonicalize()
        .with_context(|| format!("resolving stored review path {}", path.display()))?;
    if !resolved.starts_with(&root) {
        bail!("stored review path escapes the artifact directory: {relative:?}");
    }
    Ok(resolved)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path, description: &str) -> Result<T> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {description} {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {description} {}", path.display()))
}

#[cfg(test)]
#[path = "review_show_tests.rs"]
mod tests;
