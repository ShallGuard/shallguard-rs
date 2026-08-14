//! Runs the requirement traceability check and prints the report.
//! Exits nonzero when any hard check fails.
//!
//! Usage:
//!
//! ```text
//! cargo shallguard [check] [<doc.md> ...]
//! cargo shallguard fmt [--check] [<doc.md> ...]
//! cargo shallguard lint [<doc.md> ...]
//! cargo shallguard clean
//! cargo shallguard baseline check
//! cargo shallguard baseline prune
//! cargo shallguard impact --base <revision> --json requirement-impact.json
//! cargo shallguard impact --target origin/main --json requirement-impact.json
//! cargo shallguard bundle --impact requirement-impact.json --output requirement-review
//! cargo shallguard test-index --enumerate --json requirement-tests.json
//! cargo shallguard test-index --catalog harness-tests.json --json requirement-tests.json
//! cargo shallguard coverage --requirement REQ-HRS-001 --json requirement-coverage.json
//! cargo shallguard review --provider codex --bundle requirement-review
//! cargo run -p cargo-shallguard -- shallguard check
//! ```
//!
//! Each argument is a workspace-relative requirements document; its
//! owning crate (for resolving unprefixed `src/` references and for
//! choosing which sources to scan) is the path's first component.
//! Without arguments the workspace's configured default documents are checked.
//!
//! Note: only the given documents' crates are scanned, so checking a
//! single document reports cross-crate anchors (e.g. `router:` enforced
//! sites) as missing — pass every related document for a complete view.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result, bail};

#[path = "cli_progress.rs"]
mod cli_progress;
#[path = "cli_review.rs"]
mod cli_review;
use cli_progress::print_progress;
use cli_review::parse_review_args;

/// Warnings printed in full detail before the rest is summarized.
const WARNING_DETAIL_LIMIT: usize = 15;
const COMMAND_NAME: &str = "cargo shallguard";

enum Command {
    Help,
    Clean,
    Check(Vec<String>),
    Format(FormatArgs),
    BaselineInit,
    BaselinePrune,
    Impact(ImpactArgs),
    Bundle(BundleArgs),
    TestIndex(TestIndexArgs),
    Coverage(CoverageArgs),
    Review(ReviewArgs),
}

struct FormatArgs {
    check: bool,
    docs: Vec<String>,
}

struct ImpactArgs {
    base: Option<CliBase>,
    json: PathBuf,
    markdown: Option<PathBuf>,
}

enum CliBase {
    Revision(String),
    Target(String),
}

struct BundleArgs {
    impact: PathBuf,
    coverage: Option<PathBuf>,
    output: Option<PathBuf>,
}

struct TestIndexArgs {
    harness: TestHarnessCli,
    packages: BTreeSet<String>,
    catalog_output: Option<PathBuf>,
    json: PathBuf,
    markdown: Option<PathBuf>,
}

enum TestHarnessCli {
    Enumerate,
    Catalog(PathBuf),
}

struct CoverageArgs {
    packages: BTreeSet<String>,
    requirements: BTreeSet<String>,
    work_dir: Option<PathBuf>,
    json: PathBuf,
    markdown: Option<PathBuf>,
}

struct ReviewArgs {
    provider: Option<shallguard::review::ReviewProvider>,
    base: Option<CliBase>,
    with_coverage: Option<bool>,
    bundle: Option<PathBuf>,
    output: Option<PathBuf>,
    model: Option<String>,
    local_provider: Option<String>,
    requirements: BTreeSet<String>,
    timeout: Option<Duration>,
    resume: bool,
    cache_dir: Option<PathBuf>,
}

#[shallguard::enforces("REQ-CLI-001", "REQ-CLI-002", "REQ-PORT-008", "REQ-SEC-001")]
fn main() -> ExitCode {
    let args = normalized_args(std::env::args().skip(1).collect());
    let command = match args.as_slice() {
        [help] if help == "help" || help == "--help" || help == "-h" => Command::Help,
        [clean] if clean == "clean" => Command::Clean,
        [format, rest @ ..] if format == "fmt" => match parse_format_args(rest, false) {
            Ok(args) => Command::Format(args),
            Err(err) => {
                eprintln!("{COMMAND_NAME} fmt: {err:#}");
                return ExitCode::FAILURE;
            }
        },
        [lint, rest @ ..] if lint == "lint" => match parse_format_args(rest, true) {
            Ok(args) => Command::Format(args),
            Err(err) => {
                eprintln!("{COMMAND_NAME} lint: {err:#}");
                return ExitCode::FAILURE;
            }
        },
        [review, rest @ ..] if review == "review" => match parse_review_args(rest) {
            Ok(args) => Command::Review(args),
            Err(err) => {
                eprintln!("{COMMAND_NAME} review: {err:#}");
                return ExitCode::FAILURE;
            }
        },
        [coverage, rest @ ..] if coverage == "coverage" => match parse_coverage_args(rest) {
            Ok(args) => Command::Coverage(args),
            Err(err) => {
                eprintln!("{COMMAND_NAME} coverage: {err:#}");
                return ExitCode::FAILURE;
            }
        },
        [test_index, rest @ ..] if test_index == "test-index" => {
            match parse_test_index_args(rest) {
                Ok(args) => Command::TestIndex(args),
                Err(err) => {
                    eprintln!("{COMMAND_NAME} test-index: {err:#}");
                    return ExitCode::FAILURE;
                }
            }
        }
        [bundle, rest @ ..] if bundle == "bundle" => match parse_bundle_args(rest) {
            Ok(args) => Command::Bundle(args),
            Err(err) => {
                eprintln!("{COMMAND_NAME} bundle: {err:#}");
                return ExitCode::FAILURE;
            }
        },
        [impact, rest @ ..] if impact == "impact" => match parse_impact_args(rest) {
            Ok(args) => Command::Impact(args),
            Err(err) => {
                eprintln!("{COMMAND_NAME} impact: {err:#}");
                return ExitCode::FAILURE;
            }
        },
        [baseline, action] if baseline == "baseline" && action == "init" => Command::BaselineInit,
        [baseline, action] if baseline == "baseline" && action == "prune" => Command::BaselinePrune,
        [baseline, action, docs @ ..] if baseline == "baseline" && action == "check" => {
            Command::Check(docs.to_vec())
        }
        [baseline, ..] if baseline == "baseline" => {
            eprintln!(
                "{COMMAND_NAME}: expected `baseline check`, `baseline init`, or `baseline prune`"
            );
            return ExitCode::FAILURE;
        }
        [check, docs @ ..] if check == "check" => Command::Check(docs.to_vec()),
        _ => Command::Check(args),
    };

    if matches!(&command, Command::Help) {
        print_help();
        return ExitCode::SUCCESS;
    }

    let doc_args = match &command {
        Command::Check(args) => args.as_slice(),
        Command::Format(args) => args.docs.as_slice(),
        Command::Help
        | Command::Clean
        | Command::BaselineInit
        | Command::BaselinePrune
        | Command::Impact(_)
        | Command::Bundle(_)
        | Command::TestIndex(_)
        | Command::Coverage(_)
        | Command::Review(_) => &[],
    };
    let root = match shallguard::workspace_root() {
        Ok(root) => root,
        Err(err) => {
            eprintln!("{COMMAND_NAME}: {err:#}");
            return ExitCode::FAILURE;
        }
    };
    let config = match shallguard::config::RepositoryConfig::load(&root) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("{COMMAND_NAME}: {err:#}");
            return ExitCode::FAILURE;
        }
    };
    let docs = match config.select_documents(doc_args) {
        Ok(docs) => docs,
        Err(err) => {
            eprintln!("{COMMAND_NAME}: {err:#}");
            return ExitCode::FAILURE;
        }
    };
    match command {
        Command::Help => unreachable!("help returned before workspace discovery"),
        Command::Clean => {
            return run_clean(&root, &config);
        }
        Command::Format(args) => {
            return run_format(&root, &docs, &args);
        }
        Command::BaselineInit => {
            return match shallguard::check::initialize_baseline(&root, &docs, &config) {
                Ok(change) => {
                    println!(
                        "created {} with {} historical gap(s)",
                        change.path.display(),
                        change.entries
                    );
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("{COMMAND_NAME} baseline init failed: {err:#}");
                    ExitCode::FAILURE
                }
            };
        }
        Command::BaselinePrune => {
            return match shallguard::check::prune_baseline(&root, &docs, &config) {
                Ok(change) => {
                    println!(
                        "pruned {} resolved gap(s); {} historical gap(s) remain in {}",
                        change.removed,
                        change.entries,
                        change.path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("{COMMAND_NAME} baseline prune failed: {err:#}");
                    ExitCode::FAILURE
                }
            };
        }
        Command::Impact(args) => {
            return run_impact(&root, &docs, &config, &args);
        }
        Command::Bundle(args) => {
            return run_bundle(&root, &docs, &config, &args);
        }
        Command::TestIndex(args) => {
            return run_test_index(&root, &docs, &args);
        }
        Command::Coverage(args) => {
            return run_coverage(&root, &docs, &config, &args);
        }
        Command::Review(args) => {
            return run_review(&root, &docs, &config, &args);
        }
        Command::Check(_) => {}
    }

    let report = match shallguard::check::run(&root, &docs, &config) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("{COMMAND_NAME} failed to run: {err:#}");
            return ExitCode::FAILURE;
        }
    };
    if report.print(WARNING_DETAIL_LIMIT) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[shallguard::enforces("REQ-CLI-001")]
fn normalized_args(mut args: Vec<String>) -> Vec<String> {
    // Cargo passes the external subcommand name as argv[1] when it invokes an
    // installed `cargo-shallguard` executable. The repository's development
    // alias invokes this binary through `cargo run` and omits that argument.
    if args
        .first()
        .is_some_and(|argument| argument == "shallguard")
    {
        args.remove(0);
    }
    args
}

#[shallguard::enforces("REQ-SEC-004")]
fn run_clean(root: &Path, config: &shallguard::config::RepositoryConfig) -> ExitCode {
    let bundle_dir = config.bundle_dir();
    match shallguard::bundle::clean_bundle(root, &bundle_dir) {
        Ok(Some(path)) => {
            println!("removed generated requirement bundle {}", path.display());
            ExitCode::SUCCESS
        }
        Ok(None) => {
            println!(
                "no generated requirement bundle at {}",
                root.join(bundle_dir).display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{COMMAND_NAME} clean failed: {error:#}");
            ExitCode::FAILURE
        }
    }
}

#[shallguard::enforces("REQ-SPEC-006")]
fn run_format(root: &Path, docs: &[shallguard::DocSpec], args: &FormatArgs) -> ExitCode {
    let report = if args.check {
        shallguard::requirement_format::check(root, docs)
    } else {
        shallguard::requirement_format::format(root, docs)
    };
    let report = match report {
        Ok(report) => report,
        Err(error) => {
            eprintln!("{COMMAND_NAME} fmt failed: {error:#}");
            return ExitCode::FAILURE;
        }
    };

    for diagnostic in &report.diagnostics {
        eprintln!(
            "{}:{}: {}",
            diagnostic.document, diagnostic.line, diagnostic.message
        );
    }
    if !report.diagnostics.is_empty() {
        eprintln!(
            "{COMMAND_NAME} fmt: {} lint failure(s) in {} requirement(s); no files changed",
            report.diagnostics.len(),
            report.requirements
        );
        return ExitCode::FAILURE;
    }

    if args.check && !report.changed_documents.is_empty() {
        for document in &report.changed_documents {
            eprintln!("{}: requirement formatting differs", document.display());
        }
        eprintln!(
            "{COMMAND_NAME} fmt --check: {} of {} document(s) need formatting",
            report.changed_documents.len(),
            report.documents
        );
        return ExitCode::FAILURE;
    }

    if args.check {
        println!(
            "checked {} requirement(s) in {} document(s); formatting and structure are valid",
            report.requirements, report.documents
        );
    } else {
        for document in &report.changed_documents {
            println!("formatted {}", document.display());
        }
        println!(
            "formatted {} requirement(s) in {} document(s); {} changed",
            report.requirements,
            report.documents,
            report.changed_documents.len()
        );
    }
    ExitCode::SUCCESS
}

fn print_help() {
    println!(
        r#"Requirement traceability, executable coverage, and local semantic review.

Usage:
  cargo shallguard [check] [<doc.md> ...]
  cargo shallguard fmt [--check] [<doc.md> ...]
  cargo shallguard lint [<doc.md> ...]
  cargo shallguard clean
  cargo shallguard baseline <check|init|prune>
  cargo shallguard impact --base <revision> [--json <path>] [--markdown <path>]
  cargo shallguard bundle --impact <impact.json> [--coverage <coverage.json>] [options]
  cargo shallguard test-index <--enumerate|--catalog <path>> [options]
  cargo shallguard coverage [--package <crate>] [--requirement <REQ-ID>] [options]
  cargo shallguard review [--base <revision>|--target <branch>] [options]

Review options:
  --provider <name>          Local model CLI [default: shallguard.toml, then codex]
  --base <revision>          Compare against an exact revision
  --target <branch>          Compare against its merge base [default: shallguard.toml]
  --with-coverage            Run impacted verification tests [configured default]
  --without-coverage         Skip executable coverage collection
  --bundle <directory>       Generated bundle, or existing bundle in replay mode
                             [default: shallguard.toml artifact root]
  --output <directory>       New auditable result directory
                             [default: shallguard.toml artifact root]
  --resume                   Continue the compatible run in --output
  --cache-dir <directory>    Reuse validated responses across run directories
  --model <identifier>       Provider-specific model override
  --local-provider <name>    Codex on-device inference: ollama or lmstudio
  --requirement <REQ-ID>     Review only this requirement; repeatable
  --timeout-seconds <n>      Per-requirement timeout [default: shallguard.toml, then 300]

`fmt` lints and formats requirement blocks; `fmt --check` and `lint` never write.
`clean` removes only the validated bundle at the configured artifact location.
Model verdicts are advisory. Provider or schema failures return nonzero."#
    );
}

fn parse_format_args(args: &[String], check_by_default: bool) -> Result<FormatArgs> {
    let mut check = check_by_default;
    let mut check_seen = false;
    let mut docs = Vec::new();
    for argument in args {
        if argument == "--check" {
            if check_seen {
                bail!("--check may be specified only once");
            }
            check = true;
            check_seen = true;
        } else if argument.starts_with('-') {
            bail!("unknown argument {argument:?}; expected --check or a document path");
        } else {
            docs.push(argument.clone());
        }
    }
    Ok(FormatArgs { check, docs })
}

fn parse_impact_args(args: &[String]) -> Result<ImpactArgs> {
    let mut base = None;
    let mut json = PathBuf::from("-");
    let mut markdown = None;
    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index].as_str();
        index += 1;
        let value = match flag {
            "--base" | "--target" | "--json" | "--markdown" => args
                .get(index)
                .with_context(|| format!("{flag} requires a value"))?,
            unknown => {
                bail!(
                    "unknown argument {unknown:?}; expected --base, --target, --json, or \
                     --markdown"
                )
            }
        };
        index += 1;
        match flag {
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
            "--json" => json = PathBuf::from(value),
            "--markdown" => markdown = Some(PathBuf::from(value)),
            _ => unreachable!("flag matched above"),
        }
    }
    let base = base
        .or_else(|| {
            std::env::var("CI_MERGE_REQUEST_DIFF_BASE_SHA")
                .ok()
                .map(CliBase::Revision)
        })
        .or_else(|| {
            std::env::var("CI_DEFAULT_BRANCH")
                .ok()
                .map(|branch| CliBase::Target(format!("origin/{branch}")))
        });
    Ok(ImpactArgs {
        base,
        json,
        markdown,
    })
}

fn parse_bundle_args(args: &[String]) -> Result<BundleArgs> {
    let mut impact = None;
    let mut coverage = None;
    let mut output = None;
    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index].as_str();
        index += 1;
        let value = match flag {
            "--impact" | "--coverage" | "--output" => args
                .get(index)
                .with_context(|| format!("{flag} requires a value"))?,
            unknown => {
                bail!("unknown argument {unknown:?}; expected --impact, --coverage, or --output")
            }
        };
        index += 1;
        match flag {
            "--impact" => impact = Some(PathBuf::from(value)),
            "--coverage" => coverage = Some(PathBuf::from(value)),
            "--output" => output = Some(PathBuf::from(value)),
            _ => unreachable!("flag matched above"),
        }
    }
    Ok(BundleArgs {
        impact: impact.context("provide --impact <requirement-impact.json>")?,
        coverage,
        output,
    })
}

fn parse_test_index_args(args: &[String]) -> Result<TestIndexArgs> {
    let mut harness = None;
    let mut packages = BTreeSet::new();
    let mut catalog_output = None;
    let mut json = PathBuf::from("-");
    let mut markdown = None;
    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index].as_str();
        index += 1;
        if flag == "--enumerate" {
            if harness.is_some() {
                bail!("choose exactly one of --enumerate and --catalog");
            }
            harness = Some(TestHarnessCli::Enumerate);
            continue;
        }
        let value = match flag {
            "--catalog" | "--catalog-output" | "--package" | "--json" | "--markdown" => args
                .get(index)
                .with_context(|| format!("{flag} requires a value"))?,
            unknown => bail!(
                "unknown argument {unknown:?}; expected --enumerate, --catalog, \
                 --catalog-output, --package, --json, or --markdown"
            ),
        };
        index += 1;
        match flag {
            "--catalog" => {
                if harness.is_some() {
                    bail!("choose exactly one of --enumerate and --catalog");
                }
                harness = Some(TestHarnessCli::Catalog(PathBuf::from(value)));
            }
            "--catalog-output" => catalog_output = Some(PathBuf::from(value)),
            "--package" => {
                packages.insert(value.clone());
            }
            "--json" => json = PathBuf::from(value),
            "--markdown" => markdown = Some(PathBuf::from(value)),
            _ => unreachable!("flag matched above"),
        }
    }
    let harness = harness.context("provide --enumerate or --catalog <harness-tests.json>")?;
    if catalog_output.is_some() && !matches!(&harness, TestHarnessCli::Enumerate) {
        bail!("--catalog-output is valid only with --enumerate");
    }
    Ok(TestIndexArgs {
        harness,
        packages,
        catalog_output,
        json,
        markdown,
    })
}

fn parse_coverage_args(args: &[String]) -> Result<CoverageArgs> {
    let mut packages = BTreeSet::new();
    let mut requirements = BTreeSet::new();
    let mut work_dir = None;
    let mut json = PathBuf::from("-");
    let mut markdown = None;
    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index].as_str();
        index += 1;
        let value = match flag {
            "--package" | "--requirement" | "--work-dir" | "--json" | "--markdown" => args
                .get(index)
                .with_context(|| format!("{flag} requires a value"))?,
            unknown => bail!(
                "unknown argument {unknown:?}; expected --package, --requirement, --work-dir, \
                 --json, or --markdown"
            ),
        };
        index += 1;
        match flag {
            "--package" => {
                packages.insert(value.clone());
            }
            "--requirement" => {
                requirements.insert(value.clone());
            }
            "--work-dir" => work_dir = Some(PathBuf::from(value)),
            "--json" => json = PathBuf::from(value),
            "--markdown" => markdown = Some(PathBuf::from(value)),
            _ => unreachable!("flag matched above"),
        }
    }
    Ok(CoverageArgs {
        packages,
        requirements,
        work_dir,
        json,
        markdown,
    })
}

#[shallguard::enforces("REQ-IMP-007")]
fn run_impact(
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

fn write_artifact(path: &Path, content: &str) -> Result<()> {
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

#[shallguard::enforces("REQ-CLI-004")]
fn run_bundle(
    root: &Path,
    docs: &[shallguard::DocSpec],
    config: &shallguard::config::RepositoryConfig,
    args: &BundleArgs,
) -> ExitCode {
    let output_dir = args
        .output
        .clone()
        .unwrap_or_else(|| root.join(config.bundle_dir()));
    let options = shallguard::bundle::BundleOptions {
        impact_file: &args.impact,
        coverage_file: args.coverage.as_deref(),
        output_dir: &output_dir,
    };
    match shallguard::bundle::generate(root, docs, &options) {
        Ok(result) => {
            println!(
                "created {} review capsule(s) in {}",
                result.capsules,
                result.output_dir.display()
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{COMMAND_NAME} bundle failed: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run_test_index(root: &Path, docs: &[shallguard::DocSpec], args: &TestIndexArgs) -> ExitCode {
    let harness = match &args.harness {
        TestHarnessCli::Enumerate => shallguard::test_index::HarnessSource::Enumerate,
        TestHarnessCli::Catalog(path) => shallguard::test_index::HarnessSource::Catalog(path),
    };
    let options = shallguard::test_index::TestIndexOptions {
        harness,
        packages: &args.packages,
        catalog_output: args.catalog_output.as_deref(),
    };
    let artifact = match shallguard::test_index::generate(root, docs, &options) {
        Ok(artifact) => artifact,
        Err(err) => {
            eprintln!("{COMMAND_NAME} test-index failed: {err:#}");
            return ExitCode::FAILURE;
        }
    };
    let resolution_failed = artifact.has_resolution_errors();
    let json = match artifact.to_json() {
        Ok(json) => json,
        Err(err) => {
            eprintln!("{COMMAND_NAME} test-index failed: {err:#}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(err) = write_artifact(&args.json, &json) {
        eprintln!("{COMMAND_NAME} test-index failed: {err:#}");
        return ExitCode::FAILURE;
    }
    if let Some(path) = &args.markdown
        && let Err(err) = write_artifact(path, &artifact.to_markdown())
    {
        eprintln!("{COMMAND_NAME} test-index failed: {err:#}");
        return ExitCode::FAILURE;
    }
    if resolution_failed {
        eprintln!(
            "{COMMAND_NAME} test-index: one or more verification tests did not resolve exactly"
        );
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[shallguard::enforces("REQ-CLI-004", "REQ-COV-006")]
fn run_coverage(
    root: &Path,
    docs: &[shallguard::DocSpec],
    config: &shallguard::config::RepositoryConfig,
    args: &CoverageArgs,
) -> ExitCode {
    let work_dir = args
        .work_dir
        .clone()
        .unwrap_or_else(|| root.join(config.coverage_work_dir()));
    let options = shallguard::coverage::CoverageOptions {
        packages: &args.packages,
        requirements: &args.requirements,
        work_dir: &work_dir,
        progress: Some(print_progress),
    };
    let artifact = match shallguard::coverage::generate(root, docs, &options) {
        Ok(artifact) => artifact,
        Err(err) => {
            eprintln!("{COMMAND_NAME} coverage failed: {err:#}");
            return ExitCode::FAILURE;
        }
    };
    let execution_failed = artifact.has_execution_errors();
    let json = match artifact.to_json() {
        Ok(json) => json,
        Err(err) => {
            eprintln!("{COMMAND_NAME} coverage failed: {err:#}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(err) = write_artifact(&args.json, &json) {
        eprintln!("{COMMAND_NAME} coverage failed: {err:#}");
        return ExitCode::FAILURE;
    }
    if let Some(path) = &args.markdown
        && let Err(err) = write_artifact(path, &artifact.to_markdown())
    {
        eprintln!("{COMMAND_NAME} coverage failed: {err:#}");
        return ExitCode::FAILURE;
    }
    if execution_failed {
        eprintln!("{COMMAND_NAME} coverage: selected test or coverage infrastructure failed");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[shallguard::enforces("REQ-CLI-002", "REQ-CLI-004")]
fn run_review(
    root: &Path,
    docs: &[shallguard::DocSpec],
    config: &shallguard::config::RepositoryConfig,
    args: &ReviewArgs,
) -> ExitCode {
    let configured_target = if args.base.is_none() && !args.resume && args.bundle.is_none() {
        config.review.target.as_deref()
    } else {
        None
    };
    let base = args
        .base
        .as_ref()
        .map(|base| match base {
            CliBase::Revision(revision) => shallguard::impact::BaseSelection::Revision(revision),
            CliBase::Target(target) => shallguard::impact::BaseSelection::MergeBaseWith(target),
        })
        .or_else(|| configured_target.map(shallguard::impact::BaseSelection::MergeBaseWith));
    if base.is_none() && !args.resume && args.bundle.is_none() {
        eprintln!(
            "{COMMAND_NAME} review failed: configure review.target in shallguard.toml or pass \
             --base, --target, or --bundle"
        );
        return ExitCode::FAILURE;
    }
    let provider = match args.provider {
        Some(provider) => provider,
        None => match config.review.provider.as_deref() {
            Some(provider) => match provider.parse() {
                Ok(provider) => provider,
                Err(error) => {
                    eprintln!("{COMMAND_NAME} review failed: {error:#}");
                    return ExitCode::FAILURE;
                }
            },
            None => shallguard::review::ReviewProvider::Codex,
        },
    };
    let bundle_dir = args
        .bundle
        .clone()
        .unwrap_or_else(|| root.join(config.bundle_dir()));
    let review_output_dir = args
        .output
        .clone()
        .unwrap_or_else(|| root.join(config.review_dir()));
    let with_coverage = if base.is_some() {
        args.with_coverage
            .or(config.review.with_coverage)
            .unwrap_or(true)
    } else {
        args.with_coverage.unwrap_or(false)
    };
    let timeout = args
        .timeout
        .unwrap_or_else(|| Duration::from_secs(config.review.timeout_seconds.unwrap_or(300)));
    let model = args.model.as_deref().or(config.review.model.as_deref());
    let local_provider = args
        .local_provider
        .as_deref()
        .or(config.review.local_provider.as_deref());
    let artifact_root = root.join(&config.artifacts.root);
    let impact_json = artifact_root.join("requirement-impact.json");
    let impact_markdown = artifact_root.join("requirement-impact.md");
    let coverage_json = artifact_root.join("requirement-coverage.json");
    let coverage_markdown = artifact_root.join("requirement-coverage.md");
    let coverage_work_dir = root.join(config.coverage_work_dir());
    let options = shallguard::review_workflow::ReviewWorkflowOptions {
        base,
        baseline_path: &config.baseline,
        with_coverage,
        provider,
        model,
        local_provider,
        requirements: &args.requirements,
        timeout,
        resume: args.resume,
        cache_dir: args.cache_dir.as_deref(),
        artifacts: shallguard::review_workflow::ReviewArtifactPaths {
            impact_json: &impact_json,
            impact_markdown: &impact_markdown,
            coverage_json: &coverage_json,
            coverage_markdown: &coverage_markdown,
            coverage_work_dir: &coverage_work_dir,
            bundle_dir: &bundle_dir,
            review_output_dir: &review_output_dir,
        },
        progress: Some(print_progress),
    };
    match shallguard::review_workflow::run(root, docs, &options) {
        Ok(run) if !run.has_failures() => {
            if run.impacted_requirements == 0 {
                println!(
                    "replayed {} validated local review(s) in {}",
                    run.review.reviews,
                    run.review.output_dir.display()
                );
            } else {
                println!(
                    "created {} validated local review(s) from {} impacted requirement(s); \
                     coverage selected {} requirement(s); output {}",
                    run.review.reviews,
                    run.impacted_requirements,
                    run.covered_requirements,
                    run.review.output_dir.display()
                );
            }
            ExitCode::SUCCESS
        }
        Ok(run) => {
            eprintln!(
                "{COMMAND_NAME} review: {} review(s) validated, {} unavailable or invalid, \
                 impact policy failed: {}, coverage execution failed: {}; see {}",
                run.review.reviews,
                run.review.failures,
                run.impact_policy_failed,
                run.coverage_execution_failed,
                run.review.output_dir.display()
            );
            ExitCode::FAILURE
        }
        Err(err) => {
            eprintln!("{COMMAND_NAME} review failed: {err:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
