use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

use super::validation::CapsuleMetadata;
use super::*;
use crate::bundle::MANIFEST_SCHEMA;

fn metadata() -> CapsuleMetadata {
    CapsuleMetadata {
        requirement_id: "REQ-ZZ-001".to_string(),
        capsule_digest: "sha256:capsule".to_string(),
        clauses: BTreeSet::from(["REQ-ZZ-001/C1".to_string()]),
        citation_ranges: BTreeMap::from([("src/lib.rs".to_string(), vec![10..=20])]),
    }
}

fn valid_result() -> ReviewResult {
    ReviewResult {
        schema: REVIEW_RESULT_SCHEMA.to_string(),
        capsule_digest: "sha256:capsule".to_string(),
        requirement_id: "REQ-ZZ-001".to_string(),
        verdict: ReviewVerdict::Satisfied,
        confidence: 0.8,
        clause_reviews: vec![ClauseReview {
            clause_id: "REQ-ZZ-001/C1".to_string(),
            verdict: ReviewVerdict::Satisfied,
            reason: "The supplied path enforces the clause.".to_string(),
            citations: vec![Citation {
                file: "src/lib.rs".to_string(),
                line: 15,
            }],
            counterexample: "Invalid input would be accepted.".to_string(),
            evidence_assessment: "The supplied test exercises invalid input.".to_string(),
        }],
        findings: Vec::new(),
        missing_evidence: Vec::new(),
        context_limitations: Vec::new(),
    }
}

#[shallguard::verifies("REQ-REV-001", "REQ-REV-009")]
#[test]
fn parses_provider_names() {
    assert_eq!(
        "codex".parse::<ReviewProvider>().expect("codex parses"),
        ReviewProvider::Codex
    );
    assert_eq!(
        "claude".parse::<ReviewProvider>().expect("claude parses"),
        ReviewProvider::Claude
    );
    assert_eq!(
        "copilot".parse::<ReviewProvider>().expect("copilot parses"),
        ReviewProvider::Copilot
    );
    assert!("other".parse::<ReviewProvider>().is_err());
}

#[shallguard::verifies("REQ-REV-001", "REQ-REV-002", "REQ-SEC-003")]
#[test]
fn codex_command_is_ephemeral_and_read_only() {
    let spec = command_spec(
        ReviewProvider::Codex,
        Some("gpt-test"),
        Some("ollama"),
        "{}",
    );
    let args = spec
        .arguments
        .iter()
        .map(|argument| argument.to_string_lossy())
        .collect::<Vec<_>>();
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--sandbox", "read-only"])
    );
    assert!(args.iter().any(|argument| argument == "--ephemeral"));
    assert!(args.windows(2).any(|pair| pair == ["--model", "gpt-test"]));
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--local-provider", "ollama"])
    );
    assert!(args.iter().any(|argument| argument == "--oss"));
}

#[shallguard::verifies("REQ-REV-001", "REQ-REV-002", "REQ-SEC-003")]
#[test]
fn claude_command_disables_tools_and_sessions() {
    let spec = command_spec(ReviewProvider::Claude, None, None, "{}");
    let args = spec
        .arguments
        .iter()
        .map(|argument| argument.to_string_lossy())
        .collect::<Vec<_>>();
    assert!(args.windows(2).any(|pair| pair == ["--tools", ""]));
    assert!(
        args.iter()
            .any(|argument| argument == "--no-session-persistence")
    );
    assert!(args.iter().any(|argument| argument == "--safe-mode"));
}

#[shallguard::verifies("REQ-REV-009")]
#[test]
fn copilot_command_is_headless_and_toolless() {
    let spec = command_spec(ReviewProvider::Copilot, Some("gpt-test"), None, "{}");
    let args = spec
        .arguments
        .iter()
        .map(|argument| argument.to_string_lossy())
        .collect::<Vec<_>>();

    assert_eq!(spec.executable, "copilot");
    for expected in [
        "--silent",
        "--no-ask-user",
        "--no-color",
        "--no-custom-instructions",
        "--no-remote",
        "--no-remote-export",
        "--disable-builtin-mcps",
        "--deny-tool=shell",
        "--deny-tool=write",
        "--deny-tool=read",
        "--deny-tool=url",
        "--deny-tool=memory",
    ] {
        assert!(
            args.iter().any(|argument| argument == expected),
            "missing Copilot argument {expected}"
        );
    }
    assert!(args.windows(2).any(|pair| pair == ["--model", "gpt-test"]));

    let input = copilot_input("Frozen capsule prompt", "{\"type\":\"object\"}");
    assert!(input.starts_with("Frozen capsule prompt\n\n"));
    assert!(input.ends_with("Exact response JSON Schema:\n{\"type\":\"object\"}"));
}

#[shallguard::verifies("REQ-REV-002", "REQ-REV-009", "REQ-SEC-003")]
#[test]
fn provider_environment_excludes_unrelated_ci_secrets() {
    assert!(provider_environment_allowed(OsStr::new("PATH")));
    assert!(provider_environment_allowed(OsStr::new("OPENAI_API_KEY")));
    assert!(provider_environment_allowed(OsStr::new(
        "ANTHROPIC_API_KEY"
    )));
    assert!(provider_environment_allowed(OsStr::new(
        "COPILOT_GITHUB_TOKEN"
    )));
    assert!(!provider_environment_allowed(OsStr::new("GITHUB_TOKEN")));
    assert!(!provider_environment_allowed(OsStr::new("CI_JOB_TOKEN")));
    assert!(!provider_environment_allowed(OsStr::new(
        "PROD_REGISTRY_TOKEN"
    )));
    assert!(!provider_environment_allowed(OsStr::new("JIRA_API_TOKEN")));
}

#[shallguard::verifies("REQ-REV-004")]
#[test]
fn validates_complete_result_with_supplied_citation() {
    let result = validate_response(valid_result(), &metadata()).expect("result validates");
    assert_eq!(result.verdict, ReviewVerdict::Satisfied);
}

#[shallguard::verifies("REQ-REV-003")]
#[test]
fn response_schema_binds_capsule_and_requirement_identity_exactly() {
    let schema = response_schema(&metadata());

    assert_eq!(
        schema["properties"]["capsule_digest"]["enum"],
        json!(["sha256:capsule"])
    );
    assert_eq!(
        schema["properties"]["requirement_id"]["enum"],
        json!(["REQ-ZZ-001"])
    );
    assert_eq!(
        schema["properties"]["clause_reviews"]["items"]["properties"]["clause_id"]["enum"],
        json!(["REQ-ZZ-001/C1"])
    );
    assert_eq!(schema["properties"]["clause_reviews"]["minItems"], 1);
    assert_eq!(schema["properties"]["clause_reviews"]["maxItems"], 1);
    assert_eq!(schema["properties"]["confidence"]["minimum"], 0.0);
    assert_eq!(schema["properties"]["confidence"]["maximum"], 1.0);
}

#[test]
fn classifies_identity_and_citation_protocol_failures() {
    let mut identity = valid_result();
    identity.capsule_digest = "sha256:model-invented-text".to_string();
    assert!(matches!(
        validate_response(identity, &metadata()),
        Err(ReviewValidationError::Identity(_))
    ));

    let mut citation = valid_result();
    citation.clause_reviews[0].citations[0].line = 99;
    assert!(matches!(
        validate_response(citation, &metadata()),
        Err(ReviewValidationError::Citation(_))
    ));
}

#[shallguard::verifies("REQ-REV-004")]
#[test]
fn coverage_anchor_and_scope_are_citable_protocol_locations() {
    let (capsule, capsule_digest) = crate::bundle::review_test_capsule_with_coverage();
    let entry = BundleEntry {
        requirement: "REQ-ZZ-001".to_string(),
        description: "Retain evidence".to_string(),
        file: "REQ-ZZ-001.json".to_string(),
        digest: capsule_digest.clone(),
    };
    let metadata = capsule_metadata(&capsule, &entry).expect("capsule metadata parses");
    let ranges = metadata
        .citation_ranges
        .get("src/lib.rs")
        .expect("coverage file is citable");
    assert!(ranges.iter().any(|range| range.contains(&15)));
    assert!(ranges.iter().any(|range| range.contains(&42)));
    assert!(ranges.iter().any(|range| range.contains(&49)));

    let mut enforcement_result = valid_result();
    enforcement_result.capsule_digest = capsule_digest.clone();
    validate_response(enforcement_result, &metadata)
        .expect("enforcement source citation validates");

    let mut result = valid_result();
    result.capsule_digest = capsule_digest;
    result.clause_reviews[0].citations[0].line = 42;
    validate_response(result, &metadata).expect("coverage anchor citation validates");
}

#[shallguard::verifies("REQ-REV-004")]
#[test]
fn rejects_citation_outside_capsule() {
    let mut result = valid_result();
    result.clause_reviews[0].citations[0].line = 21;
    let error = validate_response(result, &metadata()).expect_err("citation outside capsule fails");
    assert!(error.to_string().contains("outside supplied capsule"));
}

#[shallguard::verifies("REQ-REV-003")]
#[test]
fn rejects_missing_clause_review() {
    let mut result = valid_result();
    result.clause_reviews.clear();
    let error = validate_response(result, &metadata()).expect_err("missing clause review fails");
    assert!(error.to_string().contains("every normative clause"));
}

#[test]
fn extracts_claude_structured_output() {
    let result = valid_result();
    let envelope = json!({ "structured_output": result });
    let parsed = parse_provider_response(ReviewProvider::Claude, &envelope.to_string())
        .expect("Claude envelope parses");
    assert_eq!(parsed.requirement_id, "REQ-ZZ-001");
}

#[shallguard::verifies("REQ-REV-009")]
#[test]
fn extracts_copilot_structured_output() {
    let result = valid_result();
    let parsed = parse_provider_response(ReviewProvider::Copilot, &result_to_json(&result))
        .expect("Copilot response parses");
    assert_eq!(parsed.requirement_id, "REQ-ZZ-001");
}

fn result_to_json(result: &ReviewResult) -> String {
    serde_json::to_string(result).expect("BUG: review result fixture serializes")
}

#[test]
fn completed_review_progress_reports_concise_findings_and_result_path() {
    let mut result = valid_result();
    result.findings.push(ReviewFinding {
        severity: FindingSeverity::High,
        clause_id: "REQ-ZZ-001/C1".to_string(),
        category: FindingCategory::Evidence,
        title: "Missing\ntransition \u{202e}coverage".to_string(),
        explanation: "The transition is not exercised.".to_string(),
        scenario: "State changes between passes.".to_string(),
        citations: vec![Citation {
            file: "src/lib.rs".to_string(),
            line: 15,
        }],
        affected_outcome: "Stale state can survive.".to_string(),
        suggested_verification: "Add a two-pass test.".to_string(),
    });
    result.missing_evidence = vec![
        "first item".to_string(),
        "second item".to_string(),
        "third item".to_string(),
        "fourth item".to_string(),
    ];
    result.context_limitations = vec!["Source excerpt is bounded.".to_string()];
    let review = completed_review_entry();

    let lines = progress::review_result_progress_lines(
        Path::new("target/requirement-local-review"),
        &review,
        Some(&result),
    );

    assert_eq!(
        lines[0],
        "review:   result: confidence 0.80; 1 finding(s); 4 missing evidence item(s); 1 context limitation(s)"
    );
    assert_eq!(
        lines[1],
        "review:   finding [high]: Missing transition coverage at src/lib.rs:15"
    );
    assert!(
        lines
            .iter()
            .any(|line| line == "review:   1 additional missing evidence item(s); see details")
    );
    assert_eq!(
        lines.last().expect("details line exists"),
        "review:   details: target/requirement-local-review/units/REQ-ZZ-001/attempts/0001/result.json"
    );
    assert!(
        lines
            .iter()
            .all(|line| !line.contains(['\n', '\u{1b}', '\u{202e}']))
    );
}

#[shallguard::verifies("REQ-REV-006")]
#[test]
fn aggregate_artifact_is_refreshed_from_running_to_completed() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let mut artifact = ReviewRunArtifact {
        schema: REVIEW_RUN_SCHEMA,
        protocol: REVIEW_PROTOCOL,
        status: ReviewRunStatus::Running,
        selected_requirements: 2,
        provider: ReviewProvider::Codex,
        model: "configured-default".to_string(),
        local_provider: None,
        cli_version: "codex-cli test".to_string(),
        bundle_schema: MANIFEST_SCHEMA.to_string(),
        repository: "workspace".to_string(),
        base_commit: "base".to_string(),
        head_commit: "head".to_string(),
        response_schema_digest: "sha256:schema".to_string(),
        started_unix_seconds: 1,
        duration_ms: 100,
        reviews: vec![completed_review_entry()],
    };

    persist_review_artifact(directory.path(), &artifact).expect("running artifact writes");
    let running: Value = serde_json::from_str(
        &std::fs::read_to_string(directory.path().join("manifest.json"))
            .expect("running manifest reads"),
    )
    .expect("running manifest parses");
    let running_summary = std::fs::read_to_string(directory.path().join("summary.md"))
        .expect("running summary reads");
    assert_eq!(running["status"], "running");
    assert_eq!(running["selected_requirements"], 2);
    assert_eq!(running["reviews"].as_array().map(Vec::len), Some(1));
    assert!(running_summary.contains("- Status: `running`"));
    assert!(running_summary.contains("- Progress: 1/2 requirement(s) processed"));

    let mut second = completed_review_entry();
    second.requirement_id = "REQ-ZZ-002".to_string();
    artifact.reviews.push(second);
    artifact.status = ReviewRunStatus::Completed;
    artifact.duration_ms = 200;
    persist_review_artifact(directory.path(), &artifact).expect("completed artifact writes");

    let completed: Value = serde_json::from_str(
        &std::fs::read_to_string(directory.path().join("manifest.json"))
            .expect("completed manifest reads"),
    )
    .expect("completed manifest parses");
    let completed_summary = std::fs::read_to_string(directory.path().join("summary.md"))
        .expect("completed summary reads");
    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["reviews"].as_array().map(Vec::len), Some(2));
    assert!(completed_summary.contains("- Status: `completed`"));
    assert!(completed_summary.contains("- Progress: 2/2 requirement(s) processed"));
}

fn completed_review_entry() -> ReviewEntry {
    ReviewEntry {
        requirement_id: "REQ-ZZ-001".to_string(),
        capsule_file: "REQ-ZZ-001.json".to_string(),
        capsule_digest: "sha256:capsule".to_string(),
        prompt_digest: "sha256:prompt".to_string(),
        response_digest: Some("sha256:response".to_string()),
        duration_ms: 100,
        status: ReviewStatus::Completed,
        verdict: Some(ReviewVerdict::Satisfied),
        confidence: Some(0.8),
        result_file: Some("units/REQ-ZZ-001/attempts/0001/result.json".to_string()),
        origin: ReviewOrigin::Fresh,
        attempt: 1,
        failure_kind: None,
        error: None,
    }
}

#[shallguard::verifies("REQ-REV-005")]
#[test]
fn semantic_verdict_counts_exclude_unavailable_reviews() {
    let mut reviews = Vec::new();
    for verdict in [
        ReviewVerdict::Satisfied,
        ReviewVerdict::Violated,
        ReviewVerdict::InsufficientEvidence,
        ReviewVerdict::InsufficientEvidence,
        ReviewVerdict::NotImpacted,
    ] {
        let mut review = completed_review_entry();
        review.verdict = Some(verdict);
        reviews.push(review);
    }
    let mut unavailable = completed_review_entry();
    unavailable.status = ReviewStatus::Failed;
    unavailable.verdict = None;
    reviews.push(unavailable);

    assert_eq!(
        review_verdict_counts(&reviews),
        ReviewVerdictCounts {
            satisfied: 1,
            violated: 1,
            insufficient_evidence: 2,
            not_impacted: 1,
        }
    );
}
