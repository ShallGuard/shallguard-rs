//! Parsing of the `USER_STORIES_AND_REQUIREMENTS.md` documents.
//!
//! The requirement format is regular (see the Conventions section of
//! either document): a requirement is a top-level list item
//! `- **REQ-<AREA>-<NNN>** — ...` whose continuation lines are indented
//! by two spaces, carrying `*Enforced:*` and `*Verified:*` segments.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use regex::Regex;

use crate::DocSpec;

/// One concrete piece of automated evidence cited in a *Verified:* line:
/// a test file plus, ideally, the test function name.
#[derive(Debug, PartialEq, Eq)]
pub struct Evidence {
    /// Workspace-relative test file.
    pub file: PathBuf,
    /// Test function named next to the file, when given.
    pub test_fn: Option<String>,
}

/// One parsed requirement.
pub struct Requirement {
    pub id: String,
    pub area: String,
    /// Short human-readable excerpt of the normative statement, for
    /// report messages.
    pub title: String,
    /// Whitespace-normalized normative statement used for base/head
    /// comparison. This is not persisted as a fingerprint.
    pub statement: String,
    /// Whitespace-normalized *Enforced:* segment.
    pub enforced_text: String,
    /// Whitespace-normalized *Verified:* segment.
    pub verified_text: String,
    /// Document path (workspace-relative) the requirement came from.
    pub doc: String,
    /// 1-based line of the requirement definition.
    pub line: usize,
    /// Workspace-relative code paths from the *Enforced:* segment.
    /// Directory references keep their trailing `/`.
    pub enforced_paths: Vec<PathBuf>,
    /// The *Enforced:* segment declares the capability unimplemented or
    /// enforced by design only — exempt from anchor checks.
    pub not_implemented: bool,
    /// Retired requirement — exempt from all checks except uniqueness.
    pub retired: bool,
    /// `✅` automated-test evidence claimed.
    pub automated: bool,
    /// Concrete evidence citations parsed from the *Verified:* segment:
    /// file spans, each optionally followed by test-function spans.
    pub evidence: Vec<Evidence>,
    /// `🔬` end-to-end evidence claimed.
    pub e2e: bool,
    /// `👁` code-review-only evidence.
    pub review_only: bool,
    /// `⏳` pending evidence.
    pub pending: bool,
}

/// A fully parsed document.
pub struct ParsedDoc {
    pub requirements: Vec<Requirement>,
    /// Every resolvable code-path span in the document (not only inside
    /// requirements), with its 1-based line — used for existence checks.
    pub path_spans: Vec<(usize, PathBuf)>,
}

#[shallguard_macros::enforces("REQ-TRACE-005")]
pub fn parse_doc(root: &Path, spec: &DocSpec) -> Result<ParsedDoc> {
    let text = std::fs::read_to_string(root.join(&spec.path))
        .with_context(|| format!("reading {}", spec.path))?;
    Ok(parse_text(&text, spec))
}

/// Parses requirement-document content supplied by callers such as the
/// base-revision impact analyzer.
#[shallguard_macros::enforces("REQ-SPEC-001")]
pub(crate) fn parse_text(text: &str, spec: &DocSpec) -> ParsedDoc {
    let def_re =
        Regex::new(r"^- \*\*(REQ-([A-Z]{2,})-\d{3})\*\*").expect("BUG: invalid requirement regex");
    let span_re = Regex::new("`([^`]+)`").expect("BUG: invalid span regex");

    let lines: Vec<&str> = text.lines().collect();
    let mut requirements = Vec::new();
    let mut path_spans = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        for caps in span_re.captures_iter(line) {
            if let Some(path) = resolve_path_span(spec, &caps[1]) {
                path_spans.push((idx + 1, path));
            }
        }
    }

    let mut i = 0;
    while i < lines.len() {
        let Some(caps) = def_re.captures(lines[i]) else {
            i += 1;
            continue;
        };
        let id = caps[1].to_string();
        let area = caps[2].to_string();
        let def_line = i + 1;
        let mut chunk = lines[i].to_string();
        i += 1;
        while i < lines.len() && lines[i].starts_with("  ") {
            chunk.push('\n');
            chunk.push_str(lines[i]);
            i += 1;
        }
        requirements.push(parse_chunk(spec, id, area, def_line, &chunk, &span_re));
    }

    ParsedDoc {
        requirements,
        path_spans,
    }
}

#[shallguard_macros::enforces("REQ-SPEC-003", "REQ-SPEC-004")]
fn parse_chunk(
    spec: &DocSpec,
    id: String,
    area: String,
    line: usize,
    chunk: &str,
    span_re: &Regex,
) -> Requirement {
    let retired = chunk.contains("(retired");

    // Segment the chunk: [.. requirement text ..] *Enforced:* [..] *Verified:* [..]
    let (statement_text, enforced_text, verified_text) = match chunk.split_once("*Enforced:*") {
        Some((statement, rest)) => match rest.split_once("*Verified:*") {
            Some((enforced, verified)) => (statement, enforced, verified),
            None => (statement, rest, ""),
        },
        None => (chunk, "", ""),
    };

    let mut enforced_paths = Vec::new();
    for caps in span_re.captures_iter(enforced_text) {
        if let Some(path) = resolve_path_span(spec, &caps[1]) {
            enforced_paths.push(path);
        }
    }

    let not_implemented =
        enforced_text.contains("not implemented") || enforced_text.contains("by design");

    // Evidence citations: file spans in the Verified segment, each
    // optionally followed by identifier spans naming test functions.
    let mut evidence: Vec<Evidence> = Vec::new();
    for caps in span_re.captures_iter(verified_text) {
        let raw = &caps[1];
        if let Some(file) = resolve_path_span(spec, raw) {
            evidence.push(Evidence {
                file,
                test_fn: None,
            });
        } else if is_identifier(raw) {
            match evidence.last_mut() {
                Some(last) if last.test_fn.is_none() => last.test_fn = Some(raw.to_string()),
                Some(last) => {
                    let file = last.file.clone();
                    evidence.push(Evidence {
                        file,
                        test_fn: Some(raw.to_string()),
                    });
                }
                // Identifier before any file span: not attributable.
                None => {}
            }
        }
    }

    Requirement {
        id,
        area,
        title: statement_title(chunk),
        statement: normalize_requirement_text(statement_text),
        enforced_text: normalize_requirement_text(enforced_text),
        verified_text: normalize_requirement_text(verified_text),
        doc: spec.path.clone(),
        line,
        enforced_paths,
        not_implemented,
        retired,
        automated: verified_text.contains('\u{2705}'), // ✅
        evidence,
        e2e: verified_text.contains('\u{1F52C}'), // 🔬
        review_only: verified_text.contains('\u{1F441}'), // 👁
        pending: verified_text.contains('\u{23F3}'), // ⏳
    }
}

/// Normalizes Markdown layout while retaining wording and punctuation.
/// Requirement code spans do not contain meaningful whitespace today,
/// so ordinary whitespace collapsing is sufficient for change impact.
fn normalize_requirement_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A plain Rust identifier (candidate test-function name).
fn is_identifier(raw: &str) -> bool {
    let mut chars = raw.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Extracts a short excerpt of the normative statement: the text after
/// the em-dash following the bold ID, whitespace-collapsed, markdown
/// bold stripped, cut at a word boundary.
fn statement_title(chunk: &str) -> String {
    const MAX_CHARS: usize = 64;
    let after = chunk
        .split_once("\u{2014} ")
        .map_or(chunk, |(_, rest)| rest);
    let end = after
        .find("*Enforced:*")
        .or_else(|| after.find("*Verified:*"))
        .unwrap_or(after.len());
    let full = after[..end]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("**", "");
    if full.chars().count() <= MAX_CHARS {
        return full;
    }
    let mut cut = String::new();
    for word in full.split(' ') {
        if cut.chars().count() + word.chars().count() + 1 > MAX_CHARS {
            break;
        }
        if !cut.is_empty() {
            cut.push(' ');
        }
        cut.push_str(word);
    }
    if cut.is_empty() {
        cut = full.chars().take(MAX_CHARS).collect();
    }
    cut.push_str("...");
    cut
}

/// Resolves a backticked span to a workspace-relative code path, or
/// `None` when the span is not a code-path reference. Accepted spans
/// start with `src/` or `tests/` (optionally behind a crate prefix like
/// `router:`), may carry a `:NNN` / `:NNN-MMM` line suffix after `.rs`,
/// and either name a `.rs` file or a directory (`.../`).
#[shallguard_macros::enforces("REQ-PORT-004")]
fn resolve_path_span(spec: &DocSpec, raw: &str) -> Option<PathBuf> {
    // A leading segment before `:` is a source-root prefix only when it maps
    // in this document's repository configuration. Otherwise the `:` may be
    // a line suffix and remains part of `raw` until stripped below.
    let (source_root, rest) = match raw.split_once(':') {
        Some((prefix, rest)) if spec.prefixes.contains_key(prefix) => (
            spec.prefixes
                .get(prefix)
                .expect("BUG: prefix matched above")
                .as_str(),
            rest,
        ),
        _ => (spec.source_root.as_str(), raw),
    };
    // Strip a line suffix such as `:88-289` after the file name.
    let path = match rest.find(".rs:") {
        Some(i) => &rest[..i + ".rs".len()],
        None => rest,
    };
    let looks_like_root = path.starts_with("src/") || path.starts_with("tests/");
    let looks_like_code = path.ends_with(".rs") || path.ends_with('/');
    if !looks_like_root || !looks_like_code || path.contains(char::is_whitespace) {
        return None;
    }
    Some(if source_root == "." {
        PathBuf::from(path)
    } else {
        PathBuf::from(source_root).join(path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> DocSpec {
        DocSpec::new(
            "example-app/docs/USER_STORIES_AND_REQUIREMENTS.md",
            "example-app",
            std::collections::BTreeMap::from([("core".to_string(), "example-core".to_string())]),
        )
    }

    const SAMPLE: &str = "\
## Some Section

- **REQ-HRS-002** — A goal absent from the request SHALL be implicitly
  zeroed **only if** its routing domain appears in the request.
  *Enforced:* `core:src/router/tasks/config_manager.rs`
  (`update_goal_weights_with_scope`) · *Verified:* ✅ regression test ·
  🔬 e2e flux-cli differential
- **REQ-AUTH-022** — If the user-cache is unavailable, the service SHALL
  fall back automatically to direct MySQL loading.
  *Enforced:* not implemented — the initial load retries and then fails
  startup (`src/auth.rs`, error propagates) · *Verified:* ⏳ pending
- **REQ-CM-021** — *(retired into REQ-CM-019 — whitelist support.)*
- **REQ-OP-001** — Something with a directory reference.
  *Enforced:* `src/router/optimizer/greedy_heuristics/` ·
  *Verified:* 👁 code review only

Prose citing `tests/basic.rs` and a symbol span `update_goal_weights`.
";

    #[shallguard_macros::verifies("REQ-SPEC-001", "REQ-SPEC-003")]
    #[test]
    fn parses_requirements_and_segments() {
        let doc = parse_text(SAMPLE, &spec());
        assert_eq!(doc.requirements.len(), 4);

        let hrs = &doc.requirements[0];
        assert_eq!(hrs.id, "REQ-HRS-002");
        assert_eq!(hrs.area, "HRS");
        assert_eq!(
            hrs.title,
            "A goal absent from the request SHALL be implicitly zeroed only..."
        );
        assert!(hrs.automated && hrs.e2e && !hrs.pending);
        assert_eq!(
            hrs.enforced_paths,
            vec![PathBuf::from(
                "example-core/src/router/tasks/config_manager.rs"
            )]
        );

        let auth = &doc.requirements[1];
        assert!(auth.not_implemented && auth.pending && !auth.automated);

        let retired = &doc.requirements[2];
        assert!(retired.retired);

        let op = &doc.requirements[3];
        assert!(op.review_only);
        assert_eq!(
            op.enforced_paths,
            vec![PathBuf::from(
                "example-app/src/router/optimizer/greedy_heuristics/"
            )]
        );
    }

    #[test]
    fn collects_doc_wide_path_spans() {
        let doc = parse_text(SAMPLE, &spec());
        let spans: Vec<String> = doc
            .path_spans
            .iter()
            .map(|(_, p)| p.to_string_lossy().into_owned())
            .collect();
        assert!(spans.contains(&"example-app/tests/basic.rs".to_string()));
        // Symbol spans are not path spans.
        assert!(!spans.iter().any(|s| s.ends_with("update_goal_weights")));
    }

    #[test]
    fn resolve_rejects_non_code_spans() {
        assert!(resolve_path_span(&spec(), "REQ-HRS-002").is_none());
        assert!(resolve_path_span(&spec(), "docs/ARCHITECTURE.md").is_none());
        assert!(resolve_path_span(&spec(), "src/runtime/TESTING.md").is_none());
        assert!(resolve_path_span(&spec(), "unknown:src/auth.rs").is_none());
    }

    #[test]
    fn resolve_strips_line_suffixes_and_maps_prefixes() {
        assert_eq!(
            resolve_path_span(&spec(), "src/auth.rs:88-289"),
            Some(PathBuf::from("example-app/src/auth.rs"))
        );
        assert_eq!(
            resolve_path_span(&spec(), "core:src/router/hash_rate.rs:34"),
            Some(PathBuf::from("example-core/src/router/hash_rate.rs"))
        );
    }

    #[test]
    fn repository_root_source_paths_are_normalized() {
        let spec = DocSpec::new(
            "docs/requirements.md",
            ".",
            std::collections::BTreeMap::new(),
        );
        assert_eq!(
            resolve_path_span(&spec, "src/lib.rs"),
            Some(PathBuf::from("src/lib.rs"))
        );
    }

    #[test]
    fn requirement_comparison_text_ignores_markdown_reflow() {
        let reflowed = SAMPLE.replace(
            "A goal absent from the request SHALL be implicitly\n  zeroed",
            "A goal absent from the request SHALL be\n  implicitly zeroed",
        );
        let original = parse_text(SAMPLE, &spec());
        let reflowed = parse_text(&reflowed, &spec());
        assert_eq!(
            original.requirements[0].statement,
            reflowed.requirements[0].statement
        );
        assert_eq!(
            original.requirements[0].enforced_text,
            reflowed.requirements[0].enforced_text
        );
        assert_eq!(
            original.requirements[0].verified_text,
            reflowed.requirements[0].verified_text
        );
    }
}
