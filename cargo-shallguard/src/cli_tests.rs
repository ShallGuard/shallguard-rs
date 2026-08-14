use super::*;
use shallguard_macros::verifies;

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[verifies("REQ-CLI-001")]
#[test]
fn removes_cargo_external_subcommand_argument() {
    assert_eq!(
        normalized_args(strings(&["shallguard", "coverage"])),
        strings(&["coverage"])
    );
    assert_eq!(
        normalized_args(strings(&["coverage"])),
        strings(&["coverage"])
    );
}

#[test]
fn parses_requirement_format_modes_and_documents() {
    let format = parse_format_args(
        &strings(&[
            "--check",
            "example-app/docs/USER_STORIES_AND_REQUIREMENTS.md",
        ]),
        false,
    )
    .expect("format arguments parse");
    assert!(format.check);
    assert_eq!(format.docs.len(), 1);

    let lint = parse_format_args(&[], true).expect("lint arguments parse");
    assert!(lint.check);
    assert!(lint.docs.is_empty());
}

#[test]
fn rejects_unknown_requirement_format_flags() {
    let error = parse_format_args(&strings(&["--write"]), false)
        .err()
        .expect("unknown format flag fails");
    assert!(error.to_string().contains("unknown argument"));
}

#[test]
fn parses_local_review_options() {
    let args = parse_review_args(&strings(&[
        "--provider",
        "codex",
        "--bundle",
        "review-input",
        "--output",
        "review-output",
        "--model",
        "gpt-test",
        "--local-provider",
        "ollama",
        "--requirement",
        "REQ-HRS-001",
        "--timeout-seconds",
        "45",
        "--resume",
        "--cache-dir",
        "review-cache",
    ]))
    .expect("review arguments parse");
    assert_eq!(
        args.provider,
        Some(shallguard::review::ReviewProvider::Codex)
    );
    assert!(args.base.is_none());
    assert_eq!(args.with_coverage, None);
    assert_eq!(args.bundle, Some(PathBuf::from("review-input")));
    assert_eq!(args.output, Some(PathBuf::from("review-output")));
    assert_eq!(args.model.as_deref(), Some("gpt-test"));
    assert_eq!(args.local_provider.as_deref(), Some("ollama"));
    assert!(args.requirements.contains("REQ-HRS-001"));
    assert_eq!(args.timeout, Some(Duration::from_secs(45)));
    assert!(args.resume);
    assert_eq!(args.cache_dir, Some(PathBuf::from("review-cache")));
}

#[verifies("REQ-CLI-002")]
#[test]
fn leaves_repository_review_defaults_for_configuration() {
    let args = parse_review_args(&[]).expect("default review arguments parse");

    assert_eq!(args.provider, None);
    assert!(args.base.is_none());
    assert_eq!(args.with_coverage, None);
    assert!(!args.resume);
    assert!(args.cache_dir.is_none());
    assert_eq!(args.bundle, None);
    assert_eq!(args.output, None);
    assert_eq!(args.timeout, None);
}

#[test]
fn resume_replays_the_frozen_default_bundle() {
    let args = parse_review_args(&strings(&["--resume"])).expect("resume review arguments parse");

    assert!(args.base.is_none());
    assert_eq!(args.with_coverage, None);
    assert_eq!(args.bundle, None);
    assert_eq!(args.output, None);
}

#[test]
fn parses_requested_one_command_review() {
    let args = parse_review_args(&strings(&[
        "--provider",
        "codex",
        "--base",
        "2810dced",
        "--with-coverage",
    ]))
    .expect("orchestrated review arguments parse");

    assert!(matches!(
        args.base,
        Some(CliBase::Revision(ref base)) if base == "2810dced"
    ));
    assert_eq!(args.with_coverage, Some(true));
}

#[test]
fn parses_explicit_impact_outputs() {
    let args = parse_impact_args(&strings(&[
        "--base",
        "abc123",
        "--json",
        "impact.json",
        "--markdown",
        "impact.md",
    ]))
    .expect("impact arguments parse");
    assert!(matches!(
        args.base,
        Some(CliBase::Revision(ref base)) if base == "abc123"
    ));
    assert_eq!(args.json, PathBuf::from("impact.json"));
    assert_eq!(args.markdown, Some(PathBuf::from("impact.md")));
}

#[test]
fn rejects_ambiguous_base_selection() {
    let error = parse_impact_args(&strings(&["--base", "abc123", "--target", "origin/main"]))
        .err()
        .expect("ambiguous selection fails");
    assert!(error.to_string().contains("exactly one"));
}

#[test]
fn parses_bundle_paths() {
    let args = parse_bundle_args(&strings(&[
        "--impact",
        "impact.json",
        "--coverage",
        "coverage.json",
        "--output",
        "review",
    ]))
    .expect("bundle arguments parse");
    assert_eq!(args.impact, PathBuf::from("impact.json"));
    assert_eq!(args.coverage, Some(PathBuf::from("coverage.json")));
    assert_eq!(args.output, Some(PathBuf::from("review")));
}

#[verifies("REQ-CLI-004")]
#[test]
fn leaves_bundle_output_for_repository_configuration() {
    let args = parse_bundle_args(&strings(&["--impact", "impact.json"]))
        .expect("default bundle arguments parse");

    assert_eq!(args.output, None);
}

#[test]
fn parses_enumerated_test_index_outputs_and_package_filter() {
    let args = parse_test_index_args(&strings(&[
        "--enumerate",
        "--package",
        "example-core",
        "--catalog-output",
        "catalog.json",
        "--json",
        "tests.json",
        "--markdown",
        "tests.md",
    ]))
    .expect("test-index arguments parse");
    assert!(matches!(args.harness, TestHarnessCli::Enumerate));
    assert!(args.packages.contains("example-core"));
    assert_eq!(args.catalog_output, Some(PathBuf::from("catalog.json")));
    assert_eq!(args.json, PathBuf::from("tests.json"));
    assert_eq!(args.markdown, Some(PathBuf::from("tests.md")));
}

#[test]
fn rejects_ambiguous_test_index_harness_source() {
    let error = parse_test_index_args(&strings(&["--enumerate", "--catalog", "catalog.json"]))
        .err()
        .expect("ambiguous harness source fails");
    assert!(error.to_string().contains("exactly one"));
}

#[test]
fn parses_filtered_coverage_outputs() {
    let args = parse_coverage_args(&strings(&[
        "--package",
        "example-core",
        "--requirement",
        "REQ-HRS-001",
        "--work-dir",
        "target/coverage-fixture",
        "--json",
        "coverage.json",
        "--markdown",
        "coverage.md",
    ]))
    .expect("coverage arguments parse");

    assert!(args.packages.contains("example-core"));
    assert!(args.requirements.contains("REQ-HRS-001"));
    assert_eq!(
        args.work_dir,
        Some(PathBuf::from("target/coverage-fixture"))
    );
    assert_eq!(args.json, PathBuf::from("coverage.json"));
    assert_eq!(args.markdown, Some(PathBuf::from("coverage.md")));
}
