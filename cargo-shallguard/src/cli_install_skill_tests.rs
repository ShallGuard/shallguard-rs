use super::*;
use tempfile::tempdir;

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn embedded(name: &str) -> &'static str {
    SKILL_FILES
        .iter()
        .find(|(file, _)| *file == name)
        .map(|(_, content)| *content)
        .unwrap_or_else(|| panic!("BUG: the skill embeds {name}"))
}

#[shallguard::verifies("REQ-CLI-013")]
#[test]
fn embedded_skill_is_the_repository_skill() {
    let skill_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("BUG: CLI package must have a workspace parent")
        .join("docs/skill");
    assert_eq!(
        SKILL_FILES.map(|(name, _)| name),
        ["SKILL.md", "rust.md"],
        "the skill embeds the generic file and the Rust file"
    );
    for (name, content) in SKILL_FILES {
        let path = skill_dir.join(name);
        let expected = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        assert_eq!(
            content,
            expected,
            "the embedded {name} must equal {}",
            path.display()
        );
    }

    let skill = embedded("SKILL.md");
    assert!(
        skill.starts_with("---\nname: shallguard\n"),
        "SKILL.md starts with its front matter"
    );
    assert_eq!(
        front_matter_value(skill, "version"),
        Some(env!("CARGO_PKG_VERSION")),
        "docs/skill/SKILL.md must name the package version under metadata.version"
    );
    assert!(
        front_matter_value(skill, "spec").is_some(),
        "docs/skill/SKILL.md must name the shared skill version under metadata.spec"
    );
    assert!(
        skill.contains("`rust.md`"),
        "the generic skill sends the agent to the language file"
    );

    assert_eq!(front_matter_value("no front matter", "version"), None);
    assert_eq!(
        front_matter_value("---\nname: x\n---\nversion: 9\n", "version"),
        None
    );
    assert_eq!(
        front_matter_value("---\nversion-x: 9\n---\n", "version"),
        None
    );
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
        vec![home.path().join(".claude/skills/shallguard")]
    );

    fs::create_dir(home.path().join(".codex")).expect("mark Codex as installed");
    assert_eq!(
        destinations(&default_args, Some(home.path()), None).expect("both agents are selected"),
        vec![
            home.path().join(".claude/skills/shallguard"),
            home.path().join(".agents/skills/shallguard"),
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
            project.path().join(".claude/skills/shallguard"),
            project.path().join(".agents/skills/shallguard"),
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
        vec![empty_home.path().join(".agents/skills/shallguard")]
    );

    let dir_args = InstallSkillArgs {
        dir: Some(PathBuf::from("custom/skills")),
        ..InstallSkillArgs::default()
    };
    assert_eq!(
        destinations(&dir_args, None, None).expect("an explicit directory needs no home"),
        vec![PathBuf::from("custom/skills")]
    );
}

#[shallguard::verifies("REQ-CLI-015")]
#[test]
fn reports_installed_updated_and_unchanged() {
    let root = tempdir().expect("create destination root");
    let path = root.path().join("nested/skills/shallguard/rust.md");
    let content = embedded("rust.md");

    assert_eq!(
        write_file(&path, content).expect("first write"),
        "installed"
    );
    assert_eq!(fs::read_to_string(&path).expect("read skill"), content);

    assert_eq!(
        write_file(&path, content).expect("second write"),
        "unchanged"
    );

    fs::write(&path, "an older manual").expect("replace the skill");
    assert_eq!(write_file(&path, content).expect("third write"), "updated");
    assert_eq!(fs::read_to_string(&path).expect("read skill"), content);
}

#[shallguard::verifies("REQ-CLI-016")]
#[test]
fn check_reports_current_outdated_and_missing() {
    let root = tempdir().expect("create destination root");
    let path = root.path().join("skills/shallguard/SKILL.md");
    let content = embedded("SKILL.md");

    assert_eq!(
        check_file(&path, content).expect("check a missing file"),
        SkillStatus::Missing
    );
    assert!(!path.exists(), "a check never writes");

    write_file(&path, content).expect("install the skill");
    assert_eq!(
        check_file(&path, content).expect("check a current file"),
        SkillStatus::Current
    );

    fs::write(
        &path,
        "---\nname: shallguard\nmetadata:\n  version: 0.0.1\n---\nold\n",
    )
    .expect("replace with an older skill");
    assert_eq!(
        check_file(&path, content).expect("check an older file"),
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

    fs::write(&path, "a file without front matter").expect("replace with a changed file");
    assert_eq!(
        check_file(&path, content).expect("check a file without a version"),
        SkillStatus::Outdated { installed: None }
    );
    assert_eq!(
        SkillStatus::Outdated { installed: None }.line(Path::new("x/rust.md")),
        "outdated x/rust.md",
        "a file without a version names only the path"
    );
}
