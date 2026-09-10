//! The `install-skill` command. It writes the agent skill manual that the
//! executable embeds into the skill directory of a coding agent.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};

/// The agent skill manual of this release.
///
/// Requirements:
/// The path `skill/SKILL.md` is a symbolic link to `docs/skill/SKILL.md`
/// of the repository. Cargo packages the link as a plain file, so the
/// executable ships the manual of its own release.
#[shallguard::enforces("REQ-CLI-013")]
pub(super) const SKILL: &str = include_str!("skill/SKILL.md");

/// The version of the executable, which the skill front matter repeats.
const SKILL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The value of `version:` in the front matter of a skill, if present.
#[shallguard::enforces("REQ-CLI-013")]
pub(super) fn skill_version(skill: &str) -> Option<&str> {
    let body = skill.strip_prefix("---\n")?;
    let front_matter = &body[..body.find("\n---\n")?];
    front_matter
        .lines()
        .find_map(|line| line.trim().strip_prefix("version:"))
        .map(str::trim)
        .filter(|version| !version.is_empty())
}

const COMMAND: &str = "cargo shallguard install-skill";

/// A coding agent that loads a skill from a known directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Agent {
    Claude,
    Codex,
}

impl Agent {
    const ALL: [Agent; 2] = [Agent::Claude, Agent::Codex];

    fn parse(name: &str) -> Option<Agent> {
        match name {
            "claude" => Some(Agent::Claude),
            "codex" => Some(Agent::Codex),
            _ => None,
        }
    }

    /// The directory below the home directory that exists when the agent
    /// is installed.
    fn home_marker(self) -> &'static str {
        match self {
            Agent::Claude => ".claude",
            Agent::Codex => ".codex",
        }
    }

    /// The skill file, relative to the home directory or to the project
    /// root.
    fn skill_path(self) -> &'static str {
        match self {
            Agent::Claude => ".claude/skills/shallguard/SKILL.md",
            Agent::Codex => ".agents/skills/shallguard/SKILL.md",
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct InstallSkillArgs {
    pub(super) agents: BTreeSet<Agent>,
    pub(super) project: bool,
    pub(super) dir: Option<PathBuf>,
    pub(super) check: bool,
}

#[shallguard::enforces("REQ-CLI-014")]
pub(super) fn parse_install_skill_args(args: &[String]) -> Result<InstallSkillArgs> {
    let mut parsed = InstallSkillArgs::default();
    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index].as_str();
        index += 1;
        match flag {
            "--agent" => {
                let name = args
                    .get(index)
                    .with_context(|| format!("{flag} requires a value"))?;
                index += 1;
                let agent = Agent::parse(name)
                    .with_context(|| format!("unknown agent {name:?}; expected claude or codex"))?;
                parsed.agents.insert(agent);
            }
            "--project" => parsed.project = true,
            "--check" => parsed.check = true,
            "--dir" => {
                let dir = args
                    .get(index)
                    .with_context(|| format!("{flag} requires a value"))?;
                index += 1;
                parsed.dir = Some(PathBuf::from(dir));
            }
            _ => bail!("unknown argument {flag:?}; expected --agent, --project, --dir, or --check"),
        }
    }
    if parsed.dir.is_some() && (parsed.project || !parsed.agents.is_empty()) {
        bail!("--dir cannot be combined with --agent or --project");
    }
    Ok(parsed)
}

/// The files that the command writes.
///
/// Requirements:
/// The home directory selects the agents when `--agent` is absent. The
/// project root replaces the home directory as the base of the paths when
/// `--project` is given. An explicit `--dir` needs neither.
#[shallguard::enforces("REQ-CLI-014")]
pub(super) fn destinations(
    args: &InstallSkillArgs,
    home: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<Vec<PathBuf>> {
    if let Some(dir) = &args.dir {
        return Ok(vec![dir.join("SKILL.md")]);
    }
    let home = home.context("cannot find the home directory of the user")?;
    let agents: Vec<Agent> = if args.agents.is_empty() {
        Agent::ALL
            .into_iter()
            .filter(|agent| home.join(agent.home_marker()).is_dir())
            .collect()
    } else {
        args.agents.iter().copied().collect()
    };
    if agents.is_empty() {
        bail!(
            "no coding agent found below {}; pass --agent claude or --agent codex",
            home.display()
        );
    }
    let base = project_root.unwrap_or(home);
    Ok(agents
        .into_iter()
        .map(|agent| base.join(agent.skill_path()))
        .collect())
}

#[shallguard::enforces("REQ-CLI-015")]
pub(super) fn run(args: &InstallSkillArgs) -> ExitCode {
    match install(args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("{COMMAND} failed: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// Writes or checks every destination. Returns `false` when a check finds
/// a destination that is not current.
#[shallguard::enforces("REQ-CLI-016")]
fn install(args: &InstallSkillArgs) -> Result<bool> {
    let project_root = if args.project {
        Some(shallguard::workspace_root()?)
    } else {
        None
    };
    let home = std::env::home_dir();
    let targets = destinations(args, home.as_deref(), project_root.as_deref())?;
    let mut stale = 0usize;
    for path in targets {
        if args.check {
            let status = check_skill(&path)?;
            if !matches!(status, SkillStatus::Current) {
                stale += 1;
            }
            println!("{}", status.line(&path));
        } else {
            let outcome = write_skill(&path)?;
            println!("{outcome} {}", path.display());
        }
    }
    if stale > 0 {
        eprintln!("{COMMAND} --check: {stale} file(s) need `{COMMAND}`");
    }
    Ok(stale == 0)
}

/// The state of one installed skill file, compared with the embedded one.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SkillStatus {
    Current,
    Missing,
    Outdated { installed: Option<String> },
}

impl SkillStatus {
    fn line(&self, path: &Path) -> String {
        match self {
            SkillStatus::Current => format!("current {}", path.display()),
            SkillStatus::Missing => format!("missing {}", path.display()),
            SkillStatus::Outdated { installed } => format!(
                "outdated {}: installed {}, executable {SKILL_VERSION}",
                path.display(),
                installed.as_deref().unwrap_or("unknown")
            ),
        }
    }
}

/// Compares the file with the embedded manual without a write.
#[shallguard::enforces("REQ-CLI-016")]
pub(super) fn check_skill(path: &Path) -> Result<SkillStatus> {
    match fs::read(path) {
        Ok(existing) if existing == SKILL.as_bytes() => Ok(SkillStatus::Current),
        Ok(existing) => Ok(SkillStatus::Outdated {
            installed: skill_version(&String::from_utf8_lossy(&existing)).map(str::to_string),
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(SkillStatus::Missing),
        Err(err) => Err(err).with_context(|| format!("reading {}", path.display())),
    }
}

/// Writes the manual and reports `installed`, `updated`, or `unchanged`.
fn write_skill(path: &Path) -> Result<&'static str> {
    let outcome = match fs::read(path) {
        Ok(existing) if existing == SKILL.as_bytes() => return Ok("unchanged"),
        Ok(_) => "updated",
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => "installed",
        Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, SKILL).with_context(|| format!("writing {}", path.display()))?;
    Ok(outcome)
}

#[cfg(test)]
#[path = "cli_install_skill_tests.rs"]
mod tests;
