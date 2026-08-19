//! Requirement traceability, executable coverage, and review tooling for Rust.
//!
//! Cross-checks numbered system requirements in selected
//! `USER_STORIES_AND_REQUIREMENTS.md` documents against the code and test
//! anchors in the source tree (`#[shallguard::enforces]` /
//! `#[shallguard::verifies]` attributes and `shallguard::enforces_here!`
//! branch anchors).
//!
//! Hard errors (always fail):
//! - a document fails to parse or yields fewer configured requirements;
//! - duplicate requirement IDs across the documents;
//! - a `src/` or `tests/` path cited anywhere in a document that does not
//!   exist (except repository-configured historical removals);
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

extern crate self as shallguard;

pub mod baseline;
pub mod bundle;
pub mod check;
pub mod config;
pub mod coverage;
mod coverage_llvm;
mod coverage_markdown;
pub mod docs;
pub mod impact;
mod impact_dependency;
pub mod oracle;
pub mod requirement_format;
pub mod review;
pub mod review_workflow;
pub mod scan;
pub mod test_index;
mod test_index_markdown;
mod workspace;

use std::collections::{BTreeMap, BTreeSet};

#[shallguard_macros::enforces("REQ-TRACE-008")]
pub use shallguard_macros::{enforces, enforces_here, verifies};
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

/// A requirements document and the source tree that unprefixed `src/` and
/// `tests/` references inside it resolve to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[shallguard::enforces("REQ-PORT-002")]
pub struct DocSpec {
    /// Workspace-relative document path.
    pub path: String,
    /// Repository-relative owning package or workspace source root.
    pub source_root: String,
    /// Optional path-span prefix to repository-relative source-root mappings.
    pub prefixes: BTreeMap<String, String>,
}

impl DocSpec {
    pub fn new(
        path: impl Into<String>,
        source_root: impl Into<String>,
        prefixes: BTreeMap<String, String>,
    ) -> Self {
        Self {
            path: path.into(),
            source_root: source_root.into(),
            prefixes,
        }
    }

    /// Source roots that can contain anchors referenced by this document.
    pub fn source_roots(&self) -> BTreeSet<&str> {
        std::iter::once(self.source_root.as_str())
            .chain(self.prefixes.values().map(String::as_str))
            .collect()
    }

    /// Rust source directories beneath every configured source root.
    pub fn scan_roots(&self) -> BTreeSet<String> {
        self.source_roots()
            .into_iter()
            .flat_map(|root| {
                let root = if root == "." { "" } else { root };
                [
                    format!("{root}{}src", if root.is_empty() { "" } else { "/" }),
                    format!("{root}{}tests", if root.is_empty() { "" } else { "/" }),
                ]
            })
            .collect()
    }
}
