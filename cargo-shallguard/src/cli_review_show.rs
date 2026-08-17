//! Parsing and terminal presentation for stored local-review inspection.

use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use shallguard::review::{
    StoredCitation, StoredRequirementReview, StoredRequirementStatus, StoredReview,
    StoredReviewDetails,
};

use super::{COMMAND_NAME, cli_color};

pub(super) struct ReviewShowArgs {
    output: Option<PathBuf>,
    requirements: BTreeSet<String>,
    format: ReviewShowFormat,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ReviewShowFormat {
    #[default]
    Text,
    Markdown,
}

#[shallguard::enforces("REQ-CLI-007", "REQ-CLI-008", "REQ-CLI-011")]
pub(super) fn parse_review_show_args(args: &[String]) -> Result<ReviewShowArgs> {
    let mut output = None;
    let mut requirements = BTreeSet::new();
    let mut format = None;
    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index].as_str();
        index += 1;
        if !flag.starts_with('-') {
            requirements.insert(flag.to_string());
            continue;
        }
        let value = match flag {
            "--output" | "--requirement" | "--format" => args
                .get(index)
                .with_context(|| format!("{flag} requires a value"))?,
            unknown => bail!(
                "unknown argument {unknown:?}; expected a requirement ID, --output, or \
                 --requirement, or --format"
            ),
        };
        index += 1;
        match flag {
            "--output" => {
                if output.is_some() {
                    bail!("--output may be specified only once");
                }
                output = Some(PathBuf::from(value));
            }
            "--requirement" => {
                requirements.insert(value.clone());
            }
            "--format" => {
                if format.is_some() {
                    bail!("--format may be specified only once");
                }
                format = Some(match value.as_str() {
                    "text" => ReviewShowFormat::Text,
                    "markdown" => ReviewShowFormat::Markdown,
                    _ => bail!("--format must be text or markdown"),
                });
            }
            _ => unreachable!("flag matched above"),
        }
    }
    Ok(ReviewShowArgs {
        output,
        requirements,
        format: format.unwrap_or_default(),
    })
}

#[shallguard::enforces("REQ-CLI-007", "REQ-CLI-008", "REQ-CLI-010", "REQ-CLI-011")]
pub(super) fn run(
    root: &Path,
    config: &shallguard::config::RepositoryConfig,
    args: &ReviewShowArgs,
) -> ExitCode {
    let output_dir = args
        .output
        .clone()
        .unwrap_or_else(|| root.join(config.review_dir()));
    match shallguard::review::inspect_stored_review(&output_dir, &args.requirements) {
        Ok(review) => {
            let details = !args.requirements.is_empty();
            let rendered = match args.format {
                ReviewShowFormat::Text => {
                    render_stored_review(&review, details, cli_color::stdout_enabled())
                }
                ReviewShowFormat::Markdown => render_stored_review_markdown(&review, details),
            };
            print!("{rendered}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{COMMAND_NAME} review show failed: {error:#}");
            ExitCode::FAILURE
        }
    }
}

#[shallguard::enforces("REQ-CLI-012")]
fn render_stored_review_markdown(review: &StoredReview, details: bool) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "<!-- shallguard-review -->");
    let _ = writeln!(output, "# ShallGuard semantic review");
    let _ = writeln!(output);
    let _ = writeln!(output, "| Run | Value |");
    let _ = writeln!(output, "| --- | --- |");
    let _ = writeln!(
        output,
        "| Artifact | {} |",
        markdown_code(&review.output_dir.display().to_string())
    );
    let _ = writeln!(
        output,
        "| Status | {} |",
        markdown_code(review.status.as_str())
    );
    let _ = writeln!(output, "| Provider | {} |", markdown_code(&review.provider));
    let _ = writeln!(output, "| Model | {} |", markdown_code(&review.model));
    let _ = writeln!(
        output,
        "| Progress | {}/{} processed |",
        review.processed_responses, review.selected_requirements
    );

    let _ = writeln!(output);
    let _ = writeln!(output, "## Semantic verdicts");
    let _ = writeln!(output);
    let _ = writeln!(output, "| Outcome | Count |");
    let _ = writeln!(output, "| --- | ---: |");
    let _ = writeln!(output, "| Satisfied | {} |", review.verdicts.satisfied);
    let _ = writeln!(output, "| Violated | {} |", review.verdicts.violated);
    let _ = writeln!(
        output,
        "| Insufficient evidence | {} |",
        review.verdicts.insufficient_evidence
    );
    let _ = writeln!(
        output,
        "| Not impacted | {} |",
        review.verdicts.not_impacted
    );
    let _ = writeln!(
        output,
        "| Unavailable or invalid | {} |",
        review.unavailable_or_invalid
    );

    if details {
        for requirement in &review.requirements {
            render_requirement_markdown_details(&mut output, requirement);
        }
    } else {
        let _ = writeln!(output);
        let _ = writeln!(output, "## Requirements");
        let _ = writeln!(output);
        let _ = writeln!(output, "| Requirement | Status | Confidence |");
        let _ = writeln!(output, "| --- | --- | ---: |");
        for requirement in &review.requirements {
            render_requirement_markdown_summary(&mut output, requirement);
        }
    }
    output
}

fn render_requirement_markdown_summary(output: &mut String, requirement: &StoredRequirementReview) {
    let id = markdown_code(&requirement.requirement_id);
    match &requirement.status {
        StoredRequirementStatus::Pending => {
            let _ = writeln!(output, "| {id} | pending | — |");
        }
        StoredRequirementStatus::Completed {
            verdict,
            confidence,
        } => {
            let _ = writeln!(
                output,
                "| {id} | {} | {confidence:.2} |",
                markdown_text(verdict.as_str())
            );
        }
        StoredRequirementStatus::Unavailable { kind, .. } => {
            let _ = writeln!(
                output,
                "| {id} | unavailable ({}) | — |",
                markdown_text(kind)
            );
        }
        StoredRequirementStatus::Invalid { kind, .. } => {
            let _ = writeln!(output, "| {id} | invalid ({}) | — |", markdown_text(kind));
        }
    }
}

fn render_requirement_markdown_details(output: &mut String, requirement: &StoredRequirementReview) {
    let _ = writeln!(output);
    let _ = writeln!(
        output,
        "## Requirement {}",
        markdown_code(&requirement.requirement_id)
    );
    let _ = writeln!(output);
    if let Some(attempt) = requirement.attempt {
        let origin = requirement.origin.as_deref().unwrap_or("unknown");
        let duration = requirement.duration_ms.unwrap_or(0);
        let _ = writeln!(
            output,
            "- **Attempt:** {attempt} ({}, {duration} ms)",
            markdown_text(origin)
        );
    }
    match &requirement.status {
        StoredRequirementStatus::Pending => {
            let _ = writeln!(output, "- **Status:** pending");
        }
        StoredRequirementStatus::Completed {
            verdict,
            confidence,
        } => {
            let _ = writeln!(output, "- **Status:** {}", markdown_text(verdict.as_str()));
            let _ = writeln!(output, "- **Confidence:** {confidence:.2}");
            if let Some(path) = &requirement.result_path {
                let _ = writeln!(
                    output,
                    "- **Result:** {}",
                    markdown_code(&path.display().to_string())
                );
            }
            if let Some(details) = &requirement.details {
                render_markdown_details(output, details);
            }
        }
        StoredRequirementStatus::Unavailable { kind, error } => {
            render_markdown_failure(output, "unavailable", kind, error);
        }
        StoredRequirementStatus::Invalid { kind, error } => {
            render_markdown_failure(output, "invalid", kind, error);
        }
    }
}

fn render_markdown_failure(output: &mut String, status: &str, kind: &str, error: &str) {
    let _ = writeln!(output, "- **Status:** {}", markdown_text(status));
    let _ = writeln!(output, "- **Failure:** {}", markdown_text(kind));
    let _ = writeln!(output, "- **Error:** {}", markdown_text(error));
}

fn render_markdown_details(output: &mut String, details: &StoredReviewDetails) {
    let _ = writeln!(output);
    let _ = writeln!(output, "### Clauses");
    for clause in &details.clause_reviews {
        let _ = writeln!(output);
        let _ = writeln!(
            output,
            "#### {} — {}",
            markdown_code(&clause.clause_id),
            markdown_text(clause.verdict.as_str())
        );
        let _ = writeln!(output);
        let _ = writeln!(output, "- **Reason:** {}", markdown_text(&clause.reason));
        render_markdown_citations(output, &clause.citations);
        let _ = writeln!(
            output,
            "- **Counterexample:** {}",
            markdown_text(&clause.counterexample)
        );
        let _ = writeln!(
            output,
            "- **Evidence assessment:** {}",
            markdown_text(&clause.evidence_assessment)
        );
    }

    let _ = writeln!(output);
    let _ = writeln!(output, "### Findings");
    if details.findings.is_empty() {
        let _ = writeln!(output);
        let _ = writeln!(output, "_None._");
    }
    for finding in &details.findings {
        let _ = writeln!(output);
        let _ = writeln!(
            output,
            "#### {} — {}",
            markdown_text(&finding.severity),
            markdown_text(&finding.title)
        );
        let _ = writeln!(output);
        let _ = writeln!(
            output,
            "- **Clause:** {} ({})",
            markdown_code(&finding.clause_id),
            markdown_text(&finding.category)
        );
        let _ = writeln!(
            output,
            "- **Explanation:** {}",
            markdown_text(&finding.explanation)
        );
        let _ = writeln!(
            output,
            "- **Scenario:** {}",
            markdown_text(&finding.scenario)
        );
        render_markdown_citations(output, &finding.citations);
        let _ = writeln!(
            output,
            "- **Affected outcome:** {}",
            markdown_text(&finding.affected_outcome)
        );
        let _ = writeln!(
            output,
            "- **Suggested verification:** {}",
            markdown_text(&finding.suggested_verification)
        );
    }

    render_markdown_items(output, "Missing evidence", &details.missing_evidence);
    render_markdown_items(output, "Context limitations", &details.context_limitations);
}

fn render_markdown_citations(output: &mut String, citations: &[StoredCitation]) {
    if citations.is_empty() {
        let _ = writeln!(output, "- **Citations:** none");
        return;
    }
    let rendered = citations
        .iter()
        .map(|citation| markdown_code(&format!("{}:{}", citation.file, citation.line)))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(output, "- **Citations:** {rendered}");
}

fn render_markdown_items(output: &mut String, heading: &str, items: &[String]) {
    let _ = writeln!(output);
    let _ = writeln!(output, "### {heading}");
    let _ = writeln!(output);
    if items.is_empty() {
        let _ = writeln!(output, "_None._");
    }
    for item in items {
        let _ = writeln!(output, "- {}", markdown_text(item));
    }
}

fn markdown_text(value: &str) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '@' => escaped.push_str("&#64;"),
            '\\' | '`' | '*' | '_' | '[' | ']' | '(' | ')' | '#' | '!' | '|' => {
                escaped.push('\\');
                escaped.push(character);
            }
            _ => escaped.push(character),
        }
    }
    escaped
}

fn markdown_code(value: &str) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('`');
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '@' => escaped.push_str("&#64;"),
            '`' => escaped.push_str("&#96;"),
            _ => escaped.push(character),
        }
    }
    escaped.push('`');
    escaped
}

#[shallguard::enforces(
    "REQ-CLI-003",
    "REQ-CLI-007",
    "REQ-CLI-008",
    "REQ-CLI-009",
    "REQ-CLI-011"
)]
fn render_stored_review(review: &StoredReview, details: bool, color: bool) -> String {
    let mut output = String::new();
    let review_path = review.output_dir.display().to_string();
    let _ = writeln!(
        output,
        "{}: {}",
        cli_color::section("Review", color),
        cli_color::path(&review_path, color)
    );
    let _ = writeln!(
        output,
        "{}: {}",
        cli_color::label("Status", color),
        cli_color::review_status(review.status.as_str(), color)
    );
    let _ = writeln!(
        output,
        "{}: {}",
        cli_color::label("Provider", color),
        review.provider
    );
    let _ = writeln!(
        output,
        "{}: {}",
        cli_color::label("Model", color),
        review.model
    );
    let _ = writeln!(output);
    let _ = writeln!(
        output,
        "{}: {}/{} processed",
        cli_color::label("Progress", color),
        review.processed_responses,
        review.selected_requirements
    );
    let summary = format!(
        "{} satisfied, {} violated, {} insufficient evidence, {} not impacted; {} unavailable or invalid",
        review.verdicts.satisfied,
        review.verdicts.violated,
        review.verdicts.insufficient_evidence,
        review.verdicts.not_impacted,
        review.unavailable_or_invalid,
    );
    let _ = writeln!(
        output,
        "{}: {}",
        cli_color::label("Semantic verdicts", color),
        cli_color::review_outcomes(&summary, color)
    );

    if details {
        for requirement in &review.requirements {
            let _ = writeln!(output);
            render_requirement_details(&mut output, requirement, color);
        }
    } else {
        let _ = writeln!(output, "\n{}:", cli_color::section("Requirements", color));
        for requirement in &review.requirements {
            render_requirement_summary(&mut output, requirement, color);
        }
    }
    output
}

fn render_requirement_summary(
    output: &mut String,
    requirement: &StoredRequirementReview,
    color: bool,
) {
    match &requirement.status {
        StoredRequirementStatus::Pending => {
            let _ = writeln!(
                output,
                "  {}  {}",
                cli_color::identifier(&requirement.requirement_id, color),
                cli_color::review_status("pending", color)
            );
        }
        StoredRequirementStatus::Completed {
            verdict,
            confidence,
        } => {
            let verdict = cli_color::review_outcomes(verdict.as_str(), color);
            let _ = writeln!(
                output,
                "  {}  {}  confidence {:.2}",
                cli_color::identifier(&requirement.requirement_id, color),
                verdict,
                confidence
            );
        }
        StoredRequirementStatus::Unavailable { kind, .. } => {
            let _ = writeln!(
                output,
                "  {}  {} ({kind})",
                cli_color::identifier(&requirement.requirement_id, color),
                cli_color::review_status("unavailable", color)
            );
        }
        StoredRequirementStatus::Invalid { kind, .. } => {
            let _ = writeln!(
                output,
                "  {}  {} ({kind})",
                cli_color::identifier(&requirement.requirement_id, color),
                cli_color::review_status("invalid", color)
            );
        }
    }
}

fn render_requirement_details(
    output: &mut String,
    requirement: &StoredRequirementReview,
    color: bool,
) {
    let _ = writeln!(
        output,
        "{}: {}",
        cli_color::section("Requirement", color),
        cli_color::identifier(&requirement.requirement_id, color)
    );
    if let Some(attempt) = requirement.attempt {
        let origin = requirement.origin.as_deref().unwrap_or("unknown");
        let duration = requirement.duration_ms.unwrap_or(0);
        let _ = writeln!(
            output,
            "{}: {attempt} ({origin}, {duration} ms)",
            cli_color::label("Attempt", color)
        );
    }
    let _ = writeln!(output);
    match &requirement.status {
        StoredRequirementStatus::Pending => {
            let _ = writeln!(
                output,
                "{}: {}",
                cli_color::label("Status", color),
                cli_color::review_status("pending", color)
            );
        }
        StoredRequirementStatus::Completed {
            verdict,
            confidence,
        } => {
            let verdict = cli_color::review_outcomes(verdict.as_str(), color);
            let _ = writeln!(output, "{}: {verdict}", cli_color::label("Status", color));
            let _ = writeln!(
                output,
                "{}: {confidence:.2}",
                cli_color::label("Confidence", color)
            );
            if let Some(path) = &requirement.result_path {
                let path = path.display().to_string();
                let _ = writeln!(
                    output,
                    "{}: {}",
                    cli_color::label("Result", color),
                    cli_color::path(&path, color)
                );
            }
            if let Some(details) = &requirement.details {
                let _ = writeln!(output);
                render_details(output, details, color);
            }
        }
        StoredRequirementStatus::Unavailable { kind, error } => {
            render_failure(output, "unavailable", kind, error, color);
        }
        StoredRequirementStatus::Invalid { kind, error } => {
            render_failure(output, "invalid", kind, error, color);
        }
    }
}

fn render_failure(output: &mut String, status: &str, kind: &str, error: &str, color: bool) {
    let _ = writeln!(
        output,
        "{}: {}",
        cli_color::label("Status", color),
        cli_color::review_status(status, color)
    );
    let _ = writeln!(output, "{}: {kind}", cli_color::label("Failure", color));
    let _ = writeln!(output, "{}: {error}", cli_color::label("Error", color));
}

fn render_details(output: &mut String, details: &StoredReviewDetails, color: bool) {
    let _ = writeln!(output, "{}:", cli_color::section("Clauses", color));
    for clause in &details.clause_reviews {
        let verdict = cli_color::review_outcomes(clause.verdict.as_str(), color);
        let _ = writeln!(
            output,
            "  {}: {verdict}",
            cli_color::identifier(&clause.clause_id, color)
        );
        let _ = writeln!(
            output,
            "    {}: {}",
            cli_color::label("Reason", color),
            clause.reason
        );
        render_citations(output, "    Citations", &clause.citations, color);
        let _ = writeln!(
            output,
            "    {}: {}",
            cli_color::label("Counterexample", color),
            clause.counterexample
        );
        let _ = writeln!(
            output,
            "    {}: {}",
            cli_color::label("Evidence assessment", color),
            clause.evidence_assessment
        );
    }

    let _ = writeln!(output);
    let _ = writeln!(output, "{}:", cli_color::section("Findings", color));
    if details.findings.is_empty() {
        let _ = writeln!(output, "  (none)");
    }
    for finding in &details.findings {
        let _ = writeln!(
            output,
            "  [{}] {} — {} ({})",
            cli_color::severity(&finding.severity, color),
            finding.title,
            cli_color::identifier(&finding.clause_id, color),
            finding.category
        );
        let _ = writeln!(
            output,
            "    {}: {}",
            cli_color::label("Explanation", color),
            finding.explanation
        );
        let _ = writeln!(
            output,
            "    {}: {}",
            cli_color::label("Scenario", color),
            finding.scenario
        );
        render_citations(output, "    Citations", &finding.citations, color);
        let _ = writeln!(
            output,
            "    {}: {}",
            cli_color::label("Affected outcome", color),
            finding.affected_outcome
        );
        let _ = writeln!(
            output,
            "    {}: {}",
            cli_color::label("Suggested verification", color),
            finding.suggested_verification
        );
    }
    let _ = writeln!(output);
    render_items(output, "Missing evidence", &details.missing_evidence, color);
    let _ = writeln!(output);
    render_items(
        output,
        "Context limitations",
        &details.context_limitations,
        color,
    );
}

fn render_citations(output: &mut String, heading: &str, citations: &[StoredCitation], color: bool) {
    let _ = writeln!(output, "{}:", cli_color::label(heading, color));
    if citations.is_empty() {
        let _ = writeln!(output, "      (none)");
    }
    for citation in citations {
        let _ = writeln!(
            output,
            "      {}:{}",
            cli_color::path(&citation.file, color),
            citation.line
        );
    }
}

fn render_items(output: &mut String, heading: &str, items: &[String], color: bool) {
    let _ = writeln!(output, "{}:", cli_color::section(heading, color));
    if items.is_empty() {
        let _ = writeln!(output, "  (none)");
    }
    for item in items {
        let _ = writeln!(output, "  - {item}");
    }
}

#[shallguard::enforces("REQ-REV-005")]
pub(super) fn review_outcome_summary(
    review: &shallguard::review::ReviewRun,
    color: bool,
) -> String {
    let summary = format!(
        "{} satisfied, {} violated, {} insufficient evidence, {} not impacted; {} unavailable or invalid",
        review.verdicts.satisfied,
        review.verdicts.violated,
        review.verdicts.insufficient_evidence,
        review.verdicts.not_impacted,
        review.failures,
    );
    cli_color::review_outcomes(&summary, color).into_owned()
}

#[cfg(test)]
#[path = "cli_review_show_tests.rs"]
mod tests;
