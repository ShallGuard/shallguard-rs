//! Repository-owned ShallGuard configuration.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use shallguard_macros::enforces;

use crate::DocSpec;

/// Repository-relative configuration file discovered by the CLI.
pub const CONFIG_PATH: &str = "shallguard.toml";
/// Configuration schema understood by this release.
pub const CONFIG_SCHEMA: u32 = 1;

/// All repository-specific inputs consumed by deterministic analysis.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryConfig {
    pub schema: u32,
    pub minimum_requirements: usize,
    pub baseline: PathBuf,
    pub verify_outlier_threshold: usize,
    pub documents: Vec<DocumentConfig>,
    #[serde(default)]
    pub prefixes: BTreeMap<String, PathBuf>,
    pub areas: BTreeMap<String, AreaConfig>,
    #[serde(default)]
    pub allow_missing_paths: BTreeSet<PathBuf>,
    pub artifacts: ArtifactConfig,
    #[serde(default)]
    pub review: ReviewConfig,
}

/// One requirements document and the source tree that owns its unprefixed paths.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentConfig {
    pub path: PathBuf,
    pub source_root: PathBuf,
}

/// Display and traceability-ratchet policy for one requirement area.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaConfig {
    pub label: String,
    pub hard_enforcement: bool,
    pub hard_verification: bool,
}

/// Default root for generated, disposable artifacts.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactConfig {
    pub root: PathBuf,
}

/// Optional defaults for local semantic review.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewConfig {
    pub target: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub local_provider: Option<String>,
    pub with_coverage: Option<bool>,
    pub timeout_seconds: Option<u64>,
}

impl RepositoryConfig {
    /// Loads and validates the configuration owned by `root`.
    #[enforces("REQ-PORT-002", "REQ-PORT-003", "REQ-PORT-004")]
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(CONFIG_PATH);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading ShallGuard configuration {}", path.display()))?;
        let config: Self = toml::from_str(&text)
            .with_context(|| format!("parsing ShallGuard configuration {}", path.display()))?;
        config.validate(root)?;
        Ok(config)
    }

    /// Converts every configured document into analysis input.
    pub fn documents(&self) -> Vec<DocSpec> {
        self.documents
            .iter()
            .map(|document| {
                DocSpec::new(
                    normalize(&document.path),
                    normalize(&document.source_root),
                    self.prefixes
                        .iter()
                        .map(|(prefix, root)| (prefix.clone(), normalize(root)))
                        .collect(),
                )
            })
            .collect()
    }

    /// Resolves an optional CLI document allowlist against configured documents.
    pub fn select_documents(&self, requested: &[String]) -> Result<Vec<DocSpec>> {
        if requested.is_empty() {
            return Ok(self.documents());
        }
        let by_path = self
            .documents()
            .into_iter()
            .map(|document| (document.path.clone(), document))
            .collect::<BTreeMap<_, _>>();
        requested
            .iter()
            .map(|path| {
                by_path.get(path).cloned().with_context(|| {
                    format!("requirements document {path:?} is not declared in {CONFIG_PATH}")
                })
            })
            .collect()
    }

    /// Returns the configured label, preserving unknown acronyms in diagnostics.
    pub fn area_label(&self, area: &str) -> String {
        self.areas.get(area).map_or_else(
            || area.to_string(),
            |policy| format!("{} ({area})", policy.label),
        )
    }

    /// Whether a historical gap is forbidden for this area and dimension.
    pub fn area_is_hard(&self, area: &str, verification: bool) -> bool {
        self.areas.get(area).is_some_and(|policy| {
            if verification {
                policy.hard_verification
            } else {
                policy.hard_enforcement
            }
        })
    }

    pub fn bundle_dir(&self) -> PathBuf {
        self.artifacts.root.join("review-bundle")
    }

    pub fn review_dir(&self) -> PathBuf {
        self.artifacts.root.join("local-review")
    }

    pub fn coverage_work_dir(&self) -> PathBuf {
        self.artifacts.root.join("coverage-work")
    }

    fn validate(&self, root: &Path) -> Result<()> {
        if self.schema != CONFIG_SCHEMA {
            bail!(
                "unsupported ShallGuard configuration schema {} (expected {CONFIG_SCHEMA})",
                self.schema
            );
        }
        if self.minimum_requirements == 0 {
            bail!("minimum_requirements must be greater than zero");
        }
        if self.verify_outlier_threshold == 0 {
            bail!("verify_outlier_threshold must be greater than zero");
        }
        validate_relative_path("baseline", &self.baseline, false)?;
        validate_relative_path("artifacts.root", &self.artifacts.root, false)?;
        if self.documents.is_empty() {
            bail!("at least one [[documents]] entry is required");
        }

        let mut document_paths = BTreeSet::new();
        let mut source_roots = BTreeSet::new();
        for document in &self.documents {
            validate_relative_path("documents.path", &document.path, false)?;
            validate_relative_path("documents.source_root", &document.source_root, true)?;
            if !document_paths.insert(document.path.clone()) {
                bail!(
                    "duplicate requirements document {}",
                    document.path.display()
                );
            }
            if !root.join(&document.path).is_file() {
                bail!(
                    "requirements document does not exist: {}",
                    document.path.display()
                );
            }
            if !root.join(&document.source_root).is_dir() {
                bail!(
                    "source root does not exist: {}",
                    document.source_root.display()
                );
            }
            source_roots.insert(document.source_root.clone());
        }
        for (prefix, source_root) in &self.prefixes {
            if !valid_prefix(prefix) {
                bail!("invalid path prefix {prefix:?}; expected a Rust-like identifier");
            }
            validate_relative_path("prefix source root", source_root, true)?;
            if !root.join(source_root).is_dir() {
                bail!(
                    "prefix {prefix:?} source root does not exist: {}",
                    source_root.display()
                );
            }
            source_roots.insert(source_root.clone());
        }
        for path in &self.allow_missing_paths {
            validate_relative_path("allow_missing_paths", path, false)?;
        }
        if self.areas.is_empty() {
            bail!("at least one [areas.<ID>] policy is required");
        }
        for (area, policy) in &self.areas {
            if area.len() < 2 || !area.chars().all(|character| character.is_ascii_uppercase()) {
                bail!("invalid requirement area {area:?}; expected at least two uppercase letters");
            }
            if policy.label.trim().is_empty() {
                bail!("area {area} has an empty label");
            }
        }
        if let Some(provider) = &self.review.provider
            && !matches!(provider.as_str(), "codex" | "claude")
        {
            bail!("review.provider must be codex or claude");
        }
        if let Some(local) = &self.review.local_provider
            && !matches!(local.as_str(), "ollama" | "lmstudio")
        {
            bail!("review.local_provider must be ollama or lmstudio");
        }
        if self.review.timeout_seconds == Some(0) {
            bail!("review.timeout_seconds must be greater than zero");
        }
        Ok(())
    }
}

fn validate_relative_path(field: &str, path: &Path, allow_dot: bool) -> Result<()> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        bail!("{field} must be a non-empty repository-relative path");
    }
    let mut saw_normal = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => saw_normal = true,
            Component::CurDir if allow_dot => {}
            _ => bail!(
                "{field} must be normalized and may not escape the repository: {}",
                path.display()
            ),
        }
    }
    if !(saw_normal || allow_dot && path == Path::new(".")) {
        bail!("{field} must identify a path");
    }
    Ok(())
}

fn normalize(path: &Path) -> String {
    if path == Path::new(".") {
        ".".to_string()
    } else {
        path.to_string_lossy().replace('\\', "/")
    }
}

fn valid_prefix(prefix: &str) -> bool {
    let mut characters = prefix.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

#[cfg(test)]
mod tests {
    use std::fs;

    use shallguard_macros::verifies;
    use tempfile::tempdir;

    use super::*;

    fn write_fixture(root: &Path, source_root: &str, document: &str) {
        fs::create_dir_all(root.join(source_root).join("src")).expect("create fixture source");
        fs::create_dir_all(root.join("docs")).expect("create fixture docs");
        fs::write(root.join(document), "# Requirements\n").expect("write fixture document");
        fs::write(
            root.join(CONFIG_PATH),
            format!(
                r#"schema = 1
minimum_requirements = 1
baseline = ".shallguard/baseline.toml"
verify_outlier_threshold = 6

[[documents]]
path = "{document}"
source_root = "{source_root}"

[areas.PORT]
label = "Portability"
hard_enforcement = true
hard_verification = true

[artifacts]
root = "target/shallguard"
"#
            ),
        )
        .expect("write fixture configuration");
    }

    #[verifies("REQ-CLI-004", "REQ-PORT-002", "REQ-PORT-003", "REQ-PORT-004")]
    #[test]
    fn loads_single_package_repository_configuration() {
        let fixture = tempdir().expect("create fixture");
        write_fixture(fixture.path(), ".", "docs/requirements.md");

        let config = RepositoryConfig::load(fixture.path()).expect("load repository config");
        let documents = config.documents();
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].source_root, ".");
        assert_eq!(
            config.bundle_dir(),
            PathBuf::from("target/shallguard/review-bundle")
        );
        assert!(config.area_is_hard("PORT", false));
    }

    #[verifies("REQ-PORT-002", "REQ-PORT-003", "REQ-PORT-004")]
    #[test]
    fn loads_virtual_workspace_repository_configuration() {
        let fixture = tempdir().expect("create fixture");
        write_fixture(fixture.path(), "crates/app", "docs/requirements.md");
        fs::create_dir_all(fixture.path().join("crates/core/src")).expect("create prefix source");
        let path = fixture.path().join(CONFIG_PATH);
        let mut config = fs::read_to_string(&path).expect("read config");
        config.push_str("\n[prefixes]\ncore = \"crates/core\"\n");
        fs::write(path, config).expect("extend config");

        let config = RepositoryConfig::load(fixture.path()).expect("load workspace config");
        let document = &config.documents()[0];
        assert_eq!(document.source_root, "crates/app");
        assert_eq!(document.prefixes["core"], "crates/core");
    }

    #[verifies("REQ-PORT-003")]
    #[test]
    fn rejects_paths_that_escape_repository() {
        let fixture = tempdir().expect("create fixture");
        write_fixture(fixture.path(), ".", "docs/requirements.md");
        let path = fixture.path().join(CONFIG_PATH);
        let config = fs::read_to_string(&path)
            .expect("read config")
            .replace("target/shallguard", "../outside");
        fs::write(path, config).expect("write invalid config");

        let error = RepositoryConfig::load(fixture.path()).expect_err("config must be rejected");
        assert!(error.to_string().contains("may not escape"));
    }
}
