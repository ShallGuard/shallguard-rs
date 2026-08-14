//! One-command orchestration for impact, coverage, bundle, and local review.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::DocSpec;
use crate::bundle::{self, BundleOptions};
use crate::coverage::{self, CoverageOptions};
use crate::docs::parse_doc;
use crate::impact::{self, BaseSelection, ImpactOptions};
use crate::review::{self, ReviewOptions, ReviewProvider, ReviewRun};
use crate::{ProgressCallback, report_progress};

/// Conventional artifacts produced by orchestrated local review.
#[derive(Debug)]
pub struct ReviewArtifactPaths<'a> {
    /// Machine-readable requirement impact.
    pub impact_json: &'a Path,
    /// Human-readable requirement impact.
    pub impact_markdown: &'a Path,
    /// Machine-readable executable coverage.
    pub coverage_json: &'a Path,
    /// Human-readable executable coverage.
    pub coverage_markdown: &'a Path,
    /// Intermediate LLVM profiles and exports.
    pub coverage_work_dir: &'a Path,
    /// Deterministic review bundle directory.
    pub bundle_dir: &'a Path,
    /// Validated local review output directory.
    pub review_output_dir: &'a Path,
}

/// Inputs for replaying a bundle or orchestrating a complete local review.
pub struct ReviewWorkflowOptions<'a> {
    /// Comparison base. `None` replays the existing bundle.
    pub base: Option<BaseSelection<'a>>,
    /// Repository-relative traceability baseline used by impact analysis.
    pub baseline_path: &'a Path,
    /// Whether orchestrated mode should execute relevant verification tests.
    pub with_coverage: bool,
    /// Local model CLI.
    pub provider: ReviewProvider,
    /// Optional provider-specific model identifier.
    pub model: Option<&'a str>,
    /// Optional on-device Codex backend.
    pub local_provider: Option<&'a str>,
    /// Optional final requirement allowlist.
    pub requirements: &'a BTreeSet<String>,
    /// Per-capsule model timeout.
    pub timeout: std::time::Duration,
    /// Continue a compatible review output and reuse its completed checkpoints.
    pub resume: bool,
    /// Optional portable content-addressed validated-response cache.
    pub cache_dir: Option<&'a Path>,
    /// Paths used for generated and final artifacts.
    pub artifacts: ReviewArtifactPaths<'a>,
    /// Optional human-readable progress callback.
    pub progress: Option<ProgressCallback>,
}

/// Outcome of a complete or replayed local review workflow.
#[derive(Debug)]
pub struct ReviewWorkflowRun {
    /// Validated and failed local model review counts.
    pub review: ReviewRun,
    /// Number of requirements selected by impact analysis.
    pub impacted_requirements: usize,
    /// Number of impacted automated requirements selected for coverage.
    pub covered_requirements: usize,
    /// Whether deterministic impact policy reported errors.
    pub impact_policy_failed: bool,
    /// Whether a selected test or LLVM coverage infrastructure failed.
    pub coverage_execution_failed: bool,
}

impl ReviewWorkflowRun {
    /// Returns true when any deterministic, execution, or provider stage failed.
    pub fn has_failures(&self) -> bool {
        self.impact_policy_failed || self.coverage_execution_failed || self.review.failures > 0
    }
}

/// Executes an existing bundle or generates every prerequisite from a base.
///
/// Orchestrated mode writes the impact artifact first, runs coverage only for
/// impacted requirements with automated evidence, embeds that coverage into a
/// deterministic bundle, and finally performs the advisory model review.
///
/// # Errors
///
/// Returns an error when impact analysis, artifact serialization, coverage
/// setup, bundle generation, or local-review setup cannot proceed reliably.
pub fn run(
    root: &Path,
    docs: &[DocSpec],
    options: &ReviewWorkflowOptions<'_>,
) -> Result<ReviewWorkflowRun> {
    let output_exists = options
        .artifacts
        .review_output_dir
        .try_exists()
        .with_context(|| {
            format!(
                "checking review output {}",
                options.artifacts.review_output_dir.display()
            )
        })?;
    let resume_existing = options.resume && output_exists;
    let base = if resume_existing { None } else { options.base };
    if options.with_coverage && base.is_none() && !resume_existing {
        bail!("--with-coverage requires --base <revision> or --target <branch>");
    }
    if !options.resume {
        require_absent(options.artifacts.review_output_dir, "review output")?;
    }
    if base.is_some() {
        require_absent(options.artifacts.bundle_dir, "generated bundle")?;
    }

    let mut impacted_requirements = 0usize;
    let mut covered_requirements = 0usize;
    let mut impact_policy_failed = false;
    let mut coverage_execution_failed = false;

    if let Some(base) = &base {
        report_progress(
            options.progress,
            format!("impact: analyzing changes against {}", base_label(*base)),
        );
        let impact = impact::analyze(
            root,
            docs,
            &ImpactOptions {
                base: *base,
                baseline_path: options.baseline_path,
            },
        )?;
        impacted_requirements = impact.requirements.len();
        if impacted_requirements == 0 {
            bail!("impact analysis selected no requirements for local review");
        }
        impact_policy_failed = impact.has_policy_errors();
        report_progress(
            options.progress,
            format!(
                "impact: selected {impacted_requirements} requirement(s); {} policy finding(s)",
                impact.findings.len()
            ),
        );
        write_artifact(options.artifacts.impact_json, &impact.to_json()?)?;
        write_artifact(options.artifacts.impact_markdown, &impact.to_markdown())?;

        let coverage_file = if options.with_coverage {
            let automated = automated_requirement_descriptions(root, docs)?;
            let automated_ids = automated.keys().cloned().collect::<BTreeSet<_>>();
            let impacted = impact
                .requirements
                .iter()
                .map(|requirement| requirement.id.as_str())
                .collect::<BTreeSet<_>>();
            let selected =
                select_coverage_requirements(&impacted, &automated_ids, options.requirements);
            covered_requirements = selected.len();
            if selected.is_empty() {
                report_progress(
                    options.progress,
                    "coverage: no impacted requirement has automated evidence; skipping",
                );
                None
            } else {
                report_progress(
                    options.progress,
                    format!(
                        "coverage: selecting tests for {covered_requirements} impacted automated \
                         requirement(s)"
                    ),
                );
                report_coverage_requirements(options.progress, &selected, &automated);
                let coverage = coverage::generate(
                    root,
                    docs,
                    &CoverageOptions {
                        packages: &BTreeSet::new(),
                        requirements: &selected,
                        work_dir: options.artifacts.coverage_work_dir,
                        progress: options.progress,
                    },
                )?;
                coverage_execution_failed = coverage.has_execution_errors();
                write_artifact(options.artifacts.coverage_json, &coverage.to_json()?)?;
                write_artifact(options.artifacts.coverage_markdown, &coverage.to_markdown())?;
                Some(options.artifacts.coverage_json)
            }
        } else {
            report_progress(options.progress, "coverage: disabled by --without-coverage");
            None
        };

        report_progress(
            options.progress,
            format!(
                "bundle: creating {impacted_requirements} capsule(s) in {}",
                options.artifacts.bundle_dir.display()
            ),
        );
        bundle::generate(
            root,
            docs,
            &BundleOptions {
                impact_file: options.artifacts.impact_json,
                coverage_file,
                output_dir: options.artifacts.bundle_dir,
            },
        )?;
        report_progress(options.progress, "bundle: deterministic capsules are ready");
    } else {
        report_progress(
            options.progress,
            format!(
                "bundle: {} existing capsules from {}",
                if resume_existing {
                    "resuming"
                } else {
                    "replaying"
                },
                options.artifacts.bundle_dir.display()
            ),
        );
    }

    report_progress(options.progress, "review: starting local semantic review");
    let review = review::generate(&ReviewOptions {
        bundle_dir: options.artifacts.bundle_dir,
        output_dir: options.artifacts.review_output_dir,
        provider: options.provider,
        model: options.model,
        local_provider: options.local_provider,
        requirements: options.requirements,
        timeout: options.timeout,
        resume: options.resume,
        cache_dir: options.cache_dir,
        progress: options.progress,
    })?;

    Ok(ReviewWorkflowRun {
        review,
        impacted_requirements,
        covered_requirements,
        impact_policy_failed,
        coverage_execution_failed,
    })
}

fn base_label<'a>(base: BaseSelection<'a>) -> &'a str {
    match base {
        BaseSelection::Revision(revision) => revision,
        BaseSelection::MergeBaseWith(target) => target,
    }
}

fn automated_requirement_descriptions(
    root: &Path,
    docs: &[DocSpec],
) -> Result<BTreeMap<String, String>> {
    let mut automated = BTreeMap::new();
    for doc in docs {
        for requirement in parse_doc(root, doc)?
            .requirements
            .into_iter()
            .filter(|requirement| requirement.automated)
        {
            automated.insert(requirement.id, requirement.title);
        }
    }
    Ok(automated)
}

fn select_coverage_requirements(
    impacted: &BTreeSet<&str>,
    automated: &BTreeSet<String>,
    requested: &BTreeSet<String>,
) -> BTreeSet<String> {
    automated
        .iter()
        .filter(|requirement| impacted.contains(requirement.as_str()))
        .filter(|requirement| requested.is_empty() || requested.contains(requirement.as_str()))
        .cloned()
        .collect()
}

fn report_coverage_requirements(
    progress: Option<ProgressCallback>,
    requirements: &BTreeSet<String>,
    descriptions: &BTreeMap<String, String>,
) {
    for line in coverage_requirement_progress_lines(requirements, descriptions) {
        report_progress(progress, line);
    }
}

#[shallguard_macros::enforces("REQ-CLI-003")]
fn coverage_requirement_progress_lines(
    requirements: &BTreeSet<String>,
    descriptions: &BTreeMap<String, String>,
) -> Vec<String> {
    requirements
        .iter()
        .enumerate()
        .map(|(index, requirement)| {
            let description = descriptions
                .get(requirement)
                .map_or("description unavailable", String::as_str);
            format!(
                "coverage: requirement [{}/{}] {requirement} - {description}",
                index + 1,
                requirements.len(),
            )
        })
        .collect()
}

fn write_artifact(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating artifact parent {}", parent.display()))?;
    }
    std::fs::write(path, content).with_context(|| format!("writing artifact {}", path.display()))
}

fn require_absent(path: &Path, description: &str) -> Result<()> {
    if path
        .try_exists()
        .with_context(|| format!("checking {}", path.display()))?
    {
        bail!(
            "{description} {} already exists; choose another path or retain it as an artifact",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn successful_run() -> ReviewWorkflowRun {
        ReviewWorkflowRun {
            review: ReviewRun {
                output_dir: Path::new("review-output").to_path_buf(),
                reviews: 1,
                failures: 0,
            },
            impacted_requirements: 1,
            covered_requirements: 1,
            impact_policy_failed: false,
            coverage_execution_failed: false,
        }
    }

    #[test]
    fn coverage_selection_intersects_impact_automation_and_request() {
        let impacted = BTreeSet::from(["REQ-AA-001", "REQ-AA-002", "REQ-AA-003"]);
        let automated = BTreeSet::from([
            "REQ-AA-001".to_string(),
            "REQ-AA-002".to_string(),
            "REQ-BB-001".to_string(),
        ]);
        let requested = BTreeSet::from(["REQ-AA-002".to_string(), "REQ-AA-003".to_string()]);

        assert_eq!(
            select_coverage_requirements(&impacted, &automated, &requested),
            BTreeSet::from(["REQ-AA-002".to_string()])
        );
    }

    #[test]
    fn empty_request_selects_every_automated_impact() {
        let impacted = BTreeSet::from(["REQ-AA-001", "REQ-AA-002"]);
        let automated = BTreeSet::from(["REQ-AA-001".to_string()]);

        assert_eq!(
            select_coverage_requirements(&impacted, &automated, &BTreeSet::new()),
            automated
        );
    }

    #[shallguard_macros::verifies("REQ-CLI-003")]
    #[test]
    fn coverage_requirement_progress_is_sorted_one_per_line_with_descriptions() {
        let requirements = BTreeSet::from(["REQ-AA-002".to_string(), "REQ-AA-001".to_string()]);
        let descriptions = BTreeMap::from([
            ("REQ-AA-001".to_string(), "First behavior".to_string()),
            ("REQ-AA-002".to_string(), "Second behavior".to_string()),
        ]);

        assert_eq!(
            coverage_requirement_progress_lines(&requirements, &descriptions),
            vec![
                "coverage: requirement [1/2] REQ-AA-001 - First behavior",
                "coverage: requirement [2/2] REQ-AA-002 - Second behavior",
            ]
        );
    }

    #[test]
    fn deterministic_or_provider_failure_fails_the_workflow() {
        let mut run = successful_run();
        assert!(!run.has_failures());

        run.coverage_execution_failed = true;
        assert!(run.has_failures());
        run.coverage_execution_failed = false;
        run.review.failures = 1;
        assert!(run.has_failures());
    }
}
