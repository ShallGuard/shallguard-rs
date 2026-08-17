//! Requirement traceability, executable coverage, and review tooling for Rust.
//!
//! ShallGuard keeps numbered system requirements written in Markdown
//! (`REQ-<AREA>-<NNN>` entries with RFC 2119 **SHALL** statements) connected
//! to the Rust code that enforces them and the tests that verify them — and
//! fails CI the moment that connection breaks.
//!
//! This crate is what consuming repositories depend on. It provides:
//!
//! - the **anchor macros** ([`enforces`], [`enforces_here!`](enforces_here),
//!   [`verifies`]) that mark enforcement sites and verification tests, and
//! - the **deterministic analysis library** behind the
//!   [`cargo-shallguard`](https://crates.io/crates/cargo-shallguard) CLI,
//!   for embedding requirement assurance into your own tooling.
//!
//! # Anchoring requirements
//!
//! A requirement lives in a configured Markdown document:
//!
//! ```markdown
//! - **REQ-HRS-002** — The scheduler SHALL never emit a zero worker floor.
//!   *Enforced:* `src/floor.rs` (`floor`) · *Verified:* ✅ `src/floor.rs`
//!   (`floor_never_returns_zero`)
//! ```
//!
//! Code and tests anchor it. All anchor forms expand to the item unchanged
//! (or to nothing) — zero runtime cost — while malformed requirement IDs are
//! compile errors:
//!
//! ```no_run
//! /// Item-level enforcement: this function exists because the contract
//! /// exists.
//! #[shallguard::enforces("REQ-HRS-002")]
//! fn floor(configured: usize) -> usize {
//!     configured.max(1)
//! }
//!
//! /// Branch-level enforcement: anchors the exact statement, arm, or block.
//! fn resolve(fixed: Option<usize>) -> usize {
//!     match fixed {
//!         Some(n) => {
//!             shallguard::enforces_here!("REQ-HRS-002");
//!             n.max(1)
//!         }
//!         None => 4,
//!     }
//! }
//!
//! /// Verification evidence: valid only on a real, non-`#[ignore]`d test.
//! #[shallguard::verifies("REQ-HRS-002")]
//! #[test]
//! fn floor_never_returns_zero() {
//!     assert_eq!(floor(0), 1);
//! }
//! # fn main() { assert_eq!(resolve(Some(0)), 1); }
//! ```
//!
//! `cargo shallguard check` then cross-checks documents against anchors.
//! Hard errors (always fail): an unparseable document, duplicate requirement
//! IDs, a cited `src/` or `tests/` path that does not exist, or an ID
//! referenced in code that no document defines. Ratcheted checks (tolerated
//! only for exact committed baseline entries): a requirement whose
//! *Enforced:* files carry no anchor of its ID, and automated `✅` evidence
//! that does not resolve to an anchored test.
//!
//! # Library API
//!
//! The CLI is a thin presentation layer over these modules:
//!
//! - [`config`] — repository-owned `shallguard.toml` policy.
//! - [`docs`] — parsing of the requirement documents.
//! - [`scan`] — syntactic anchor scanning of Rust sources.
//! - [`check`] — the cross-checks and the per-area report (the CI gate).
//! - [`requirement_format`] — deterministic document formatting and linting.
//! - [`baseline`] — the ratcheted historical-gap baseline.
//! - [`impact`] — Git base/head requirement impact analysis.
//! - [`test_index`] — exact Cargo test identities behind verification anchors.
//! - [`coverage`] — LLVM coverage projected onto enforcement scopes.
//! - [`bundle`], [`review`], [`review_workflow`] — bounded source capsules
//!   and optional LLM-assisted semantic review (advisory only).
//!
//! Library behavior is deterministic and needs no network access or model;
//! long-running operations report progress through an optional
//! [`ProgressCallback`] instead of printing.
//!
//! ```no_run
//! use shallguard::config::RepositoryConfig;
//!
//! fn main() -> anyhow::Result<()> {
//!     let root = shallguard::workspace_root()?;
//!     let config = RepositoryConfig::load(&root)?;
//!     let report = shallguard::check::run(&root, &config.documents(), &config)?;
//!     if !report.print(10) {
//!         std::process::exit(1);
//!     }
//!     Ok(())
//! }
//! ```
//!
//! # More documentation
//!
//! The [repository](https://github.com/sigi64/shallguard) hosts the complete
//! guide: configuration reference, command reference, CI recipes, and the
//! design documents.

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
