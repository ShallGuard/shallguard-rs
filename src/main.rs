//! Runs the requirement traceability check and prints the report.
//! Exits nonzero when any hard check fails.
//!
//! Usage:
//!
//! ```text
//! cargo req-cov [check] [<doc.md> ...]
//! cargo req-cov fmt [--check] [<doc.md> ...]
//! cargo req-cov lint [<doc.md> ...]
//! cargo req-cov clean
//! cargo req-cov baseline check
//! cargo req-cov baseline prune
//! cargo req-cov impact --base <revision> --json requirement-impact.json
//! cargo req-cov impact --target origin/master --json requirement-impact.json
//! cargo req-cov bundle --impact requirement-impact.json --output requirement-review
//! cargo req-cov test-index --enumerate --json requirement-tests.json
//! cargo req-cov test-index --catalog harness-tests.json --json requirement-tests.json
//! cargo req-cov coverage --requirement REQ-HRS-001 --json requirement-coverage.json
//! cargo req-cov review --provider codex --bundle requirement-review
//! cargo run -p req-trace -- example-app/docs/USER_STORIES_AND_REQUIREMENTS.md
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
const COMMAND_NAME: &str = "cargo req-cov";

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
    base: CliBase,
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
    output: PathBuf,
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
    work_dir: PathBuf,
    json: PathBuf,
    markdown: Option<PathBuf>,
}

struct ReviewArgs {
    provider: req_trace::review::ReviewProvider,
    base: Option<CliBase>,
    with_coverage: bool,
    bundle: PathBuf,
    output: PathBuf,
    model: Option<String>,
    local_provider: Option<String>,
    requirements: BTreeSet<String>,
    timeout: Duration,
    resume: bool,
    cache_dir: Option<PathBuf>,
}

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
    let docs = if doc_args.is_empty() {
        req_trace::default_docs()
    } else {
        match doc_args
            .iter()
            .map(|a| req_trace::DocSpec::from_path(a))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(docs) => docs,
            Err(err) => {
                eprintln!("{COMMAND_NAME}: {err:#}");
                return ExitCode::FAILURE;
            }
        }
    };

    let root = match req_trace::workspace_root() {
        Ok(root) => root,
        Err(err) => {
            eprintln!("{COMMAND_NAME}: {err:#}");
            return ExitCode::FAILURE;
        }
    };
    match command {
        Command::Help => unreachable!("help returned before workspace discovery"),
        Command::Clean => {
            return run_clean(&root);
        }
        Command::Format(args) => {
            return run_format(&root, &docs, &args);
        }
        Command::BaselineInit => {
            return match req_trace::check::initialize_baseline(&root, &docs) {
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
            return match req_trace::check::prune_baseline(&root, &docs) {
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
            return run_impact(&root, &docs, &args);
        }
        Command::Bundle(args) => {
            return run_bundle(&root, &docs, &args);
        }
        Command::TestIndex(args) => {
            return run_test_index(&root, &docs, &args);
        }
        Command::Coverage(args) => {
            return run_coverage(&root, &docs, &args);
        }
        Command::Review(args) => {
            return run_review(&root, &docs, &args);
        }
        Command::Check(_) => {}
    }

    let report = match req_trace::check::run(&root, &docs) {
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

fn normalized_args(mut args: Vec<String>) -> Vec<String> {
    // Cargo passes the external subcommand name as argv[1] when it invokes an
    // installed `cargo-req-cov` executable. The workspace alias invokes this
    // binary through `cargo run` and therefore does not add that argument.
    if args.first().is_some_and(|argument| argument == "req-cov") {
        args.remove(0);
    }
    args
}

fn run_clean(root: &Path) -> ExitCode {
    match req_trace::bundle::clean_default_bundle(root) {
        Ok(Some(path)) => {
            println!("removed generated requirement bundle {}", path.display());
            ExitCode::SUCCESS
        }
        Ok(None) => {
            println!(
                "no generated requirement bundle at {}",
                root.join(req_trace::bundle::DEFAULT_BUNDLE_DIR).display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{COMMAND_NAME} clean failed: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run_format(root: &Path, docs: &[req_trace::DocSpec], args: &FormatArgs) -> ExitCode {
    let report = if args.check {
        req_trace::requirement_format::check(root, docs)
    } else {
        req_trace::requirement_format::format(root, docs)
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
  cargo req-cov [check] [<doc.md> ...]
  cargo req-cov fmt [--check] [<doc.md> ...]
  cargo req-cov lint [<doc.md> ...]
  cargo req-cov clean
  cargo req-cov baseline <check|init|prune>
  cargo req-cov impact --base <revision> [--json <path>] [--markdown <path>]
  cargo req-cov bundle --impact <impact.json> [--coverage <coverage.json>] [options]
  cargo req-cov test-index <--enumerate|--catalog <path>> [options]
  cargo req-cov coverage [--package <crate>] [--requirement <REQ-ID>] [options]
  cargo req-cov review [--base <revision>|--target <branch>] [options]

Review options:
  --provider <name>          Local model CLI [default: codex]
  --base <revision>          Compare against an exact revision
  --target <branch>          Compare against its merge base [default: origin/master]
  --with-coverage            Run impacted verification tests [default]
  --without-coverage         Skip executable coverage collection
  --bundle <directory>       Generated bundle, or existing bundle in replay mode
                             [default: target/requirement-review]
  --output <directory>       New auditable result directory
                             [default: target/requirement-local-review]
  --resume                   Continue the compatible run in --output
  --cache-dir <directory>    Reuse validated responses across run directories
  --model <identifier>       Provider-specific model override
  --local-provider <name>    Codex on-device inference: ollama or lmstudio
  --requirement <REQ-ID>     Review only this requirement; repeatable
  --timeout-seconds <n>      Per-requirement timeout [default: 300]

`fmt` lints and formats requirement blocks; `fmt --check` and `lint` never write.
`clean` removes only the validated default bundle at target/requirement-review.
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
        })
        .context(
            "provide --base <revision> or --target <branch>; CI may provide \
             CI_MERGE_REQUEST_DIFF_BASE_SHA or CI_DEFAULT_BRANCH",
        )?;
    Ok(ImpactArgs {
        base,
        json,
        markdown,
    })
}

fn parse_bundle_args(args: &[String]) -> Result<BundleArgs> {
    let mut impact = None;
    let mut coverage = None;
    let mut output = PathBuf::from(req_trace::bundle::DEFAULT_BUNDLE_DIR);
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
            "--output" => output = PathBuf::from(value),
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
    let mut work_dir = PathBuf::from("target/req-trace-coverage");
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
            "--work-dir" => work_dir = PathBuf::from(value),
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

fn run_impact(root: &Path, docs: &[req_trace::DocSpec], args: &ImpactArgs) -> ExitCode {
    let base = match &args.base {
        CliBase::Revision(revision) => req_trace::impact::BaseSelection::Revision(revision),
        CliBase::Target(target) => req_trace::impact::BaseSelection::MergeBaseWith(target),
    };
    let options = req_trace::impact::ImpactOptions { base };
    let artifact = match req_trace::impact::analyze(root, docs, &options) {
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
    std::fs::write(path, content).with_context(|| format!("writing artifact {}", path.display()))
}

fn run_bundle(root: &Path, docs: &[req_trace::DocSpec], args: &BundleArgs) -> ExitCode {
    let options = req_trace::bundle::BundleOptions {
        impact_file: &args.impact,
        coverage_file: args.coverage.as_deref(),
        output_dir: &args.output,
    };
    match req_trace::bundle::generate(root, docs, &options) {
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

fn run_test_index(root: &Path, docs: &[req_trace::DocSpec], args: &TestIndexArgs) -> ExitCode {
    let harness = match &args.harness {
        TestHarnessCli::Enumerate => req_trace::test_index::HarnessSource::Enumerate,
        TestHarnessCli::Catalog(path) => req_trace::test_index::HarnessSource::Catalog(path),
    };
    let options = req_trace::test_index::TestIndexOptions {
        harness,
        packages: &args.packages,
        catalog_output: args.catalog_output.as_deref(),
    };
    let artifact = match req_trace::test_index::generate(root, docs, &options) {
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

fn run_coverage(root: &Path, docs: &[req_trace::DocSpec], args: &CoverageArgs) -> ExitCode {
    let options = req_trace::coverage::CoverageOptions {
        packages: &args.packages,
        requirements: &args.requirements,
        work_dir: &args.work_dir,
        progress: Some(print_progress),
    };
    let artifact = match req_trace::coverage::generate(root, docs, &options) {
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

fn run_review(root: &Path, docs: &[req_trace::DocSpec], args: &ReviewArgs) -> ExitCode {
    let base = args.base.as_ref().map(|base| match base {
        CliBase::Revision(revision) => req_trace::impact::BaseSelection::Revision(revision),
        CliBase::Target(target) => req_trace::impact::BaseSelection::MergeBaseWith(target),
    });
    let options = req_trace::review_workflow::ReviewWorkflowOptions {
        base,
        with_coverage: args.with_coverage,
        provider: args.provider,
        model: args.model.as_deref(),
        local_provider: args.local_provider.as_deref(),
        requirements: &args.requirements,
        timeout: args.timeout,
        resume: args.resume,
        cache_dir: args.cache_dir.as_deref(),
        artifacts: req_trace::review_workflow::ReviewArtifactPaths {
            impact_json: Path::new("target/requirement-impact.json"),
            impact_markdown: Path::new("target/requirement-impact.md"),
            coverage_json: Path::new("target/requirement-coverage.json"),
            coverage_markdown: Path::new("target/requirement-coverage.md"),
            coverage_work_dir: Path::new("target/req-trace-coverage"),
            bundle_dir: &args.bundle,
            review_output_dir: &args.output,
        },
        progress: Some(print_progress),
    };
    match req_trace::review_workflow::run(root, docs, &options) {
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
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn removes_cargo_external_subcommand_argument() {
        assert_eq!(
            normalized_args(strings(&["req-cov", "coverage"])),
            strings(&["coverage"])
        );
        assert_eq!(
            normalized_args(strings(&["coverage"])),
            strings(&["coverage"])
        );
    }

    #[test]
    fn parses_requirement_format_modes_and_documents() {
        let format = parse_format_args(
            &strings(&[
                "--check",
                "example-app/docs/USER_STORIES_AND_REQUIREMENTS.md",
            ]),
            false,
        )
        .expect("format arguments parse");
        assert!(format.check);
        assert_eq!(format.docs.len(), 1);

        let lint = parse_format_args(&[], true).expect("lint arguments parse");
        assert!(lint.check);
        assert!(lint.docs.is_empty());
    }

    #[test]
    fn rejects_unknown_requirement_format_flags() {
        let error = parse_format_args(&strings(&["--write"]), false)
            .err()
            .expect("unknown format flag fails");
        assert!(error.to_string().contains("unknown argument"));
    }

    #[test]
    fn parses_local_review_options() {
        let args = parse_review_args(&strings(&[
            "--provider",
            "codex",
            "--bundle",
            "review-input",
            "--output",
            "review-output",
            "--model",
            "gpt-test",
            "--local-provider",
            "ollama",
            "--requirement",
            "REQ-HRS-001",
            "--timeout-seconds",
            "45",
            "--resume",
            "--cache-dir",
            "review-cache",
        ]))
        .expect("review arguments parse");
        assert_eq!(args.provider, req_trace::review::ReviewProvider::Codex);
        assert!(args.base.is_none());
        assert!(!args.with_coverage);
        assert_eq!(args.bundle, PathBuf::from("review-input"));
        assert_eq!(args.output, PathBuf::from("review-output"));
        assert_eq!(args.model.as_deref(), Some("gpt-test"));
        assert_eq!(args.local_provider.as_deref(), Some("ollama"));
        assert!(args.requirements.contains("REQ-HRS-001"));
        assert_eq!(args.timeout, Duration::from_secs(45));
        assert!(args.resume);
        assert_eq!(args.cache_dir, Some(PathBuf::from("review-cache")));
    }

    #[test]
    fn defaults_review_to_codex_master_impact_and_coverage() {
        let args = parse_review_args(&[]).expect("default review arguments parse");

        assert_eq!(args.provider, req_trace::review::ReviewProvider::Codex);
        assert!(matches!(
            args.base,
            Some(CliBase::Target(ref target)) if target == "origin/master"
        ));
        assert!(args.with_coverage);
        assert!(!args.resume);
        assert!(args.cache_dir.is_none());
        assert_eq!(args.bundle, PathBuf::from("target/requirement-review"));
        assert_eq!(
            args.output,
            PathBuf::from("target/requirement-local-review")
        );
    }

    #[test]
    fn resume_replays_the_frozen_default_bundle() {
        let args =
            parse_review_args(&strings(&["--resume"])).expect("resume review arguments parse");

        assert!(args.base.is_none());
        assert!(!args.with_coverage);
        assert_eq!(args.bundle, PathBuf::from("target/requirement-review"));
        assert_eq!(
            args.output,
            PathBuf::from("target/requirement-local-review")
        );
    }

    #[test]
    fn parses_requested_one_command_review() {
        let args = parse_review_args(&strings(&[
            "--provider",
            "codex",
            "--base",
            "2810dced",
            "--with-coverage",
        ]))
        .expect("orchestrated review arguments parse");

        assert!(matches!(
            args.base,
            Some(CliBase::Revision(ref base)) if base == "2810dced"
        ));
        assert!(args.with_coverage);
    }

    #[test]
    fn parses_explicit_impact_outputs() {
        let args = parse_impact_args(&strings(&[
            "--base",
            "abc123",
            "--json",
            "impact.json",
            "--markdown",
            "impact.md",
        ]))
        .expect("impact arguments parse");
        assert!(matches!(args.base, CliBase::Revision(ref base) if base == "abc123"));
        assert_eq!(args.json, PathBuf::from("impact.json"));
        assert_eq!(args.markdown, Some(PathBuf::from("impact.md")));
    }

    #[test]
    fn rejects_ambiguous_base_selection() {
        let error = parse_impact_args(&strings(&["--base", "abc123", "--target", "origin/master"]))
            .err()
            .expect("ambiguous selection fails");
        assert!(error.to_string().contains("exactly one"));
    }

    #[test]
    fn parses_bundle_paths() {
        let args = parse_bundle_args(&strings(&[
            "--impact",
            "impact.json",
            "--coverage",
            "coverage.json",
            "--output",
            "review",
        ]))
        .expect("bundle arguments parse");
        assert_eq!(args.impact, PathBuf::from("impact.json"));
        assert_eq!(args.coverage, Some(PathBuf::from("coverage.json")));
        assert_eq!(args.output, PathBuf::from("review"));
    }

    #[test]
    fn defaults_bundle_output_under_target() {
        let args = parse_bundle_args(&strings(&["--impact", "impact.json"]))
            .expect("default bundle arguments parse");

        assert_eq!(args.output, PathBuf::from("target/requirement-review"));
    }

    #[test]
    fn parses_enumerated_test_index_outputs_and_package_filter() {
        let args = parse_test_index_args(&strings(&[
            "--enumerate",
            "--package",
            "example-core",
            "--catalog-output",
            "catalog.json",
            "--json",
            "tests.json",
            "--markdown",
            "tests.md",
        ]))
        .expect("test-index arguments parse");
        assert!(matches!(args.harness, TestHarnessCli::Enumerate));
        assert!(args.packages.contains("example-core"));
        assert_eq!(args.catalog_output, Some(PathBuf::from("catalog.json")));
        assert_eq!(args.json, PathBuf::from("tests.json"));
        assert_eq!(args.markdown, Some(PathBuf::from("tests.md")));
    }

    #[test]
    fn rejects_ambiguous_test_index_harness_source() {
        let error = parse_test_index_args(&strings(&["--enumerate", "--catalog", "catalog.json"]))
            .err()
            .expect("ambiguous harness source fails");
        assert!(error.to_string().contains("exactly one"));
    }

    #[test]
    fn parses_filtered_coverage_outputs() {
        let args = parse_coverage_args(&strings(&[
            "--package",
            "example-core",
            "--requirement",
            "REQ-HRS-001",
            "--work-dir",
            "target/coverage-fixture",
            "--json",
            "coverage.json",
            "--markdown",
            "coverage.md",
        ]))
        .expect("coverage arguments parse");

        assert!(args.packages.contains("example-core"));
        assert!(args.requirements.contains("REQ-HRS-001"));
        assert_eq!(args.work_dir, PathBuf::from("target/coverage-fixture"));
        assert_eq!(args.json, PathBuf::from("coverage.json"));
        assert_eq!(args.markdown, Some(PathBuf::from("coverage.md")));
    }
}
