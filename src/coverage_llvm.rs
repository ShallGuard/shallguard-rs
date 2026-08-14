//! Isolated `cargo llvm-cov` execution and LLVM JSON region ingestion.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::scan::SourceRange;
use crate::test_index::{CargoTestIdentity, TestTargetKind};

#[derive(Debug)]
pub(super) enum InvocationOutcome {
    Passed {
        regions: RegionIndex,
        export_digest: String,
    },
    Failed(String),
    InfrastructureError(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ExecutableRegion {
    pub range: SourceRange,
    pub execution_count: u64,
}

#[derive(Debug, Default)]
pub(super) struct RegionIndex {
    pub by_file: BTreeMap<String, Vec<ExecutableRegion>>,
}

impl RegionIndex {
    pub fn regions_for(&self, file: &str) -> &[ExecutableRegion] {
        self.by_file.get(file).map_or(&[], Vec::as_slice)
    }

    fn digest(&self) -> String {
        let mut digest = Sha256::new();
        for (file, regions) in &self.by_file {
            digest.update(file.as_bytes());
            digest.update([0]);
            for region in regions {
                digest.update(region.range.start_line.to_le_bytes());
                digest.update(region.range.start_column.to_le_bytes());
                digest.update(region.range.end_line.to_le_bytes());
                digest.update(region.range.end_column.to_le_bytes());
                digest.update(region.execution_count.to_le_bytes());
            }
        }
        let digest = digest.finalize();
        format!("sha256:{digest:x}")
    }
}

pub(super) fn tool_version(root: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = ProcessCommand::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| format!("running {program} {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "{program} {} failed: {}",
            args.join(" "),
            output_excerpt(&output.stdout, &output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(super) fn prepare(root: &Path) -> Result<()> {
    let output = ProcessCommand::new("cargo")
        .args(["llvm-cov", "clean", "--workspace", "--locked"])
        .current_dir(root)
        .output()
        .context("cleaning prior cargo-llvm-cov build and profile artifacts")?;
    if !output.status.success() {
        bail!(
            "cargo llvm-cov clean failed: {}",
            output_excerpt(&output.stdout, &output.stderr)
        );
    }
    Ok(())
}

pub(super) fn collect_test(
    root: &Path,
    work_dir: &Path,
    identity: &CargoTestIdentity,
) -> Result<InvocationOutcome> {
    clean_profiles(root)?;
    std::fs::create_dir_all(work_dir)
        .with_context(|| format!("creating coverage work directory {}", work_dir.display()))?;
    let output_path = work_dir.join(format!("{}.json", identity_digest(identity)));

    let mut command = ProcessCommand::new("cargo");
    command.args([
        "llvm-cov",
        "test",
        "--locked",
        "--no-clean",
        "--json",
        "--output-path",
    ]);
    command.arg(&output_path).args(["-p", &identity.package]);
    match identity.target_kind {
        TestTargetKind::Lib => {
            command.arg("--lib");
        }
        TestTargetKind::Bin => {
            command.args(["--bin", &identity.target_name]);
        }
        TestTargetKind::Integration => {
            command.args(["--test", &identity.target_name]);
        }
    }
    command
        .args(["--", &identity.fully_qualified_name, "--exact"])
        .current_dir(root);

    let output = command
        .output()
        .with_context(|| format!("running coverage for {}", identity.fully_qualified_name))?;
    let command_output = output_excerpt(&output.stdout, &output.stderr);
    if !output.status.success() {
        if command_output.contains("test result: FAILED") {
            return Ok(InvocationOutcome::Failed(command_output));
        }
        return Ok(InvocationOutcome::InfrastructureError(command_output));
    }
    if !String::from_utf8_lossy(&output.stdout).contains("running 1 test") {
        return Ok(InvocationOutcome::InfrastructureError(format!(
            "exact harness invocation did not run one test: {command_output}"
        )));
    }

    let bytes = std::fs::read(&output_path)
        .with_context(|| format!("reading LLVM export {}", output_path.display()))?;
    let regions = parse_export(root, &bytes)
        .with_context(|| format!("parsing LLVM export {}", output_path.display()))?;
    let export_digest = regions.digest();
    std::fs::remove_file(&output_path)
        .with_context(|| format!("removing ingested LLVM export {}", output_path.display()))?;
    Ok(InvocationOutcome::Passed {
        regions,
        export_digest,
    })
}

fn clean_profiles(root: &Path) -> Result<()> {
    let output = ProcessCommand::new("cargo")
        .args(["llvm-cov", "clean", "--profraw-only", "--locked"])
        .current_dir(root)
        .output()
        .context("cleaning prior per-test LLVM profiles")?;
    if !output.status.success() {
        bail!(
            "cargo llvm-cov clean --profraw-only failed: {}",
            output_excerpt(&output.stdout, &output.stderr)
        );
    }
    Ok(())
}

fn identity_digest(identity: &CargoTestIdentity) -> String {
    let value = format!(
        "{}:{:?}:{}:{}",
        identity.package, identity.target_kind, identity.target_name, identity.fully_qualified_name
    );
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn output_excerpt(stdout: &[u8], stderr: &[u8]) -> String {
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(stderr),
        String::from_utf8_lossy(stdout)
    );
    let lines = combined.lines().collect::<Vec<_>>();
    lines[lines.len().saturating_sub(24)..].join("\n")
}

#[derive(Deserialize)]
struct LlvmExport {
    #[serde(rename = "type")]
    export_type: String,
    data: Vec<LlvmData>,
}

#[derive(Deserialize)]
struct LlvmData {
    functions: Vec<LlvmFunction>,
}

#[derive(Deserialize)]
struct LlvmFunction {
    filenames: Vec<String>,
    regions: Vec<[u64; 8]>,
}

fn parse_export(root: &Path, bytes: &[u8]) -> Result<RegionIndex> {
    let export: LlvmExport =
        serde_json::from_slice(bytes).context("decoding LLVM coverage JSON")?;
    if export.export_type != "llvm.coverage.json.export" {
        bail!("unexpected LLVM export type {:?}", export.export_type);
    }
    let mut unique = BTreeMap::<(String, SourceRange), u64>::new();
    for data in export.data {
        for function in data.functions {
            for raw in function.regions {
                let [
                    start_line,
                    start_column,
                    end_line,
                    end_column,
                    count,
                    file_id,
                    _,
                    kind,
                ] = raw;
                if kind != 0 || start_line == 0 || end_line == 0 {
                    continue;
                }
                let Some(filename) = function.filenames.get(file_id as usize) else {
                    continue;
                };
                let Some(file) = workspace_relative(root, Path::new(filename)) else {
                    continue;
                };
                let range = SourceRange {
                    start_line: start_line as usize,
                    start_column: start_column as usize,
                    end_line: end_line as usize,
                    end_column: end_column as usize,
                };
                unique
                    .entry((file, range))
                    .and_modify(|existing| *existing = (*existing).max(count))
                    .or_insert(count);
            }
        }
    }

    let mut by_file = BTreeMap::<String, Vec<ExecutableRegion>>::new();
    for ((file, range), execution_count) in unique {
        by_file.entry(file).or_default().push(ExecutableRegion {
            range,
            execution_count,
        });
    }
    Ok(RegionIndex { by_file })
}

fn workspace_relative(root: &Path, source: &Path) -> Option<String> {
    let relative = if source.is_absolute() {
        source.strip_prefix(root).ok()?
    } else {
        source.strip_prefix(".").unwrap_or(source)
    };
    if relative.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir | std::path::Component::RootDir
        )
    }) {
        return None;
    }
    Some(relative.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
#[path = "coverage_llvm_tests.rs"]
mod tests;
