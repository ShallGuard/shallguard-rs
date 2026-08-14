//! Concise human report for requirement-level executable coverage.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::coverage::{CoverageArtifact, CoverageStatus, TestExecutionResult};

impl CoverageArtifact {
    /// Renders the requirement coverage artifact as Markdown.
    pub fn to_markdown(&self) -> String {
        let passed = self
            .tests
            .iter()
            .filter(|test| test.result == TestExecutionResult::Passed)
            .count();
        let mut statuses = BTreeMap::<&str, usize>::new();
        for requirement in &self.requirements {
            *statuses
                .entry(status_label(requirement.status))
                .or_default() += 1;
        }

        let mut out = String::new();
        let _ = writeln!(out, "# Requirement executable coverage\n");
        let _ = writeln!(out, "- Head: `{}`", self.head_commit);
        let _ = writeln!(out, "- Exact tests passed: {passed}/{}", self.tests.len());
        let _ = writeln!(out, "- Requirements evaluated: {}", self.requirements.len());
        let _ = writeln!(
            out,
            "- Execution/infrastructure findings: {}",
            self.infrastructure_findings.len()
        );

        if !statuses.is_empty() {
            let _ = writeln!(out, "\n## Status summary\n");
            for (status, count) in statuses {
                let _ = writeln!(out, "- {status}: {count}");
            }
        }

        if !self.infrastructure_findings.is_empty() {
            let _ = writeln!(out, "\n## Execution findings\n");
            for finding in &self.infrastructure_findings {
                let test = finding
                    .test
                    .as_ref()
                    .map_or(String::new(), |test| format!(" `{test}`"));
                let _ = writeln!(out, "- `{}`{test}: {}", finding.code, finding.message);
            }
        }

        let _ = writeln!(out, "\n## Requirement evidence\n");
        for requirement in &self.requirements {
            let _ = writeln!(
                out,
                "- `{}` — {}: tests {}/{}, executable sites {}/{}/{} reached/instrumented/total; \
                 structural {}; unmapped {}",
                requirement.id,
                status_label(requirement.status),
                requirement
                    .tests
                    .iter()
                    .filter(|test| test.result == TestExecutionResult::Passed)
                    .count(),
                requirement.tests.len(),
                requirement.executable_sites.reached,
                requirement.executable_sites.instrumented,
                requirement.executable_sites.total,
                requirement.structural_sites,
                requirement.unmapped_sites,
            );
        }
        out
    }
}

fn status_label(status: CoverageStatus) -> &'static str {
    match status {
        CoverageStatus::Covered => "covered",
        CoverageStatus::PartiallyCovered => "partially covered",
        CoverageStatus::NotReached => "not reached",
        CoverageStatus::StructuralOnly => "structural only",
        CoverageStatus::NoExecutableEvidence => "no executable evidence",
        CoverageStatus::TestFailed => "test failed",
        CoverageStatus::InfrastructureError => "infrastructure error",
    }
}
