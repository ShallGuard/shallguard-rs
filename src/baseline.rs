//! Persistent exceptions for historical traceability gaps.
//!
//! A baseline entry identifies one gap on one requirement. It cannot
//! exempt hard-area policy. Changes to baselined requirements are
//! handled by merge-request impact analysis, not mutable fingerprints.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Oldest baseline schema this release reads.
pub const BASELINE_SCHEMA_MIN: u32 = 1;
/// Newest baseline schema this release reads and writes. Schema 2 adds
/// the vacuity gap kinds and records that the detector knows them.
pub const BASELINE_SCHEMA_MAX: u32 = 2;

/// A traceability dimension that may have historical debt.
#[shallguard::enforces("REQ-TRACE-013")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GapKind {
    EnforcementAnchor,
    VerificationAnchor,
    EvidenceCitation,
    /// Every ✅ citation resolves only to tests that cannot fail.
    VacuousEvidence,
    /// The best available evidence is structurally weak (e.g. bare
    /// `#[should_panic]`).
    WeakEvidence,
}

impl GapKind {
    /// The oldest detector and serialization schema that supports this kind.
    #[shallguard::enforces("REQ-BASE-006", "REQ-BASE-007")]
    pub(crate) fn minimum_schema(self) -> u32 {
        match self {
            Self::EnforcementAnchor | Self::VerificationAnchor | Self::EvidenceCitation => 1,
            Self::VacuousEvidence | Self::WeakEvidence => 2,
        }
    }
}

impl fmt::Display for GapKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::EnforcementAnchor => "enforcement-anchor",
            Self::VerificationAnchor => "verification-anchor",
            Self::EvidenceCitation => "evidence-citation",
            Self::VacuousEvidence => "vacuous-evidence",
            Self::WeakEvidence => "weak-evidence",
        };
        f.write_str(name)
    }
}

/// Stable identity of one requirement gap.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GapKey {
    pub requirement: String,
    pub kind: GapKind,
}

impl GapKey {
    pub fn new(requirement: impl Into<String>, kind: GapKind) -> Self {
        Self {
            requirement: requirement.into(),
            kind,
        }
    }
}

/// One committed exception for historical debt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[shallguard::enforces("REQ-BASE-001")]
pub struct BaselineEntry {
    pub requirement: String,
    pub kind: GapKind,
}

impl BaselineEntry {
    pub fn key(&self) -> GapKey {
        GapKey::new(self.requirement.clone(), self.kind)
    }
}

/// The complete committed baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    pub schema: u32,
    #[serde(default, rename = "gap")]
    pub gaps: Vec<BaselineEntry>,
}

impl Baseline {
    /// Creates a baseline recorded by the current detector.
    #[shallguard::enforces("REQ-BASE-007")]
    pub fn from_entries(gaps: Vec<BaselineEntry>) -> Self {
        let mut baseline = Self {
            schema: BASELINE_SCHEMA_MAX,
            gaps,
        };
        baseline.sort();
        baseline
    }

    pub fn load(root: &Path, relative_path: &Path) -> Result<Self> {
        let path = root.join(relative_path);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading traceability baseline {}", path.display()))?;
        Self::parse(&text, &path.display().to_string())
    }

    /// Parses and validates baseline content obtained from a Git
    /// revision or another non-filesystem source.
    #[shallguard::enforces("REQ-BASE-007")]
    pub fn parse(text: &str, source: &str) -> Result<Self> {
        // Validate the schema before full deserialization, so a newer
        // baseline fails with the version in the message instead of an
        // opaque unknown-variant error.
        #[derive(Deserialize)]
        struct SchemaProbe {
            schema: u32,
        }
        let probe: SchemaProbe = toml::from_str(text)
            .with_context(|| format!("parsing traceability baseline {source}"))?;
        if !(BASELINE_SCHEMA_MIN..=BASELINE_SCHEMA_MAX).contains(&probe.schema) {
            bail!(
                "unsupported traceability baseline schema {} in {} (this release supports {}..={})",
                probe.schema,
                source,
                BASELINE_SCHEMA_MIN,
                BASELINE_SCHEMA_MAX
            );
        }
        let mut baseline: Self = toml::from_str(text)
            .with_context(|| format!("parsing traceability baseline {source}"))?;
        if let Some(entry) = baseline
            .gaps
            .iter()
            .find(|entry| entry.kind.minimum_schema() > baseline.schema)
        {
            bail!(
                "traceability baseline entry {} ({}) requires schema {} but {} declares schema {}",
                entry.requirement,
                entry.kind,
                entry.kind.minimum_schema(),
                source,
                baseline.schema
            );
        }
        baseline.sort();
        Ok(baseline)
    }

    pub fn duplicate_keys(&self) -> BTreeSet<GapKey> {
        let mut seen = BTreeSet::new();
        let mut duplicates = BTreeSet::new();
        for entry in &self.gaps {
            let key = entry.key();
            if !seen.insert(key.clone()) {
                duplicates.insert(key);
            }
        }
        duplicates
    }

    /// Serializes the baseline without changing its recorded detector capability.
    #[shallguard::enforces("REQ-BASE-007")]
    pub fn render(&self) -> Result<String> {
        let mut sorted = self.clone();
        sorted.sort();
        let body = toml::to_string_pretty(&sorted).context("serializing traceability baseline")?;
        Ok(format!(
            "# Historical traceability debt. Do not add or refresh entries.\n\
             # `cargo shallguard baseline prune` only removes resolved debt.\n\n\
             {body}"
        ))
    }

    /// Creates the initial baseline without ever replacing an existing
    /// policy file.
    pub fn create_new(&self, root: &Path, relative_path: &Path) -> Result<PathBuf> {
        let path = root.join(relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating baseline directory {}", parent.display()))?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| {
                format!(
                    "creating initial baseline {} (refusing to overwrite an existing file)",
                    path.display()
                )
            })?;
        file.write_all(self.render()?.as_bytes())
            .with_context(|| format!("writing initial baseline {}", path.display()))?;
        Ok(path)
    }

    /// Replaces an existing baseline after a monotonic prune.
    pub fn write_pruned(&self, root: &Path, relative_path: &Path) -> Result<PathBuf> {
        let path = root.join(relative_path);
        std::fs::write(&path, self.render()?)
            .with_context(|| format!("writing pruned baseline {}", path.display()))?;
        Ok(path)
    }

    fn sort(&mut self) {
        self.gaps.sort_by_key(BaselineEntry::key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(requirement: &str, kind: GapKind) -> BaselineEntry {
        BaselineEntry {
            requirement: requirement.to_string(),
            kind,
        }
    }

    #[shallguard::verifies("REQ-BASE-001")]
    #[test]
    fn serialization_is_sorted_and_round_trips() {
        let baseline = Baseline::from_entries(vec![
            entry("REQ-ZZ-002", GapKind::VerificationAnchor),
            entry("REQ-AA-001", GapKind::EnforcementAnchor),
        ]);
        let rendered = baseline.render().expect("baseline renders");
        let aa = rendered.find("REQ-AA-001").expect("AA entry exists");
        let zz = rendered.find("REQ-ZZ-002").expect("ZZ entry exists");
        assert!(aa < zz);

        let parsed: Baseline = toml::from_str(&rendered).expect("rendered TOML parses");
        assert_eq!(parsed, baseline);
    }

    #[shallguard::verifies("REQ-BASE-007")]
    #[test]
    fn schema_is_validated_before_full_deserialization() {
        // A future schema fails by number, even when the body contains
        // gap kinds this release has never heard of.
        let text =
            "schema = 3\n\n[[gap]]\nrequirement = \"REQ-AA-001\"\nkind = \"quantum-evidence\"\n";
        let err = Baseline::parse(text, "test").expect_err("schema 3 must be rejected");
        let message = format!("{err:#}");
        assert!(message.contains("unsupported traceability baseline schema 3"));
        assert!(message.contains("1..=2"));
    }

    #[shallguard::verifies("REQ-BASE-007")]
    #[test]
    fn declared_schema_must_support_entry_kinds() {
        let text =
            "schema = 1\n\n[[gap]]\nrequirement = \"REQ-AA-001\"\nkind = \"vacuous-evidence\"\n";
        let error = Baseline::parse(text, "test").expect_err("schema 1 cannot encode vacuity");
        let message = format!("{error:#}");
        assert!(message.contains("vacuous-evidence"));
        assert!(message.contains("requires schema 2"));
        assert!(message.contains("declares schema 1"));
    }

    #[shallguard::verifies("REQ-BASE-007")]
    #[test]
    fn current_baseline_records_detector_capability() {
        let empty = Baseline::from_entries(Vec::new());
        assert_eq!(empty.schema, BASELINE_SCHEMA_MAX);

        let old_only =
            Baseline::from_entries(vec![entry("REQ-AA-001", GapKind::EnforcementAnchor)]);
        assert_eq!(old_only.schema, BASELINE_SCHEMA_MAX);

        let with_vacuous = Baseline::from_entries(vec![
            entry("REQ-AA-001", GapKind::EnforcementAnchor),
            entry("REQ-AA-002", GapKind::VacuousEvidence),
        ]);
        assert_eq!(with_vacuous.schema, BASELINE_SCHEMA_MAX);
        let rendered = with_vacuous.render().expect("renders");
        assert!(rendered.contains("schema = 2"));
        let parsed = Baseline::parse(&rendered, "test").expect("schema 2 reads back");
        assert_eq!(parsed, with_vacuous);
    }

    #[shallguard::verifies("REQ-BASE-007")]
    #[test]
    fn render_preserves_detector_capability() {
        let legacy = Baseline {
            schema: BASELINE_SCHEMA_MIN,
            gaps: vec![entry("REQ-AA-001", GapKind::EnforcementAnchor)],
        };
        let rendered = legacy.render().expect("legacy baseline renders");
        assert!(rendered.contains("schema = 1"));
        assert_eq!(
            Baseline::parse(&rendered, "test").expect("legacy baseline reads back"),
            legacy
        );
    }

    #[shallguard::verifies("REQ-TRACE-013")]
    #[test]
    fn evidence_gap_kinds_round_trip_through_baseline() {
        let baseline = Baseline::from_entries(vec![
            entry("REQ-AA-001", GapKind::VacuousEvidence),
            entry("REQ-AA-002", GapKind::WeakEvidence),
        ]);
        let rendered = baseline.render().expect("baseline renders");
        assert!(rendered.contains("vacuous-evidence"));
        assert!(rendered.contains("weak-evidence"));
        let parsed = Baseline::parse(&rendered, "test").expect("rendered TOML parses");
        assert_eq!(parsed, baseline);
    }

    #[test]
    fn duplicate_keys_are_rejected() {
        let first = entry("REQ-AA-001", GapKind::EvidenceCitation);
        let second = first.clone();
        let baseline = Baseline::from_entries(vec![first, second]);
        assert_eq!(
            baseline.duplicate_keys(),
            BTreeSet::from([GapKey::new("REQ-AA-001", GapKind::EvidenceCitation)])
        );
    }
}
