//! Terminal presentation of check results: grouped findings, the
//! per-area summary table, and the oracle-suppression listing.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::config::RepositoryConfig;
use crate::oracle::OracleClass;
use crate::scan::{Anchors, VerificationAnchor};

/// One finding, locatable in a file.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Workspace-relative file (document or source) the finding points at.
    pub file: String,
    /// 1-based line.
    pub line: usize,
    pub message: String,
}

pub struct Report {
    pub errors: Vec<Finding>,
    pub warnings: Vec<Finding>,
    pub summary: String,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.errors.is_empty()
    }

    /// Prints the summary, warnings grouped by file (file name shown
    /// once, up to `warning_limit` entries in total), and every error;
    /// returns [`Report::passed`].
    pub fn print(&self, warning_limit: usize) -> bool {
        print!("{}", self.summary);

        if !self.warnings.is_empty() {
            println!("\nwarnings ({}):", self.warnings.len());
            print_grouped(&self.warnings, warning_limit, false);
        }

        if !self.errors.is_empty() {
            eprintln!("\nerrors ({}):", self.errors.len());
            print_grouped(&self.errors, usize::MAX, true);
            return false;
        }

        println!("\ncargo shallguard: OK");
        true
    }
}

/// Prints findings grouped by file, the file name once per group,
/// entries ordered by line, stopping after `limit` entries in total.
pub(crate) fn print_grouped(findings: &[Finding], limit: usize, to_stderr: bool) {
    let mut by_file: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for finding in findings {
        by_file.entry(&finding.file).or_default().push(finding);
    }
    let mut printed = 0usize;
    let out = |line: String| {
        if to_stderr {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    };
    for (file, mut group) in by_file {
        if printed >= limit {
            break;
        }
        group.sort_by_key(|f| f.line);
        out(format!("  {file}:"));
        for finding in group {
            if printed >= limit {
                break;
            }
            out(format!("    {:>5}  {}", finding.line, finding.message));
            printed += 1;
        }
    }
    let rest = findings.len().saturating_sub(printed);
    if rest > 0 {
        out(format!("  ... and {rest} more"));
    }
}

#[derive(Default)]
pub(crate) struct AreaStats {
    pub(crate) total: usize,
    pub(crate) retired: usize,
    pub(crate) automated: usize,
    pub(crate) e2e: usize,
    pub(crate) review_only: usize,
    pub(crate) pending: usize,
    /// Requirements with at least one Enforced code path (anchor expected).
    pub(crate) anchorable: usize,
    /// ... of which the Enforced files actually carry the anchor.
    pub(crate) anchored: usize,
    /// Automated-evidence requirements with a matching test anchor.
    pub(crate) test_anchored: usize,
}

#[derive(Default)]
pub(crate) struct BaselineStats {
    pub(crate) known: usize,
    pub(crate) resolved: usize,
    pub(crate) new: usize,
}

pub(crate) fn render_summary(
    stats: &BTreeMap<String, AreaStats>,
    anchors: &Anchors,
    baseline: &BaselineStats,
    config: &RepositoryConfig,
) -> String {
    // Label column sized to the longest `Full Name (ACRONYM)` label.
    let label_width = stats
        .keys()
        .map(|area| config.area_label(area).chars().count())
        .max()
        .unwrap_or(0)
        .max("area".len());
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<label_width$} {:>5} {:>5} {:>4} {:>7} {:>8} {:>16} {:>13}",
        "area", "total", "test", "e2e", "review", "pending", "code-anchored", "test-anchored"
    );
    let mut totals = AreaStats::default();
    for (area, s) in stats {
        let _ = writeln!(
            out,
            "{:<label_width$} {:>5} {:>5} {:>4} {:>7} {:>8} {:>13}/{:<3} {:>9}/{:<3}",
            config.area_label(area),
            s.total,
            s.automated,
            s.e2e,
            s.review_only,
            s.pending,
            s.anchored,
            s.anchorable,
            s.test_anchored,
            s.automated
        );
        totals.total += s.total;
        totals.automated += s.automated;
        totals.e2e += s.e2e;
        totals.review_only += s.review_only;
        totals.pending += s.pending;
        totals.anchorable += s.anchorable;
        totals.anchored += s.anchored;
        totals.test_anchored += s.test_anchored;
    }
    let _ = writeln!(
        out,
        "{:<label_width$} {:>5} {:>5} {:>4} {:>7} {:>8} {:>13}/{:<3} {:>9}/{:<3}",
        "all",
        totals.total,
        totals.automated,
        totals.e2e,
        totals.review_only,
        totals.pending,
        totals.anchored,
        totals.anchorable,
        totals.test_anchored,
        totals.automated
    );
    let _ = writeln!(
        out,
        "anchors found in code: {} enforcement, {} verification \
         ({} textual reference(s))",
        anchors.enforcement.len(),
        anchors.verification.len(),
        anchors.references.values().map(Vec::len).sum::<usize>(),
    );
    // Suppression is visible, never silent: every opted-out oracle is
    // counted and listed.
    let suppressed: Vec<(&VerificationAnchor, &str)> = anchors
        .verification
        .iter()
        .filter_map(|anchor| match &anchor.oracle {
            OracleClass::Suppressed(class) => Some((anchor, class.as_str())),
            _ => None,
        })
        .collect();
    if !suppressed.is_empty() {
        let _ = writeln!(out, "oracle suppressions ({}):", suppressed.len());
        for (anchor, class) in suppressed {
            let _ = writeln!(
                out,
                "  {}:{} `{}` (oracle = \"{class}\") verifies {}",
                anchor.file.display(),
                anchor.line,
                anchor.test_fn,
                anchor.ids.join(", ")
            );
        }
    }
    let _ = writeln!(
        out,
        "traceability baseline: {} known gap(s), {} resolved/stale, {} new regression(s)",
        baseline.known, baseline.resolved, baseline.new
    );
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::*;
    use crate::config::{ArtifactConfig, ReviewConfig};

    fn config() -> RepositoryConfig {
        RepositoryConfig {
            schema: 1,
            minimum_requirements: 1,
            baseline: PathBuf::from(".shallguard/baseline.toml"),
            verify_outlier_threshold: 6,
            documents: Vec::new(),
            prefixes: BTreeMap::new(),
            areas: BTreeMap::new(),
            allow_missing_paths: Default::default(),
            artifacts: ArtifactConfig {
                root: PathBuf::from("target/shallguard"),
            },
            review: ReviewConfig::default(),
        }
    }

    // Anchored to REQ-TRACE-014 once the compile-time opt-out lands.
    #[test]
    fn suppressed_oracles_are_listed_in_the_summary() {
        let anchors = Anchors {
            references: HashMap::new(),
            enforcement: Vec::new(),
            verification: vec![VerificationAnchor {
                file: PathBuf::from("src/lib.rs"),
                line: 7,
                test_fn: "candidate".to_string(),
                inline_modules: Vec::new(),
                ids: vec!["REQ-ZZ-001".to_string()],
                oracle: OracleClass::Suppressed("compile".to_string()),
            }],
            invalid: Vec::new(),
        };
        let summary = render_summary(
            &BTreeMap::new(),
            &anchors,
            &BaselineStats::default(),
            &config(),
        );
        assert!(summary.contains("oracle suppressions (1):"));
        assert!(summary.contains("`candidate` (oracle = \"compile\") verifies REQ-ZZ-001"));
    }
}
