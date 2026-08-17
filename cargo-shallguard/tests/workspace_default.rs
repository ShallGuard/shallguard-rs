use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

#[test]
fn cargo_run_selects_cli_binary_from_workspace_root() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("BUG: CLI package must have a workspace parent");
    let target_dir = tempdir().expect("create isolated Cargo target directory");

    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--locked", "--", "version"])
        .current_dir(workspace_root)
        .env("CARGO_TARGET_DIR", target_dir.path())
        .output()
        .expect("run the workspace's default binary");

    assert!(
        output.status.success(),
        "cargo run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("cargo-shallguard {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty(), "quiet cargo run wrote to stderr");
}
