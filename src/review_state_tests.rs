use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use super::super::{
    ClauseReview, REVIEW_RESULT_SCHEMA, ReviewFailureKind, ReviewOrigin, ReviewVerdict,
};
use crate::bundle::MANIFEST_SCHEMA;

#[test]
fn safe_output_path_rejects_parent_and_absolute_paths() {
    let root = Path::new("output");

    assert_eq!(
        safe_output_path(root, "units/REQ-AA-001/result.json").expect("safe path"),
        root.join("units/REQ-AA-001/result.json")
    );
    assert!(safe_output_path(root, "../result.json").is_err());
    assert!(safe_output_path(root, "/tmp/result.json").is_err());
}

#[test]
fn atomic_json_replaces_complete_document() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("state.json");

    write_json_atomic(&path, &serde_json::json!({"generation": 1})).expect("first write");
    write_json_atomic(&path, &serde_json::json!({"generation": 2})).expect("second write");

    let value: serde_json::Value = read_json(&path).expect("state parses");
    assert_eq!(value["generation"], 2);
}

#[test]
fn compatible_resume_reuses_only_revalidated_completed_checkpoint() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let output = directory.path().join("output");
    let requirements = BTreeSet::new();
    let entry = entry();
    let manifest = manifest(&entry);
    let initial_options = options(&output, None, false, &requirements);
    let store = ReviewStore::open(
        &initial_options,
        &manifest,
        std::slice::from_ref(&entry),
        "codex 1",
    )
    .expect("new store opens");
    let result = valid_result();
    let response_digest = digest(&serde_json::to_vec(&result).expect("result serializes"));
    let attempt = store.start_attempt(&entry).expect("attempt starts");
    write_json_atomic(&attempt.dir.join("result.json"), &result).expect("result writes");
    let review = completed_review(&entry, &attempt, response_digest);
    store
        .write_attempt(&attempt, &review)
        .expect("attempt writes");
    store
        .write_checkpoint(&entry, "sha256:cache-key", &review)
        .expect("checkpoint writes");
    drop(store);

    let resume = options(&output, None, true, &requirements);
    let store = ReviewStore::open(&resume, &manifest, std::slice::from_ref(&entry), "codex 1")
        .expect("compatible resume opens");
    let resumed = match store.checkpoint(&entry, "sha256:cache-key", &metadata()) {
        Reuse::Hit(review) => review,
        Reuse::Miss => panic!("checkpoint unexpectedly missed"),
        Reuse::Invalid(error) => panic!("checkpoint unexpectedly invalid: {error}"),
    };
    assert_eq!(resumed.origin, ReviewOrigin::Resumed);
    assert_eq!(resumed.attempt, 1);
    assert_eq!(resumed.verdict, Some(ReviewVerdict::Satisfied));

    let checkpoint_path = output.join("units/REQ-ZZ-001/checkpoint.json");
    let mut checkpoint: UnitCheckpoint = read_json(&checkpoint_path).expect("checkpoint reads");
    checkpoint.review.verdict = Some(ReviewVerdict::Violated);
    write_json_atomic(&checkpoint_path, &checkpoint).expect("checkpoint is tampered");
    assert!(matches!(
        store.checkpoint(&entry, "sha256:cache-key", &metadata()),
        Reuse::Invalid(_)
    ));
}

#[test]
fn portable_cache_is_revalidated_before_reuse() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let cache = directory.path().join("cache");
    let first_output = directory.path().join("first");
    let second_output = directory.path().join("second");
    let requirements = BTreeSet::new();
    let entry = entry();
    let manifest = manifest(&entry);
    let first_options = options(&first_output, Some(&cache), false, &requirements);
    let first = ReviewStore::open(
        &first_options,
        &manifest,
        std::slice::from_ref(&entry),
        "codex 1",
    )
    .expect("first store opens");
    let result = valid_result();
    let response_digest = digest(&serde_json::to_vec(&result).expect("result serializes"));
    first
        .write_cache("sha256:aabb", &result, &response_digest, 123)
        .expect("cache writes");
    drop(first);

    let second_options = options(&second_output, Some(&cache), false, &requirements);
    let second = ReviewStore::open(
        &second_options,
        &manifest,
        std::slice::from_ref(&entry),
        "codex 1",
    )
    .expect("second store opens");
    match second.cache("sha256:aabb", &metadata()) {
        Reuse::Hit(cached) => assert_eq!(cached.result.verdict, ReviewVerdict::Satisfied),
        Reuse::Miss => panic!("cache unexpectedly missed"),
        Reuse::Invalid(error) => panic!("cache unexpectedly invalid: {error}"),
    }

    let cached_result = cache.join("v1/aa/aabb/result.json");
    let mut tampered = result;
    tampered.requirement_id = "REQ-ZZ-999".to_string();
    write_json_atomic(&cached_result, &tampered).expect("cache result is tampered");
    assert!(matches!(
        second.cache("sha256:aabb", &metadata()),
        Reuse::Invalid(_)
    ));
}

#[test]
fn incompatible_resume_is_rejected_before_work_starts() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let output = directory.path().join("output");
    let requirements = BTreeSet::new();
    let entry = entry();
    let manifest = manifest(&entry);
    let initial_options = options(&output, None, false, &requirements);
    let store = ReviewStore::open(
        &initial_options,
        &manifest,
        std::slice::from_ref(&entry),
        "codex 1",
    )
    .expect("new store opens");
    drop(store);

    let resume = options(&output, None, true, &requirements);
    let error = ReviewStore::open(&resume, &manifest, std::slice::from_ref(&entry), "codex 2")
        .err()
        .expect("changed CLI identity must fail");
    assert!(error.to_string().contains("does not match"));
}

#[test]
fn legacy_output_without_run_state_is_not_modified() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let output = directory.path().join("legacy-output");
    std::fs::create_dir(&output).expect("legacy output directory is created");
    std::fs::write(output.join("manifest.json"), "{}\n").expect("legacy manifest writes");
    let requirements = BTreeSet::new();
    let entry = entry();
    let manifest = manifest(&entry);
    let resume = options(&output, None, true, &requirements);

    let error = ReviewStore::open(&resume, &manifest, std::slice::from_ref(&entry), "codex 1")
        .err()
        .expect("legacy output must not resume");
    assert!(error.to_string().contains("has no run.json"));
    assert!(!output.join(".lock").exists());
    assert!(!output.join("units").exists());
}

fn entry() -> BundleEntry {
    BundleEntry {
        requirement: "REQ-ZZ-001".to_string(),
        description: "Test requirement".to_string(),
        file: "REQ-ZZ-001.json".to_string(),
        digest: "sha256:capsule".to_string(),
    }
}

fn manifest(entry: &BundleEntry) -> BundleManifest {
    BundleManifest {
        schema: MANIFEST_SCHEMA.to_string(),
        repository: "repository".to_string(),
        base_commit: "base".to_string(),
        head_commit: "head".to_string(),
        protocol: REVIEW_PROTOCOL.to_string(),
        capsules: vec![entry.clone()],
        digest: "sha256:manifest".to_string(),
    }
}

fn options<'a>(
    output: &'a Path,
    cache: Option<&'a Path>,
    resume: bool,
    requirements: &'a BTreeSet<String>,
) -> ReviewOptions<'a> {
    ReviewOptions {
        bundle_dir: Path::new("unused-bundle"),
        output_dir: output,
        provider: ReviewProvider::Codex,
        model: None,
        local_provider: None,
        requirements,
        timeout: Duration::from_secs(1),
        resume,
        cache_dir: cache,
        progress: None,
    }
}

fn metadata() -> CapsuleMetadata {
    CapsuleMetadata {
        requirement_id: "REQ-ZZ-001".to_string(),
        capsule_digest: "sha256:capsule".to_string(),
        clauses: BTreeSet::from(["REQ-ZZ-001/C1".to_string()]),
        citation_ranges: BTreeMap::new(),
    }
}

fn valid_result() -> ReviewResult {
    ReviewResult {
        schema: REVIEW_RESULT_SCHEMA.to_string(),
        capsule_digest: "sha256:capsule".to_string(),
        requirement_id: "REQ-ZZ-001".to_string(),
        verdict: ReviewVerdict::Satisfied,
        confidence: 0.9,
        clause_reviews: vec![ClauseReview {
            clause_id: "REQ-ZZ-001/C1".to_string(),
            verdict: ReviewVerdict::Satisfied,
            reason: "bounded evidence supports the clause".to_string(),
            citations: Vec::new(),
            counterexample: "a violating input".to_string(),
            evidence_assessment: "the evidence reaches the path".to_string(),
        }],
        findings: Vec::new(),
        missing_evidence: Vec::new(),
        context_limitations: Vec::new(),
    }
}

fn completed_review(
    entry: &BundleEntry,
    attempt: &Attempt,
    response_digest: String,
) -> ReviewEntry {
    ReviewEntry {
        requirement_id: entry.requirement.clone(),
        capsule_file: entry.file.clone(),
        capsule_digest: entry.digest.clone(),
        prompt_digest: "sha256:prompt".to_string(),
        response_digest: Some(response_digest),
        duration_ms: 100,
        status: ReviewStatus::Completed,
        verdict: Some(ReviewVerdict::Satisfied),
        confidence: Some(0.9),
        result_file: Some(attempt.result_file.clone()),
        origin: ReviewOrigin::Fresh,
        attempt: attempt.number,
        failure_kind: None::<ReviewFailureKind>,
        error: None,
    }
}
