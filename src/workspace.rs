//! Cargo workspace discovery for installed and source-tree invocations.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Deserialize)]
struct CargoMetadata {
    workspace_root: PathBuf,
}

/// Discovers the Cargo workspace that contains the invocation directory.
///
/// # Errors
///
/// Returns an error when the current directory cannot be read, Cargo metadata
/// fails, or Cargo returns malformed metadata.
#[shallguard::enforces("REQ-PORT-001")]
pub fn workspace_root() -> Result<PathBuf> {
    let invocation_dir = std::env::current_dir().context("reading the invocation directory")?;
    workspace_root_from(&invocation_dir)
}

fn workspace_root_from(invocation_dir: &Path) -> Result<PathBuf> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(invocation_dir)
        .output()
        .with_context(|| {
            format!(
                "running Cargo metadata from invocation directory {}",
                invocation_dir.display()
            )
        })?;
    if !output.status.success() {
        bail!(
            "Cargo workspace discovery failed from {}: {}",
            invocation_dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let metadata: CargoMetadata =
        serde_json::from_slice(&output.stdout).context("parsing Cargo workspace metadata")?;
    Ok(metadata.workspace_root)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::workspace_root_from;

    #[shallguard::verifies("REQ-PORT-001")]
    #[test]
    fn discovers_single_package_and_virtual_workspace_roots() {
        let single = tempdir().expect("BUG: single-package temporary directory should exist");
        fs::create_dir(single.path().join("src"))
            .expect("BUG: single-package source directory should be created");
        fs::write(
            single.path().join("Cargo.toml"),
            "[package]\nname = \"single\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("BUG: single-package manifest should be written");
        fs::write(single.path().join("src/lib.rs"), "")
            .expect("BUG: single-package library should be written");

        let single_root = workspace_root_from(&single.path().join("src"))
            .expect("single-package workspace discovery should succeed");
        assert_eq!(single_root, single.path());

        let workspace = tempdir().expect("BUG: virtual-workspace temporary directory should exist");
        fs::create_dir_all(workspace.path().join("crates/member/src"))
            .expect("BUG: virtual-workspace member source directory should be created");
        fs::write(
            workspace.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/member\"]\nresolver = \"3\"\n",
        )
        .expect("BUG: virtual-workspace manifest should be written");
        fs::write(
            workspace.path().join("crates/member/Cargo.toml"),
            "[package]\nname = \"member\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("BUG: virtual-workspace member manifest should be written");
        fs::write(workspace.path().join("crates/member/src/lib.rs"), "")
            .expect("BUG: virtual-workspace member library should be written");

        let workspace_root = workspace_root_from(&workspace.path().join("crates/member/src"))
            .expect("virtual-workspace discovery should succeed");
        assert_eq!(workspace_root, workspace.path());
    }
}
