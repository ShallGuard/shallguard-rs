use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::json;
use tempfile::TempDir;

use super::*;

#[derive(Clone, Copy)]
enum FixtureOutcome {
    Completed,
    ProviderFailed,
    IdentityInvalid,
}

fn valid_result(capsule_digest: &str) -> ReviewResult {
    ReviewResult {
        schema: REVIEW_RESULT_SCHEMA.to_string(),
        capsule_digest: capsule_digest.to_string(),
        requirement_id: "REQ-ZZ-001".to_string(),
        verdict: ReviewVerdict::Violated,
        confidence: 0.95,
        clause_reviews: vec![super::super::ClauseReview {
            clause_id: "REQ-ZZ-001/C1".to_string(),
            verdict: ReviewVerdict::Violated,
            reason: "The supplied path accepts a counterexample.".to_string(),
            citations: vec![super::super::Citation {
                file: "src/lib.rs".to_string(),
                line: 15,
            }],
            counterexample: "Invalid input is accepted.".to_string(),
            evidence_assessment: "No direct invocation test is supplied.".to_string(),
        }],
        findings: vec![super::super::ReviewFinding {
            severity: FindingSeverity::Medium,
            clause_id: "REQ-ZZ-001/C1".to_string(),
            category: FindingCategory::Evidence,
            title: "Direct behavior is unverified".to_string(),
            explanation: "The capsule has no direct invocation test.".to_string(),
            scenario: "Invoke the executable directly.".to_string(),
            citations: vec![super::super::Citation {
                file: "src/lib.rs".to_string(),
                line: 15,
            }],
            affected_outcome: "A direct operand may be consumed.".to_string(),
            suggested_verification: "Add an end-to-end direct invocation test.".to_string(),
        }],
        missing_evidence: vec!["Direct invocation trace".to_string()],
        context_limitations: vec!["Only capsule excerpts were supplied".to_string()],
    }
}

fn write_json(path: &Path, value: &impl Serialize) {
    let mut text = serde_json::to_string_pretty(value).expect("fixture serializes");
    text.push('\n');
    std::fs::write(path, text).expect("fixture writes");
}

fn write_fixture(outcome: FixtureOutcome, partial: bool) -> TempDir {
    let directory = tempfile::tempdir().expect("temporary review directory");
    let output = directory.path();
    let (capsule, capsule_digest) = crate::bundle::review_test_capsule_with_coverage();
    let prompt = b"bounded review prompt";
    let prompt_digest = digest(prompt);
    let attempt_dir = output.join("units/REQ-ZZ-001/attempts/0002");
    std::fs::create_dir_all(&attempt_dir).expect("attempt directory creates");
    std::fs::create_dir_all(output.join("units/REQ-ZZ-001/attempts/0001"))
        .expect("older attempt directory creates");
    std::fs::write(attempt_dir.join("capsule.json"), capsule).expect("capsule writes");
    std::fs::write(attempt_dir.join("prompt.txt"), prompt).expect("prompt writes");
    std::fs::write(
        output.join("units/REQ-ZZ-001/attempts/0001/ignored.txt"),
        "older attempt must not be selected",
    )
    .expect("older marker writes");

    let result = valid_result(&capsule_digest);
    let response_digest = digest(&serde_json::to_vec(&result).expect("result digest serializes"));
    let review = match outcome {
        FixtureOutcome::Completed => {
            write_json(&attempt_dir.join("result.json"), &result);
            ReviewEntry {
                requirement_id: "REQ-ZZ-001".to_string(),
                capsule_file: "REQ-ZZ-001.json".to_string(),
                capsule_digest: capsule_digest.clone(),
                prompt_digest,
                response_digest: Some(response_digest),
                duration_ms: 42,
                status: ReviewStatus::Completed,
                verdict: Some(ReviewVerdict::Violated),
                confidence: Some(0.95),
                result_file: Some("units/REQ-ZZ-001/attempts/0002/result.json".to_string()),
                origin: ReviewOrigin::Resumed,
                attempt: 2,
                failure_kind: None,
                error: None,
            }
        }
        FixtureOutcome::ProviderFailed | FixtureOutcome::IdentityInvalid => ReviewEntry {
            requirement_id: "REQ-ZZ-001".to_string(),
            capsule_file: "REQ-ZZ-001.json".to_string(),
            capsule_digest: capsule_digest.clone(),
            prompt_digest,
            response_digest: None,
            duration_ms: 42,
            status: ReviewStatus::Failed,
            verdict: None,
            confidence: None,
            result_file: None,
            origin: ReviewOrigin::Fresh,
            attempt: 2,
            failure_kind: Some(match outcome {
                FixtureOutcome::ProviderFailed => ReviewFailureKind::ProviderFailed,
                FixtureOutcome::IdentityInvalid => ReviewFailureKind::IdentityInvalid,
                FixtureOutcome::Completed => unreachable!("completed handled above"),
            }),
            error: Some(match outcome {
                FixtureOutcome::ProviderFailed => "provider exited with status 1".to_string(),
                FixtureOutcome::IdentityInvalid => {
                    "response requirement ID does not match capsule".to_string()
                }
                FixtureOutcome::Completed => unreachable!("completed handled above"),
            }),
        },
    };
    write_json(&attempt_dir.join("attempt.json"), &review);

    let mut units = vec![json!({
        "requirement": "REQ-ZZ-001",
        "capsule_digest": capsule_digest,
    })];
    if partial {
        units.push(json!({
            "requirement": "REQ-ZZ-002",
            "capsule_digest": digest(b"pending capsule"),
        }));
    }
    write_json(
        &output.join("run.json"),
        &json!({
            "schema": RUN_STATE_SCHEMA,
            "identity": {
                "protocol": REVIEW_PROTOCOL,
                "bundle_manifest_digest": digest(b"bundle manifest"),
                "provider": "codex",
                "model": "gpt-test",
                "local_provider": null,
                "cli_version": "cargo-shallguard 0.1.0",
                "units": units,
            },
            "started_unix_seconds": 1,
        }),
    );
    write_json(
        &output.join("manifest.json"),
        &json!({
            "schema": REVIEW_RUN_SCHEMA,
            "protocol": REVIEW_PROTOCOL,
            "status": if partial { "running" } else { "completed" },
            "selected_requirements": if partial { 2 } else { 1 },
            "provider": "codex",
            "model": "gpt-test",
            "local_provider": null,
            "cli_version": "cargo-shallguard 0.1.0",
            "bundle_schema": MANIFEST_SCHEMA,
            "repository": "fixture",
            "base_commit": "base",
            "head_commit": "head",
            "response_schema_digest": digest(REVIEW_RESULT_SCHEMA.as_bytes()),
            "started_unix_seconds": 1,
            "duration_ms": 42,
            "reviews": [review],
        }),
    );
    directory
}

fn snapshot_files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, snapshot: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for item in std::fs::read_dir(path).expect("fixture directory reads") {
            let item = item.expect("fixture entry reads");
            let item_path = item.path();
            if item.file_type().expect("fixture type reads").is_dir() {
                visit(root, &item_path, snapshot);
            } else {
                let relative = item_path
                    .strip_prefix(root)
                    .expect("fixture path is contained")
                    .to_path_buf();
                snapshot.insert(
                    relative,
                    std::fs::read(item_path).expect("fixture file reads"),
                );
            }
        }
    }

    let mut snapshot = BTreeMap::new();
    visit(root, root, &mut snapshot);
    snapshot
}

#[shallguard::verifies("REQ-CLI-007", "REQ-CLI-008", "REQ-CLI-009", "REQ-CLI-010")]
#[test]
fn reads_completed_current_attempt_and_preserves_the_artifact() {
    let directory = write_fixture(FixtureOutcome::Completed, false);
    let before = snapshot_files(directory.path());

    let review = inspect_stored_review(directory.path(), &BTreeSet::new())
        .expect("completed stored review validates");

    assert_eq!(review.status, StoredReviewRunStatus::Completed);
    assert_eq!(review.processed_responses, 1);
    assert_eq!(review.verdicts.violated, 1);
    assert_eq!(review.requirements[0].attempt, Some(2));
    assert!(matches!(
        review.requirements[0].status,
        StoredRequirementStatus::Completed {
            verdict: StoredReviewVerdict::Violated,
            confidence: 0.95,
        }
    ));
    assert_eq!(
        review.requirements[0]
            .details
            .as_ref()
            .expect("completed review has details")
            .findings[0]
            .severity,
        "medium"
    );
    assert_eq!(before, snapshot_files(directory.path()));
}

#[shallguard::verifies("REQ-CLI-008", "REQ-CLI-009", "REQ-CLI-011")]
#[test]
fn partial_run_lists_pending_units_and_checks_requested_ids() {
    let directory = write_fixture(FixtureOutcome::Completed, true);
    let review = inspect_stored_review(directory.path(), &BTreeSet::new())
        .expect("partial stored review validates");
    assert_eq!(review.status, StoredReviewRunStatus::Running);
    assert_eq!(review.processed_responses, 1);
    assert_eq!(review.selected_requirements, 2);
    assert!(matches!(
        review.requirements[1].status,
        StoredRequirementStatus::Pending
    ));

    let filtered = inspect_stored_review(
        directory.path(),
        &BTreeSet::from(["REQ-ZZ-002".to_string()]),
    )
    .expect("pending requirement filter validates");
    assert_eq!(filtered.requirements.len(), 1);
    assert_eq!(filtered.requirements[0].requirement_id, "REQ-ZZ-002");

    let error = inspect_stored_review(
        directory.path(),
        &BTreeSet::from(["REQ-ZZ-999".to_string()]),
    )
    .expect_err("absent requirement fails");
    assert!(error.to_string().contains("absent from stored review"));
}

#[shallguard::verifies("REQ-CLI-009", "REQ-CLI-011")]
#[test]
fn distinguishes_unavailable_and_invalid_attempts() {
    let unavailable = write_fixture(FixtureOutcome::ProviderFailed, false);
    let review = inspect_stored_review(unavailable.path(), &BTreeSet::new())
        .expect("provider failure artifact validates");
    assert!(matches!(
        review.requirements[0].status,
        StoredRequirementStatus::Unavailable { .. }
    ));

    let invalid = write_fixture(FixtureOutcome::IdentityInvalid, false);
    let review = inspect_stored_review(invalid.path(), &BTreeSet::new())
        .expect("validation failure artifact validates");
    assert!(matches!(
        review.requirements[0].status,
        StoredRequirementStatus::Invalid { .. }
    ));
}

#[shallguard::verifies("REQ-CLI-010", "REQ-CLI-011")]
#[test]
fn rejects_tampered_result_digest() {
    let directory = write_fixture(FixtureOutcome::Completed, false);
    let manifest_path = directory.path().join("manifest.json");
    let attempt_path = directory
        .path()
        .join("units/REQ-ZZ-001/attempts/0002/attempt.json");
    for path in [&manifest_path, &attempt_path] {
        let mut value: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(path).expect("fixture reads for tampering"),
        )
        .expect("fixture JSON parses for tampering");
        let review = if path == &manifest_path {
            &mut value["reviews"][0]
        } else {
            &mut value
        };
        review["response_digest"] = json!(digest(b"tampered response"));
        write_json(path, &value);
    }

    let error = inspect_stored_review(directory.path(), &BTreeSet::new())
        .expect_err("tampered digest fails");
    assert!(error.to_string().contains("response digest"));
}
