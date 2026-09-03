//! Unit tests for the check cross-checks and baseline policy.

use super::*;
use crate::config::{AreaConfig, ArtifactConfig, ReviewConfig};

fn requirement(id: &str, area: &str, retired: bool) -> Requirement {
    Requirement {
        id: id.to_string(),
        area: area.to_string(),
        title: "Test requirement".to_string(),
        statement: "Test requirement SHALL hold.".to_string(),
        enforced_text: "src/lib.rs".to_string(),
        verified_text: "code review".to_string(),
        doc: "crate/docs/requirements.md".to_string(),
        line: 12,
        enforced_paths: Vec::new(),
        not_implemented: false,
        retired,
        automated: false,
        evidence: Vec::new(),
        e2e: false,
        review_only: true,
        pending: false,
    }
}

fn analysis(req: Requirement, kind: Option<GapKind>) -> Analysis {
    let mut gaps = BTreeMap::new();
    if let Some(kind) = kind {
        gaps.insert(
            GapKey::new(&req.id, kind),
            TraceabilityGap {
                area: req.area.clone(),
                findings: vec![Finding {
                    file: req.doc.clone(),
                    line: req.line,
                    message: "gap detail".to_string(),
                }],
            },
        );
    }
    Analysis {
        errors: Vec::new(),
        warnings: Vec::new(),
        requirements: vec![req],
        gaps,
        stats: BTreeMap::new(),
        anchors: Anchors {
            references: HashMap::new(),
            enforcement: Vec::new(),
            verification: Vec::new(),
            invalid: Vec::new(),
        },
    }
}

fn baseline(id: &str, kind: GapKind) -> Baseline {
    Baseline::from_entries(vec![BaselineEntry {
        requirement: id.to_string(),
        kind,
    }])
}

fn config(hard_area: Option<&str>) -> RepositoryConfig {
    RepositoryConfig {
        schema: 1,
        minimum_requirements: 1,
        baseline: PathBuf::from(".shallguard/baseline.toml"),
        verify_outlier_threshold: 6,
        documents: Vec::new(),
        prefixes: BTreeMap::new(),
        areas: hard_area
            .map(|area| {
                BTreeMap::from([(
                    area.to_string(),
                    AreaConfig {
                        label: "Test".to_string(),
                        hard_enforcement: true,
                        hard_verification: true,
                    },
                )])
            })
            .unwrap_or_default(),
        allow_missing_paths: Default::default(),
        artifacts: ArtifactConfig {
            root: PathBuf::from("target/shallguard"),
        },
        review: ReviewConfig::default(),
    }
}

#[shallguard::verifies("REQ-TRACE-006")]
#[test]
fn requires_an_anchor_in_every_documented_enforcement_file() {
    let anchored = Path::new("src/anchored.rs");
    let missing = Path::new("src/missing.rs");
    let files = HashSet::from([anchored]);

    assert!(enforced_path_has_anchor(anchored, Some(&files)));
    assert!(!enforced_path_has_anchor(missing, Some(&files)));
    assert!(!enforced_path_has_anchor(anchored, None));
}

#[shallguard::verifies("REQ-BASE-002")]
#[test]
fn exact_baseline_gap_is_known_warning() {
    let kind = GapKind::EnforcementAnchor;
    let mut analysis = analysis(requirement("REQ-ZZ-001", "ZZ", false), Some(kind));
    let stats = apply_baseline(
        &mut analysis,
        &baseline("REQ-ZZ-001", kind),
        true,
        true,
        &config(None),
    );
    assert!(analysis.errors.is_empty());
    assert_eq!(analysis.warnings.len(), 1);
    assert!(analysis.warnings[0].message.contains("grandfathered"));
    assert_eq!(stats.known, 1);
}

#[shallguard::verifies("REQ-BASE-002")]
#[test]
fn unbaselined_gap_is_a_regression() {
    let kind = GapKind::VerificationAnchor;
    let mut analysis = analysis(requirement("REQ-ZZ-001", "ZZ", false), Some(kind));
    let stats = apply_baseline(
        &mut analysis,
        &Baseline::from_entries(Vec::new()),
        true,
        true,
        &config(None),
    );
    assert_eq!(analysis.errors.len(), 1);
    assert!(analysis.errors[0].message.contains("new regression"));
    assert_eq!(stats.new, 1);
}

#[shallguard::verifies("REQ-BASE-004")]
#[test]
fn fixed_gap_makes_entry_stale() {
    let kind = GapKind::EvidenceCitation;
    let mut analysis = analysis(requirement("REQ-ZZ-001", "ZZ", false), None);
    let stats = apply_baseline(
        &mut analysis,
        &baseline("REQ-ZZ-001", kind),
        true,
        true,
        &config(None),
    );
    assert_eq!(analysis.errors.len(), 1);
    assert!(analysis.errors[0].message.contains("gap is resolved"));
    assert_eq!(stats.resolved, 1);
}

#[shallguard::verifies("REQ-BASE-004")]
#[test]
fn prune_mode_accepts_resolved_entry_for_removal() {
    let kind = GapKind::EvidenceCitation;
    let mut analysis = analysis(requirement("REQ-ZZ-001", "ZZ", true), None);
    let stats = apply_baseline(
        &mut analysis,
        &baseline("REQ-ZZ-001", kind),
        true,
        false,
        &config(None),
    );
    assert!(analysis.errors.is_empty());
    assert_eq!(stats.resolved, 1);
}

#[shallguard::verifies("REQ-BASE-003")]
#[test]
fn hard_area_cannot_be_baselined() {
    let kind = GapKind::EnforcementAnchor;
    let area = "SAFE";
    let mut analysis = analysis(requirement("REQ-SAFE-999", area, false), Some(kind));
    apply_baseline(
        &mut analysis,
        &baseline("REQ-SAFE-999", kind),
        true,
        true,
        &config(Some(area)),
    );
    assert!(analysis.errors.len() >= 2);
    assert!(
        analysis
            .errors
            .iter()
            .any(|finding| finding.message.contains("forbidden"))
    );
}
