//! Execution and artifact output for the deterministic impact command.

use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};

use super::{COMMAND_NAME, CliBase, ImpactArgs};

#[shallguard::enforces("REQ-IMP-007")]
pub(super) fn run_impact(
    root: &Path,
    docs: &[shallguard::DocSpec],
    config: &shallguard::config::RepositoryConfig,
    args: &ImpactArgs,
) -> ExitCode {
    let base = match args.base.as_ref() {
        Some(CliBase::Revision(revision)) => shallguard::impact::BaseSelection::Revision(revision),
        Some(CliBase::Target(target)) => shallguard::impact::BaseSelection::MergeBaseWith(target),
        None => match config.review.target.as_deref() {
            Some(target) => shallguard::impact::BaseSelection::MergeBaseWith(target),
            None => {
                eprintln!(
                    "{COMMAND_NAME} impact failed: provide --base or --target, set a CI base, \
                     or configure review.target in shallguard.toml"
                );
                return ExitCode::FAILURE;
            }
        },
    };
    let options = shallguard::impact::ImpactOptions {
        base,
        baseline_path: &config.baseline,
    };
    let artifact = match shallguard::impact::analyze(root, docs, &options) {
        Ok(artifact) => artifact,
        Err(err) => {
            eprintln!("{COMMAND_NAME} impact failed: {err:#}");
            return ExitCode::FAILURE;
        }
    };
    let policy_failed = artifact.has_policy_errors();
    let json = match artifact.to_json() {
        Ok(json) => json,
        Err(err) => {
            eprintln!("{COMMAND_NAME} impact failed: {err:#}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(err) = write_artifact(&args.json, &json) {
        eprintln!("{COMMAND_NAME} impact failed: {err:#}");
        return ExitCode::FAILURE;
    }
    if let Some(path) = &args.markdown
        && let Err(err) = write_artifact(path, &artifact.to_markdown())
    {
        eprintln!("{COMMAND_NAME} impact failed: {err:#}");
        return ExitCode::FAILURE;
    }
    if policy_failed {
        eprintln!("{COMMAND_NAME} impact: deterministic policy findings require changes");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

pub(super) fn write_artifact(path: &Path, content: &str) -> Result<()> {
    if path == Path::new("-") {
        print!("{content}");
        return Ok(());
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating artifact directory {}", parent.display()))?;
    }
    std::fs::write(path, content).with_context(|| format!("writing artifact {}", path.display()))
}
