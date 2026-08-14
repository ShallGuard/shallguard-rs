//! The cross-checks and the report.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::baseline::{BASELINE_PATH, Baseline, BaselineEntry, GapKey, GapKind};
use crate::docs::{Requirement, parse_doc};
use crate::scan::{Anchors, scan};
use crate::{
    DocSpec, HARD_CODE_ANCHOR_AREAS, HARD_TEST_ANCHOR_AREAS, KNOWN_REMOVED_PATHS,
    VERIFY_OUTLIER_THRESHOLD, area_label,
};

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

/// Result of a monotonic baseline maintenance command.
pub struct BaselineChange {
    pub path: PathBuf,
    pub entries: usize,
    pub removed: usize,
}

#[derive(Default)]
struct BaselineStats {
    known: usize,
    resolved: usize,
    new: usize,
}

struct TraceabilityGap {
    area: String,
    findings: Vec<Finding>,
}

struct Analysis {
    errors: Vec<Finding>,
    warnings: Vec<Finding>,
    requirements: Vec<Requirement>,
    gaps: BTreeMap<GapKey, TraceabilityGap>,
    stats: BTreeMap<String, AreaStats>,
    anchors: Anchors,
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

        println!("\ncargo req-cov: OK");
        true
    }
}

/// Prints findings grouped by file, the file name once per group,
/// entries ordered by line, stopping after `limit` entries in total.
fn print_grouped(findings: &[Finding], limit: usize, to_stderr: bool) {
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

/// Runs every check, including committed-baseline policy, for the given
/// documents against the workspace at `root`.
pub fn run(root: &Path, docs: &[DocSpec]) -> Result<Report> {
    let mut analysis = analyze(root, docs)?;
    let baseline = Baseline::load(root)?;
    let baseline_stats = apply_baseline(
        &mut analysis,
        &baseline,
        covers_default_documents(docs),
        true,
    );
    let summary = render_summary(&analysis.stats, &analysis.anchors, &baseline_stats);
    Ok(Report {
        errors: analysis.errors,
        warnings: analysis.warnings,
        summary,
    })
}

/// Creates the one-time initial baseline. Existing baseline files are
/// never replaced.
pub fn initialize_baseline(root: &Path, docs: &[DocSpec]) -> Result<BaselineChange> {
    require_complete_scope(docs, "initialize")?;
    let analysis = analyze(root, docs)?;
    ensure_no_non_gap_errors(&analysis, "initialize")?;

    if let Some((key, _)) = analysis
        .gaps
        .iter()
        .find(|(key, gap)| gap_is_hard(key.kind, &gap.area))
    {
        bail!(
            "cannot initialize baseline: hard-area gap {} ({}) must be fixed",
            key.requirement,
            key.kind
        );
    }

    let entries = analysis
        .gaps
        .keys()
        .map(|key| BaselineEntry {
            requirement: key.requirement.clone(),
            kind: key.kind,
        })
        .collect();
    let baseline = Baseline::from_entries(entries);
    let count = baseline.gaps.len();
    let path = baseline.create_new(root)?;
    Ok(BaselineChange {
        path,
        entries: count,
        removed: 0,
    })
}

/// Removes only entries whose gaps are fixed or whose requirements are
/// retired. It never adds exceptions.
pub fn prune_baseline(root: &Path, docs: &[DocSpec]) -> Result<BaselineChange> {
    require_complete_scope(docs, "prune")?;
    let mut analysis = analyze(root, docs)?;
    let mut baseline = Baseline::load(root)?;
    let before = baseline.gaps.len();
    apply_baseline(&mut analysis, &baseline, true, false);
    ensure_no_non_gap_errors(&analysis, "prune")?;

    let current: HashSet<GapKey> = analysis.gaps.keys().cloned().collect();
    let retired: HashSet<&str> = analysis
        .requirements
        .iter()
        .filter(|req| req.retired)
        .map(|req| req.id.as_str())
        .collect();
    baseline.gaps.retain(|entry| {
        current.contains(&entry.key()) && !retired.contains(entry.requirement.as_str())
    });
    let removed = before - baseline.gaps.len();
    let path = if removed == 0 {
        root.join(BASELINE_PATH)
    } else {
        baseline.write_pruned(root)?
    };
    Ok(BaselineChange {
        path,
        entries: baseline.gaps.len(),
        removed,
    })
}

/// Detects invariant failures and traceability gaps without deciding
/// whether historical debt is allowed.
fn analyze(root: &Path, docs: &[DocSpec]) -> Result<Analysis> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let mut requirements: Vec<Requirement> = Vec::new();
    let mut path_spans: Vec<(String, usize, PathBuf)> = Vec::new();
    for spec in docs {
        let doc = parse_doc(root, spec)?;
        if doc.requirements.len() < 20 {
            errors.push(Finding {
                file: spec.path.clone(),
                line: 1,
                message: format!(
                    "parsed only {} requirements - document format drifted \
                     from what cargo req-cov understands",
                    doc.requirements.len()
                ),
            });
        }
        path_spans.extend(
            doc.path_spans
                .into_iter()
                .map(|(l, p)| (spec.path.clone(), l, p)),
        );
        requirements.extend(doc.requirements);
    }

    let scan_roots: Vec<String> = {
        let mut crates: Vec<&str> = docs.iter().map(|d| d.default_crate.as_str()).collect();
        crates.sort_unstable();
        crates.dedup();
        crates
            .iter()
            .flat_map(|c| [format!("{c}/src"), format!("{c}/tests")])
            .collect()
    };

    // Duplicate IDs across both documents.
    let mut by_id: HashMap<&str, &Requirement> = HashMap::new();
    for req in &requirements {
        if let Some(prev) = by_id.insert(&req.id, req) {
            errors.push(Finding {
                file: req.doc.clone(),
                line: req.line,
                message: format!(
                    "duplicate requirement ID {} (also at {}:{})",
                    req.id, prev.doc, prev.line
                ),
            });
        }
    }

    // Every code path cited anywhere in a document must exist.
    let mut seen_spans = HashSet::new();
    for (doc, line, path) in &path_spans {
        if KNOWN_REMOVED_PATHS.iter().any(|k| Path::new(k) == path) {
            continue;
        }
        if !root.join(path).exists() && seen_spans.insert(path.clone()) {
            errors.push(Finding {
                file: doc.clone(),
                line: *line,
                message: format!("cited path does not exist: {}", path.display()),
            });
        }
    }

    let scan_root_refs: Vec<&str> = scan_roots.iter().map(String::as_str).collect();
    let anchors = scan(root, &scan_root_refs)?;

    // Every requirement ID referenced in code must be defined; retired
    // requirements should not be referenced.
    let mut unknown: BTreeMap<&str, &(PathBuf, usize)> = BTreeMap::new();
    for (id, sites) in &anchors.references {
        let site = sites.first().expect("BUG: reference entry without site");
        match by_id.get(id.as_str()) {
            None => {
                unknown.insert(id, site);
            }
            Some(req) if req.retired => warnings.push(Finding {
                file: site.0.display().to_string(),
                line: site.1,
                message: format!("references retired requirement {id}"),
            }),
            Some(_) => {}
        }
    }
    for (id, (file, line)) in unknown {
        errors.push(Finding {
            file: file.display().to_string(),
            line: *line,
            message: format!("unknown requirement ID {id} (not defined in any document)"),
        });
    }

    // Structurally invalid anchors are always worth surfacing.
    for invalid in &anchors.invalid {
        warnings.push(Finding {
            file: invalid.file.display().to_string(),
            line: invalid.line,
            message: invalid.message.clone(),
        });
    }

    // Typed enforcement anchors: id -> files that carry one.
    let mut enforcing_files: HashMap<&str, HashSet<&Path>> = HashMap::new();
    for anchor in &anchors.enforcement {
        for id in &anchor.ids {
            enforcing_files
                .entry(id.as_str())
                .or_default()
                .insert(anchor.file.as_path());
        }
    }

    // A verification anchor for a requirement that does not claim ✅
    // means either the document or the anchor is stale.
    let mut stale_verified: BTreeMap<&str, (&Path, usize)> = BTreeMap::new();
    for anchor in &anchors.verification {
        for id in &anchor.ids {
            if let Some(req) = by_id.get(id.as_str())
                && !req.automated
                && !req.retired
            {
                stale_verified
                    .entry(id.as_str())
                    .or_insert((anchor.file.as_path(), anchor.line));
            }
        }
    }
    for (id, (file, line)) in stale_verified {
        warnings.push(Finding {
            file: file.display().to_string(),
            line,
            message: format!(
                "test anchor cites {id}, which does not claim ✅ automated \
                 evidence - the document or the anchor is stale"
            ),
        });
    }

    // Anchor presence, ratcheted per area.
    let mut stats: BTreeMap<String, AreaStats> = BTreeMap::new();
    let mut gaps: BTreeMap<GapKey, TraceabilityGap> = BTreeMap::new();
    let verified_ids: HashSet<&str> = anchors.verified_ids().collect();

    for req in &requirements {
        let stat = stats.entry(req.area.clone()).or_default();
        stat.total += 1;
        if req.retired {
            stat.retired += 1;
            continue;
        }
        if req.automated {
            stat.automated += 1;
        } else if req.e2e {
            stat.e2e += 1;
        } else if req.pending {
            stat.pending += 1;
        } else if req.review_only {
            stat.review_only += 1;
        }

        if !req.not_implemented && !req.enforced_paths.is_empty() {
            stat.anchorable += 1;
            let anchored = enforcing_files.get(req.id.as_str()).is_some_and(|files| {
                req.enforced_paths.iter().any(|enforced| {
                    files.iter().any(|file| {
                        if enforced.to_string_lossy().ends_with('/') {
                            file.starts_with(enforced)
                        } else {
                            *file == enforced.as_path()
                        }
                    })
                })
            });
            if anchored {
                stat.anchored += 1;
            } else {
                let finding = Finding {
                    file: req.doc.to_string(),
                    line: req.line,
                    message: format!(
                        "{} \"{}\" - no enforcement anchor (#[enforces] or \
                         enforces_here!) in its Enforced file(s)",
                        req.id, req.title
                    ),
                };
                record_gap(&mut gaps, req, GapKind::EnforcementAnchor, finding);
            }
        }

        if req.automated {
            if verified_ids.contains(req.id.as_str()) {
                stat.test_anchored += 1;
            } else {
                record_gap(
                    &mut gaps,
                    req,
                    GapKind::VerificationAnchor,
                    Finding {
                        file: req.doc.to_string(),
                        line: req.line,
                        message: format!(
                            "{} \"{}\" - claims automated-test evidence but no \
                             test anchor verifies it",
                            req.id, req.title
                        ),
                    },
                );
            }

            // Evidence binding: the Verified line must cite the concrete
            // test file(s), and every citation must resolve to a matching
            // verification anchor.
            if req.evidence.is_empty() {
                record_gap(
                    &mut gaps,
                    req,
                    GapKind::EvidenceCitation,
                    Finding {
                        file: req.doc.to_string(),
                        line: req.line,
                        message: format!(
                            "{} \"{}\" - claims ✅ but cites no concrete test \
                             file in its Verified line",
                            req.id, req.title
                        ),
                    },
                );
            }
            for evidence in &req.evidence {
                let resolved = anchors.verification.iter().any(|anchor| {
                    anchor.ids.contains(&req.id)
                        && anchor.file == evidence.file
                        && evidence
                            .test_fn
                            .as_ref()
                            .is_none_or(|name| *name == anchor.test_fn)
                });
                if !resolved {
                    let cited = match &evidence.test_fn {
                        Some(name) => format!("{} ({name})", evidence.file.display()),
                        None => evidence.file.display().to_string(),
                    };
                    record_gap(
                        &mut gaps,
                        req,
                        GapKind::EvidenceCitation,
                        Finding {
                            file: req.doc.to_string(),
                            line: req.line,
                            message: format!(
                                "{} - cited evidence {cited} does not resolve to \
                                 a #[verifies] anchor for this requirement",
                                req.id
                            ),
                        },
                    );
                } else if evidence.test_fn.is_none() {
                    warnings.push(Finding {
                        file: req.doc.to_string(),
                        line: req.line,
                        message: format!(
                            "{} - evidence cites {} but names no test function",
                            req.id,
                            evidence.file.display()
                        ),
                    });
                }
            }
        }
    }

    // Suspiciously broad test-evidence anchors.
    for anchor in &anchors.verification {
        if anchor.ids.len() >= VERIFY_OUTLIER_THRESHOLD {
            warnings.push(Finding {
                file: anchor.file.display().to_string(),
                line: anchor.line,
                message: format!(
                    "`{}` claims {} requirements - review whether the claim \
                     is that broad",
                    anchor.test_fn,
                    anchor.ids.len()
                ),
            });
        }
    }

    Ok(Analysis {
        errors,
        warnings,
        requirements,
        gaps,
        stats,
        anchors,
    })
}

fn record_gap(
    gaps: &mut BTreeMap<GapKey, TraceabilityGap>,
    req: &Requirement,
    kind: GapKind,
    finding: Finding,
) {
    gaps.entry(GapKey::new(&req.id, kind))
        .or_insert_with(|| TraceabilityGap {
            area: req.area.clone(),
            findings: Vec::new(),
        })
        .findings
        .push(finding);
}

fn apply_baseline(
    analysis: &mut Analysis,
    baseline: &Baseline,
    complete_scope: bool,
    stale_is_error: bool,
) -> BaselineStats {
    let mut stats = BaselineStats::default();
    let requirements: HashMap<&str, &Requirement> = analysis
        .requirements
        .iter()
        .map(|req| (req.id.as_str(), req))
        .collect();
    let mut entries: BTreeMap<GapKey, &BaselineEntry> = BTreeMap::new();

    for duplicate in baseline.duplicate_keys() {
        analysis.errors.push(baseline_finding(format!(
            "duplicate baseline entry for {} ({})",
            duplicate.requirement, duplicate.kind
        )));
    }
    for entry in &baseline.gaps {
        entries.entry(entry.key()).or_insert(entry);
    }

    for (key, gap) in &analysis.gaps {
        if gap_is_hard(key.kind, &gap.area) {
            analysis.errors.extend(
                gap.findings
                    .iter()
                    .cloned()
                    .map(|finding| annotate_gap(finding, "hard-area", key.kind)),
            );
        } else if entries.contains_key(key) {
            stats.known += 1;
            analysis.warnings.extend(
                gap.findings
                    .iter()
                    .cloned()
                    .map(|finding| annotate_gap(finding, "grandfathered", key.kind)),
            );
        } else {
            stats.new += 1;
            analysis.errors.extend(
                gap.findings
                    .iter()
                    .cloned()
                    .map(|finding| annotate_gap(finding, "new regression", key.kind)),
            );
        }
    }

    for (key, entry) in entries {
        let Some(req) = requirements.get(entry.requirement.as_str()) else {
            if complete_scope {
                analysis.errors.push(baseline_finding(format!(
                    "baseline entry {} ({}) references a missing requirement",
                    entry.requirement, entry.kind
                )));
            }
            continue;
        };

        if gap_is_hard(entry.kind, &req.area) {
            analysis.errors.push(baseline_finding(format!(
                "baseline entry {} ({}) is forbidden because area {} is already hard",
                entry.requirement, entry.kind, req.area
            )));
            continue;
        }

        let stale_reason = if req.retired {
            Some("the requirement is retired")
        } else if !analysis.gaps.contains_key(&key) {
            Some("the gap is resolved")
        } else {
            None
        };
        if let Some(reason) = stale_reason {
            stats.resolved += 1;
            if stale_is_error {
                analysis.errors.push(baseline_finding(format!(
                    "stale baseline entry {} ({}): {reason}; remove it with `cargo run -p \
                     cargo req-cov baseline prune`",
                    entry.requirement, entry.kind
                )));
            }
        }
    }

    stats
}

fn annotate_gap(mut finding: Finding, status: &str, kind: GapKind) -> Finding {
    finding.message = format!("[{status} {kind}] {}", finding.message);
    finding
}

fn baseline_finding(message: String) -> Finding {
    Finding {
        file: BASELINE_PATH.to_string(),
        line: 1,
        message,
    }
}

fn gap_is_hard(kind: GapKind, area: &str) -> bool {
    match kind {
        GapKind::EnforcementAnchor => HARD_CODE_ANCHOR_AREAS.contains(&area),
        GapKind::VerificationAnchor | GapKind::EvidenceCitation => {
            HARD_TEST_ANCHOR_AREAS.contains(&area)
        }
    }
}

fn covers_default_documents(docs: &[DocSpec]) -> bool {
    let paths: HashSet<&str> = docs.iter().map(|doc| doc.path.as_str()).collect();
    crate::default_docs()
        .iter()
        .all(|doc| paths.contains(doc.path.as_str()))
}

fn require_complete_scope(docs: &[DocSpec], operation: &str) -> Result<()> {
    if !covers_default_documents(docs) {
        bail!(
            "cannot {operation} traceability baseline with a partial document set; use the \
             default workspace documents"
        );
    }
    Ok(())
}

fn ensure_no_non_gap_errors(analysis: &Analysis, operation: &str) -> Result<()> {
    if let Some(first) = analysis.errors.first() {
        bail!(
            "cannot {operation} traceability baseline while checks fail: {}:{}: {}",
            first.file,
            first.line,
            first.message
        );
    }
    Ok(())
}

#[derive(Default)]
struct AreaStats {
    total: usize,
    retired: usize,
    automated: usize,
    e2e: usize,
    review_only: usize,
    pending: usize,
    /// Requirements with at least one Enforced code path (anchor expected).
    anchorable: usize,
    /// ... of which the Enforced files actually carry the anchor.
    anchored: usize,
    /// Automated-evidence requirements with a matching test anchor.
    test_anchored: usize,
}

fn render_summary(
    stats: &BTreeMap<String, AreaStats>,
    anchors: &Anchors,
    baseline: &BaselineStats,
) -> String {
    // Label column sized to the longest `Full Name (ACRONYM)` label.
    let label_width = stats
        .keys()
        .map(|area| area_label(area).chars().count())
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
            area_label(area),
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
    let _ = writeln!(
        out,
        "traceability baseline: {} known gap(s), {} resolved/stale, {} new regression(s)",
        baseline.known, baseline.resolved, baseline.new
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn exact_baseline_gap_is_known_warning() {
        let kind = GapKind::EnforcementAnchor;
        let mut analysis = analysis(requirement("REQ-ZZ-001", "ZZ", false), Some(kind));
        let stats = apply_baseline(&mut analysis, &baseline("REQ-ZZ-001", kind), true, true);
        assert!(analysis.errors.is_empty());
        assert_eq!(analysis.warnings.len(), 1);
        assert!(analysis.warnings[0].message.contains("grandfathered"));
        assert_eq!(stats.known, 1);
    }

    #[test]
    fn unbaselined_gap_is_a_regression() {
        let kind = GapKind::VerificationAnchor;
        let mut analysis = analysis(requirement("REQ-ZZ-001", "ZZ", false), Some(kind));
        let stats = apply_baseline(
            &mut analysis,
            &Baseline::from_entries(Vec::new()),
            true,
            true,
        );
        assert_eq!(analysis.errors.len(), 1);
        assert!(analysis.errors[0].message.contains("new regression"));
        assert_eq!(stats.new, 1);
    }

    #[test]
    fn fixed_gap_makes_entry_stale() {
        let kind = GapKind::EvidenceCitation;
        let mut analysis = analysis(requirement("REQ-ZZ-001", "ZZ", false), None);
        let stats = apply_baseline(&mut analysis, &baseline("REQ-ZZ-001", kind), true, true);
        assert_eq!(analysis.errors.len(), 1);
        assert!(analysis.errors[0].message.contains("gap is resolved"));
        assert_eq!(stats.resolved, 1);
    }

    #[test]
    fn prune_mode_accepts_resolved_entry_for_removal() {
        let kind = GapKind::EvidenceCitation;
        let mut analysis = analysis(requirement("REQ-ZZ-001", "ZZ", true), None);
        let stats = apply_baseline(&mut analysis, &baseline("REQ-ZZ-001", kind), true, false);
        assert!(analysis.errors.is_empty());
        assert_eq!(stats.resolved, 1);
    }

    #[test]
    fn hard_area_cannot_be_baselined() {
        let kind = GapKind::EnforcementAnchor;
        let area = HARD_CODE_ANCHOR_AREAS
            .first()
            .expect("at least one hard code area");
        let mut analysis = analysis(requirement("REQ-SAFE-999", area, false), Some(kind));
        apply_baseline(&mut analysis, &baseline("REQ-SAFE-999", kind), true, true);
        assert!(analysis.errors.len() >= 2);
        assert!(
            analysis
                .errors
                .iter()
                .any(|finding| finding.message.contains("forbidden"))
        );
    }
}
