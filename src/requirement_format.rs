//! Structural linting and deterministic formatting for requirement documents.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::DocSpec;
use crate::docs::parse_text;

const REQUIREMENT_LINE_WIDTH: usize = 88;

/// One requirement-document lint failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatDiagnostic {
    /// Workspace-relative document containing the failure.
    pub document: String,
    /// One-based source line where the requirement starts.
    pub line: usize,
    /// Human-readable explanation of the malformed structure.
    pub message: String,
}

/// Result of checking or formatting one or more requirement documents.
#[derive(Debug)]
pub struct FormatReport {
    /// Number of documents inspected.
    pub documents: usize,
    /// Number of requirement blocks inspected.
    pub requirements: usize,
    /// Documents whose canonical representation differs from disk.
    pub changed_documents: Vec<PathBuf>,
    /// Structural lint failures. Formatting refuses to write when non-empty.
    pub diagnostics: Vec<FormatDiagnostic>,
}

impl FormatReport {
    /// Returns whether every document is structurally valid and canonically formatted.
    pub fn is_clean(&self) -> bool {
        self.diagnostics.is_empty() && self.changed_documents.is_empty()
    }
}

struct PreparedDocument {
    path: PathBuf,
    formatted: String,
    changed: bool,
    requirements: usize,
    diagnostics: Vec<FormatDiagnostic>,
}

struct FormattedText {
    text: String,
    requirements: usize,
    diagnostics: Vec<FormatDiagnostic>,
}

/// Checks requirement structure and canonical formatting without changing files.
///
/// # Errors
///
/// Returns an error when a document cannot be read or when semantic-equivalence
/// verification of the formatted representation fails.
#[shallguard_macros::enforces("REQ-SPEC-006")]
pub fn check(root: &Path, specs: &[DocSpec]) -> Result<FormatReport> {
    run(root, specs, false)
}

/// Lints and canonically formats requirement blocks in place.
///
/// No document is changed when any selected document has a structural lint
/// failure. Surrounding Markdown outside requirement blocks is retained.
///
/// # Errors
///
/// Returns an error when a document cannot be read or written, or when formatting
/// would change the parsed requirement meaning.
#[shallguard_macros::enforces("REQ-SPEC-005")]
pub fn format(root: &Path, specs: &[DocSpec]) -> Result<FormatReport> {
    run(root, specs, true)
}

fn run(root: &Path, specs: &[DocSpec], write: bool) -> Result<FormatReport> {
    let mut prepared = Vec::with_capacity(specs.len());
    for spec in specs {
        prepared.push(prepare_document(root, spec)?);
    }

    let diagnostics = prepared
        .iter()
        .flat_map(|document| document.diagnostics.iter().cloned())
        .collect::<Vec<_>>();
    if write && diagnostics.is_empty() {
        for document in prepared.iter().filter(|document| document.changed) {
            write_atomic(&document.path, document.formatted.as_bytes())?;
        }
    }

    Ok(FormatReport {
        documents: specs.len(),
        requirements: prepared.iter().map(|document| document.requirements).sum(),
        changed_documents: prepared
            .iter()
            .filter(|document| document.changed)
            .map(|document| workspace_relative(root, &document.path))
            .collect(),
        diagnostics,
    })
}

fn prepare_document(root: &Path, spec: &DocSpec) -> Result<PreparedDocument> {
    let path = root.join(&spec.path);
    let original = std::fs::read_to_string(&path)
        .with_context(|| format!("reading requirement document {}", path.display()))?;
    let formatted = format_text(&original, spec);
    if formatted.diagnostics.is_empty() {
        verify_semantic_equivalence(&original, &formatted.text, spec)?;
    }
    Ok(PreparedDocument {
        changed: original != formatted.text,
        path,
        formatted: formatted.text,
        requirements: formatted.requirements,
        diagnostics: formatted.diagnostics,
    })
}

fn format_text(text: &str, spec: &DocSpec) -> FormattedText {
    let candidate_re =
        Regex::new(r"^\s*-\s+\*\*REQ-").expect("BUG: invalid requirement candidate regex");
    let lines = text.lines().collect::<Vec<_>>();
    let mut output = Vec::with_capacity(lines.len());
    let mut diagnostics = Vec::new();
    let mut requirements = 0;
    let mut index = 0;

    while index < lines.len() {
        if !candidate_re.is_match(lines[index]) {
            output.push(lines[index].to_string());
            index += 1;
            continue;
        }

        let start = index;
        index += 1;
        while index < lines.len()
            && !lines[index].is_empty()
            && lines[index].chars().next().is_some_and(char::is_whitespace)
        {
            index += 1;
        }
        let block = &lines[start..index];
        requirements += 1;
        let block_diagnostics = lint_block(block, spec, start + 1);
        if block_diagnostics.is_empty() {
            output.extend(format_block(block));
        } else {
            output.extend(block.iter().map(|line| (*line).to_string()));
            diagnostics.extend(block_diagnostics);
        }
    }

    let mut text = output.join("\n");
    text.push('\n');
    FormattedText {
        text,
        requirements,
        diagnostics,
    }
}

#[shallguard_macros::enforces("REQ-SPEC-001", "REQ-SPEC-002", "REQ-SPEC-004")]
fn lint_block(block: &[&str], spec: &DocSpec, line: usize) -> Vec<FormatDiagnostic> {
    let header_re = Regex::new(r"^- \*\*(REQ-([A-Z]{2,})-\d{3})\*\* — \S")
        .expect("BUG: invalid requirement header regex");
    let normative_re =
        Regex::new(r"\b(?:SHALL(?: NOT)?|MAY)\b").expect("BUG: invalid normative regex");
    let combined = normalized_block(block);
    let mut messages = Vec::new();

    if !header_re.is_match(&combined) {
        messages.push(
            "expected `- **REQ-<AREA>-<NNN>** — <normative statement>` with an uppercase area and three-digit number"
                .to_string(),
        );
    }

    let enforced_count = combined.matches("*Enforced:*").count();
    let verified_count = combined.matches("*Verified:*").count();
    let retired = combined.to_ascii_lowercase().contains("(retired");
    if retired && enforced_count == 0 && verified_count == 0 {
        return diagnostics(spec, line, messages);
    }
    if enforced_count != 1 {
        messages.push(format!(
            "active requirement must contain exactly one `*Enforced:*` segment; found {enforced_count}"
        ));
    }
    if verified_count != 1 {
        messages.push(format!(
            "active requirement must contain exactly one `*Verified:*` segment; found {verified_count}"
        ));
    }
    let Some((statement, after_enforced)) = combined.split_once("*Enforced:*") else {
        return diagnostics(spec, line, messages);
    };
    let Some((enforced, verified)) = after_enforced.split_once("*Verified:*") else {
        return diagnostics(spec, line, messages);
    };
    if !normative_re.is_match(statement) && !enforced.contains("not implemented") {
        messages.push("active requirement statement must use SHALL, SHALL NOT, or MAY".to_string());
    }
    if enforced.trim().trim_end_matches('·').trim().is_empty() {
        messages.push("`*Enforced:*` segment must name enforcement evidence".to_string());
    }
    if !enforced.trim_end().ends_with('·') {
        messages.push("separate `*Enforced:*` and `*Verified:*` with `·`".to_string());
    }
    if verified.trim().is_empty() {
        messages.push("`*Verified:*` segment must name verification evidence".to_string());
    }
    if !['✅', '🔬', '👁', '⏳']
        .iter()
        .any(|indicator| verified.contains(*indicator))
    {
        messages.push("`*Verified:*` must contain ✅, 🔬, 👁, or ⏳ evidence status".to_string());
    }
    diagnostics(spec, line, messages)
}

fn diagnostics(spec: &DocSpec, line: usize, messages: Vec<String>) -> Vec<FormatDiagnostic> {
    messages
        .into_iter()
        .map(|message| FormatDiagnostic {
            document: spec.path.clone(),
            line,
            message,
        })
        .collect()
}

fn normalized_block(block: &[&str]) -> String {
    block
        .iter()
        .flat_map(|line| line.split_whitespace())
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_block(block: &[&str]) -> Vec<String> {
    let mut formatted = Vec::new();
    for (index, line) in block.iter().enumerate() {
        wrap_existing_line(line.trim(), index == 0, &mut formatted);
    }
    formatted
}

fn wrap_existing_line(line: &str, first_line: bool, output: &mut Vec<String>) {
    let indent = if first_line { "" } else { "  " };
    let mut current = indent.to_string();
    for word in line.split_whitespace() {
        let has_content = current.len() > indent.len();
        let separator = usize::from(has_content);
        if has_content
            && current.chars().count() + separator + word.chars().count() > REQUIREMENT_LINE_WIDTH
        {
            output.push(current);
            current = "  ".to_string();
        }
        let current_indent = if current.starts_with("  ") { 2 } else { 0 };
        if current.len() > current_indent {
            current.push(' ');
        }
        current.push_str(word);
    }
    if current.len() > indent.len() {
        output.push(current);
    }
}

fn verify_semantic_equivalence(original: &str, formatted: &str, spec: &DocSpec) -> Result<()> {
    let before = parse_text(original, spec);
    let after = parse_text(formatted, spec);
    if before.requirements.len() != after.requirements.len() {
        bail!(
            "formatting {} would change the parsed requirement count from {} to {}",
            spec.path,
            before.requirements.len(),
            after.requirements.len()
        );
    }
    for (before, after) in before.requirements.iter().zip(&after.requirements) {
        if before.id != after.id
            || before.statement != after.statement
            || before.enforced_text != after.enforced_text
            || before.verified_text != after.verified_text
        {
            bail!(
                "formatting {} would change the parsed meaning of {}",
                spec.path,
                before.id
            );
        }
    }
    Ok(())
}

fn write_atomic(path: &Path, content: &[u8]) -> Result<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("requirement document has no UTF-8 file name")?;
    let temporary = path.with_file_name(format!(".{file_name}.fmt-{}", std::process::id()));
    std::fs::write(&temporary, content)
        .with_context(|| format!("writing formatted document {}", temporary.display()))?;
    std::fs::rename(&temporary, path)
        .with_context(|| format!("publishing formatted document {}", path.display()))
}

fn workspace_relative(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> DocSpec {
        DocSpec::new(
            "crate/docs/USER_STORIES_AND_REQUIREMENTS.md",
            "crate",
            std::collections::BTreeMap::new(),
        )
    }

    #[shallguard_macros::verifies("REQ-SPEC-005")]
    #[test]
    fn formats_requirement_blocks_without_touching_surrounding_markdown() {
        let input = "# Story\n\n- **REQ-AA-001** — The service SHALL retain a deliberately long value across every ordinary processing pass so the formatter has to wrap it.\n    *Enforced:* `src/lib.rs` (`apply`) · *Verified:* ✅ `src/lib.rs` (`test_apply`)\n\n| prose | stays |\n";

        let formatted = format_text(input, &spec());

        assert!(formatted.diagnostics.is_empty());
        assert_eq!(formatted.requirements, 1);
        assert!(
            formatted
                .text
                .lines()
                .any(|line| line.starts_with("  ") && line.contains("formatter"))
        );
        assert!(formatted.text.contains("*Enforced:*"));
        assert!(formatted.text.ends_with("\n| prose | stays |\n"));
        assert!(
            formatted
                .text
                .lines()
                .filter(|line| line.starts_with("- **REQ-") || line.starts_with("  "))
                .all(|line| line.chars().count() <= REQUIREMENT_LINE_WIDTH)
        );
    }

    #[shallguard_macros::verifies("REQ-SPEC-005")]
    #[test]
    fn formatting_is_idempotent_and_semantically_equivalent() {
        let input = "- **REQ-AA-001** — The service SHALL retain state.\n  *Enforced:* `src/lib.rs` (`apply`) · *Verified:* ✅ `src/lib.rs` (`test_apply`)\n";
        let once = format_text(input, &spec());
        let twice = format_text(&once.text, &spec());

        assert!(once.diagnostics.is_empty());
        assert_eq!(once.text, twice.text);
        verify_semantic_equivalence(input, &once.text, &spec())
            .expect("formatting preserves requirement meaning");
    }

    #[shallguard_macros::verifies("REQ-SPEC-001", "REQ-SPEC-002")]
    #[test]
    fn rejects_missing_segments_and_evidence_status() {
        let missing_verified =
            "- **REQ-AA-001** — The service SHALL retain state.\n  *Enforced:* `src/lib.rs`\n";
        let no_status = "- **REQ-AA-002** — The service SHALL retain state.\n  *Enforced:* `src/lib.rs` · *Verified:* `src/lib.rs` (`test_apply`)\n";

        let first = format_text(missing_verified, &spec());
        let second = format_text(no_status, &spec());

        assert!(
            first
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("exactly one `*Verified:*`"))
        );
        assert!(
            second
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("must contain"))
        );
    }

    #[shallguard_macros::verifies("REQ-SPEC-004")]
    #[test]
    fn permits_retired_requirements_without_evidence_segments() {
        let input = "- **REQ-AA-001** — *(retired into REQ-AA-002.)*\n";

        let formatted = format_text(input, &spec());

        assert!(formatted.diagnostics.is_empty());
        assert_eq!(formatted.text, input);
    }

    #[test]
    fn formats_files_atomically_and_then_checks_clean() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let document = directory
            .path()
            .join("crate/docs/USER_STORIES_AND_REQUIREMENTS.md");
        std::fs::create_dir_all(document.parent().expect("document has parent"))
            .expect("document directory creates");
        std::fs::write(
            &document,
            "- **REQ-AA-001** — The service SHALL retain state.\n    *Enforced:* `src/lib.rs` (`apply`) · *Verified:* ✅ `src/lib.rs` (`test_apply`)\n",
        )
        .expect("fixture writes");

        let formatted = super::format(directory.path(), &[spec()]).expect("format succeeds");
        assert_eq!(
            formatted.changed_documents,
            vec![PathBuf::from("crate/docs/USER_STORIES_AND_REQUIREMENTS.md")]
        );
        let text = std::fs::read_to_string(&document).expect("formatted document reads");
        assert!(text.contains("\n  *Enforced:*"));

        let checked = check(directory.path(), &[spec()]).expect("check succeeds");
        assert!(checked.is_clean());
    }

    #[shallguard_macros::verifies("REQ-SPEC-005")]
    #[test]
    fn lint_failures_prevent_formatter_writes() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let document = directory
            .path()
            .join("crate/docs/USER_STORIES_AND_REQUIREMENTS.md");
        std::fs::create_dir_all(document.parent().expect("document has parent"))
            .expect("document directory creates");
        let malformed =
            "- **REQ-AA-001** — The service SHALL retain state.\n  *Enforced:* `src/lib.rs`\n";
        std::fs::write(&document, malformed).expect("fixture writes");

        let report = super::format(directory.path(), &[spec()]).expect("lint report returns");

        assert!(!report.diagnostics.is_empty());
        assert_eq!(
            std::fs::read_to_string(document).expect("unchanged document reads"),
            malformed
        );
    }
}
