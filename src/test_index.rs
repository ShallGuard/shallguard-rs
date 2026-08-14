//! Exact Cargo test identity resolution for verification anchors.
//!
//! Source paths and function names are first mapped to Cargo targets, then
//! checked against an enumerated or pre-recorded test-harness catalog.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::DocSpec;
use crate::scan::{VerificationAnchor, scan};

/// Version of the exact verification-test index artifact.
pub const TEST_INDEX_SCHEMA: &str = "shallguard.requirement-test-index/v1";
/// Version of the reusable Cargo harness-list input.
pub const HARNESS_CATALOG_SCHEMA: &str = "shallguard.test-harness-catalog/v1";

/// Inputs controlling exact test identity resolution.
pub struct TestIndexOptions<'a> {
    /// Source of authoritative Cargo harness test names.
    pub harness: HarnessSource<'a>,
    /// Optional package allowlist. Empty selects every package with anchors.
    pub packages: &'a BTreeSet<String>,
    /// Optional reusable harness catalog written after live enumeration.
    pub catalog_output: Option<&'a Path>,
}

/// Source of authoritative Cargo harness test names.
#[derive(Clone, Copy)]
pub enum HarnessSource<'a> {
    /// Invoke `cargo test -- --list` once for each required target.
    Enumerate,
    /// Load a previously recorded harness catalog.
    Catalog(&'a Path),
}

/// Complete deterministic verification-test index.
#[derive(Debug, Serialize)]
pub struct TestIndexArtifact {
    pub schema: &'static str,
    pub head_commit: String,
    pub working_tree_dirty: bool,
    pub configuration: TestIndexConfiguration,
    pub tests: Vec<IndexedVerificationTest>,
    pub findings: Vec<TestIndexFinding>,
}

/// Resolution inputs recorded for reproducibility.
#[derive(Debug, Serialize)]
pub struct TestIndexConfiguration {
    pub harness_source: String,
    pub default_features: bool,
    pub features: Vec<String>,
    pub packages: Vec<String>,
    pub targets: Vec<CargoTargetIdentity>,
}

/// One valid `#[verifies]` source anchor and its Cargo resolution.
#[derive(Debug, Serialize)]
pub struct IndexedVerificationTest {
    pub file: String,
    pub line: usize,
    pub function: String,
    pub syntactic_name: Option<String>,
    pub requirements: Vec<String>,
    pub status: ResolutionStatus,
    pub match_kind: Option<ResolutionMatch>,
    pub identity: Option<CargoTestIdentity>,
    pub candidates: Vec<String>,
    pub message: Option<String>,
}

/// Exact identity used to invoke and attribute one Cargo test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CargoTestIdentity {
    pub package: String,
    pub target_kind: TestTargetKind,
    pub target_name: String,
    pub fully_qualified_name: String,
}

/// Cargo test target without the harness test name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CargoTargetIdentity {
    pub package: String,
    pub target_kind: TestTargetKind,
    pub target_name: String,
}

/// Target classes supported by Cargo test harness enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestTargetKind {
    Lib,
    Bin,
    Integration,
}

/// Outcome of resolving one verification anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStatus {
    Resolved,
    Unresolved,
    Ambiguous,
    TargetUnresolved,
}

/// How a resolved source identity matched the Cargo harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionMatch {
    ExactSyntacticName,
    UniqueFunctionSuffix,
}

/// Deterministic resolution or enumeration finding.
#[derive(Debug, Serialize)]
pub struct TestIndexFinding {
    pub code: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub message: String,
}

/// Reusable input containing test names emitted by Cargo harnesses.
#[derive(Debug, Serialize, Deserialize)]
pub struct HarnessCatalog {
    pub schema: String,
    pub head_commit: String,
    pub default_features: bool,
    pub features: Vec<String>,
    pub targets: Vec<HarnessTarget>,
}

/// Harness names for one Cargo target.
#[derive(Debug, Serialize, Deserialize)]
pub struct HarnessTarget {
    pub package: String,
    pub target_kind: TestTargetKind,
    pub target_name: String,
    pub tests: Vec<String>,
}

impl TestIndexArtifact {
    /// Returns true when one or more anchors lack an exact identity.
    pub fn has_resolution_errors(&self) -> bool {
        !self.findings.is_empty()
    }

    /// Serializes the artifact as stable, pretty-printed JSON.
    pub fn to_json(&self) -> Result<String> {
        let mut json = serde_json::to_string_pretty(self).context("serializing test index")?;
        json.push('\n');
        Ok(json)
    }
}

impl TestTargetKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Lib => "lib",
            Self::Bin => "bin",
            Self::Integration => "integration",
        }
    }
}

/// Resolves all valid verification anchors in the selected packages.
///
/// # Errors
///
/// Returns an error when Cargo metadata, Git state, source scanning, or a
/// supplied catalog cannot be read reliably. Individual target enumeration
/// failures are findings so the artifact can still be published.
pub fn generate(
    root: &Path,
    docs: &[DocSpec],
    options: &TestIndexOptions<'_>,
) -> Result<TestIndexArtifact> {
    let working_tree_dirty = working_tree_dirty(root)?;
    if working_tree_dirty
        && (matches!(options.harness, HarnessSource::Catalog(_))
            || options.catalog_output.is_some())
    {
        bail!(
            "harness catalog input/output requires a clean working tree; use --enumerate for \
             dirty local sources"
        );
    }
    let metadata = load_metadata(root)?;
    validate_package_filter(&metadata, options.packages)?;
    let scan_roots = docs
        .iter()
        .map(|doc| doc.default_crate.as_str())
        .collect::<BTreeSet<_>>();
    let anchors = scan(root, &scan_roots.into_iter().collect::<Vec<_>>())?;

    let candidates = anchors
        .verification
        .iter()
        .filter_map(|anchor| source_candidate(root, anchor, &metadata, options.packages))
        .collect::<Vec<_>>();
    let candidates = merge_candidates(candidates);

    let required_targets = candidates
        .iter()
        .filter_map(|candidate| candidate.target.as_ref().ok().cloned())
        .collect::<BTreeSet<_>>();
    let head_commit = git_head(root)?;
    let catalog = match options.harness {
        HarnessSource::Enumerate => enumerate_targets(root, &required_targets),
        HarnessSource::Catalog(path) => load_catalog(path, &head_commit, &required_targets)?,
    };
    if let Some(path) = options.catalog_output {
        if !matches!(options.harness, HarnessSource::Enumerate) {
            bail!("--catalog-output is valid only with live harness enumeration");
        }
        write_catalog(path, &head_commit, &catalog.tests)?;
    }

    let mut findings = catalog.findings;
    let tests = candidates
        .into_iter()
        .map(|candidate| resolve_candidate(candidate, &catalog.tests, &mut findings))
        .collect::<Vec<_>>();
    findings.sort_by(|a, b| {
        a.code
            .cmp(&b.code)
            .then(a.file.cmp(&b.file))
            .then(a.line.cmp(&b.line))
            .then(a.message.cmp(&b.message))
    });

    Ok(TestIndexArtifact {
        schema: TEST_INDEX_SCHEMA,
        head_commit,
        working_tree_dirty,
        configuration: TestIndexConfiguration {
            harness_source: match options.harness {
                HarnessSource::Enumerate => "cargo-enumeration".to_string(),
                HarnessSource::Catalog(path) => format!("catalog:{}", path.display()),
            },
            default_features: true,
            features: Vec::new(),
            packages: if options.packages.is_empty() {
                required_targets
                    .iter()
                    .map(|target| target.package.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect()
            } else {
                options.packages.iter().cloned().collect()
            },
            targets: required_targets.into_iter().collect(),
        },
        tests,
        findings,
    })
}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<MetadataPackage>,
}

#[derive(Debug, Deserialize)]
struct MetadataPackage {
    name: String,
    manifest_path: PathBuf,
    targets: Vec<MetadataTarget>,
}

#[derive(Debug, Deserialize)]
struct MetadataTarget {
    name: String,
    kind: Vec<String>,
    src_path: PathBuf,
    test: bool,
}

#[derive(Debug)]
struct SourceCandidate {
    file: String,
    line: usize,
    function: String,
    requirements: Vec<String>,
    target: std::result::Result<CargoTargetIdentity, String>,
    syntactic_name: Option<String>,
}

#[derive(Debug, Default)]
struct LoadedCatalog {
    tests: BTreeMap<CargoTargetIdentity, Vec<String>>,
    findings: Vec<TestIndexFinding>,
}

fn merge_candidates(mut candidates: Vec<SourceCandidate>) -> Vec<SourceCandidate> {
    candidates.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.syntactic_name.cmp(&b.syntactic_name))
            .then(a.function.cmp(&b.function))
            .then(a.line.cmp(&b.line))
    });
    let mut merged: Vec<SourceCandidate> = Vec::with_capacity(candidates.len());
    for mut candidate in candidates {
        candidate.requirements.sort();
        candidate.requirements.dedup();
        if let Some(existing) = merged.last_mut()
            && existing.file == candidate.file
            && existing.function == candidate.function
            && existing.syntactic_name == candidate.syntactic_name
            && existing.target == candidate.target
        {
            existing.line = existing.line.min(candidate.line);
            existing.requirements.extend(candidate.requirements);
            existing.requirements.sort();
            existing.requirements.dedup();
        } else {
            merged.push(candidate);
        }
    }
    merged
}

fn load_metadata(root: &Path) -> Result<CargoMetadata> {
    let output = ProcessCommand::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
        .current_dir(root)
        .output()
        .context("running cargo metadata for verification tests")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    serde_json::from_slice(&output.stdout).context("parsing Cargo metadata")
}

fn validate_package_filter(metadata: &CargoMetadata, selected: &BTreeSet<String>) -> Result<()> {
    let known = metadata
        .packages
        .iter()
        .map(|package| package.name.as_str())
        .collect::<BTreeSet<_>>();
    let unknown = selected
        .iter()
        .filter(|package| !known.contains(package.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        bail!("unknown Cargo package(s): {}", unknown.join(", "));
    }
    Ok(())
}

fn source_candidate(
    root: &Path,
    anchor: &VerificationAnchor,
    metadata: &CargoMetadata,
    selected: &BTreeSet<String>,
) -> Option<SourceCandidate> {
    let absolute = root.join(&anchor.file);
    let package = owning_package(&absolute, metadata)?;
    if !selected.is_empty() && !selected.contains(&package.name) {
        return None;
    }
    let target = select_target(&absolute, package);
    let syntactic_name = target
        .as_ref()
        .ok()
        .and_then(|target| static_test_name(&absolute, package, target, anchor));
    Some(SourceCandidate {
        file: anchor.file.to_string_lossy().into_owned(),
        line: anchor.line,
        function: anchor.test_fn.clone(),
        requirements: anchor.ids.clone(),
        target,
        syntactic_name,
    })
}

fn owning_package<'a>(source: &Path, metadata: &'a CargoMetadata) -> Option<&'a MetadataPackage> {
    metadata
        .packages
        .iter()
        .filter(|package| {
            package
                .manifest_path
                .parent()
                .is_some_and(|root| source.starts_with(root))
        })
        .max_by_key(|package| package.manifest_path.components().count())
}

fn select_target(
    source: &Path,
    package: &MetadataPackage,
) -> std::result::Result<CargoTargetIdentity, String> {
    let supported = package
        .targets
        .iter()
        .filter_map(|target| {
            target_identity(&package.name, target).map(|identity| (target, identity))
        })
        .collect::<Vec<_>>();
    let exact = supported
        .iter()
        .filter(|(target, _)| target.src_path == source)
        .map(|(_, identity)| identity.clone())
        .collect::<Vec<_>>();
    if exact.len() == 1 {
        return Ok(exact[0].clone());
    }

    let package_root = package
        .manifest_path
        .parent()
        .expect("BUG: Cargo manifest path has no parent");
    let relative = source.strip_prefix(package_root).map_err(|_| {
        format!(
            "source {} is outside package {}",
            source.display(),
            package.name
        )
    })?;
    let mut matches = Vec::new();
    if relative.starts_with("src/bin") {
        matches.extend(
            supported
                .iter()
                .filter(|(target, identity)| {
                    identity.target_kind == TestTargetKind::Bin
                        && target
                            .src_path
                            .file_name()
                            .is_some_and(|name| name == "main.rs")
                        && target
                            .src_path
                            .parent()
                            .is_some_and(|parent| source.starts_with(parent))
                })
                .map(|(_, identity)| identity.clone()),
        );
    } else if relative.starts_with("src") {
        matches.extend(
            supported
                .iter()
                .filter(|(_, identity)| identity.target_kind == TestTargetKind::Lib)
                .map(|(_, identity)| identity.clone()),
        );
        if matches.is_empty() {
            matches.extend(
                supported
                    .iter()
                    .filter(|(target, identity)| {
                        identity.target_kind == TestTargetKind::Bin
                            && target
                                .src_path
                                .file_name()
                                .is_some_and(|name| name == "main.rs")
                            && target
                                .src_path
                                .parent()
                                .is_some_and(|parent| source.starts_with(parent))
                    })
                    .map(|(_, identity)| identity.clone()),
            );
        }
    } else if relative.starts_with("tests") {
        matches.extend(
            supported
                .iter()
                .filter(|(target, identity)| {
                    identity.target_kind == TestTargetKind::Integration
                        && integration_source_contains(&target.src_path, source)
                })
                .map(|(_, identity)| identity.clone()),
        );
    }
    matches.sort();
    matches.dedup();
    match matches.as_slice() {
        [target] => Ok(target.clone()),
        [] => Err(format!(
            "no Cargo test target owns source {} in package {}",
            source.display(),
            package.name
        )),
        _ => Err(format!(
            "multiple Cargo test targets may own source {}: {}",
            source.display(),
            matches
                .iter()
                .map(|target| format!("{}:{}", target.target_kind.as_str(), target.target_name))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn target_identity(package: &str, target: &MetadataTarget) -> Option<CargoTargetIdentity> {
    if !target.test {
        return None;
    }
    let target_kind = if target.kind.iter().any(|kind| kind == "lib") {
        TestTargetKind::Lib
    } else if target.kind.iter().any(|kind| kind == "bin") {
        TestTargetKind::Bin
    } else if target.kind.iter().any(|kind| kind == "test") {
        TestTargetKind::Integration
    } else {
        return None;
    };
    Some(CargoTargetIdentity {
        package: package.to_string(),
        target_kind,
        target_name: target.name.clone(),
    })
}

fn integration_source_contains(target_root: &Path, source: &Path) -> bool {
    if target_root
        .file_name()
        .is_some_and(|name| name == "main.rs")
    {
        return target_root
            .parent()
            .is_some_and(|parent| source.starts_with(parent));
    }
    let Some(stem) = target_root.file_stem() else {
        return false;
    };
    target_root
        .parent()
        .is_some_and(|tests| source.starts_with(tests.join(stem)) || source == target_root)
}

fn static_test_name(
    source: &Path,
    package: &MetadataPackage,
    target: &CargoTargetIdentity,
    anchor: &VerificationAnchor,
) -> Option<String> {
    let metadata_target = package
        .targets
        .iter()
        .find(|candidate| target_identity(&package.name, candidate).as_ref() == Some(target))?;
    let mut modules = match target.target_kind {
        TestTargetKind::Lib => {
            let package_root = package.manifest_path.parent()?;
            file_modules(source.strip_prefix(package_root.join("src")).ok()?)
        }
        TestTargetKind::Bin | TestTargetKind::Integration => {
            if source == metadata_target.src_path {
                Vec::new()
            } else {
                let module_root = if metadata_target
                    .src_path
                    .file_name()
                    .is_some_and(|name| name == "main.rs")
                {
                    metadata_target.src_path.parent()?.to_path_buf()
                } else {
                    metadata_target
                        .src_path
                        .parent()?
                        .join(&metadata_target.name)
                };
                file_modules(source.strip_prefix(&module_root).ok()?)
            }
        }
    };
    modules.extend(anchor.inline_modules.iter().cloned());
    modules.push(anchor.test_fn.clone());
    Some(modules.join("::"))
}

fn file_modules(relative: &Path) -> Vec<String> {
    let mut modules = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str().map(str::to_string),
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => None,
        })
        .collect::<Vec<_>>();
    let Some(file) = modules.pop() else {
        return modules;
    };
    let stem = file.strip_suffix(".rs").unwrap_or(&file);
    if !matches!(stem, "lib" | "main" | "mod") {
        modules.push(stem.to_string());
    }
    modules
        .into_iter()
        .map(|module| module.replace('-', "_"))
        .collect()
}

fn load_catalog(
    path: &Path,
    head_commit: &str,
    required: &BTreeSet<CargoTargetIdentity>,
) -> Result<LoadedCatalog> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading harness catalog {}", path.display()))?;
    let catalog: HarnessCatalog = serde_json::from_str(&text)
        .with_context(|| format!("parsing harness catalog {}", path.display()))?;
    if catalog.schema != HARNESS_CATALOG_SCHEMA {
        bail!(
            "unsupported harness catalog schema {:?}; expected {HARNESS_CATALOG_SCHEMA:?}",
            catalog.schema
        );
    }
    if catalog.head_commit != head_commit {
        bail!(
            "harness catalog belongs to commit {}, but HEAD is {}",
            catalog.head_commit,
            head_commit
        );
    }
    if !catalog.default_features || !catalog.features.is_empty() {
        bail!("harness catalog build configuration differs from supported default features");
    }
    let mut loaded = LoadedCatalog::default();
    for mut target in catalog.targets {
        let identity = CargoTargetIdentity {
            package: target.package,
            target_kind: target.target_kind,
            target_name: target.target_name,
        };
        target.tests.sort();
        target.tests.dedup();
        if loaded
            .tests
            .insert(identity.clone(), target.tests)
            .is_some()
        {
            bail!(
                "harness catalog repeats target {}:{}:{}",
                identity.package,
                identity.target_kind.as_str(),
                identity.target_name
            );
        }
    }
    for target in required {
        if !loaded.tests.contains_key(target) {
            loaded.findings.push(TestIndexFinding {
                code: "harness-target-missing".to_string(),
                file: None,
                line: None,
                message: format!(
                    "catalog has no harness list for {}:{}:{}",
                    target.package,
                    target.target_kind.as_str(),
                    target.target_name
                ),
            });
        }
    }
    Ok(loaded)
}

fn write_catalog(
    path: &Path,
    head_commit: &str,
    targets: &BTreeMap<CargoTargetIdentity, Vec<String>>,
) -> Result<()> {
    let catalog = HarnessCatalog {
        schema: HARNESS_CATALOG_SCHEMA.to_string(),
        head_commit: head_commit.to_string(),
        default_features: true,
        features: Vec::new(),
        targets: targets
            .iter()
            .map(|(identity, tests)| HarnessTarget {
                package: identity.package.clone(),
                target_kind: identity.target_kind,
                target_name: identity.target_name.clone(),
                tests: tests.clone(),
            })
            .collect(),
    };
    let mut json =
        serde_json::to_string_pretty(&catalog).context("serializing Cargo harness catalog")?;
    json.push('\n');
    std::fs::write(path, json)
        .with_context(|| format!("writing harness catalog {}", path.display()))
}

fn enumerate_targets(root: &Path, targets: &BTreeSet<CargoTargetIdentity>) -> LoadedCatalog {
    let mut loaded = LoadedCatalog::default();
    for target in targets {
        match enumerate_target(root, target) {
            Ok(tests) => {
                loaded.tests.insert(target.clone(), tests);
            }
            Err(error) => loaded.findings.push(TestIndexFinding {
                code: "harness-enumeration-failed".to_string(),
                file: None,
                line: None,
                message: format!(
                    "cannot enumerate {}:{}:{}: {error:#}",
                    target.package,
                    target.target_kind.as_str(),
                    target.target_name
                ),
            }),
        }
    }
    loaded
}

fn enumerate_target(root: &Path, target: &CargoTargetIdentity) -> Result<Vec<String>> {
    let mut command = ProcessCommand::new("cargo");
    command.args(["test", "--locked", "-p", &target.package]);
    match target.target_kind {
        TestTargetKind::Lib => {
            command.arg("--lib");
        }
        TestTargetKind::Bin => {
            command.args(["--bin", &target.target_name]);
        }
        TestTargetKind::Integration => {
            command.args(["--test", &target.target_name]);
        }
    }
    let output = command
        .args(["--", "--list", "--format", "terse"])
        .current_dir(root)
        .output()
        .context("starting Cargo test harness enumeration")?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    let stdout = String::from_utf8(output.stdout).context("test harness list is not UTF-8")?;
    Ok(parse_harness_list(&stdout))
}

fn parse_harness_list(output: &str) -> Vec<String> {
    let mut tests = output
        .lines()
        .filter_map(|line| {
            line.strip_suffix(": test")
                .or_else(|| line.strip_suffix(": benchmark"))
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    tests.sort();
    tests.dedup();
    tests
}

fn resolve_candidate(
    candidate: SourceCandidate,
    catalog: &BTreeMap<CargoTargetIdentity, Vec<String>>,
    findings: &mut Vec<TestIndexFinding>,
) -> IndexedVerificationTest {
    let target = match &candidate.target {
        Ok(target) => target.clone(),
        Err(message) => {
            let message = message.clone();
            findings.push(anchor_finding(
                "cargo-target-unresolved",
                &candidate,
                &message,
            ));
            return unresolved_test(
                candidate,
                ResolutionStatus::TargetUnresolved,
                Vec::new(),
                message,
            );
        }
    };
    let Some(harness_tests) = catalog.get(&target) else {
        let message = format!(
            "no harness list available for {}:{}:{}",
            target.package,
            target.target_kind.as_str(),
            target.target_name
        );
        findings.push(anchor_finding(
            "harness-target-unavailable",
            &candidate,
            &message,
        ));
        return unresolved_test(candidate, ResolutionStatus::Unresolved, Vec::new(), message);
    };

    let exact = candidate
        .syntactic_name
        .as_ref()
        .filter(|name| harness_tests.binary_search(name).is_ok())
        .cloned();
    let match_kind = if exact.is_some() {
        ResolutionMatch::ExactSyntacticName
    } else {
        ResolutionMatch::UniqueFunctionSuffix
    };
    let matches = exact.map_or_else(
        || {
            harness_tests
                .iter()
                .filter(|name| name.rsplit("::").next() == Some(candidate.function.as_str()))
                .cloned()
                .collect::<Vec<_>>()
        },
        |name| vec![name],
    );
    match matches.as_slice() {
        [name] => IndexedVerificationTest {
            file: candidate.file,
            line: candidate.line,
            function: candidate.function,
            syntactic_name: candidate.syntactic_name,
            requirements: candidate.requirements,
            status: ResolutionStatus::Resolved,
            match_kind: Some(match_kind),
            identity: Some(CargoTestIdentity {
                package: target.package,
                target_kind: target.target_kind,
                target_name: target.target_name,
                fully_qualified_name: name.clone(),
            }),
            candidates: Vec::new(),
            message: None,
        },
        [] => {
            let message = format!(
                "test function {:?} is absent from harness {}:{}:{}",
                candidate.function,
                target.package,
                target.target_kind.as_str(),
                target.target_name
            );
            findings.push(anchor_finding(
                "harness-test-unresolved",
                &candidate,
                &message,
            ));
            unresolved_test(candidate, ResolutionStatus::Unresolved, Vec::new(), message)
        }
        _ => {
            let message = format!(
                "test function {:?} matches {} harness tests",
                candidate.function,
                matches.len()
            );
            findings.push(anchor_finding(
                "harness-test-ambiguous",
                &candidate,
                &message,
            ));
            unresolved_test(candidate, ResolutionStatus::Ambiguous, matches, message)
        }
    }
}

fn anchor_finding(code: &str, candidate: &SourceCandidate, message: &str) -> TestIndexFinding {
    TestIndexFinding {
        code: code.to_string(),
        file: Some(candidate.file.clone()),
        line: Some(candidate.line),
        message: message.to_string(),
    }
}

fn unresolved_test(
    candidate: SourceCandidate,
    status: ResolutionStatus,
    candidates: Vec<String>,
    message: String,
) -> IndexedVerificationTest {
    IndexedVerificationTest {
        file: candidate.file,
        line: candidate.line,
        function: candidate.function,
        syntactic_name: candidate.syntactic_name,
        requirements: candidate.requirements,
        status,
        match_kind: None,
        identity: None,
        candidates,
        message: Some(message),
    }
}

fn git_head(root: &Path) -> Result<String> {
    let output = ProcessCommand::new("git")
        .args(["rev-parse", "--verify", "HEAD^{commit}"])
        .current_dir(root)
        .output()
        .context("resolving HEAD for test index")?;
    if !output.status.success() {
        bail!(
            "cannot resolve HEAD: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout)
        .context("HEAD commit is not UTF-8")
        .map(|head| head.trim().to_string())
}

fn working_tree_dirty(root: &Path) -> Result<bool> {
    let output = ProcessCommand::new("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(root)
        .output()
        .context("inspecting working tree for test index")?;
    if !output.status.success() {
        bail!(
            "cannot inspect working tree: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(!output.stdout.is_empty())
}

#[cfg(test)]
#[path = "test_index_tests.rs"]
mod tests;
