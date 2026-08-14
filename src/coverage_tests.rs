use crate::coverage_llvm::ExecutableRegion;

use super::*;

#[test]
fn progress_requirement_ids_are_sorted_and_deduplicated() {
    let requirements = vec![
        "REQ-SR-031".to_string(),
        "REQ-DYN-009".to_string(),
        "REQ-SR-031".to_string(),
    ];

    assert_eq!(
        formatted_requirement_ids(&requirements),
        "REQ-DYN-009, REQ-SR-031"
    );
}

fn range(start_line: usize, end_line: usize) -> SourceRange {
    SourceRange {
        start_line,
        start_column: 1,
        end_line,
        end_column: 1,
    }
}

fn accumulator(kind: EnforcementScopeKind) -> RequirementAccumulator {
    let key = SiteKey {
        file: "crate/src/lib.rs".to_string(),
        scope_kind: kind,
        scope: Some(range(10, 20)),
    };
    RequirementAccumulator {
        id: "REQ-TEST-001".to_string(),
        area: "TEST".to_string(),
        title: "fixture".to_string(),
        tests: Vec::new(),
        sites: BTreeMap::from([(
            key.clone(),
            SiteAccumulator {
                key,
                anchor_line: 9,
                instrumented: BTreeSet::new(),
                covered: BTreeSet::new(),
                reached_by: BTreeSet::new(),
            },
        )]),
        test_failed: false,
        infrastructure_error: false,
    }
}

#[shallguard::verifies("REQ-COV-004")]
#[test]
fn source_range_intersection_is_half_open() {
    assert!(ranges_overlap(range(10, 20), range(19, 21)));
    assert!(!ranges_overlap(range(10, 20), range(20, 21)));
    assert!(!ranges_overlap(range(10, 20), range(1, 10)));
}

#[shallguard::verifies("REQ-COV-004")]
#[test]
fn covered_llvm_regions_reach_the_owning_enforcement_scope() {
    let mut requirement = accumulator(EnforcementScopeKind::FunctionBody);
    let regions = RegionIndex {
        by_file: BTreeMap::from([(
            "crate/src/lib.rs".to_string(),
            vec![
                ExecutableRegion {
                    range: range(12, 13),
                    execution_count: 2,
                },
                ExecutableRegion {
                    range: range(30, 31),
                    execution_count: 5,
                },
            ],
        )]),
    };

    apply_regions(&mut requirement, &regions, "crate:lib:crate:test");
    let result = requirement.finish();

    assert_eq!(result.status, CoverageStatus::Covered);
    assert_eq!(result.executable_sites.reached, 1);
    assert_eq!(result.sites[0].instrumented_regions, 1);
    assert_eq!(result.sites[0].covered_regions, 1);
    assert_eq!(result.sites[0].reached_by, ["crate:lib:crate:test"]);
}

#[shallguard::verifies("REQ-COV-005")]
#[test]
fn zero_count_region_is_instrumented_but_not_reached() {
    let mut requirement = accumulator(EnforcementScopeKind::Block);
    let regions = RegionIndex {
        by_file: BTreeMap::from([(
            "crate/src/lib.rs".to_string(),
            vec![ExecutableRegion {
                range: range(12, 13),
                execution_count: 0,
            }],
        )]),
    };

    apply_regions(&mut requirement, &regions, "crate:lib:crate:test");
    let result = requirement.finish();

    assert_eq!(result.status, CoverageStatus::NotReached);
    assert_eq!(result.executable_sites.instrumented, 1);
    assert_eq!(result.executable_sites.reached, 0);
}

#[shallguard::verifies("REQ-COV-005")]
#[test]
fn declarations_are_structural_only() {
    let result = accumulator(EnforcementScopeKind::Structural).finish();

    assert_eq!(result.status, CoverageStatus::StructuralOnly);
    assert_eq!(result.structural_sites, 1);
    assert_eq!(result.executable_sites.total, 0);
}

#[test]
fn initializer_without_an_llvm_region_is_structural() {
    let result = accumulator(EnforcementScopeKind::ConstInitializer).finish();

    assert_eq!(result.status, CoverageStatus::StructuralOnly);
    assert_eq!(result.structural_sites, 1);
    assert_eq!(result.executable_sites.total, 0);
}

#[shallguard::verifies("REQ-COV-005")]
#[test]
fn execution_errors_take_precedence_over_reach() {
    let mut requirement = accumulator(EnforcementScopeKind::FunctionBody);
    requirement.test_failed = true;

    assert_eq!(requirement.finish().status, CoverageStatus::TestFailed);
}
