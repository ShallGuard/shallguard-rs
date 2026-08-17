use super::*;
use shallguard::review::{
    ReviewVerdictCounts, StoredClauseReview, StoredFinding, StoredReviewRunStatus,
    StoredReviewVerdict,
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
                findings: vec![StoredFinding {
                    severity: "medium".to_string(),
                    clause_id: "REQ-ZZ-001/C1".to_string(),
                    category: "evidence".to_string(),
                    title: "Direct behavior is unverified".to_string(),
                    explanation: "No direct invocation test is supplied.".to_string(),
                    scenario: "Invoke the executable directly.".to_string(),
                    citations: vec![StoredCitation {
                        file: "src/main.rs".to_string(),
                        line: 42,
                    }],
                    affected_outcome: "A direct operand may be consumed.".to_string(),
                    suggested_verification: "Add an end-to-end test.".to_string(),
                }],
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

#[shallguard::verifies("REQ-CLI-008", "REQ-CLI-011")]
#[test]
fn treats_positional_requirement_ids_as_filters() {
    let args = parse_review_show_args(&["REQ-ZZ-001".to_string(), "REQ-ZZ-002".to_string()])
        .expect("positional requirements parse");

    assert_eq!(
        args.requirements,
        BTreeSet::from(["REQ-ZZ-001".to_string(), "REQ-ZZ-002".to_string()])
    );
}

#[shallguard::verifies("REQ-CLI-007", "REQ-CLI-008", "REQ-CLI-011")]
#[test]
fn renders_summary_and_requested_evidence_details() {
    let review = completed_review();
    let summary = render_stored_review(&review, false, false);
    assert!(summary.contains("Progress: 1/1 processed"));
    assert!(summary.contains("REQ-ZZ-001  violated  confidence 0.95"));
    assert!(summary.contains("Model: configured-default\n\nProgress:"));

    let details = render_stored_review(&review, true, false);
    assert!(details.contains("Requirement: REQ-ZZ-001"));
    assert!(details.contains("REQ-ZZ-001/C1: violated"));
    assert!(details.contains("src/main.rs:42"));
    assert!(details.contains("Missing evidence:\n  - direct invocation test"));
    assert!(
        details.contains(
            "Result: review-output/units/REQ-ZZ-001/attempts/0002/result.json\n\nClauses:"
        )
    );
    assert!(details.contains("Evidence assessment: no end-to-end test\n\nFindings:"));
    assert!(
        details.contains("Suggested verification: Add an end-to-end test.\n\nMissing evidence:")
    );
    assert!(details.contains("direct invocation test\n\nContext limitations:"));
    assert!(!details.contains('\u{1b}'));
}

#[shallguard::verifies("REQ-CLI-003")]
#[test]
fn colors_review_sections_only_when_enabled() {
    let review = completed_review();

    let colored = render_stored_review(&review, true, true);
    assert!(colored.contains("\x1b[1;34mReview\x1b[0m"));
    assert!(colored.contains("\x1b[1mStatus\x1b[0m"));
    assert!(colored.contains("\x1b[35mREQ-ZZ-001\x1b[0m"));
    assert!(colored.contains("\x1b[31mviolated\x1b[0m"));
    assert!(colored.contains("\x1b[33mmedium\x1b[0m"));
    assert!(colored.contains("\x1b[36mreview-output\x1b[0m"));

    let plain = render_stored_review(&review, true, false);
    assert!(!plain.contains('\u{1b}'));
}
