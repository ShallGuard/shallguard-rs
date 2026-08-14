//! Human-readable progress messages for local model execution.

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;
use std::time::Duration;

use super::{
    BundleEntry, FindingSeverity, ReviewEntry, ReviewFailureKind, ReviewOptions, ReviewOrigin,
    ReviewResult, ReviewRunArtifact, ReviewStatus,
};
use crate::{ProgressCallback, clear_live_progress, report_live_progress, report_progress};

const LIVE_REFRESH_INTERVAL: Duration = Duration::from_millis(250);
const LOG_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
const SPINNER_FRAMES: [&str; 4] = ["|", "/", "-", "\\"];
const MAX_REPORTED_FINDINGS: usize = 3;
const MAX_REPORTED_MISSING_EVIDENCE: usize = 3;
const MAX_MODEL_TEXT_CHARS: usize = 160;

#[derive(Clone, Copy)]
pub(super) struct ReviewUnitProgress<'a> {
    current: usize,
    total: usize,
    requirement: &'a str,
    description: &'a str,
}

impl<'a> ReviewUnitProgress<'a> {
    pub(super) fn new(index: usize, total: usize, entry: &'a BundleEntry) -> Self {
        Self {
            current: index + 1,
            total,
            requirement: &entry.requirement,
            description: &entry.description,
        }
    }
}

impl fmt::Display for ReviewUnitProgress<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.requirement)?;
        if !self.description.is_empty() {
            write!(formatter, " - {}", self.description)?;
        }
        Ok(())
    }
}

pub(super) struct ProviderProgress<'a> {
    progress: Option<ProgressCallback>,
    provider: &'a str,
    unit: ReviewUnitProgress<'a>,
    next_refresh: Duration,
    next_heartbeat: Duration,
    frame: usize,
}

impl<'a> ProviderProgress<'a> {
    pub(super) fn new(
        progress: Option<ProgressCallback>,
        provider: &'a str,
        unit: ReviewUnitProgress<'a>,
    ) -> Self {
        Self {
            progress,
            provider,
            unit,
            next_refresh: Duration::ZERO,
            next_heartbeat: LOG_HEARTBEAT_INTERVAL,
            frame: 0,
        }
    }

    pub(super) fn update(&mut self, elapsed: Duration) {
        if elapsed < self.next_refresh {
            return;
        }
        let log_when_redirected = elapsed >= self.next_heartbeat;
        report_live_progress(
            self.progress,
            provider_status_message(
                self.provider,
                self.unit,
                elapsed,
                SPINNER_FRAMES[self.frame % SPINNER_FRAMES.len()],
            ),
            log_when_redirected,
        );
        self.frame += 1;
        self.next_refresh = elapsed + LIVE_REFRESH_INTERVAL;
        while elapsed >= self.next_heartbeat {
            self.next_heartbeat += LOG_HEARTBEAT_INTERVAL;
        }
    }
}

impl Drop for ProviderProgress<'_> {
    fn drop(&mut self) {
        clear_live_progress(self.progress);
    }
}

#[shallguard::enforces("REQ-CLI-003")]
fn provider_status_message(
    provider: &str,
    unit: ReviewUnitProgress<'_>,
    elapsed: Duration,
    spinner: &str,
) -> String {
    format!(
        "review: [{}/{}] [{spinner}] {provider} {:.0}s: {unit}",
        unit.current,
        unit.total,
        elapsed.as_secs_f64()
    )
}

pub(super) fn concise_provider_error(stderr: &str, stdout: &str) -> String {
    let text = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    let mut concise = text.lines().take(4).collect::<Vec<_>>().join(" ");
    if concise.len() > 500 {
        concise.truncate(500);
        concise.push_str("...");
    }
    if concise.is_empty() {
        "no diagnostic output".to_string()
    } else {
        concise
    }
}

pub(super) fn review_started(options: &ReviewOptions<'_>, capsule_count: usize) {
    report_progress(
        options.progress,
        format!(
            "review: checking {} CLI for {capsule_count} capsule(s)",
            options.provider.as_str()
        ),
    );
}

pub(super) fn review_unit_started(options: &ReviewOptions<'_>, unit: ReviewUnitProgress<'_>) {
    report_progress(
        options.progress,
        format!(
            "review: [{}/{}] submitting to {}: {unit}",
            unit.current,
            unit.total,
            options.provider.as_str(),
        ),
    );
}

pub(super) fn review_unit_retrying(
    options: &ReviewOptions<'_>,
    unit: ReviewUnitProgress<'_>,
    attempt: u32,
) {
    report_progress(
        options.progress,
        format!(
            "review: [{}/{}] retrying attempt {attempt} with {}: {unit}",
            unit.current,
            unit.total,
            options.provider.as_str(),
        ),
    );
}

pub(super) fn review_reuse_invalid(
    options: &ReviewOptions<'_>,
    unit: ReviewUnitProgress<'_>,
    source: &str,
    error: &str,
) {
    report_progress(
        options.progress,
        format!(
            "review: [{}/{}] ignoring invalid {source} for {unit}: {error}",
            unit.current, unit.total
        ),
    );
}

pub(super) fn review_unit_complete(
    options: &ReviewOptions<'_>,
    unit: ReviewUnitProgress<'_>,
    review: &ReviewEntry,
) {
    let outcome = match (review.status, review.verdict, review.origin) {
        (ReviewStatus::Completed, Some(verdict), ReviewOrigin::Fresh) => {
            verdict.as_str().to_string()
        }
        (ReviewStatus::Completed, Some(verdict), ReviewOrigin::Resumed) => {
            format!("resumed {}", verdict.as_str())
        }
        (ReviewStatus::Completed, Some(verdict), ReviewOrigin::Cache) => {
            format!("cache hit {}", verdict.as_str())
        }
        (ReviewStatus::Failed, None, _) => review.failure_kind.map_or_else(
            || "unavailable".to_string(),
            |kind| format!("unavailable ({})", failure_kind_name(kind)),
        ),
        _ => "invalid".to_string(),
    };
    let timing = if review.origin == ReviewOrigin::Fresh {
        format!(" in {:.1}s", review.duration_ms as f64 / 1000.0)
    } else {
        String::new()
    };
    report_progress(
        options.progress,
        format!(
            "review: [{}/{}] {outcome}{timing}: {unit}",
            unit.current, unit.total,
        ),
    );
}

pub(super) fn review_unit_result(
    options: &ReviewOptions<'_>,
    review: &ReviewEntry,
    result: Option<&ReviewResult>,
) {
    for line in review_result_progress_lines(options.output_dir, review, result) {
        report_progress(options.progress, line);
    }
}

pub(super) fn review_result_progress_lines(
    output_dir: &std::path::Path,
    review: &ReviewEntry,
    result: Option<&ReviewResult>,
) -> Vec<String> {
    let mut lines = Vec::new();
    match (review.status, result) {
        (ReviewStatus::Completed, Some(result)) => {
            lines.push(format!(
                "review:   result: confidence {:.2}; {} finding(s); {} missing evidence item(s); {} context limitation(s)",
                result.confidence,
                result.findings.len(),
                result.missing_evidence.len(),
                result.context_limitations.len(),
            ));
            for finding in result.findings.iter().take(MAX_REPORTED_FINDINGS) {
                let location = finding
                    .citations
                    .first()
                    .map_or_else(String::new, |citation| {
                        format!(" at {}:{}", citation.file, citation.line)
                    });
                lines.push(format!(
                    "review:   finding [{}]: {}{location}",
                    severity_name(&finding.severity),
                    concise_model_text(&finding.title),
                ));
            }
            append_omitted_count(
                &mut lines,
                "finding(s)",
                result.findings.len(),
                MAX_REPORTED_FINDINGS,
            );
            for missing in result
                .missing_evidence
                .iter()
                .take(MAX_REPORTED_MISSING_EVIDENCE)
            {
                lines.push(format!(
                    "review:   missing evidence: {}",
                    concise_model_text(missing)
                ));
            }
            append_omitted_count(
                &mut lines,
                "missing evidence item(s)",
                result.missing_evidence.len(),
                MAX_REPORTED_MISSING_EVIDENCE,
            );
        }
        (ReviewStatus::Failed, None) => {
            if let Some(error) = &review.error {
                lines.push(format!("review:   error: {}", concise_model_text(error)));
            }
        }
        _ => {
            lines.push("review:   result details are inconsistent; see attempt record".to_string())
        }
    }

    let details_file = review.result_file.clone().unwrap_or_else(|| {
        format!(
            "units/{}/attempts/{:04}/attempt.json",
            review.requirement_id, review.attempt
        )
    });
    lines.push(format!(
        "review:   details: {}",
        output_dir.join(details_file).display()
    ));
    lines
}

fn append_omitted_count(lines: &mut Vec<String>, label: &str, total: usize, displayed: usize) {
    if total > displayed {
        lines.push(format!(
            "review:   {} additional {label}; see details",
            total - displayed
        ));
    }
}

fn severity_name(severity: &FindingSeverity) -> &'static str {
    match severity {
        FindingSeverity::Critical => "critical",
        FindingSeverity::High => "high",
        FindingSeverity::Medium => "medium",
        FindingSeverity::Low => "low",
        FindingSeverity::Note => "note",
    }
}

fn concise_model_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len().min(MAX_MODEL_TEXT_CHARS));
    let mut output_chars = 0;
    let mut pending_space = false;
    let mut truncated = false;
    for character in value.chars() {
        if character.is_whitespace()
            || character.is_control()
            || matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            pending_space = !output.is_empty();
            continue;
        }
        let required = usize::from(pending_space) + 1;
        if output_chars + required > MAX_MODEL_TEXT_CHARS {
            truncated = true;
            break;
        }
        if pending_space {
            output.push(' ');
            output_chars += 1;
            pending_space = false;
        }
        output.push(character);
        output_chars += 1;
    }
    if output.is_empty() {
        return "(empty)".to_string();
    }
    if truncated {
        output.push_str("...");
    }
    output
}

pub(super) fn review_complete(options: &ReviewOptions<'_>, completed: usize, failures: usize) {
    report_progress(
        options.progress,
        format!("review: completed {completed} response(s); {failures} unavailable or invalid"),
    );
}

pub(super) fn render_summary(artifact: &ReviewRunArtifact) -> String {
    let completed = artifact
        .reviews
        .iter()
        .filter(|review| review.status == ReviewStatus::Completed)
        .count();
    let failed = artifact.reviews.len() - completed;
    let mut verdicts = BTreeMap::<&str, usize>::new();
    let mut origins = BTreeMap::<&str, usize>::new();
    let mut failure_kinds = BTreeMap::<&str, usize>::new();
    for review in &artifact.reviews {
        if let Some(verdict) = review.verdict {
            *verdicts.entry(verdict.as_str()).or_default() += 1;
        }
        *origins.entry(origin_name(review.origin)).or_default() += 1;
        if let Some(kind) = review.failure_kind {
            *failure_kinds.entry(failure_kind_name(kind)).or_default() += 1;
        }
    }
    let mut out = String::new();
    let _ = writeln!(out, "# Local requirement review\n");
    let _ = writeln!(out, "- Status: `{}`", artifact.status.as_str());
    let _ = writeln!(
        out,
        "- Progress: {}/{} requirement(s) processed",
        artifact.reviews.len(),
        artifact.selected_requirements
    );
    let _ = writeln!(out, "- Provider: `{}`", artifact.provider.as_str());
    let _ = writeln!(out, "- Model: `{}`", artifact.model);
    if let Some(local_provider) = &artifact.local_provider {
        let _ = writeln!(out, "- Inference: local `{local_provider}` backend");
    }
    let _ = writeln!(out, "- Completed: {completed}");
    let _ = writeln!(out, "- Unavailable or invalid: {failed}");
    if !verdicts.is_empty() {
        let _ = writeln!(out, "- Verdicts: {}", format_counts(&verdicts));
    }
    let _ = writeln!(out, "- Origins: {}", format_counts(&origins));
    if !failure_kinds.is_empty() {
        let _ = writeln!(out, "- Failure kinds: {}", format_counts(&failure_kinds));
    }
    let _ = writeln!(out, "\n## Requirements\n");
    for review in &artifact.reviews {
        match (review.status, review.verdict, review.confidence) {
            (ReviewStatus::Completed, Some(verdict), Some(confidence)) => {
                let _ = writeln!(
                    out,
                    "- `{}`: `{}` ({confidence:.2})",
                    review.requirement_id,
                    verdict.as_str(),
                );
            }
            (ReviewStatus::Failed, _, _) => {
                let kind = review
                    .failure_kind
                    .map(failure_kind_name)
                    .unwrap_or("unknown");
                let _ = writeln!(
                    out,
                    "- `{}`: review unavailable (`{kind}`)",
                    review.requirement_id
                );
            }
            _ => unreachable!("BUG: inconsistent local review entry"),
        }
    }
    out
}

fn format_counts(counts: &BTreeMap<&str, usize>) -> String {
    counts
        .iter()
        .map(|(name, count)| format!("`{name}` {count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn origin_name(origin: ReviewOrigin) -> &'static str {
    match origin {
        ReviewOrigin::Fresh => "fresh",
        ReviewOrigin::Resumed => "resumed",
        ReviewOrigin::Cache => "cache",
    }
}

fn failure_kind_name(kind: ReviewFailureKind) -> &'static str {
    match kind {
        ReviewFailureKind::ProviderTimeout => "provider_timeout",
        ReviewFailureKind::ProviderFailed => "provider_failed",
        ReviewFailureKind::IdentityInvalid => "identity_invalid",
        ReviewFailureKind::CitationInvalid => "citation_invalid",
        ReviewFailureKind::SchemaInvalid => "schema_invalid",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shallguard::verifies("REQ-CLI-003")]
    #[test]
    fn provider_status_includes_position_description_and_elapsed_time() {
        let entry = BundleEntry {
            requirement: "REQ-DYN-009".to_string(),
            description: "Every controller pass emits a sink split".to_string(),
            file: "REQ-DYN-009.json".to_string(),
            digest: "sha256:test".to_string(),
        };
        let unit = ReviewUnitProgress::new(0, 10, &entry);

        assert_eq!(
            provider_status_message("codex", unit, Duration::from_secs(15), "/"),
            "review: [1/10] [/] codex 15s: REQ-DYN-009 - Every controller pass emits a sink split"
        );
    }
}
