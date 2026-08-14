use std::fs;
use std::path::Path;
use std::process::Command;

use shallguard_macros::verifies;
use tempfile::tempdir;

#[verifies("REQ-PORT-002", "REQ-PORT-003", "REQ-PORT-004")]
#[test]
fn installed_subcommand_checks_a_single_package_fixture() {
    let fixture = tempdir().expect("create fixture repository");
    write_fixture(fixture.path());

    let binary = Path::new(env!("CARGO_BIN_EXE_cargo-shallguard"));
    let binary_dir = binary.parent().expect("binary has a parent directory");
    let current_path = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![binary_dir.to_path_buf()];
    paths.extend(std::env::split_paths(&current_path));
    let search_path = std::env::join_paths(paths).expect("build subprocess search path");
    let output = Command::new("cargo")
        .args(["shallguard", "check"])
        .current_dir(fixture.path())
        .env("PATH", search_path)
        .output()
        .expect("invoke Cargo external subcommand");

    assert!(
        output.status.success(),
        "cargo shallguard failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("cargo shallguard: OK"),
        "successful output should identify ShallGuard"
    );
}

#[verifies("REQ-PORT-008")]
#[test]
fn repository_configuration_has_zero_traceability_debt() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("BUG: CLI package must have a workspace parent");
    let config = shallguard::config::RepositoryConfig::load(root)
        .expect("repository ShallGuard configuration should load");
    let report = shallguard::check::run(root, &config.documents(), &config)
        .expect("repository traceability check should run");

    assert!(report.errors.is_empty(), "errors: {:#?}", report.errors);
    assert!(
        report.warnings.is_empty(),
        "warnings: {:#?}",
        report.warnings
    );
}

fn write_fixture(root: &Path) {
    fs::create_dir(root.join("src")).expect("create source directory");
    fs::create_dir(root.join("docs")).expect("create documentation directory");
    fs::create_dir(root.join(".shallguard")).expect("create policy directory");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"shallguard-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write Cargo manifest");
    fs::write(
        root.join("src/lib.rs"),
        r#"#[shallguard_macros::enforces("REQ-DEMO-001")]
pub fn answer() -> u8 { 42 }

#[cfg(test)]
mod tests {
    #[shallguard_macros::verifies("REQ-DEMO-001")]
    #[test]
    fn answer_is_stable() { assert_eq!(super::answer(), 42); }
}
"#,
    )
    .expect("write fixture source");
    fs::write(
        root.join("docs/requirements.md"),
        "# Requirements\n\n- **REQ-DEMO-001** — The library SHALL return its stable answer.\n  \
         *Enforced:* `src/lib.rs` (`answer`) · *Verified:* ✅ `src/lib.rs` \
         (`answer_is_stable`)\n",
    )
    .expect("write requirements document");
    fs::write(
        root.join("shallguard.toml"),
        r#"schema = 1
minimum_requirements = 1
baseline = ".shallguard/baseline.toml"
verify_outlier_threshold = 6
allow_missing_paths = []

[[documents]]
path = "docs/requirements.md"
source_root = "."

[areas.DEMO]
label = "Demonstration"
hard_enforcement = true
hard_verification = true

[artifacts]
root = "target/shallguard"
"#,
    )
    .expect("write ShallGuard configuration");
    fs::write(
        root.join(".shallguard/baseline.toml"),
        "schema = 1\ngap = []\n",
    )
    .expect("write empty traceability baseline");
}
