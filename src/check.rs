//! The cross-checks and the report.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::DocSpec;
use crate::baseline::{Baseline, BaselineEntry, GapKey, GapKind};
use crate::check_evidence::{
    VerificationOutcome, evaluate_verification, vacuity_reason, weak_reason,
};
use crate::check_report::{AreaStats, BaselineStats, render_summary};
use crate::config::RepositoryConfig;
use crate::docs::{Requirement, parse_doc};
use crate::scan::{Anchors, VerificationAnchor, scan};

pub use crate::check_report::{Finding, Report};

/// Result of a monotonic baseline maintenance command.
#[derive(Debug)]
pub struct BaselineChange {
    pub path: PathBuf,
    pub entries: usize,
    pub removed: usize,
    pub added: usize,
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

/// Runs every check, including committed-baseline policy, for the given
/// documents against the workspace at `root`.
pub fn run(root: &Path, docs: &[DocSpec], config: &RepositoryConfig) -> Result<Report> {
    let mut analysis = analyze(root, docs, config)?;
    let baseline = Baseline::load(root, &config.baseline)?;
    let baseline_stats = apply_baseline(
        &mut analysis,
        &baseline,
        covers_configured_documents(docs, config),
        true,
        config,
    );
    let summary = render_summary(&analysis.stats, &analysis.anchors, &baseline_stats, config);
    Ok(Report {
        errors: analysis.errors,
        warnings: analysis.warnings,
        summary,
    })
}

/// Creates the one-time initial baseline. Existing baseline files are
/// never replaced.
pub fn initialize_baseline(
    root: &Path,
    docs: &[DocSpec],
    config: &RepositoryConfig,
) -> Result<BaselineChange> {
    require_complete_scope(docs, config, "initialize")?;
    let analysis = analyze(root, docs, config)?;
    ensure_no_non_gap_errors(&analysis, "initialize")?;

    if let Some((key, _)) = analysis
        .gaps
        .iter()
        .find(|(key, gap)| gap_is_hard(key.kind, &gap.area, config))
    {
        bail!(
            "cannot initialize baseline: hard-area gap {} ({}) must be fixed",
            key.requirement,
            key.kind
        );
    }

    let baseline = Baseline::from_entries(baseline_entries(&analysis));
    let count = baseline.gaps.len();
    let path = baseline.create_new(root, &config.baseline)?;
    Ok(BaselineChange {
        path,
        entries: count,
        removed: 0,
        added: count,
    })
}

/// Removes only entries whose gaps are fixed or whose requirements are
/// retired. It never adds exceptions.
pub fn prune_baseline(
    root: &Path,
    docs: &[DocSpec],
    config: &RepositoryConfig,
) -> Result<BaselineChange> {
    require_complete_scope(docs, config, "prune")?;
    let mut analysis = analyze(root, docs, config)?;
    let mut baseline = Baseline::load(root, &config.baseline)?;
    let before = baseline.gaps.len();
    apply_baseline(&mut analysis, &baseline, true, false, config);
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
        root.join(&config.baseline)
    } else {
        baseline.write_pruned(root, &config.baseline)?
    };
    Ok(BaselineChange {
        path,
        entries: baseline.gaps.len(),
        removed,
        added: 0,
    })
}

/// Adds exceptions only for gaps of kinds the committed baseline has
/// never recorded — the tool-upgrade path when a new release starts
/// detecting a gap kind that older releases could not. Kinds already
/// present stay removal-only.
#[shallguard::enforces("REQ-BASE-006")]
pub fn extend_baseline(
    root: &Path,
    docs: &[DocSpec],
    config: &RepositoryConfig,
) -> Result<BaselineChange> {
    require_complete_scope(docs, config, "extend")?;
    let analysis = analyze(root, docs, config)?;
    // Only structural errors block extension; the unbaselined gaps of a
    // newly detectable kind are exactly what this command records.
    ensure_no_non_gap_errors(&analysis, "extend")?;
    let baseline = Baseline::load(root, &config.baseline)?;

    let existing_kinds: HashSet<GapKind> = baseline.gaps.iter().map(|entry| entry.kind).collect();
    let candidates: Vec<&GapKey> = analysis
        .gaps
        .keys()
        .filter(|key| !existing_kinds.contains(&key.kind) && !gap_is_advisory(key.kind))
        .collect();
    if let Some(hard) = candidates
        .iter()
        .find(|key| gap_is_hard(key.kind, &analysis.gaps[**key].area, config))
    {
        bail!(
            "cannot extend baseline: hard-area gap {} ({}) must be fixed",
            hard.requirement,
            hard.kind
        );
    }
    if candidates.is_empty() {
        return Ok(BaselineChange {
            path: root.join(&config.baseline),
            entries: baseline.gaps.len(),
            removed: 0,
            added: 0,
        });
    }
    let added = candidates.len();
    let mut entries = baseline.gaps.clone();
    entries.extend(candidates.into_iter().map(|key| BaselineEntry {
        requirement: key.requirement.clone(),
        kind: key.kind,
    }));
    let merged = Baseline::from_entries(entries);
    let total = merged.gaps.len();
    let path = merged.write_pruned(root, &config.baseline)?;
    Ok(BaselineChange {
        path,
        entries: total,
        removed: 0,
        added,
    })
}

/// Detects invariant failures and traceability gaps without deciding
/// whether historical debt is allowed.
#[shallguard::enforces(
    "REQ-SPEC-001",
    "REQ-SPEC-003",
    "REQ-SPEC-004",
    "REQ-TRACE-005",
    "REQ-TRACE-006",
    "REQ-TRACE-007",
    "REQ-PORT-004",
    "REQ-PORT-008"
)]
fn analyze(root: &Path, docs: &[DocSpec], config: &RepositoryConfig) -> Result<Analysis> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let mut requirements: Vec<Requirement> = Vec::new();
    let mut path_spans: Vec<(String, usize, PathBuf)> = Vec::new();
    for spec in docs {
        let doc = parse_doc(root, spec)?;
        if doc.requirements.len() < config.minimum_requirements {
            errors.push(Finding {
                file: spec.path.clone(),
                line: 1,
                message: format!(
                    "parsed only {} requirements - document format drifted \
                     from the configured minimum of {}",
                    doc.requirements.len(),
                    config.minimum_requirements
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

    let mut unknown_areas = HashSet::new();
    for requirement in &requirements {
        if !config.areas.contains_key(&requirement.area)
            && unknown_areas.insert(requirement.area.clone())
        {
            errors.push(Finding {
                file: requirement.doc.clone(),
                line: requirement.line,
                message: format!(
                    "requirement area {} has no [areas.{}] policy in shallguard.toml",
                    requirement.area, requirement.area
                ),
            });
        }
    }

    let scan_roots = docs
        .iter()
        .flat_map(DocSpec::scan_roots)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

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
        if config.allow_missing_paths.contains(path) {
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
    let mut verifying: HashMap<&str, Vec<&VerificationAnchor>> = HashMap::new();
    for anchor in &anchors.verification {
        for id in &anchor.ids {
            verifying.entry(id.as_str()).or_default().push(anchor);
        }
    }

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
            let files = enforcing_files.get(req.id.as_str());
            let missing = req
                .enforced_paths
                .iter()
                .filter(|enforced| !enforced_path_has_anchor(enforced, files))
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>();
            if missing.is_empty() {
                stat.anchored += 1;
            } else {
                let finding = Finding {
                    file: req.doc.to_string(),
                    line: req.line,
                    message: format!(
                        "{} \"{}\" - no enforcement anchor (#[shallguard::enforces] or \
                         shallguard::enforces_here!) in documented file(s): {}",
                        req.id,
                        req.title,
                        missing.join(", ")
                    ),
                };
                record_gap(&mut gaps, req, GapKind::EnforcementAnchor, finding);
            }
        }

        if req.automated {
            let req_anchors: Vec<&VerificationAnchor> =
                verifying.get(req.id.as_str()).cloned().unwrap_or_default();
            match evaluate_verification(&req_anchors) {
                VerificationOutcome::Missing => {
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
                VerificationOutcome::Demoted(vacuous) => {
                    // Every anchored test is vacuous: the ✅ claim has no
                    // automated evidence behind it.
                    for anchor in vacuous {
                        record_gap(
                            &mut gaps,
                            req,
                            GapKind::VacuousEvidence,
                            Finding {
                                file: anchor.file.display().to_string(),
                                line: anchor.line,
                                message: format!(
                                    "`{}` verifies {} but {}; add a real assertion \
                                     or downgrade the document line to ⏳",
                                    anchor.test_fn,
                                    req.id,
                                    vacuity_reason(anchor)
                                ),
                            },
                        );
                    }
                }
                VerificationOutcome::Anchored {
                    weak,
                    redundant_vacuous,
                } => {
                    stat.test_anchored += 1;
                    for anchor in weak {
                        record_gap(
                            &mut gaps,
                            req,
                            GapKind::WeakEvidence,
                            Finding {
                                file: anchor.file.display().to_string(),
                                line: anchor.line,
                                message: format!(
                                    "`{}` verifies {} but {}; add an expected panic \
                                     message or a real assertion",
                                    anchor.test_fn,
                                    req.id,
                                    weak_reason(anchor)
                                ),
                            },
                        );
                    }
                    for anchor in redundant_vacuous {
                        warnings.push(Finding {
                            file: anchor.file.display().to_string(),
                            line: anchor.line,
                            message: format!(
                                "`{}` verifies {} but {}; other anchored evidence \
                                 remains - strengthen or remove this anchor",
                                anchor.test_fn,
                                req.id,
                                vacuity_reason(anchor)
                            ),
                        });
                    }
                }
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
                                 a #[shallguard::verifies] anchor for this requirement",
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
        if anchor.ids.len() >= config.verify_outlier_threshold {
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

/// Gap kinds that never gate on their own: they are reported as
/// warnings (unless an area policy promotes them) and are therefore
/// never recorded in the baseline, where a later fix would turn them
/// into stale-entry failures.
fn gap_is_advisory(kind: GapKind) -> bool {
    matches!(kind, GapKind::WeakEvidence)
}

/// Entries recorded by `baseline init`: every current gap except
/// advisory-only kinds.
#[shallguard::enforces("REQ-TRACE-013")]
fn baseline_entries(analysis: &Analysis) -> Vec<BaselineEntry> {
    analysis
        .gaps
        .keys()
        .filter(|key| !gap_is_advisory(key.kind))
        .map(|key| BaselineEntry {
            requirement: key.requirement.clone(),
            kind: key.kind,
        })
        .collect()
}

fn enforced_path_has_anchor(enforced: &Path, files: Option<&HashSet<&Path>>) -> bool {
    files.is_some_and(|files| {
        files.iter().any(|file| {
            if enforced.to_string_lossy().ends_with('/') {
                file.starts_with(enforced)
            } else {
                *file == enforced
            }
        })
    })
}

#[shallguard::enforces("REQ-BASE-002")]
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

#[shallguard::enforces("REQ-BASE-002", "REQ-BASE-004")]
fn apply_baseline(
    analysis: &mut Analysis,
    baseline: &Baseline,
    complete_scope: bool,
    stale_is_error: bool,
    config: &RepositoryConfig,
) -> BaselineStats {
    let mut stats = BaselineStats::default();
    let requirements: HashMap<&str, &Requirement> = analysis
        .requirements
        .iter()
        .map(|req| (req.id.as_str(), req))
        .collect();
    let mut entries: BTreeMap<GapKey, &BaselineEntry> = BTreeMap::new();

    for duplicate in baseline.duplicate_keys() {
        analysis.errors.push(baseline_finding(
            config,
            format!(
                "duplicate baseline entry for {} ({})",
                duplicate.requirement, duplicate.kind
            ),
        ));
    }
    for entry in &baseline.gaps {
        entries.entry(entry.key()).or_insert(entry);
    }

    for (key, gap) in &analysis.gaps {
        if gap_is_hard(key.kind, &gap.area, config) {
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
        } else if gap_is_advisory(key.kind) {
            // Advisory kinds stay warnings unless the area opts into
            // `strict_oracle`; hard promotion is handled above.
            analysis.warnings.extend(
                gap.findings
                    .iter()
                    .cloned()
                    .map(|finding| annotate_gap(finding, "advisory", key.kind)),
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
                analysis.errors.push(baseline_finding(
                    config,
                    format!(
                        "baseline entry {} ({}) references a missing requirement",
                        entry.requirement, entry.kind
                    ),
                ));
            }
            continue;
        };

        if gap_is_hard(entry.kind, &req.area, config) {
            analysis.errors.push(baseline_finding(
                config,
                format!(
                    "baseline entry {} ({}) is forbidden because area {} is already hard",
                    entry.requirement, entry.kind, req.area
                ),
            ));
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
                let message = format!(
                    "stale baseline entry {} ({}): {reason}; remove it with `cargo \
                     shallguard baseline prune`",
                    entry.requirement, entry.kind
                );
                if gap_is_advisory(entry.kind) {
                    // Fixing an advisory finding must never break the
                    // gate, even when its entry got in out-of-band.
                    analysis.warnings.push(baseline_finding(config, message));
                } else {
                    analysis.errors.push(baseline_finding(config, message));
                }
            }
        }
    }

    stats
}

fn annotate_gap(mut finding: Finding, status: &str, kind: GapKind) -> Finding {
    finding.message = format!("[{status} {kind}] {}", finding.message);
    finding
}

fn baseline_finding(config: &RepositoryConfig, message: String) -> Finding {
    Finding {
        file: config.baseline.to_string_lossy().into_owned(),
        line: 1,
        message,
    }
}

#[shallguard::enforces("REQ-BASE-003", "REQ-TRACE-013", "REQ-TRACE-018")]
fn gap_is_hard(kind: GapKind, area: &str, config: &RepositoryConfig) -> bool {
    match kind {
        GapKind::EnforcementAnchor => config.area_is_hard(area, false),
        GapKind::VerificationAnchor | GapKind::EvidenceCitation | GapKind::VacuousEvidence => {
            config.area_is_hard(area, true)
        }
        GapKind::WeakEvidence => config.area_strict_oracle(area),
    }
}

fn covers_configured_documents(docs: &[DocSpec], config: &RepositoryConfig) -> bool {
    let paths: HashSet<&str> = docs.iter().map(|doc| doc.path.as_str()).collect();
    config
        .documents()
        .iter()
        .all(|doc| paths.contains(doc.path.as_str()))
}

fn require_complete_scope(
    docs: &[DocSpec],
    config: &RepositoryConfig,
    operation: &str,
) -> Result<()> {
    if !covers_configured_documents(docs, config) {
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

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
