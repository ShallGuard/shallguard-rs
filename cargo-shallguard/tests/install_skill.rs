use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::{TempDir, tempdir};

/// Runs the installed binary outside a repository with a fresh home
/// directory. The binary is called directly, because a changed `HOME`
/// breaks the toolchain lookup of the `cargo` proxy.
fn install_skill(home: &TempDir, arguments: &[&str]) -> Output {
    let outside_repository = tempdir().expect("create directory outside a repository");
    Command::new(env!("CARGO_BIN_EXE_cargo-shallguard"))
        .arg("install-skill")
        .args(arguments)
        .current_dir(outside_repository.path())
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .output()
        .expect("invoke cargo-shallguard install-skill")
}

#[shallguard::verifies("REQ-CLI-014", "REQ-CLI-015")]
#[test]
fn installed_skill_command_works_without_repository() {
    let home = tempdir().expect("create home directory");
    let skill = home.path().join(".claude/skills/shallguard/SKILL.md");
    let repository_skill = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("BUG: CLI package must have a workspace parent")
        .join("docs/skill/SKILL.md");

    let missing = install_skill(&home, &[]);
    let stderr = String::from_utf8_lossy(&missing.stderr);
    assert!(
        !missing.status.success(),
        "a home directory without an agent fails:\n{stderr}"
    );
    assert!(
        stderr.contains("--agent claude") && stderr.contains("--agent codex"),
        "the failure names the accepted agents:\n{stderr}"
    );
    assert!(!skill.exists(), "nothing is written on failure");

    fs::create_dir(home.path().join(".claude")).expect("mark Claude Code as installed");
    let first = install_skill(&home, &[]);
    let stdout = String::from_utf8_lossy(&first.stdout);
    assert!(
        first.status.success(),
        "install-skill succeeds:\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(stdout, format!("installed {}\n", skill.display()));
    assert!(first.stderr.is_empty(), "install-skill is quiet");
    assert_eq!(
        fs::read_to_string(&skill).expect("read installed skill"),
        fs::read_to_string(&repository_skill).expect("read repository skill"),
        "the installed skill equals docs/skill/SKILL.md"
    );

    let second = install_skill(&home, &[]);
    assert!(second.status.success(), "a repeated install succeeds");
    assert_eq!(
        String::from_utf8_lossy(&second.stdout),
        format!("unchanged {}\n", skill.display())
    );

    let codex_skill = home.path().join(".agents/skills/shallguard/SKILL.md");
    let explicit = install_skill(&home, &["--agent", "codex"]);
    assert!(explicit.status.success(), "an explicit agent succeeds");
    assert_eq!(
        String::from_utf8_lossy(&explicit.stdout),
        format!("installed {}\n", codex_skill.display())
    );
    assert!(codex_skill.is_file(), "the Codex skill is written");
}
