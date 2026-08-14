//! Capsule identity, citable-range extraction, and response validation.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use thiserror::Error;

use super::{BundleEntry, Citation, REVIEW_RESULT_SCHEMA, ReviewResult};
use crate::bundle::{CAPSULE_SCHEMA, is_supported_capsule_schema};

#[derive(Debug, Deserialize)]
struct Capsule {
    schema: String,
    requirement: CapsuleRequirement,
    implementation: CapsuleImplementation,
    evidence: CapsuleEvidence,
    provenance: CapsuleProvenance,
}

#[derive(Debug, Deserialize)]
struct CapsuleRequirement {
    id: String,
    document: String,
    line: usize,
    clauses: Vec<CapsuleClause>,
}

#[derive(Debug, Deserialize)]
struct CapsuleClause {
    id: String,
}

#[derive(Debug, Deserialize)]
struct CapsuleImplementation {
    changes: Vec<CapsuleSource>,
    #[serde(default)]
    enforcement: Vec<CapsuleEnforcement>,
}

#[derive(Debug, Deserialize)]
struct CapsuleEvidence {
    tests: Vec<CapsuleTest>,
    coverage: Option<CapsuleCoverage>,
}

#[derive(Debug, Deserialize)]
struct CapsuleSource {
    file: String,
    base: Option<CapsuleExcerpt>,
    head: Option<CapsuleExcerpt>,
}

#[derive(Debug, Deserialize)]
struct CapsuleEnforcement {
    file: String,
    head: Option<CapsuleExcerpt>,
}

#[derive(Debug, Deserialize)]
struct CapsuleTest {
    file: String,
    head: Option<CapsuleExcerpt>,
}

#[derive(Debug, Deserialize)]
struct CapsuleExcerpt {
    start_line: usize,
    end_line: usize,
}

#[derive(Debug, Deserialize)]
struct CapsuleCoverage {
    requirement: CapsuleCoverageRequirement,
}

#[derive(Debug, Deserialize)]
struct CapsuleCoverageRequirement {
    sites: Vec<CapsuleCoverageSite>,
}

#[derive(Debug, Deserialize)]
struct CapsuleCoverageSite {
    file: String,
    anchor_line: usize,
    scope: CapsuleExcerpt,
}

#[derive(Debug, Deserialize)]
struct CapsuleProvenance {
    digest: String,
}

pub(super) struct CapsuleMetadata {
    pub(super) requirement_id: String,
    pub(super) capsule_digest: String,
    pub(super) clauses: BTreeSet<String>,
    pub(super) citation_ranges: BTreeMap<String, Vec<RangeInclusive<usize>>>,
}

impl CapsuleMetadata {
    pub(super) fn citable_locations(&self) -> String {
        self.citation_ranges
            .iter()
            .flat_map(|(file, ranges)| {
                ranges
                    .iter()
                    .map(move |range| format!("- {file}:{}-{}", range.start(), range.end()))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Error)]
pub(super) enum ReviewValidationError {
    #[error("{0}")]
    Identity(String),
    #[error("{0}")]
    Citation(String),
    #[error("{0}")]
    Schema(String),
}

pub(super) fn capsule_metadata(text: &str, entry: &BundleEntry) -> Result<CapsuleMetadata> {
    crate::bundle::verify_capsule_digest(text, &entry.digest)?;
    let capsule: Capsule = serde_json::from_str(text).context("parsing review capsule")?;
    if !is_supported_capsule_schema(&capsule.schema) {
        bail!(
            "unsupported capsule schema {:?}; expected {CAPSULE_SCHEMA:?}",
            capsule.schema
        );
    }
    if capsule.requirement.id != entry.requirement {
        bail!(
            "capsule requirement {:?} does not match manifest requirement {:?}",
            capsule.requirement.id,
            entry.requirement
        );
    }
    if capsule.provenance.digest != entry.digest {
        bail!(
            "capsule digest {:?} does not match manifest digest {:?}",
            capsule.provenance.digest,
            entry.digest
        );
    }
    let clauses = capsule
        .requirement
        .clauses
        .into_iter()
        .map(|clause| clause.id)
        .collect::<BTreeSet<_>>();
    if clauses.is_empty() {
        bail!("capsule contains no normative clauses");
    }
    let mut ranges = BTreeMap::<String, Vec<RangeInclusive<usize>>>::new();
    add_range(
        &mut ranges,
        capsule.requirement.document,
        capsule.requirement.line,
        capsule.requirement.line,
    )?;
    for enforcement in capsule.implementation.enforcement {
        if let Some(excerpt) = enforcement.head {
            add_range(
                &mut ranges,
                enforcement.file,
                excerpt.start_line,
                excerpt.end_line,
            )?;
        }
    }
    for source in capsule.implementation.changes {
        if let Some(excerpt) = source.base {
            add_range(
                &mut ranges,
                source.file.clone(),
                excerpt.start_line,
                excerpt.end_line,
            )?;
        }
        if let Some(excerpt) = source.head {
            add_range(
                &mut ranges,
                source.file,
                excerpt.start_line,
                excerpt.end_line,
            )?;
        }
    }
    for test in capsule.evidence.tests {
        if let Some(excerpt) = test.head {
            add_range(&mut ranges, test.file, excerpt.start_line, excerpt.end_line)?;
        }
    }
    if let Some(coverage) = capsule.evidence.coverage {
        for site in coverage.requirement.sites {
            add_range(
                &mut ranges,
                site.file.clone(),
                site.anchor_line,
                site.anchor_line,
            )?;
            add_range(
                &mut ranges,
                site.file,
                site.scope.start_line,
                site.scope.end_line,
            )?;
        }
    }
    Ok(CapsuleMetadata {
        requirement_id: entry.requirement.clone(),
        capsule_digest: entry.digest.clone(),
        clauses,
        citation_ranges: ranges,
    })
}

fn add_range(
    ranges: &mut BTreeMap<String, Vec<RangeInclusive<usize>>>,
    file: String,
    start: usize,
    end: usize,
) -> Result<()> {
    if start == 0 || start > end {
        bail!("invalid supplied source range {file}:{start}-{end}");
    }
    ranges.entry(file).or_default().push(start..=end);
    Ok(())
}

#[shallguard_macros::enforces("REQ-REV-003", "REQ-SEC-005")]
pub(super) fn validate_response(
    result: ReviewResult,
    metadata: &CapsuleMetadata,
) -> Result<ReviewResult, ReviewValidationError> {
    if result.schema != REVIEW_RESULT_SCHEMA {
        return Err(ReviewValidationError::Schema(format!(
            "unsupported result schema {:?}; expected {REVIEW_RESULT_SCHEMA:?}",
            result.schema
        )));
    }
    if result.capsule_digest != metadata.capsule_digest {
        return Err(ReviewValidationError::Identity(
            "response capsule digest does not match submitted capsule".to_string(),
        ));
    }
    if result.requirement_id != metadata.requirement_id {
        return Err(ReviewValidationError::Identity(
            "response requirement ID does not match submitted capsule".to_string(),
        ));
    }
    if !(0.0..=1.0).contains(&result.confidence) {
        return Err(ReviewValidationError::Schema(
            "response confidence must be between 0 and 1".to_string(),
        ));
    }
    let reviewed_clauses = result
        .clause_reviews
        .iter()
        .map(|review| review.clause_id.as_str())
        .collect::<BTreeSet<_>>();
    let expected_clauses = metadata
        .clauses
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if reviewed_clauses != expected_clauses || result.clause_reviews.len() != expected_clauses.len()
    {
        return Err(ReviewValidationError::Schema(
            "response must review every normative clause exactly once".to_string(),
        ));
    }
    for review in &result.clause_reviews {
        validate_citations(&review.citations, metadata)?;
    }
    for finding in &result.findings {
        if !metadata.clauses.contains(&finding.clause_id) {
            return Err(ReviewValidationError::Schema(format!(
                "finding references unknown clause {:?}",
                finding.clause_id
            )));
        }
        if finding.citations.is_empty() {
            return Err(ReviewValidationError::Citation(format!(
                "finding {:?} has no supplied-source citation",
                finding.title
            )));
        }
        validate_citations(&finding.citations, metadata)?;
    }
    Ok(result)
}

#[shallguard_macros::enforces("REQ-REV-004", "REQ-SEC-002")]
fn validate_citations(
    citations: &[Citation],
    metadata: &CapsuleMetadata,
) -> Result<(), ReviewValidationError> {
    for citation in citations {
        let valid = metadata
            .citation_ranges
            .get(&citation.file)
            .is_some_and(|ranges| ranges.iter().any(|range| range.contains(&citation.line)));
        if !valid {
            return Err(ReviewValidationError::Citation(format!(
                "citation {}:{} is outside supplied capsule locations",
                citation.file, citation.line
            )));
        }
    }
    Ok(())
}
