# The ShallGuard skill

The files in this directory are a copy of the shared skill in
[`skills/shallguard/`](https://github.com/shallguard/spec/tree/master/skills/shallguard)
of the specification repository. Change the skill there, not here.

- `SKILL.md` is the same for every language. Only its front matter is
  different from the shared file. `metadata.version` names the version of
  `cargo-shallguard`, and `metadata.spec` names the version of the shared
  skill.
- `rust.md` gives the Rust command and the anchor syntax.

The executable embeds both files through the symbolic links in
`cargo-shallguard/src/skill/`. The command `cargo shallguard install-skill`
writes them. The [release procedure](../RELEASING.md) describes the copy.
