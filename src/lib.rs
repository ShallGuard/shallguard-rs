//! Requirement traceability, executable coverage, and review tooling for Rust.
//!
//! Cross-checks numbered system requirements in selected
//! `USER_STORIES_AND_REQUIREMENTS.md` documents against the code and test
//! anchors in the source tree (`#[enforces]` / `#[verifies]` attributes
//! and `enforces_here!` branch anchors from `shallguard-macros`).
//!
//! Hard errors (always fail):
//! - a document fails to parse or yields implausibly few requirements;
//! - duplicate requirement IDs across the documents;
//! - a `src/` or `tests/` path cited anywhere in a document that does not
//!   exist (except the [`KNOWN_REMOVED_PATHS`] annotations);
//! - a requirement ID referenced in code that no document defines.
//!
//! Ratcheted checks (warning only for exact committed baseline entries,
//! otherwise a hard error; already-hard areas cannot be baselined):
//! - a requirement whose *Enforced:* files carry no anchor of its ID;
//! - a requirement with automated-test evidence (`✅`) that no test
//!   anchor claims to verify;
//! - automated evidence that does not resolve to its cited test.
//!
//! The binary prints a per-area coverage report; the repo-wide check also
//! runs as this crate's integration test, which is the CI gate.

pub mod baseline;
pub mod bundle;
pub mod check;
pub mod coverage;
mod coverage_llvm;
mod coverage_markdown;
pub mod docs;
pub mod impact;
mod impact_dependency;
pub mod requirement_format;
pub mod review;
pub mod review_workflow;
pub mod scan;
pub mod test_index;
mod test_index_markdown;
mod workspace;

use std::path::Path;

pub use workspace::workspace_root;

/// A human-readable update from a long-running command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressEvent<'a> {
    /// A durable line that remains in interactive and redirected output.
    Message(&'a str),
    /// A replaceable interactive status.
    LiveStatus {
        /// Complete status text without the CLI prefix.
        message: &'a str,
        /// Whether redirected output should retain this update as a heartbeat.
        log_when_redirected: bool,
    },
    /// Remove any replaceable interactive status.
    ClearLiveStatus,
}

/// Callback used by long-running commands for human-readable stderr progress.
pub type ProgressCallback = fn(ProgressEvent<'_>);

pub(crate) fn report_progress(progress: Option<ProgressCallback>, message: impl AsRef<str>) {
    if let Some(progress) = progress {
        progress(ProgressEvent::Message(message.as_ref()));
    }
}

pub(crate) fn report_live_progress(
    progress: Option<ProgressCallback>,
    message: impl AsRef<str>,
    log_when_redirected: bool,
) {
    if let Some(progress) = progress {
        progress(ProgressEvent::LiveStatus {
            message: message.as_ref(),
            log_when_redirected,
        });
    }
}

pub(crate) fn clear_live_progress(progress: Option<ProgressCallback>) {
    if let Some(progress) = progress {
        progress(ProgressEvent::ClearLiveStatus);
    }
}

/// A requirements document and the crate that unprefixed `src/` and
/// `tests/` references inside it resolve to.
pub struct DocSpec {
    /// Workspace-relative document path.
    pub path: String,
    /// The owning crate: the first component of the document path.
    pub default_crate: String,
}

impl DocSpec {
    /// Builds a spec from a workspace-relative document path; the owning
    /// crate is the path's first component
    /// (`example-app/docs/...` -> crate `example-app`).
    pub fn from_path(path: &str) -> anyhow::Result<Self> {
        let default_crate = Path::new(path)
            .components()
            .next()
            .and_then(|c| c.as_os_str().to_str())
            .filter(|c| !c.is_empty() && *c != "." && *c != "..")
            .ok_or_else(|| {
                anyhow::anyhow!("document path {path:?} must start with its crate directory")
            })?
            .to_string();
        Ok(Self {
            path: path.to_string(),
            default_crate,
        })
    }
}

/// The documents checked when no arguments are given.
pub fn default_docs() -> Vec<DocSpec> {
    [
        "example-app/docs/USER_STORIES_AND_REQUIREMENTS.md",
        "example-core/docs/USER_STORIES_AND_REQUIREMENTS.md",
    ]
    .iter()
    .map(|p| DocSpec::from_path(p).expect("BUG: default doc path is well-formed"))
    .collect()
}

/// `prefix:` before a path span selects another crate (an application
/// document can reference a library as `core:src/...`).
pub const PREFIX_CRATES: &[(&str, &str)] = &[("core", "example-core")];

/// Human-readable names of the requirement areas, taken from the owning
/// document's section headings. Used by the summary report; an area
/// missing here is printed as its bare acronym.
pub const AREA_NAMES: &[(&str, &str)] = &[
    ("AUTH", "Authentication"),
    ("REP", "Reporting System"),
    ("RD", "Routing Domain"),
    ("HRS", "Hashrate Split"),
    ("DYN", "Dynamic Routing"),
    ("SAFE", "Routing Safety"),
    ("CR", "Core Routing"),
    ("PS", "Protocol Support"),
    ("CM", "Connection Management"),
    ("JM", "Job Management"),
    ("OP", "Optimization"),
    ("EH", "Error Handling and Recovery"),
    ("CF", "Configuration"),
    ("MO", "Monitoring and Observability"),
    ("PERF", "Performance"),
    ("SR", "Security"),
];

/// `Full Name (ACRONYM)` label for an area, or the bare acronym when the
/// area has no entry in [`AREA_NAMES`].
pub fn area_label(area: &str) -> String {
    AREA_NAMES
        .iter()
        .find(|(acronym, _)| *acronym == area)
        .map_or_else(|| area.to_string(), |(_, name)| format!("{name} ({area})"))
}

/// Areas where a requirement without an anchor in its *Enforced:* files
/// is a hard error instead of a warning.
///
/// All areas are hard: every anchorable requirement must carry an
/// enforcement anchor. A new area added to the documents must be listed
/// here once its anchors land.
pub const HARD_CODE_ANCHOR_AREAS: &[&str] = &[
    "SAFE", "HRS", "RD", "DYN", "CM", "OP", "AUTH", "REP", "CR", "CF", "PS", "JM", "EH", "MO",
    "SR", "PERF",
];

/// Areas where an automated-evidence requirement without a test anchor
/// is a hard error instead of a warning. Ratcheted independently of the
/// code anchors: a test anchor is only honest once someone confirmed the
/// test actually covers the requirement. All areas are hard.
pub const HARD_TEST_ANCHOR_AREAS: &[&str] = &[
    "SAFE", "HRS", "RD", "DYN", "CM", "OP", "AUTH", "REP", "CR", "CF", "PS", "JM", "EH", "MO",
    "SR", "PERF",
];

/// Document path spans that intentionally reference deleted files (the
/// documents record the removal itself).
pub const KNOWN_REMOVED_PATHS: &[&str] = &[
    "example-core/src/network/dns.rs",
    "example-core/src/router/tasks/dns.rs",
];

/// A `#[verifies]` anchor claiming this many requirements or more is
/// reported as an outlier for review.
pub const VERIFY_OUTLIER_THRESHOLD: usize = 6;
