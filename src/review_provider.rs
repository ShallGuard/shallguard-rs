//! Isolated local model-provider process adapters.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::progress::{ProviderProgress, ReviewUnitProgress};
use super::{ProgressCallback, ReviewProvider, ReviewResult, millis};

pub(super) struct Invocation {
    pub(super) status: Option<ExitStatus>,
    pub(super) timed_out: bool,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) response: Option<String>,
    pub(super) duration_ms: u64,
}

pub(super) struct ProviderInvocation<'a> {
    pub(super) provider: ReviewProvider,
    pub(super) model: Option<&'a str>,
    pub(super) local_provider: Option<&'a str>,
    pub(super) review_dir: &'a Path,
    pub(super) schema: &'a str,
    pub(super) timeout: Duration,
    pub(super) progress: Option<ProgressCallback>,
    pub(super) unit: ReviewUnitProgress<'a>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct CommandSpec {
    pub(super) executable: &'static str,
    pub(super) arguments: Vec<OsString>,
}

pub(super) fn invoke_provider(options: &ProviderInvocation<'_>) -> Result<Invocation> {
    let stdout_path = options.review_dir.join("provider.stdout");
    let stderr_path = options.review_dir.join("provider.stderr");
    let stdout = File::create(&stdout_path).context("creating provider stdout log")?;
    let stderr = File::create(&stderr_path).context("creating provider stderr log")?;
    let stdin =
        File::open(options.review_dir.join("prompt.txt")).context("opening provider prompt")?;
    let spec = command_spec(
        options.provider,
        options.model,
        options.local_provider,
        options.schema,
    );
    let mut command = Command::new(spec.executable);
    sanitize_provider_environment(&mut command);
    let mut child = command
        .args(spec.arguments)
        .current_dir(options.review_dir)
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .with_context(|| format!("starting {} CLI", options.provider.as_str()))?;
    let started = Instant::now();
    let mut provider_progress =
        ProviderProgress::new(options.progress, options.provider.as_str(), options.unit);
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().context("waiting for provider CLI")? {
            break (Some(status), false);
        }
        let elapsed = started.elapsed();
        if elapsed >= options.timeout {
            child.kill().context("terminating timed-out provider CLI")?;
            let status = child.wait().context("reaping timed-out provider CLI")?;
            break (Some(status), true);
        }
        provider_progress.update(elapsed);
        thread::sleep(Duration::from_millis(100));
    };
    let stdout = std::fs::read_to_string(&stdout_path).context("reading provider stdout")?;
    let stderr = std::fs::read_to_string(&stderr_path).context("reading provider stderr")?;
    let response = match options.provider {
        ReviewProvider::Codex => {
            std::fs::read_to_string(options.review_dir.join("provider-response.json"))
                .ok()
                .or_else(|| (!stdout.trim().is_empty()).then(|| stdout.clone()))
        }
        ReviewProvider::Claude => (!stdout.trim().is_empty()).then(|| stdout.clone()),
    };
    Ok(Invocation {
        status,
        timed_out,
        stdout,
        stderr,
        response,
        duration_ms: millis(started.elapsed()),
    })
}

#[shallguard::enforces("REQ-REV-002", "REQ-SEC-003")]
fn sanitize_provider_environment(command: &mut Command) {
    command.env_clear();
    for (name, value) in std::env::vars_os() {
        if provider_environment_allowed(&name) {
            command.env(name, value);
        }
    }
}

pub(super) fn provider_environment_allowed(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    matches!(
        name,
        "PATH"
            | "HOME"
            | "USER"
            | "LOGNAME"
            | "SHELL"
            | "TMPDIR"
            | "XDG_CONFIG_HOME"
            | "XDG_CACHE_HOME"
            | "XDG_DATA_HOME"
            | "HTTP_PROXY"
            | "HTTPS_PROXY"
            | "ALL_PROXY"
            | "NO_PROXY"
            | "SSL_CERT_FILE"
            | "SSL_CERT_DIR"
            | "LANG"
            | "LC_ALL"
            | "TERM"
    ) || [
        "OPENAI_",
        "CODEX_",
        "ANTHROPIC_",
        "CLAUDE_",
        "OLLAMA_",
        "LMSTUDIO_",
        "LM_STUDIO_",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

#[shallguard::enforces("REQ-REV-001", "REQ-REV-002", "REQ-SEC-003")]
pub(super) fn command_spec(
    provider: ReviewProvider,
    model: Option<&str>,
    local_provider: Option<&str>,
    schema: &str,
) -> CommandSpec {
    let mut arguments = Vec::<OsString>::new();
    match provider {
        ReviewProvider::Codex => {
            arguments.extend(
                [
                    "exec",
                    "--ephemeral",
                    "--sandbox",
                    "read-only",
                    "--skip-git-repo-check",
                    "--ignore-rules",
                    "--output-schema",
                    "response-schema.json",
                    "--output-last-message",
                    "provider-response.json",
                ]
                .into_iter()
                .map(OsString::from),
            );
            if let Some(model) = model {
                arguments.extend([OsString::from("--model"), OsString::from(model)]);
            }
            if let Some(local_provider) = local_provider {
                arguments.extend([
                    OsString::from("--oss"),
                    OsString::from("--local-provider"),
                    OsString::from(local_provider),
                ]);
            }
            arguments.push(OsString::from("-"));
        }
        ReviewProvider::Claude => {
            arguments.extend(
                [
                    "--print",
                    "--bare",
                    "--safe-mode",
                    "--tools",
                    "",
                    "--no-session-persistence",
                    "--permission-mode",
                    "dontAsk",
                    "--output-format",
                    "json",
                    "--json-schema",
                ]
                .into_iter()
                .map(OsString::from),
            );
            arguments.push(OsString::from(schema));
            if let Some(model) = model {
                arguments.extend([OsString::from("--model"), OsString::from(model)]);
            }
        }
    }
    CommandSpec {
        executable: provider.executable(),
        arguments,
    }
}

pub(super) fn provider_version(provider: ReviewProvider) -> Result<String> {
    let output = Command::new(provider.executable())
        .arg("--version")
        .output()
        .with_context(|| {
            format!(
                "{} CLI is not installed or not available on PATH",
                provider.as_str()
            )
        })?;
    if !output.status.success() {
        bail!(
            "{} --version failed with {}",
            provider.as_str(),
            output.status
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let version = if stdout.trim().is_empty() {
        stderr.trim()
    } else {
        stdout.trim()
    };
    if version.is_empty() {
        bail!("{} --version returned no version", provider.as_str());
    }
    Ok(version.to_string())
}

pub(super) fn parse_provider_response(
    provider: ReviewProvider,
    text: &str,
) -> Result<ReviewResult> {
    let value: Value = serde_json::from_str(text).context("parsing provider JSON")?;
    let structured = match provider {
        ReviewProvider::Codex => value,
        ReviewProvider::Claude => {
            if let Some(value) = value.get("structured_output") {
                value.clone()
            } else if let Some(result) = value.get("result").and_then(Value::as_str) {
                serde_json::from_str(result).context("parsing Claude result JSON")?
            } else {
                value
            }
        }
    };
    serde_json::from_value(structured).context("decoding structured review response")
}
