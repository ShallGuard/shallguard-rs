use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::*;

fn target(package: &str, kind: TestTargetKind, name: &str) -> CargoTargetIdentity {
    CargoTargetIdentity {
        package: package.to_string(),
        target_kind: kind,
        target_name: name.to_string(),
    }
}

fn candidate(name: &str, syntactic_name: &str) -> SourceCandidate {
    SourceCandidate {
        file: "crate/src/tests.rs".to_string(),
        line: 10,
        function: name.to_string(),
        requirements: vec!["REQ-ZZ-001".to_string()],
        target: Ok(target("crate", TestTargetKind::Lib, "crate")),
        syntactic_name: Some(syntactic_name.to_string()),
    }
}

#[shallguard::verifies("REQ-TRACE-007")]
#[test]
fn merges_repeated_attributes_on_one_test() {
    let mut second = candidate("proves_it", "tests::proves_it");
    second.line = 11;
    second.requirements = vec!["REQ-ZZ-002".to_string(), "REQ-ZZ-001".to_string()];
    let merged = merge_candidates(vec![candidate("proves_it", "tests::proves_it"), second]);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].line, 10);
    assert_eq!(merged[0].requirements, ["REQ-ZZ-001", "REQ-ZZ-002"]);
}

#[shallguard::verifies("REQ-TEST-002")]
#[test]
fn parses_only_tests_and_benchmarks_from_harness_output() {
    let tests =
        parse_harness_list("module::one: test\nmodule::bench: benchmark\n2 tests, 0 benchmarks\n");
    assert_eq!(tests, ["module::bench", "module::one"]);
}

#[shallguard::verifies("REQ-TEST-003")]
#[test]
fn exact_syntactic_name_resolves_before_suffix_matching() {
    let key = target("crate", TestTargetKind::Lib, "crate");
    let catalog = BTreeMap::from([(
        key,
        vec![
            "other::proves_it".to_string(),
            "tests::proves_it".to_string(),
        ],
    )]);
    let mut findings = Vec::new();
    let resolved = resolve_candidate(
        candidate("proves_it", "tests::proves_it"),
        &catalog,
        &mut findings,
    );
    assert_eq!(resolved.status, ResolutionStatus::Resolved);
    assert_eq!(
        resolved.match_kind,
        Some(ResolutionMatch::ExactSyntacticName)
    );
    assert_eq!(
        resolved
            .identity
            .expect("resolved test has identity")
            .fully_qualified_name,
        "tests::proves_it"
    );
    assert!(findings.is_empty());
}

#[shallguard::verifies("REQ-TEST-003")]
#[test]
fn unique_function_suffix_is_accepted() {
    let key = target("crate", TestTargetKind::Lib, "crate");
    let catalog = BTreeMap::from([(key, vec!["actual::nested::proves_it".to_string()])]);
    let mut findings = Vec::new();
    let resolved = resolve_candidate(
        candidate("proves_it", "guessed::proves_it"),
        &catalog,
        &mut findings,
    );
    assert_eq!(resolved.status, ResolutionStatus::Resolved);
    assert_eq!(
        resolved.match_kind,
        Some(ResolutionMatch::UniqueFunctionSuffix)
    );
    assert!(findings.is_empty());
}

#[shallguard::verifies("REQ-TEST-003")]
#[test]
fn ambiguous_function_suffix_is_a_finding() {
    let key = target("crate", TestTargetKind::Lib, "crate");
    let catalog = BTreeMap::from([(
        key,
        vec!["one::proves_it".to_string(), "two::proves_it".to_string()],
    )]);
    let mut findings = Vec::new();
    let resolved = resolve_candidate(
        candidate("proves_it", "unknown::proves_it"),
        &catalog,
        &mut findings,
    );
    assert_eq!(resolved.status, ResolutionStatus::Ambiguous);
    assert_eq!(resolved.candidates.len(), 2);
    assert_eq!(findings[0].code, "harness-test-ambiguous");
}

#[shallguard::verifies("REQ-TEST-001")]
#[test]
fn maps_library_and_integration_source_targets() {
    let package = MetadataPackage {
        name: "crate".to_string(),
        manifest_path: PathBuf::from("/workspace/crate/Cargo.toml"),
        targets: vec![
            MetadataTarget {
                name: "crate".to_string(),
                kind: vec!["lib".to_string()],
                src_path: PathBuf::from("/workspace/crate/src/lib.rs"),
                test: true,
            },
            MetadataTarget {
                name: "basic".to_string(),
                kind: vec!["test".to_string()],
                src_path: PathBuf::from("/workspace/crate/tests/basic.rs"),
                test: true,
            },
        ],
    };
    assert_eq!(
        select_target(Path::new("/workspace/crate/src/router/tests.rs"), &package)
            .expect("library source resolves"),
        target("crate", TestTargetKind::Lib, "crate")
    );
    assert_eq!(
        select_target(Path::new("/workspace/crate/tests/basic.rs"), &package)
            .expect("integration source resolves"),
        target("crate", TestTargetKind::Integration, "basic")
    );
}

#[test]
fn builds_module_name_from_library_file_and_inline_modules() {
    let package = MetadataPackage {
        name: "crate".to_string(),
        manifest_path: PathBuf::from("/workspace/crate/Cargo.toml"),
        targets: vec![MetadataTarget {
            name: "crate".to_string(),
            kind: vec!["lib".to_string()],
            src_path: PathBuf::from("/workspace/crate/src/lib.rs"),
            test: true,
        }],
    };
    let anchor = VerificationAnchor {
        file: PathBuf::from("crate/src/router/config.rs"),
        line: 10,
        test_fn: "proves_it".to_string(),
        inline_modules: vec!["tests".to_string()],
        ids: vec!["REQ-ZZ-001".to_string()],
    };
    assert_eq!(
        static_test_name(
            Path::new("/workspace/crate/src/router/config.rs"),
            &package,
            &target("crate", TestTargetKind::Lib, "crate"),
            &anchor,
        ),
        Some("router::config::tests::proves_it".to_string())
    );
}

#[shallguard::verifies("REQ-TEST-005")]
#[test]
fn validates_requested_package_names() {
    let metadata = CargoMetadata {
        packages: vec![MetadataPackage {
            name: "known".to_string(),
            manifest_path: PathBuf::from("/workspace/known/Cargo.toml"),
            targets: Vec::new(),
        }],
    };
    let error = validate_package_filter(&metadata, &BTreeSet::from(["missing".to_string()]))
        .expect_err("unknown package fails");
    assert!(error.to_string().contains("missing"));
}
