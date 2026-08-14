//! Argument parsing for the orchestrated local requirement review command.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use super::{CliBase, ReviewArgs};

#[shallguard::enforces("REQ-CLI-002")]
pub(super) fn parse_review_args(args: &[String]) -> Result<ReviewArgs> {
    let mut provider = None;
    let mut base = None;
    let mut coverage = None;
    let mut bundle = None;
    let mut output = None;
    let mut model = None;
    let mut local_provider = None;
    let mut requirements = BTreeSet::new();
    let mut timeout = None;
    let mut resume = false;
    let mut cache_dir = None;
    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index].as_str();
        index += 1;
        if matches!(flag, "--with-coverage" | "--without-coverage") {
            if coverage.is_some() {
                bail!("choose only one of --with-coverage and --without-coverage");
            }
            coverage = Some(flag == "--with-coverage");
            continue;
        }
        if flag == "--resume" {
            if resume {
                bail!("--resume may be specified only once");
            }
            resume = true;
            continue;
        }
        let value = match flag {
            "--provider" | "--base" | "--target" | "--bundle" | "--output" | "--model"
            | "--local-provider" | "--requirement" | "--timeout-seconds" | "--cache-dir" => args
                .get(index)
                .with_context(|| format!("{flag} requires a value"))?,
            unknown => bail!(
                "unknown argument {unknown:?}; expected --provider, --base, --target, \
                 --with-coverage, --without-coverage, --bundle, --output, --model, \
                 --local-provider, --requirement, --timeout-seconds, --resume, or --cache-dir"
            ),
        };
        index += 1;
        match flag {
            "--provider" => {
                if provider.is_some() {
                    bail!("--provider may be specified only once");
                }
                provider = Some(value.parse()?);
            }
            "--base" => {
                if base.is_some() {
                    bail!("choose exactly one of --base and --target");
                }
                base = Some(CliBase::Revision(value.clone()));
            }
            "--target" => {
                if base.is_some() {
                    bail!("choose exactly one of --base and --target");
                }
                base = Some(CliBase::Target(value.clone()));
            }
            "--bundle" => {
                bundle = Some(PathBuf::from(value));
            }
            "--output" => output = Some(PathBuf::from(value)),
            "--model" => model = Some(value.clone()),
            "--local-provider" => match value.as_str() {
                "ollama" | "lmstudio" => local_provider = Some(value.clone()),
                _ => bail!("--local-provider must be ollama or lmstudio"),
            },
            "--requirement" => {
                requirements.insert(value.clone());
            }
            "--timeout-seconds" => {
                let seconds = value
                    .parse::<u64>()
                    .with_context(|| format!("invalid timeout {value:?}"))?;
                if seconds == 0 {
                    bail!("--timeout-seconds must be greater than zero");
                }
                timeout = Some(Duration::from_secs(seconds));
            }
            "--cache-dir" => {
                if cache_dir.is_some() {
                    bail!("--cache-dir may be specified only once");
                }
                cache_dir = Some(PathBuf::from(value));
            }
            _ => unreachable!("flag matched above"),
        }
    }
    if local_provider.is_some()
        && provider.is_some_and(|provider| provider != shallguard::review::ReviewProvider::Codex)
    {
        bail!("--local-provider is supported only with --provider codex");
    }
    Ok(ReviewArgs {
        provider,
        base,
        with_coverage: coverage,
        bundle,
        output,
        model,
        local_provider,
        requirements,
        timeout,
        resume,
        cache_dir,
    })
}
