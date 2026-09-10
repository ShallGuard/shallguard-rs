use super::*;
use tempfile::tempdir;

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[shallguard::verifies("REQ-CLI-013")]
#[test]
fn embedded_skill_is_the_repository_skill() {
    let repository_skill = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("BUG: CLI package must have a workspace parent")
        .join("docs/skill/SKILL.md");
    let expected = fs::read_to_string(&repository_skill).expect("read docs/skill/SKILL.md");
    assert!(
        expected.starts_with("---\nname: shallguard\n"),
        "the repository skill starts with its front matter"
    );
    assert_eq!(
        SKILL,
        expected,
        "the embedded skill must equal {}",
        repository_skill.display()
    );
    assert_eq!(
        skill_version(SKILL),
        Some(env!("CARGO_PKG_VERSION")),
        "docs/skill/SKILL.md must name the package version under metadata.version"
    );
    assert_eq!(skill_version("no front matter"), None);
    assert_eq!(skill_version("---\nname: x\n---\nversion: 9\n"), None);
}

#[shallguard::verifies("REQ-CLI-014")]
#[test]
fn parses_agents_project_and_dir() {
    let args = parse_install_skill_args(&strings(&[
        "--agent",
        "codex",
        "--agent",
        "claude",
        "--project",
    ]))
    .expect("agents and project parse");
    assert_eq!(
        args.agents.iter().copied().collect::<Vec<_>>(),
        vec![Agent::Claude, Agent::Codex]
    );
    assert!(args.project);
    assert!(args.dir.is_none());

    let args = parse_install_skill_args(&strings(&["--dir", "skills/here"])).expect("dir parses");
    assert_eq!(args.dir, Some(PathBuf::from("skills/here")));
    assert!(args.agents.is_empty());
    assert!(!args.project);

    let empty = parse_install_skill_args(&[]).expect("no arguments parse");
    assert_eq!(empty, InstallSkillArgs::default());

    let check =
        parse_install_skill_args(&strings(&["--check", "--project"])).expect("check parses");
    assert!(check.check && check.project);

    for (arguments, message) in [
        (&["--agent", "gemini"][..], "unknown agent"),
        (&["--agent"][..], "requires a value"),
        (&["--dir", "x", "--project"][..], "cannot be combined"),
        (
            &["--dir", "x", "--agent", "codex"][..],
            "cannot be combined",
        ),
        (&["--global"][..], "unknown argument"),
    ] {
        let error = parse_install_skill_args(&strings(arguments))
            .err()
            .unwrap_or_else(|| panic!("{arguments:?} must fail"));
        assert!(
            error.to_string().contains(message),
            "{arguments:?}: {error:#}"
        );
    }
}

#[shallguard::verifies("REQ-CLI-014")]
#[test]
fn selects_installed_agents_from_the_home_directory() {
    let home = tempdir().expect("create home directory");
    let project = tempdir().expect("create project directory");
    let default_args = InstallSkillArgs::default();

    let error = destinations(&default_args, Some(home.path()), None)
        .expect_err("an empty home directory selects no agent");
    assert!(
        error.to_string().contains("--agent claude") && error.to_string().contains("--agent codex"),
        "{error:#}"
    );
    let error =
        destinations(&default_args, None, None).expect_err("a missing home directory fails");
    assert!(error.to_string().contains("home directory"), "{error:#}");

    fs::create_dir(home.path().join(".claude")).expect("mark Claude Code as installed");
    assert_eq!(
        destinations(&default_args, Some(home.path()), None).expect("Claude Code is selected"),
        vec![home.path().join(".claude/skills/shallguard/SKILL.md")]
    );

    fs::create_dir(home.path().join(".codex")).expect("mark Codex as installed");
    assert_eq!(
        destinations(&default_args, Some(home.path()), None).expect("both agents are selected"),
        vec![
            home.path().join(".claude/skills/shallguard/SKILL.md"),
            home.path().join(".agents/skills/shallguard/SKILL.md"),
        ]
    );

    let project_args = InstallSkillArgs {
        project: true,
        ..InstallSkillArgs::default()
    };
    assert_eq!(
        destinations(&project_args, Some(home.path()), Some(project.path()))
            .expect("the project root replaces the home directory"),
        vec![
            project.path().join(".claude/skills/shallguard/SKILL.md"),
            project.path().join(".agents/skills/shallguard/SKILL.md"),
        ]
    );

    let explicit_args = InstallSkillArgs {
        agents: [Agent::Codex].into_iter().collect(),
        ..InstallSkillArgs::default()
    };
    let empty_home = tempdir().expect("create a home directory without agents");
    assert_eq!(
        destinations(&explicit_args, Some(empty_home.path()), None)
            .expect("an explicit agent needs no marker"),
        vec![empty_home.path().join(".agents/skills/shallguard/SKILL.md")]
    );

    let dir_args = InstallSkillArgs {
        dir: Some(PathBuf::from("custom/skills")),
        ..InstallSkillArgs::default()
    };
    assert_eq!(
        destinations(&dir_args, None, None).expect("an explicit directory needs no home"),
        vec![PathBuf::from("custom/skills/SKILL.md")]
    );
}

#[shallguard::verifies("REQ-CLI-015")]
#[test]
fn reports_installed_updated_and_unchanged() {
    let root = tempdir().expect("create destination root");
    let path = root.path().join("nested/skills/shallguard/SKILL.md");

    assert_eq!(write_skill(&path).expect("first write"), "installed");
    assert_eq!(fs::read_to_string(&path).expect("read skill"), SKILL);

    assert_eq!(write_skill(&path).expect("second write"), "unchanged");

    fs::write(&path, "an older manual").expect("replace the skill");
    assert_eq!(write_skill(&path).expect("third write"), "updated");
    assert_eq!(fs::read_to_string(&path).expect("read skill"), SKILL);
}

#[shallguard::verifies("REQ-CLI-016")]
#[test]
fn check_reports_current_outdated_and_missing() {
    let root = tempdir().expect("create destination root");
    let path = root.path().join("skills/shallguard/SKILL.md");

    assert_eq!(
        check_skill(&path).expect("check a missing file"),
        SkillStatus::Missing
    );
    assert!(!path.exists(), "a check never writes");

    write_skill(&path).expect("install the skill");
    assert_eq!(
        check_skill(&path).expect("check a current file"),
        SkillStatus::Current
    );

    fs::write(
        &path,
        "---\nname: shallguard\nmetadata:\n  version: 0.0.1\n---\nold\n",
    )
    .expect("replace with an older skill");
    assert_eq!(
        check_skill(&path).expect("check an older file"),
        SkillStatus::Outdated {
            installed: Some("0.0.1".to_string())
        }
    );
    assert_eq!(
        SkillStatus::Outdated {
            installed: Some("0.0.1".to_string())
        }
        .line(Path::new("x/SKILL.md")),
        format!(
            "outdated x/SKILL.md: installed 0.0.1, executable {}",
            env!("CARGO_PKG_VERSION")
        )
    );

    fs::write(&path, "a manual without front matter").expect("replace with an unknown skill");
    assert_eq!(
        check_skill(&path).expect("check a file without a version"),
        SkillStatus::Outdated { installed: None }
    );
    assert!(
        SkillStatus::Outdated { installed: None }
            .line(Path::new("x/SKILL.md"))
            .contains("installed unknown"),
        "a file without a version is reported as unknown"
    );
}
