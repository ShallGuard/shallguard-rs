//! Human-readable rendering for exact Cargo test identity artifacts.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::test_index::{
    CargoTargetIdentity, ResolutionMatch, ResolutionStatus, TestIndexArtifact,
};

impl TestIndexArtifact {
    /// Renders a concise human-readable companion report.
    pub fn to_markdown(&self) -> String {
        let mut counts = BTreeMap::<&str, usize>::new();
        let mut matches = BTreeMap::<&str, usize>::new();
        for test in &self.tests {
            let status = match test.status {
                ResolutionStatus::Resolved => "resolved",
                ResolutionStatus::Unresolved => "unresolved",
                ResolutionStatus::Ambiguous => "ambiguous",
                ResolutionStatus::TargetUnresolved => "target unresolved",
            };
            *counts.entry(status).or_default() += 1;
            if let Some(match_kind) = test.match_kind {
                let label = match match_kind {
                    ResolutionMatch::ExactSyntacticName => "exact syntactic name",
                    ResolutionMatch::UniqueFunctionSuffix => "unique function suffix",
                };
                *matches.entry(label).or_default() += 1;
            }
        }

        let mut out = String::new();
        let _ = writeln!(out, "# Requirement verification test index\n");
        let _ = writeln!(out, "- Head: `{}`", self.head_commit);
        let _ = writeln!(out, "- Verification anchors: {}", self.tests.len());
        let _ = writeln!(
            out,
            "- Resolved: {}",
            counts.get("resolved").copied().unwrap_or_default()
        );
        let _ = writeln!(
            out,
            "  - Exact syntactic name: {}",
            matches
                .get("exact syntactic name")
                .copied()
                .unwrap_or_default()
        );
        let _ = writeln!(
            out,
            "  - Unique function suffix: {}",
            matches
                .get("unique function suffix")
                .copied()
                .unwrap_or_default()
        );
        let _ = writeln!(out, "- Findings: {}", self.findings.len());

        if !self.findings.is_empty() {
            let _ = writeln!(out, "\n## Findings\n");
            for finding in &self.findings {
                let location = finding.file.as_ref().map_or_else(String::new, |file| {
                    format!(" at `{file}:{}`", finding.line.unwrap_or(1))
                });
                let _ = writeln!(out, "- `{}`{}: {}", finding.code, location, finding.message);
            }
        }

        let mut by_target = BTreeMap::<CargoTargetIdentity, usize>::new();
        for identity in self.tests.iter().filter_map(|test| test.identity.as_ref()) {
            *by_target
                .entry(CargoTargetIdentity {
                    package: identity.package.clone(),
                    target_kind: identity.target_kind,
                    target_name: identity.target_name.clone(),
                })
                .or_default() += 1;
        }
        if !by_target.is_empty() {
            let _ = writeln!(out, "\n## Resolved targets\n");
            for (target, count) in by_target {
                let _ = writeln!(
                    out,
                    "- `{}` `{}` (`{}`): {} test(s)",
                    target.package,
                    target.target_kind.as_str(),
                    target.target_name,
                    count
                );
            }
        }
        out
    }
}
