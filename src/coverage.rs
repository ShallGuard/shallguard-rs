//! Per-verification-test LLVM coverage projected onto requirement enforcement scopes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::coverage_llvm::{self, InvocationOutcome, RegionIndex};
use crate::docs::parse_doc;
use crate::scan::{EnforcementScopeKind, SourceRange, scan};
use crate::test_index::{
    CargoTargetIdentity, CargoTestIdentity, HarnessSource, TestIndexOptions, TestTargetKind,
};
use crate::{DocSpec, ProgressCallback, report_progress};

/// Version of the requirement coverage artifact.
#[shallguard_macros::enforces("REQ-CLI-005")]
pub const COVERAGE_SCHEMA: &str = "shallguard.requirement-coverage/v1";

/// Inputs controlling a full or filtered executable-coverage run.
pub struct CoverageOptions<'a> {
    /// Optional Cargo package allowlist. Empty selects every anchored package.
    pub packages: &'a BTreeSet<String>,
    /// Optional requirement allowlist. Empty selects every resolved test.
    pub requirements: &'a BTreeSet<String>,
    /// Directory for per-test LLVM JSON exports.
    pub work_dir: &'a Path,
    /// Optional human-readable progress callback.
    pub progress: Option<ProgressCallback>,
}

/// Complete requirement-level executable coverage artifact.
#[derive(Debug, Serialize)]
pub struct CoverageArtifact {
    pub schema: &'static str,
    pub repository: &'static str,
    pub head_commit: String,
    pub working_tree_dirty: bool,
    pub rust_toolchain: String,
    pub coverage_tool: String,
    pub configuration: CoverageConfiguration,
    pub tests: Vec<TestCoverage>,
    pub requirements: Vec<RequirementCoverage>,
    pub infrastructure_findings: Vec<CoverageFinding>,
}

/// Build and selection inputs recorded for reproducibility.
#[derive(Debug, Serialize)]
pub struct CoverageConfiguration {
    pub default_features: bool,
    pub features: Vec<String>,
    pub packages: Vec<String>,
    pub targets: Vec<CargoTargetIdentity>,
    pub selected_requirements: Vec<String>,
    pub profile_isolation: &'static str,
}

/// Result of one exact Cargo test invocation and LLVM export.
#[derive(Debug, Serialize)]
pub struct TestCoverage {
    pub identity: CargoTestIdentity,
    pub requirements: Vec<String>,
    pub result: TestExecutionResult,
    pub export_digest: Option<String>,
    pub message: Option<String>,
}

/// Exact-test execution outcome, kept separate from enforcement reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TestExecutionResult {
    Passed,
    Failed,
    InfrastructureError,
}

/// Coverage projection for one selected requirement.
#[derive(Debug, Serialize)]
pub struct RequirementCoverage {
    pub id: String,
    pub area: String,
    pub title: String,
    pub status: CoverageStatus,
    pub tests: Vec<RequirementTestResult>,
    pub executable_sites: SiteSummary,
    pub structural_sites: usize,
    pub unmapped_sites: usize,
    pub sites: Vec<EnforcementSiteCoverage>,
}

/// Requirement-level interpretation of independent test and reach evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageStatus {
    Covered,
    PartiallyCovered,
    NotReached,
    StructuralOnly,
    NoExecutableEvidence,
    TestFailed,
    InfrastructureError,
}

/// One selected test as referenced by a requirement result.
#[derive(Debug, Serialize)]
pub struct RequirementTestResult {
    pub identity: String,
    pub result: TestExecutionResult,
    pub export_digest: Option<String>,
}

/// Counts of syntactically executable enforcement scopes.
#[derive(Debug, Default, Serialize)]
pub struct SiteSummary {
    pub reached: usize,
    pub instrumented: usize,
    pub total: usize,
}

/// Runtime evidence for one syntactic enforcement scope.
#[derive(Debug, Serialize)]
pub struct EnforcementSiteCoverage {
    pub file: String,
    pub anchor_line: usize,
    pub scope_kind: EnforcementScopeKind,
    pub scope: Option<SourceRange>,
    pub instrumented_regions: usize,
    pub covered_regions: usize,
    pub reached_by: Vec<String>,
}

/// A selected-test or coverage-infrastructure failure.
#[derive(Debug, Serialize)]
pub struct CoverageFinding {
    pub code: String,
    pub test: Option<String>,
    pub message: String,
}

impl CoverageArtifact {
    /// Returns true when a selected test failed or coverage infrastructure broke.
    pub fn has_execution_errors(&self) -> bool {
        !self.infrastructure_findings.is_empty()
    }

    /// Serializes the artifact as stable, pretty-printed JSON.
    pub fn to_json(&self) -> Result<String> {
        let mut json =
            serde_json::to_string_pretty(self).context("serializing coverage artifact")?;
        json.push('\n');
        Ok(json)
    }
}

/// Runs exact verification tests under LLVM instrumentation and projects their
/// covered regions onto the requirements claimed by each test.
///
/// # Errors
///
/// Returns an error when test identity resolution, tool setup, document/source
/// parsing, or the initial coverage build cannot be performed reliably.
/// Individual selected-test failures are retained in the returned artifact.
pub fn generate(
    root: &Path,
    docs: &[DocSpec],
    options: &CoverageOptions<'_>,
) -> Result<CoverageArtifact> {
    let metadata = requirement_metadata(root, docs)?;
    validate_requirement_filter(&metadata, options.requirements)?;
    report_progress(
        options.progress,
        "coverage: resolving exact Cargo test identities",
    );

    let index_options = TestIndexOptions {
        harness: HarnessSource::Enumerate,
        packages: options.packages,
        catalog_output: None,
    };
    let index = crate::test_index::generate(root, docs, &index_options)?;
    if index.has_resolution_errors() {
        let findings = index
            .findings
            .iter()
            .take(8)
            .map(|finding| format!("{}: {}", finding.code, finding.message))
            .collect::<Vec<_>>()
            .join("; ");
        bail!("verification test identity resolution failed: {findings}");
    }

    let selected = select_tests(index.tests, options.requirements);
    if selected.is_empty() {
        bail!("coverage selection contains no resolved verification tests");
    }
    validate_selected_requirements(&selected, options.requirements)?;
    let selected_requirements = selected
        .iter()
        .flat_map(|test| test.requirements.iter().cloned())
        .collect::<BTreeSet<_>>();
    let mut accumulators = enforcement_sites(root, docs, &metadata, &selected_requirements)?;

    report_progress(
        options.progress,
        format!(
            "coverage: resolved {} exact test(s) for {} requirement(s)",
            selected.len(),
            selected_requirements.len()
        ),
    );

    let rust_toolchain = coverage_llvm::tool_version(root, "rustc", &["--version"])?;
    let coverage_tool = coverage_llvm::tool_version(root, "cargo", &["llvm-cov", "--version"])?;
    report_progress(options.progress, "coverage: preparing instrumented build");
    coverage_llvm::prepare(root)?;
    report_progress(options.progress, "coverage: instrumented build is ready");

    let work_dir = if options.work_dir.is_absolute() {
        options.work_dir.to_path_buf()
    } else {
        root.join(options.work_dir)
    }
    .join(std::process::id().to_string());

    let selected_test_count = selected.len();
    let mut tests = Vec::with_capacity(selected_test_count);
    let mut findings = Vec::new();
    for (test_index, test) in selected.into_iter().enumerate() {
        let canonical_identity = canonical_test_identity(&test.identity);
        let requirement_ids = formatted_requirement_ids(&test.requirements);
        report_progress(
            options.progress,
            format!(
                "coverage: [{}/{}] running [{requirement_ids}]: {}",
                test_index + 1,
                selected_test_count,
                canonical_identity
            ),
        );
        let test_started = Instant::now();
        let outcome = coverage_llvm::collect_test(root, &work_dir, &test.identity)
            .unwrap_or_else(|error| InvocationOutcome::InfrastructureError(format!("{error:#}")));
        let (result, export_digest, message, regions) = match outcome {
            InvocationOutcome::Passed {
                regions,
                export_digest,
            } => (
                TestExecutionResult::Passed,
                Some(export_digest),
                None,
                Some(regions),
            ),
            InvocationOutcome::Failed(message) => {
                findings.push(CoverageFinding {
                    code: "selected-test-failed".to_string(),
                    test: Some(canonical_identity.clone()),
                    message: message.clone(),
                });
                (TestExecutionResult::Failed, None, Some(message), None)
            }
            InvocationOutcome::InfrastructureError(message) => {
                findings.push(CoverageFinding {
                    code: "coverage-infrastructure-error".to_string(),
                    test: Some(canonical_identity.clone()),
                    message: message.clone(),
                });
                (
                    TestExecutionResult::InfrastructureError,
                    None,
                    Some(message),
                    None,
                )
            }
        };
        let result_label = match result {
            TestExecutionResult::Passed => "passed",
            TestExecutionResult::Failed => "failed",
            TestExecutionResult::InfrastructureError => "infrastructure error",
        };
        report_progress(
            options.progress,
            format!(
                "coverage: [{}/{}] {} in {:.1}s [{requirement_ids}]: {}",
                test_index + 1,
                selected_test_count,
                result_label,
                test_started.elapsed().as_secs_f64(),
                canonical_identity
            ),
        );

        for requirement in &test.requirements {
            let accumulator = accumulators
                .get_mut(requirement)
                .expect("BUG: selected requirement accumulator is missing");
            accumulator.tests.push(RequirementTestResult {
                identity: canonical_identity.clone(),
                result,
                export_digest: export_digest.clone(),
            });
            match result {
                TestExecutionResult::Passed => {}
                TestExecutionResult::Failed => accumulator.test_failed = true,
                TestExecutionResult::InfrastructureError => accumulator.infrastructure_error = true,
            }
            if let Some(regions) = &regions {
                apply_regions(accumulator, regions, &canonical_identity);
            }
        }

        tests.push(TestCoverage {
            identity: test.identity,
            requirements: test.requirements,
            result,
            export_digest,
            message,
        });
    }

    let requirements = accumulators
        .into_values()
        .map(RequirementAccumulator::finish)
        .collect::<Vec<_>>();
    let targets = tests
        .iter()
        .map(|test| CargoTargetIdentity {
            package: test.identity.package.clone(),
            target_kind: test.identity.target_kind,
            target_name: test.identity.target_name.clone(),
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let packages = targets
        .iter()
        .map(|target| target.package.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    report_progress(
        options.progress,
        format!(
            "coverage: completed {} test(s); {} infrastructure finding(s)",
            tests.len(),
            findings.len()
        ),
    );

    Ok(CoverageArtifact {
        schema: COVERAGE_SCHEMA,
        repository: "workspace",
        head_commit: index.head_commit,
        working_tree_dirty: index.working_tree_dirty,
        rust_toolchain,
        coverage_tool,
        configuration: CoverageConfiguration {
            default_features: true,
            features: Vec::new(),
            packages,
            targets,
            selected_requirements: selected_requirements.into_iter().collect(),
            profile_isolation: "one exact test per profraw/export cycle",
        },
        tests,
        requirements,
        infrastructure_findings: findings,
    })
}

#[derive(Debug)]
struct RequirementMetadata {
    area: String,
    title: String,
}

#[derive(Debug)]
struct SelectedTest {
    identity: CargoTestIdentity,
    requirements: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SiteKey {
    file: String,
    scope_kind: EnforcementScopeKind,
    scope: Option<SourceRange>,
}

#[derive(Debug)]
struct SiteAccumulator {
    key: SiteKey,
    anchor_line: usize,
    instrumented: BTreeSet<SourceRange>,
    covered: BTreeSet<SourceRange>,
    reached_by: BTreeSet<String>,
}

#[derive(Debug)]
struct RequirementAccumulator {
    id: String,
    area: String,
    title: String,
    tests: Vec<RequirementTestResult>,
    sites: BTreeMap<SiteKey, SiteAccumulator>,
    test_failed: bool,
    infrastructure_error: bool,
}

impl RequirementAccumulator {
    fn finish(self) -> RequirementCoverage {
        let mut executable_sites = SiteSummary::default();
        let mut structural_sites = 0usize;
        let mut unmapped_sites = 0usize;
        let sites = self
            .sites
            .into_values()
            .map(|site| {
                match site.key.scope_kind {
                    EnforcementScopeKind::ConstInitializer
                    | EnforcementScopeKind::StaticInitializer
                        if site.instrumented.is_empty() =>
                    {
                        structural_sites += 1;
                    }
                    kind if kind.is_potentially_executable() => {
                        executable_sites.total += 1;
                        if !site.instrumented.is_empty() {
                            executable_sites.instrumented += 1;
                        }
                        if !site.covered.is_empty() {
                            executable_sites.reached += 1;
                        }
                    }
                    EnforcementScopeKind::Structural => structural_sites += 1,
                    EnforcementScopeKind::Unmapped => unmapped_sites += 1,
                    EnforcementScopeKind::FunctionBody
                    | EnforcementScopeKind::Block
                    | EnforcementScopeKind::ConstInitializer
                    | EnforcementScopeKind::StaticInitializer => {
                        unreachable!("potentially executable scope handled above")
                    }
                }
                EnforcementSiteCoverage {
                    file: site.key.file,
                    anchor_line: site.anchor_line,
                    scope_kind: site.key.scope_kind,
                    scope: site.key.scope,
                    instrumented_regions: site.instrumented.len(),
                    covered_regions: site.covered.len(),
                    reached_by: site.reached_by.into_iter().collect(),
                }
            })
            .collect::<Vec<_>>();

        let status = if self.infrastructure_error {
            CoverageStatus::InfrastructureError
        } else if self.test_failed {
            CoverageStatus::TestFailed
        } else if executable_sites.total == 0 && structural_sites > 0 {
            CoverageStatus::StructuralOnly
        } else if executable_sites.instrumented == 0 {
            CoverageStatus::NoExecutableEvidence
        } else if executable_sites.reached == executable_sites.total
            && executable_sites.instrumented == executable_sites.total
        {
            CoverageStatus::Covered
        } else if executable_sites.reached == 0 {
            CoverageStatus::NotReached
        } else {
            CoverageStatus::PartiallyCovered
        };

        RequirementCoverage {
            id: self.id,
            area: self.area,
            title: self.title,
            status,
            tests: self.tests,
            executable_sites,
            structural_sites,
            unmapped_sites,
            sites,
        }
    }
}

fn requirement_metadata(
    root: &Path,
    docs: &[DocSpec],
) -> Result<BTreeMap<String, RequirementMetadata>> {
    let mut metadata = BTreeMap::new();
    for doc in docs {
        for requirement in parse_doc(root, doc)?.requirements {
            if !requirement.retired {
                metadata.insert(
                    requirement.id,
                    RequirementMetadata {
                        area: requirement.area,
                        title: requirement.title,
                    },
                );
            }
        }
    }
    Ok(metadata)
}

fn validate_requirement_filter(
    metadata: &BTreeMap<String, RequirementMetadata>,
    requested: &BTreeSet<String>,
) -> Result<()> {
    let unknown = requested
        .iter()
        .filter(|id| !metadata.contains_key(*id))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        bail!("unknown or retired requirement(s): {}", unknown.join(", "));
    }
    Ok(())
}

fn select_tests(
    tests: Vec<crate::test_index::IndexedVerificationTest>,
    requested: &BTreeSet<String>,
) -> Vec<SelectedTest> {
    tests
        .into_iter()
        .filter_map(|test| {
            let identity = test.identity?;
            let requirements = test
                .requirements
                .into_iter()
                .filter(|id| requested.is_empty() || requested.contains(id))
                .collect::<Vec<_>>();
            (!requirements.is_empty()).then_some(SelectedTest {
                identity,
                requirements,
            })
        })
        .collect()
}

fn validate_selected_requirements(
    tests: &[SelectedTest],
    requested: &BTreeSet<String>,
) -> Result<()> {
    if requested.is_empty() {
        return Ok(());
    }
    let selected = tests
        .iter()
        .flat_map(|test| test.requirements.iter())
        .collect::<BTreeSet<_>>();
    let missing = requested
        .iter()
        .filter(|id| !selected.contains(id))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "selected requirement(s) have no resolved test in the package filter: {}",
            missing.join(", ")
        );
    }
    Ok(())
}

fn enforcement_sites(
    root: &Path,
    docs: &[DocSpec],
    metadata: &BTreeMap<String, RequirementMetadata>,
    selected: &BTreeSet<String>,
) -> Result<BTreeMap<String, RequirementAccumulator>> {
    let scan_roots = docs
        .iter()
        .flat_map(DocSpec::scan_roots)
        .collect::<BTreeSet<_>>();
    let anchors = scan(
        root,
        &scan_roots.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    let mut requirements = selected
        .iter()
        .map(|id| {
            let requirement = metadata
                .get(id)
                .expect("BUG: selected requirement metadata is missing");
            (
                id.clone(),
                RequirementAccumulator {
                    id: id.clone(),
                    area: requirement.area.clone(),
                    title: requirement.title.clone(),
                    tests: Vec::new(),
                    sites: BTreeMap::new(),
                    test_failed: false,
                    infrastructure_error: false,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    for anchor in anchors.enforcement {
        let file = anchor.file.to_string_lossy().replace('\\', "/");
        let key = SiteKey {
            file,
            scope_kind: anchor.scope_kind,
            scope: anchor.scope,
        };
        for id in anchor.ids.into_iter().filter(|id| selected.contains(id)) {
            let sites = &mut requirements
                .get_mut(&id)
                .expect("BUG: selected enforcement requirement is missing")
                .sites;
            sites
                .entry(key.clone())
                .and_modify(|site| site.anchor_line = site.anchor_line.min(anchor.line))
                .or_insert_with(|| SiteAccumulator {
                    key: key.clone(),
                    anchor_line: anchor.line,
                    instrumented: BTreeSet::new(),
                    covered: BTreeSet::new(),
                    reached_by: BTreeSet::new(),
                });
        }
    }
    Ok(requirements)
}

fn apply_regions(
    requirement: &mut RequirementAccumulator,
    regions: &RegionIndex,
    test_identity: &str,
) {
    for site in requirement.sites.values_mut() {
        if !site.key.scope_kind.is_potentially_executable() {
            continue;
        }
        let Some(scope) = site.key.scope else {
            continue;
        };
        for region in regions.regions_for(&site.key.file) {
            if ranges_overlap(scope, region.range) {
                site.instrumented.insert(region.range);
                if region.execution_count > 0 {
                    site.covered.insert(region.range);
                    site.reached_by.insert(test_identity.to_string());
                }
            }
        }
    }
}

fn ranges_overlap(left: SourceRange, right: SourceRange) -> bool {
    let left_start = (left.start_line, left.start_column);
    let left_end = (left.end_line, left.end_column);
    let right_start = (right.start_line, right.start_column);
    let right_end = (right.end_line, right.end_column);
    left_start < right_end && right_start < left_end
}

fn canonical_test_identity(identity: &CargoTestIdentity) -> String {
    let kind = match identity.target_kind {
        TestTargetKind::Lib => "lib",
        TestTargetKind::Bin => "bin",
        TestTargetKind::Integration => "integration",
    };
    format!(
        "{}:{kind}:{}:{}",
        identity.package, identity.target_name, identity.fully_qualified_name
    )
}

fn formatted_requirement_ids(requirements: &[String]) -> String {
    requirements
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
#[path = "coverage_tests.rs"]
mod tests;
