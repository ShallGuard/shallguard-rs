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
                        strict_oracle: false,
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

fn strict_config(area: &str) -> RepositoryConfig {
    let mut config = config(None);
    config.areas.insert(
        area.to_string(),
        AreaConfig {
            label: "Test".to_string(),
            hard_enforcement: false,
            hard_verification: false,
            strict_oracle: true,
        },
    );
    config
}

#[shallguard::verifies("REQ-TRACE-013")]
#[test]
fn vacuous_evidence_flows_through_the_baseline_like_other_kinds() {
    let kind = GapKind::VacuousEvidence;
    // Baselined: grandfathered warning.
    let mut known = analysis(requirement("REQ-ZZ-001", "ZZ", false), Some(kind));
    let stats = apply_baseline(
        &mut known,
        &baseline("REQ-ZZ-001", kind),
        true,
        true,
        &config(None),
    );
    assert!(known.errors.is_empty());
    assert_eq!(stats.known, 1);
    // Unbaselined: new regression error.
    let mut fresh = analysis(requirement("REQ-ZZ-001", "ZZ", false), Some(kind));
    let stats = apply_baseline(
        &mut fresh,
        &Baseline::from_entries(Vec::new()),
        true,
        true,
        &config(None),
    );
    assert_eq!(stats.new, 1);
    assert!(fresh.errors[0].message.contains("new regression"));
    // Hard area: rejected like hard_verification.
    let mut hard = analysis(requirement("REQ-SAFE-999", "SAFE", false), Some(kind));
    apply_baseline(
        &mut hard,
        &baseline("REQ-SAFE-999", kind),
        true,
        true,
        &config(Some("SAFE")),
    );
    assert!(hard.errors.iter().any(|f| f.message.contains("forbidden")));
}

#[shallguard::verifies("REQ-TRACE-013")]
#[test]
fn weak_evidence_is_advisory_unless_strict_oracle() {
    let kind = GapKind::WeakEvidence;
    let mut advisory = analysis(requirement("REQ-ZZ-001", "ZZ", false), Some(kind));
    let stats = apply_baseline(
        &mut advisory,
        &Baseline::from_entries(Vec::new()),
        true,
        true,
        &config(None),
    );
    assert!(advisory.errors.is_empty());
    assert_eq!(stats.new, 0);
    assert!(advisory.warnings[0].message.contains("advisory"));

    let mut strict = analysis(requirement("REQ-ZZ-001", "ZZ", false), Some(kind));
    apply_baseline(
        &mut strict,
        &Baseline::from_entries(Vec::new()),
        true,
        true,
        &strict_config("ZZ"),
    );
    assert_eq!(strict.errors.len(), 1);
    assert!(strict.errors[0].message.contains("hard-area"));
}

#[shallguard::verifies("REQ-TRACE-013")]
#[test]
fn advisory_kinds_are_not_recorded_by_baseline_init() {
    let weak = analysis(
        requirement("REQ-ZZ-001", "ZZ", false),
        Some(GapKind::WeakEvidence),
    );
    assert!(baseline_entries(&weak).is_empty());
    let vacuous = analysis(
        requirement("REQ-ZZ-001", "ZZ", false),
        Some(GapKind::VacuousEvidence),
    );
    assert_eq!(baseline_entries(&vacuous).len(), 1);
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
