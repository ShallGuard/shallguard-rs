//! Isolated local-CLI execution of requirement review capsules.
//!
//! The deterministic checker and LLVM coverage collector remain independent of
//! this module. A review consumes an already generated bundle, gives one capsule
//! at a time to a locally installed Codex or Claude CLI, and validates the
//! structured response before retaining it as advisory evidence.

use std::collections::BTreeSet;
#[cfg(test)]
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::bundle::{MANIFEST_SCHEMA, REVIEW_PROTOCOL};
use crate::{ProgressCallback, report_progress};

#[path = "review_progress.rs"]
mod progress;
#[path = "review_provider.rs"]
mod provider;
#[path = "review_schema.rs"]
mod schema;
#[path = "review_show.rs"]
mod show;
#[path = "review_state.rs"]
mod state;
#[path = "review_validation.rs"]
mod validation;

use progress::{
    ReviewUnitProgress, concise_provider_error, render_summary, review_complete,
    review_reuse_invalid, review_started, review_unit_complete, review_unit_result,
    review_unit_retrying, review_unit_started,
};
use provider::{ProviderInvocation, invoke_provider, parse_provider_response, provider_version};
#[cfg(test)]
use provider::{command_spec, provider_environment_allowed};
use schema::{response_schema, review_prompt};
use state::{Attempt, CachedUnit, Reuse, ReviewStore};
use validation::{CapsuleMetadata, ReviewValidationError, capsule_metadata, validate_response};

pub use show::{
    StoredCitation, StoredClauseReview, StoredFinding, StoredRequirementReview,
    StoredRequirementStatus, StoredReview, StoredReviewDetails, StoredReviewRunStatus,
    StoredReviewVerdict, inspect_stored_review,
};

/// Version of an individual validated semantic-review response.
#[shallguard::enforces("REQ-CLI-005")]
pub const REVIEW_RESULT_SCHEMA: &str = "shallguard.requirement-review-result/v1";
/// Version of the aggregate local-review run artifact.
#[shallguard::enforces("REQ-CLI-005")]
pub const REVIEW_RUN_SCHEMA: &str = "shallguard.requirement-local-review/v1";

/// Locally installed model CLI used to review capsules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[shallguard::enforces("REQ-REV-001")]
pub enum ReviewProvider {
    /// OpenAI Codex CLI in ephemeral, read-only, non-interactive mode.
    Codex,
    /// Anthropic Claude CLI in non-interactive mode with tools disabled.
    Claude,
}

impl ReviewProvider {
    fn executable(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }
}

impl FromStr for ReviewProvider {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "codex" => Ok(Self::Codex),
            "claude" => Ok(Self::Claude),
            _ => bail!("unknown provider {value:?}; expected codex or claude"),
        }
    }
}

/// Inputs for one local semantic-review run.
pub struct ReviewOptions<'a> {
    /// Directory produced by `bundle` and containing `manifest.json`.
    pub bundle_dir: &'a Path,
    /// New directory that will contain prompts, raw output, and validated results.
    pub output_dir: &'a Path,
    /// Locally installed CLI to invoke.
    pub provider: ReviewProvider,
    /// Optional provider-specific model identifier.
    pub model: Option<&'a str>,
    /// Optional on-device Codex backend (`ollama` or `lmstudio`).
    pub local_provider: Option<&'a str>,
    /// Optional requirement allowlist. Empty selects every capsule in the bundle.
    pub requirements: &'a BTreeSet<String>,
    /// Maximum wall-clock duration for each capsule invocation.
    pub timeout: Duration,
    /// Continue a compatible existing output directory.
    pub resume: bool,
    /// Optional portable content-addressed validated-response cache.
    pub cache_dir: Option<&'a Path>,
    /// Optional human-readable progress callback.
    pub progress: Option<ProgressCallback>,
}

/// Concise outcome returned to the command-line adapter.
#[derive(Debug)]
pub struct ReviewRun {
    /// Directory containing the complete auditable run artifact.
    pub output_dir: PathBuf,
    /// Number of successfully validated model responses.
    pub reviews: usize,
    /// Advisory semantic verdict totals for successfully validated responses.
    pub verdicts: ReviewVerdictCounts,
    /// Number of unavailable, failed, or invalid model responses.
    pub failures: usize,
}

/// Advisory semantic verdict totals for a local review run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[shallguard::enforces("REQ-REV-005")]
pub struct ReviewVerdictCounts {
    /// Requirements whose supplied context supports every normative clause.
    pub satisfied: usize,
    /// Requirements with a concrete counterexample to at least one clause.
    pub violated: usize,
    /// Requirements whose supplied context cannot support a conclusion.
    pub insufficient_evidence: usize,
    /// Requirements judged unrelated to the reviewed change.
    pub not_impacted: usize,
}

/// Reviews every selected bundle capsule with an isolated local model CLI.
///
/// Model verdicts are advisory. Provider failures and invalid responses are
/// recorded in the output manifest and reported through [`ReviewRun::failures`].
///
/// # Errors
///
/// Returns an error when the bundle is malformed, a requested requirement is
/// absent, the provider executable is unavailable, or the output artifact
/// cannot be created.
#[shallguard::enforces("REQ-SEC-001")]
pub fn generate(options: &ReviewOptions<'_>) -> Result<ReviewRun> {
    validate_local_provider(options.provider, options.local_provider)?;
    report_progress(options.progress, "review: reading deterministic bundle");
    let manifest = read_bundle_manifest(options.bundle_dir)?;
    let mut entries = select_entries(&manifest, options.requirements)?;
    hydrate_descriptions(options.bundle_dir, &mut entries)?;
    review_started(options, entries.len());
    let cli_version = provider_version(options.provider)?;
    let schema_digest = digest(REVIEW_RESULT_SCHEMA.as_bytes());
    let started_at = unix_timestamp()?;
    let run_started = Instant::now();
    let store = ReviewStore::open(options, &manifest, &entries, &cli_version)?;

    let entry_count = entries.len();
    let mut artifact = ReviewRunArtifact {
        schema: REVIEW_RUN_SCHEMA,
        protocol: REVIEW_PROTOCOL,
        status: ReviewRunStatus::Running,
        selected_requirements: entry_count,
        provider: options.provider,
        model: options.model.unwrap_or("configured-default").to_string(),
        local_provider: options.local_provider.map(str::to_string),
        cli_version,
        bundle_schema: manifest.schema.clone(),
        repository: manifest.repository.clone(),
        base_commit: manifest.base_commit.clone(),
        head_commit: manifest.head_commit.clone(),
        response_schema_digest: schema_digest,
        started_unix_seconds: started_at,
        duration_ms: 0,
        reviews: Vec::with_capacity(entry_count),
    };
    for (entry_index, entry) in entries.into_iter().enumerate() {
        let unit = ReviewUnitProgress::new(entry_index, entry_count, &entry);
        let review = review_capsule(options, &entry, &manifest, unit, &store)?;
        let result = store.read_result(&review)?;
        artifact.reviews.push(review);
        artifact.duration_ms = millis(run_started.elapsed());
        if artifact.reviews.len() == artifact.selected_requirements {
            artifact.status = ReviewRunStatus::Completed;
        }
        persist_review_artifact(options.output_dir, &artifact)?;

        let review = artifact
            .reviews
            .last()
            .context("BUG: persisted review artifact has no current review")?;
        review_unit_complete(options, unit, review);
        review_unit_result(options, review, result.as_ref());
    }

    let failures = artifact
        .reviews
        .iter()
        .filter(|review| review.status == ReviewStatus::Failed)
        .count();
    let verdicts = review_verdict_counts(&artifact.reviews);
    review_complete(
        options,
        artifact.reviews.len() - failures,
        verdicts,
        failures,
    );
    Ok(ReviewRun {
        output_dir: options.output_dir.to_path_buf(),
        reviews: artifact.reviews.len() - failures,
        verdicts,
        failures,
    })
}

#[shallguard::enforces("REQ-REV-005")]
fn review_verdict_counts(reviews: &[ReviewEntry]) -> ReviewVerdictCounts {
    let mut counts = ReviewVerdictCounts::default();
    for verdict in reviews.iter().filter_map(|review| review.verdict) {
        match verdict {
            ReviewVerdict::Satisfied => counts.satisfied += 1,
            ReviewVerdict::Violated => counts.violated += 1,
            ReviewVerdict::InsufficientEvidence => counts.insufficient_evidence += 1,
            ReviewVerdict::NotImpacted => counts.not_impacted += 1,
        }
    }
    counts
}

fn validate_local_provider(provider: ReviewProvider, local_provider: Option<&str>) -> Result<()> {
    let Some(local_provider) = local_provider else {
        return Ok(());
    };
    if provider != ReviewProvider::Codex {
        bail!("an on-device local provider is supported only by the Codex adapter");
    }
    if !matches!(local_provider, "ollama" | "lmstudio") {
        bail!("unsupported Codex local provider {local_provider:?}");
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct BundleManifest {
    schema: String,
    repository: String,
    base_commit: String,
    head_commit: String,
    protocol: String,
    capsules: Vec<BundleEntry>,
    #[serde(skip)]
    digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleEntry {
    requirement: String,
    #[serde(default)]
    description: String,
    file: String,
    digest: String,
}

#[derive(Debug, Serialize)]
struct ReviewRunArtifact {
    schema: &'static str,
    protocol: &'static str,
    status: ReviewRunStatus,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ReviewRunStatus {
    Running,
    Completed,
}

impl ReviewRunStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewEntry {
    requirement_id: String,
    capsule_file: String,
    capsule_digest: String,
    prompt_digest: String,
    response_digest: Option<String>,
    duration_ms: u64,
    status: ReviewStatus,
    verdict: Option<ReviewVerdict>,
    confidence: Option<f64>,
    result_file: Option<String>,
    origin: ReviewOrigin,
    attempt: u32,
    failure_kind: Option<ReviewFailureKind>,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReviewStatus {
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReviewOrigin {
    Fresh,
    Resumed,
    Cache,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReviewFailureKind {
    ProviderTimeout,
    ProviderFailed,
    IdentityInvalid,
    CitationInvalid,
    SchemaInvalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[shallguard::enforces("REQ-REV-005")]
enum ReviewVerdict {
    Satisfied,
    Violated,
    InsufficientEvidence,
    NotImpacted,
}

impl ReviewVerdict {
    fn as_str(self) -> &'static str {
        match self {
            Self::Satisfied => "satisfied",
            Self::Violated => "violated",
            Self::InsufficientEvidence => "insufficient_evidence",
            Self::NotImpacted => "not_impacted",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewResult {
    schema: String,
    capsule_digest: String,
    requirement_id: String,
    verdict: ReviewVerdict,
    confidence: f64,
    clause_reviews: Vec<ClauseReview>,
    findings: Vec<ReviewFinding>,
    missing_evidence: Vec<String>,
    context_limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClauseReview {
    clause_id: String,
    verdict: ReviewVerdict,
    reason: String,
    citations: Vec<Citation>,
    counterexample: String,
    evidence_assessment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Citation {
    file: String,
    line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewFinding {
    severity: FindingSeverity,
    clause_id: String,
    category: FindingCategory,
    title: String,
    explanation: String,
    scenario: String,
    citations: Vec<Citation>,
    affected_outcome: String,
    suggested_verification: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FindingSeverity {
    Critical,
    High,
    Medium,
    Low,
    Note,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FindingCategory {
    Behavior,
    Safety,
    Compatibility,
    Evidence,
    Ambiguity,
    Scope,
}

struct PreparedReview {
    capsule_text: String,
    schema_text: String,
    prompt: String,
    prompt_digest: String,
    cache_key: String,
    metadata: CapsuleMetadata,
}

fn read_bundle_manifest(bundle_dir: &Path) -> Result<BundleManifest> {
    let path = bundle_dir.join("manifest.json");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading bundle manifest {}", path.display()))?;
    let mut manifest: BundleManifest = serde_json::from_str(&text)
        .with_context(|| format!("parsing bundle manifest {}", path.display()))?;
    if manifest.schema != MANIFEST_SCHEMA {
        bail!(
            "unsupported bundle schema {:?}; expected {MANIFEST_SCHEMA:?}",
            manifest.schema
        );
    }
    if manifest.protocol != REVIEW_PROTOCOL {
        bail!(
            "unsupported review protocol {:?}; expected {REVIEW_PROTOCOL:?}",
            manifest.protocol
        );
    }
    manifest.digest = digest(text.as_bytes());
    Ok(manifest)
}

fn select_entries(
    manifest: &BundleManifest,
    requirements: &BTreeSet<String>,
) -> Result<Vec<BundleEntry>> {
    let available = manifest
        .capsules
        .iter()
        .map(|entry| entry.requirement.as_str())
        .collect::<BTreeSet<_>>();
    let missing = requirements
        .iter()
        .filter(|requirement| !available.contains(requirement.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "requested requirement(s) absent from bundle: {}",
            missing.join(", ")
        );
    }
    let entries = manifest
        .capsules
        .iter()
        .filter(|entry| {
            requirements.is_empty() || requirements.contains(entry.requirement.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    if entries.is_empty() {
        bail!("review bundle contains no selected capsules");
    }
    Ok(entries)
}

fn hydrate_descriptions(bundle_dir: &Path, entries: &mut [BundleEntry]) -> Result<()> {
    for entry in entries {
        if !entry.description.is_empty() {
            continue;
        }
        validate_requirement_id(&entry.requirement)?;
        validate_bundle_file(&entry.file)?;
        let path = bundle_dir.join(&entry.file);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading review capsule {}", path.display()))?;
        crate::bundle::verify_capsule_digest(&text, &entry.digest)?;
        entry.description = crate::bundle::capsule_description(&text, &entry.requirement)?;
    }
    Ok(())
}

#[shallguard::enforces("REQ-REV-001")]
fn review_capsule(
    options: &ReviewOptions<'_>,
    entry: &BundleEntry,
    manifest: &BundleManifest,
    unit: ReviewUnitProgress<'_>,
    store: &ReviewStore<'_>,
) -> Result<ReviewEntry> {
    let prepared = prepare_review(options, entry, manifest, store)?;
    match store.checkpoint(entry, &prepared.cache_key, &prepared.metadata) {
        Reuse::Hit(review) => return Ok(review),
        Reuse::Miss => {}
        Reuse::Invalid(error) => review_reuse_invalid(options, unit, "checkpoint", &error),
    }
    match store.cache(&prepared.cache_key, &prepared.metadata) {
        Reuse::Hit(cached) => return materialize_cached_review(store, entry, &prepared, cached),
        Reuse::Miss => {}
        Reuse::Invalid(error) => review_reuse_invalid(options, unit, "cache entry", &error),
    }

    let attempt = store.start_attempt(entry)?;
    write_review_materials(&attempt, &prepared)?;
    if attempt.number > 1 {
        review_unit_retrying(options, unit, attempt.number);
    } else {
        review_unit_started(options, unit);
    }
    let invocation = invoke_provider(&ProviderInvocation {
        provider: options.provider,
        model: options.model,
        local_provider: options.local_provider,
        review_dir: &attempt.dir,
        schema: &prepared.schema_text,
        timeout: options.timeout,
        progress: options.progress,
        unit,
    })?;

    let failure = if invocation.timed_out {
        Some((
            ReviewFailureKind::ProviderTimeout,
            format!(
                "provider timed out after {} seconds",
                options.timeout.as_secs()
            ),
        ))
    } else if !invocation.status.is_some_and(|status| status.success()) {
        let status = invocation
            .status
            .map_or_else(|| "unknown".to_string(), |status| status.to_string());
        let detail = concise_provider_error(&invocation.stderr, &invocation.stdout);
        Some((
            ReviewFailureKind::ProviderFailed,
            format!("provider exited with {status}: {detail}"),
        ))
    } else if invocation.response.is_none() {
        Some((
            ReviewFailureKind::ProviderFailed,
            "provider completed without a response".to_string(),
        ))
    } else {
        None
    };
    if let Some((kind, error)) = failure {
        return record_failed_attempt(
            store,
            entry,
            &prepared,
            &attempt,
            invocation.duration_ms,
            kind,
            error,
        );
    }

    let response_text = invocation
        .response
        .as_deref()
        .context("BUG: successful provider invocation has no response")?;
    let response = match parse_provider_response(options.provider, response_text) {
        Ok(response) => match validate_response(response, &prepared.metadata) {
            Ok(response) => response,
            Err(error) => {
                let kind = validation_failure_kind(&error);
                return record_failed_attempt(
                    store,
                    entry,
                    &prepared,
                    &attempt,
                    invocation.duration_ms,
                    kind,
                    format!("invalid provider response: {error}"),
                );
            }
        },
        Err(error) => {
            return record_failed_attempt(
                store,
                entry,
                &prepared,
                &attempt,
                invocation.duration_ms,
                ReviewFailureKind::SchemaInvalid,
                format!("invalid provider response: {error:#}"),
            );
        }
    };
    let result_path = attempt.dir.join("result.json");
    write_json(&result_path, &response)?;
    let response_bytes = serde_json::to_vec(&response).context("serializing response digest")?;
    let response_digest = digest(&response_bytes);
    let review = ReviewEntry {
        requirement_id: entry.requirement.clone(),
        capsule_file: entry.file.clone(),
        capsule_digest: entry.digest.clone(),
        prompt_digest: prepared.prompt_digest,
        response_digest: Some(response_digest.clone()),
        duration_ms: invocation.duration_ms,
        status: ReviewStatus::Completed,
        verdict: Some(response.verdict),
        confidence: Some(response.confidence),
        result_file: Some(attempt.result_file.clone()),
        origin: ReviewOrigin::Fresh,
        attempt: attempt.number,
        failure_kind: None,
        error: None,
    };
    store.write_attempt(&attempt, &review)?;
    store.write_checkpoint(entry, &prepared.cache_key, &review)?;
    store.write_cache(
        &prepared.cache_key,
        &response,
        &response_digest,
        invocation.duration_ms,
    )?;
    Ok(review)
}

#[shallguard::enforces("REQ-SEC-003")]
fn prepare_review(
    options: &ReviewOptions<'_>,
    entry: &BundleEntry,
    manifest: &BundleManifest,
    store: &ReviewStore<'_>,
) -> Result<PreparedReview> {
    validate_requirement_id(&entry.requirement)?;
    validate_bundle_file(&entry.file)?;
    let source_path = options.bundle_dir.join(&entry.file);
    let capsule_text = std::fs::read_to_string(&source_path)
        .with_context(|| format!("reading review capsule {}", source_path.display()))?;
    let metadata = capsule_metadata(&capsule_text, entry)?;
    let schema = response_schema(&metadata);
    let schema_text =
        serde_json::to_string_pretty(&schema).context("serializing response schema")?;
    let prompt = review_prompt(&capsule_text, manifest, &metadata);
    let prompt_digest = digest(prompt.as_bytes());
    let response_schema_digest = digest(schema_text.as_bytes());
    let cache_key = store.cache_key(entry, &prompt_digest, &response_schema_digest)?;
    Ok(PreparedReview {
        capsule_text,
        schema_text,
        prompt,
        prompt_digest,
        cache_key,
        metadata,
    })
}

fn write_review_materials(attempt: &Attempt, prepared: &PreparedReview) -> Result<()> {
    std::fs::write(attempt.dir.join("capsule.json"), &prepared.capsule_text)
        .context("copying review capsule")?;
    std::fs::write(
        attempt.dir.join("response-schema.json"),
        &prepared.schema_text,
    )
    .context("writing response schema")?;
    std::fs::write(attempt.dir.join("prompt.txt"), &prepared.prompt)
        .context("writing review prompt")
}

#[shallguard::enforces("REQ-REV-008")]
fn materialize_cached_review(
    store: &ReviewStore<'_>,
    entry: &BundleEntry,
    prepared: &PreparedReview,
    cached: CachedUnit,
) -> Result<ReviewEntry> {
    let attempt = store.start_attempt(entry)?;
    write_review_materials(&attempt, prepared)?;
    write_json(&attempt.dir.join("result.json"), &cached.result)?;
    let review = ReviewEntry {
        requirement_id: entry.requirement.clone(),
        capsule_file: entry.file.clone(),
        capsule_digest: entry.digest.clone(),
        prompt_digest: prepared.prompt_digest.clone(),
        response_digest: Some(cached.response_digest),
        duration_ms: cached.duration_ms,
        status: ReviewStatus::Completed,
        verdict: Some(cached.result.verdict),
        confidence: Some(cached.result.confidence),
        result_file: Some(attempt.result_file.clone()),
        origin: ReviewOrigin::Cache,
        attempt: attempt.number,
        failure_kind: None,
        error: None,
    };
    store.write_attempt(&attempt, &review)?;
    store.write_checkpoint(entry, &prepared.cache_key, &review)?;
    Ok(review)
}

fn record_failed_attempt(
    store: &ReviewStore<'_>,
    entry: &BundleEntry,
    prepared: &PreparedReview,
    attempt: &Attempt,
    duration_ms: u64,
    kind: ReviewFailureKind,
    error: String,
) -> Result<ReviewEntry> {
    let review = ReviewEntry {
        requirement_id: entry.requirement.clone(),
        capsule_file: entry.file.clone(),
        capsule_digest: entry.digest.clone(),
        prompt_digest: prepared.prompt_digest.clone(),
        response_digest: None,
        duration_ms,
        status: ReviewStatus::Failed,
        verdict: None,
        confidence: None,
        result_file: None,
        origin: ReviewOrigin::Fresh,
        attempt: attempt.number,
        failure_kind: Some(kind),
        error: Some(error),
    };
    store.write_attempt(attempt, &review)?;
    Ok(review)
}

fn validation_failure_kind(error: &ReviewValidationError) -> ReviewFailureKind {
    match error {
        ReviewValidationError::Identity(_) => ReviewFailureKind::IdentityInvalid,
        ReviewValidationError::Citation(_) => ReviewFailureKind::CitationInvalid,
        ReviewValidationError::Schema(_) => ReviewFailureKind::SchemaInvalid,
    }
}

fn validate_requirement_id(requirement: &str) -> Result<()> {
    let safe = requirement.starts_with("REQ-")
        && requirement.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '-'
        });
    if !safe {
        bail!("unsafe requirement ID in bundle manifest: {requirement:?}");
    }
    Ok(())
}

fn validate_bundle_file(file: &str) -> Result<()> {
    let path = Path::new(file);
    let safe = path
        .parent()
        .is_some_and(|parent| parent.as_os_str().is_empty())
        && path
            .extension()
            .is_some_and(|extension| extension == "json");
    if !safe {
        bail!("unsafe capsule path in bundle manifest: {file:?}");
    }
    Ok(())
}

#[shallguard::enforces("REQ-REV-006")]
fn persist_review_artifact(output_dir: &Path, artifact: &ReviewRunArtifact) -> Result<()> {
    state::write_json_atomic(&output_dir.join("manifest.json"), artifact)?;
    state::write_atomic(
        &output_dir.join("summary.md"),
        render_summary(artifact).as_bytes(),
    )
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut json = serde_json::to_string_pretty(value)
        .with_context(|| format!("serializing {}", path.display()))?;
    json.push('\n');
    std::fs::write(path, json).with_context(|| format!("writing {}", path.display()))
}

fn digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("sha256:{digest:x}")
}

fn unix_timestamp() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")
        .map(|duration| duration.as_secs())
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "review_tests.rs"]
mod tests;
