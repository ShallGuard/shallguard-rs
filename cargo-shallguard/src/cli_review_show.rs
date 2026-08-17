//! Parsing and terminal presentation for stored local-review inspection.

use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use shallguard::review::{
    StoredCitation, StoredRequirementReview, StoredRequirementStatus, StoredReview,
    StoredReviewDetails,
};

use super::{COMMAND_NAME, cli_color};

pub(super) struct ReviewShowArgs {
    output: Option<PathBuf>,
    requirements: BTreeSet<String>,
}

#[shallguard::enforces("REQ-CLI-007", "REQ-CLI-008", "REQ-CLI-011")]
pub(super) fn parse_review_show_args(args: &[String]) -> Result<ReviewShowArgs> {
    let mut output = None;
    let mut requirements = BTreeSet::new();
    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index].as_str();
        index += 1;
        let value = match flag {
            "--output" | "--requirement" => args
                .get(index)
                .with_context(|| format!("{flag} requires a value"))?,
            unknown => bail!("unknown argument {unknown:?}; expected --output or --requirement"),
        };
        index += 1;
        match flag {
            "--output" => {
                if output.is_some() {
                    bail!("--output may be specified only once");
                }
                output = Some(PathBuf::from(value));
            }
            "--requirement" => {
                requirements.insert(value.clone());
            }
            _ => unreachable!("flag matched above"),
        }
    }
    Ok(ReviewShowArgs {
        output,
        requirements,
    })
}

#[shallguard::enforces("REQ-CLI-007", "REQ-CLI-008", "REQ-CLI-010", "REQ-CLI-011")]
pub(super) fn run(
    root: &Path,
    config: &shallguard::config::RepositoryConfig,
    args: &ReviewShowArgs,
) -> ExitCode {
    let output_dir = args
        .output
        .clone()
        .unwrap_or_else(|| root.join(config.review_dir()));
    match shallguard::review::inspect_stored_review(&output_dir, &args.requirements) {
        Ok(review) => {
            print!(
                "{}",
                render_stored_review(
                    &review,
                    !args.requirements.is_empty(),
                    cli_color::stdout_enabled(),
                )
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{COMMAND_NAME} review show failed: {error:#}");
            ExitCode::FAILURE
        }
    }
}

#[shallguard::enforces("REQ-CLI-007", "REQ-CLI-008", "REQ-CLI-009", "REQ-CLI-011")]
fn render_stored_review(review: &StoredReview, details: bool, color: bool) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "Review: {}", review.output_dir.display());
    let _ = writeln!(output, "Status: {}", review.status.as_str());
    let _ = writeln!(output, "Provider: {}", review.provider);
    let _ = writeln!(output, "Model: {}", review.model);
    let _ = writeln!(
        output,
        "Progress: {}/{} processed",
        review.processed_responses, review.selected_requirements
    );
    let summary = format!(
        "{} satisfied, {} violated, {} insufficient evidence, {} not impacted; {} unavailable or invalid",
        review.verdicts.satisfied,
        review.verdicts.violated,
        review.verdicts.insufficient_evidence,
        review.verdicts.not_impacted,
        review.unavailable_or_invalid,
    );
    let _ = writeln!(
        output,
        "Semantic verdicts: {}",
        cli_color::review_outcomes(&summary, color)
    );

    if details {
        for requirement in &review.requirements {
            let _ = writeln!(output);
            render_requirement_details(&mut output, requirement, color);
        }
    } else {
        let _ = writeln!(output, "\nRequirements:");
        for requirement in &review.requirements {
            render_requirement_summary(&mut output, requirement, color);
        }
    }
    output
}

fn render_requirement_summary(
    output: &mut String,
    requirement: &StoredRequirementReview,
    color: bool,
) {
    match &requirement.status {
        StoredRequirementStatus::Pending => {
            let _ = writeln!(output, "  {}  pending", requirement.requirement_id);
        }
        StoredRequirementStatus::Completed {
            verdict,
            confidence,
        } => {
            let verdict = cli_color::review_outcomes(verdict.as_str(), color);
            let _ = writeln!(
                output,
                "  {}  {}  confidence {:.2}",
                requirement.requirement_id, verdict, confidence
            );
        }
        StoredRequirementStatus::Unavailable { kind, .. } => {
            let _ = writeln!(
                output,
                "  {}  unavailable ({kind})",
                requirement.requirement_id
            );
        }
        StoredRequirementStatus::Invalid { kind, .. } => {
            let _ = writeln!(output, "  {}  invalid ({kind})", requirement.requirement_id);
        }
    }
}

fn render_requirement_details(
    output: &mut String,
    requirement: &StoredRequirementReview,
    color: bool,
) {
    let _ = writeln!(output, "Requirement: {}", requirement.requirement_id);
    if let Some(attempt) = requirement.attempt {
        let origin = requirement.origin.as_deref().unwrap_or("unknown");
        let duration = requirement.duration_ms.unwrap_or(0);
        let _ = writeln!(output, "Attempt: {attempt} ({origin}, {duration} ms)");
    }
    match &requirement.status {
        StoredRequirementStatus::Pending => {
            let _ = writeln!(output, "Status: pending");
        }
        StoredRequirementStatus::Completed {
            verdict,
            confidence,
        } => {
            let verdict = cli_color::review_outcomes(verdict.as_str(), color);
            let _ = writeln!(output, "Status: {verdict}");
            let _ = writeln!(output, "Confidence: {confidence:.2}");
            if let Some(path) = &requirement.result_path {
                let _ = writeln!(output, "Result: {}", path.display());
            }
            if let Some(details) = &requirement.details {
                render_details(output, details, color);
            }
        }
        StoredRequirementStatus::Unavailable { kind, error } => {
            let _ = writeln!(output, "Status: unavailable");
            let _ = writeln!(output, "Failure: {kind}");
            let _ = writeln!(output, "Error: {error}");
        }
        StoredRequirementStatus::Invalid { kind, error } => {
            let _ = writeln!(output, "Status: invalid");
            let _ = writeln!(output, "Failure: {kind}");
            let _ = writeln!(output, "Error: {error}");
        }
    }
}

fn render_details(output: &mut String, details: &StoredReviewDetails, color: bool) {
    let _ = writeln!(output, "Clauses:");
    for clause in &details.clause_reviews {
        let verdict = cli_color::review_outcomes(clause.verdict.as_str(), color);
        let _ = writeln!(output, "  {}: {verdict}", clause.clause_id);
        let _ = writeln!(output, "    Reason: {}", clause.reason);
        render_citations(output, "    Citations", &clause.citations);
        let _ = writeln!(output, "    Counterexample: {}", clause.counterexample);
        let _ = writeln!(
            output,
            "    Evidence assessment: {}",
            clause.evidence_assessment
        );
    }

    let _ = writeln!(output, "Findings:");
    if details.findings.is_empty() {
        let _ = writeln!(output, "  (none)");
    }
    for finding in &details.findings {
        let _ = writeln!(
            output,
            "  [{}] {} — {} ({})",
            finding.severity, finding.title, finding.clause_id, finding.category
        );
        let _ = writeln!(output, "    Explanation: {}", finding.explanation);
        let _ = writeln!(output, "    Scenario: {}", finding.scenario);
        render_citations(output, "    Citations", &finding.citations);
        let _ = writeln!(output, "    Affected outcome: {}", finding.affected_outcome);
        let _ = writeln!(
            output,
            "    Suggested verification: {}",
            finding.suggested_verification
        );
    }
    render_items(output, "Missing evidence", &details.missing_evidence);
    render_items(output, "Context limitations", &details.context_limitations);
}

fn render_citations(output: &mut String, heading: &str, citations: &[StoredCitation]) {
    let _ = writeln!(output, "{heading}:");
    if citations.is_empty() {
        let _ = writeln!(output, "      (none)");
    }
    for citation in citations {
        let _ = writeln!(output, "      {}:{}", citation.file, citation.line);
    }
}

fn render_items(output: &mut String, heading: &str, items: &[String]) {
    let _ = writeln!(output, "{heading}:");
    if items.is_empty() {
        let _ = writeln!(output, "  (none)");
    }
    for item in items {
        let _ = writeln!(output, "  - {item}");
    }
}

#[shallguard::enforces("REQ-REV-005")]
pub(super) fn review_outcome_summary(
    review: &shallguard::review::ReviewRun,
    color: bool,
) -> String {
    let summary = format!(
        "{} satisfied, {} violated, {} insufficient evidence, {} not impacted; {} unavailable or invalid",
        review.verdicts.satisfied,
        review.verdicts.violated,
        review.verdicts.insufficient_evidence,
        review.verdicts.not_impacted,
        review.failures,
    );
    cli_color::review_outcomes(&summary, color).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shallguard::review::{
        ReviewVerdictCounts, StoredClauseReview, StoredReviewRunStatus, StoredReviewVerdict,
    };

    fn completed_review() -> StoredReview {
        StoredReview {
            output_dir: PathBuf::from("review-output"),
            status: StoredReviewRunStatus::Completed,
            provider: "codex".to_string(),
            model: "configured-default".to_string(),
            selected_requirements: 1,
            processed_responses: 1,
            verdicts: ReviewVerdictCounts {
                satisfied: 0,
                violated: 1,
                insufficient_evidence: 0,
                not_impacted: 0,
            },
            unavailable_or_invalid: 0,
            requirements: vec![StoredRequirementReview {
                requirement_id: "REQ-ZZ-001".to_string(),
                status: StoredRequirementStatus::Completed {
                    verdict: StoredReviewVerdict::Violated,
                    confidence: 0.95,
                },
                attempt: Some(2),
                origin: Some("resumed".to_string()),
                duration_ms: Some(42),
                result_path: Some(PathBuf::from(
                    "review-output/units/REQ-ZZ-001/attempts/0002/result.json",
                )),
                details: Some(StoredReviewDetails {
                    clause_reviews: vec![StoredClauseReview {
                        clause_id: "REQ-ZZ-001/C1".to_string(),
                        verdict: StoredReviewVerdict::Violated,
                        reason: "direct invocation loses its operand".to_string(),
                        citations: vec![StoredCitation {
                            file: "src/main.rs".to_string(),
                            line: 42,
                        }],
                        counterexample: "invoke directly".to_string(),
                        evidence_assessment: "no end-to-end test".to_string(),
                    }],
                    findings: Vec::new(),
                    missing_evidence: vec!["direct invocation test".to_string()],
                    context_limitations: vec!["bounded capsule".to_string()],
                }),
            }],
        }
    }

    #[shallguard::verifies("REQ-CLI-007", "REQ-CLI-008", "REQ-CLI-011")]
    #[test]
    fn parses_show_filters_and_rejects_run_options() {
        let args = parse_review_show_args(&[
            "--output".to_string(),
            "stored-review".to_string(),
            "--requirement".to_string(),
            "REQ-ZZ-001".to_string(),
        ])
        .expect("show arguments parse");
        assert_eq!(args.output, Some(PathBuf::from("stored-review")));
        assert!(args.requirements.contains("REQ-ZZ-001"));
        assert!(parse_review_show_args(&["--resume".to_string()]).is_err());
    }

    #[shallguard::verifies("REQ-CLI-007", "REQ-CLI-008", "REQ-CLI-011")]
    #[test]
    fn renders_summary_and_requested_evidence_details() {
        let review = completed_review();
        let summary = render_stored_review(&review, false, false);
        assert!(summary.contains("Progress: 1/1 processed"));
        assert!(summary.contains("REQ-ZZ-001  violated  confidence 0.95"));

        let details = render_stored_review(&review, true, false);
        assert!(details.contains("Requirement: REQ-ZZ-001"));
        assert!(details.contains("REQ-ZZ-001/C1: violated"));
        assert!(details.contains("src/main.rs:42"));
        assert!(details.contains("Missing evidence:\n  - direct invocation test"));
        assert!(!details.contains('\u{1b}'));
    }
}
