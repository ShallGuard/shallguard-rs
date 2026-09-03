//! Interpretation of oracle classifications for one requirement's
//! verification anchors.

use crate::oracle::OracleClass;
use crate::scan::VerificationAnchor;

/// How the verification anchors of one automated requirement add up.
pub(crate) enum VerificationOutcome<'a> {
    /// No test anchor cites the requirement at all.
    Missing,
    /// Anchors exist, but every one of them is vacuous: the `[test]` claim is
    /// demoted to lacking automated verification.
    Demoted(Vec<&'a VerificationAnchor>),
    /// At least one anchor stands as evidence. Every weak anchor is
    /// reported regardless of stronger company (REQ-TRACE-012 is a
    /// per-test contract); `redundant_vacuous` lists vacuous anchors
    /// that other evidence makes non-fatal.
    Anchored {
        weak: Vec<&'a VerificationAnchor>,
        redundant_vacuous: Vec<&'a VerificationAnchor>,
    },
}

#[shallguard::enforces("REQ-TRACE-013")]
pub(crate) fn evaluate_verification<'a>(
    anchors: &[&'a VerificationAnchor],
) -> VerificationOutcome<'a> {
    if anchors.is_empty() {
        return VerificationOutcome::Missing;
    }
    let solid = anchors.iter().any(|anchor| {
        matches!(
            anchor.oracle,
            OracleClass::Present | OracleClass::Suppressed(_)
        )
    });
    let weak: Vec<&VerificationAnchor> = anchors
        .iter()
        .copied()
        .filter(|anchor| matches!(anchor.oracle, OracleClass::Weak(_)))
        .collect();
    let vacuous: Vec<&VerificationAnchor> = anchors
        .iter()
        .copied()
        .filter(|anchor| matches!(anchor.oracle, OracleClass::Vacuous(_)))
        .collect();
    if solid || !weak.is_empty() {
        VerificationOutcome::Anchored {
            weak,
            redundant_vacuous: vacuous,
        }
    } else {
        VerificationOutcome::Demoted(vacuous)
    }
}

pub(crate) fn vacuity_reason(anchor: &VerificationAnchor) -> &'static str {
    match &anchor.oracle {
        OracleClass::Vacuous(reason) => reason.describe(),
        _ => "contains no failure path",
    }
}

pub(crate) fn weak_reason(anchor: &VerificationAnchor) -> &'static str {
    match &anchor.oracle {
        OracleClass::Weak(reasons) => reasons
            .first()
            .map_or("offers only weak evidence", |reason| reason.describe()),
        _ => "offers only weak evidence",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::oracle::{VacuityReason, WeakReason};

    fn verification_anchor(oracle: OracleClass) -> VerificationAnchor {
        VerificationAnchor {
            file: PathBuf::from("src/lib.rs"),
            line: 7,
            test_fn: "candidate".to_string(),
            inline_modules: Vec::new(),
            ids: vec!["REQ-ZZ-001".to_string()],
            oracle,
        }
    }

    #[shallguard::verifies("REQ-TRACE-013")]
    #[test]
    fn sole_vacuous_evidence_demotes_the_requirement() {
        let vacuous = verification_anchor(OracleClass::Vacuous(VacuityReason::NoFailurePath));
        let outcome = evaluate_verification(&[&vacuous]);
        assert!(matches!(
            outcome,
            VerificationOutcome::Demoted(anchors) if anchors.len() == 1
        ));
        assert!(matches!(
            evaluate_verification(&[]),
            VerificationOutcome::Missing
        ));
    }

    #[shallguard::verifies("REQ-TRACE-013")]
    #[test]
    fn redundant_vacuous_evidence_keeps_the_requirement_anchored() {
        let vacuous =
            verification_anchor(OracleClass::Vacuous(VacuityReason::TrivialFailurePathsOnly));
        let present = verification_anchor(OracleClass::Present);
        match evaluate_verification(&[&present, &vacuous]) {
            VerificationOutcome::Anchored {
                weak,
                redundant_vacuous,
            } => {
                assert!(weak.is_empty());
                assert_eq!(redundant_vacuous.len(), 1);
            }
            _ => panic!("solid evidence must keep the requirement anchored"),
        }

        let weak_anchor = verification_anchor(OracleClass::Weak(vec![WeakReason::BareShouldPanic]));
        match evaluate_verification(&[&weak_anchor]) {
            VerificationOutcome::Anchored {
                weak,
                redundant_vacuous,
            } => {
                assert_eq!(weak.len(), 1);
                assert!(redundant_vacuous.is_empty());
            }
            _ => panic!("weak evidence still anchors the requirement"),
        }
    }

    #[shallguard::verifies("REQ-TRACE-012")]
    #[test]
    fn weak_anchors_are_reported_even_beside_solid_evidence() {
        // REQ-TRACE-012 is per-test: a bare #[should_panic] anchor stays
        // visible even while a stronger test coexists, so it cannot
        // silently become the sole evidence later.
        let present = verification_anchor(OracleClass::Present);
        let weak_anchor = verification_anchor(OracleClass::Weak(vec![WeakReason::BareShouldPanic]));
        match evaluate_verification(&[&present, &weak_anchor]) {
            VerificationOutcome::Anchored { weak, .. } => {
                assert_eq!(weak.len(), 1);
            }
            _ => panic!("solid evidence must keep the requirement anchored"),
        }
    }
}
