# ShallGuard user guide

This guide is for a developer who installs and uses the `cargo shallguard`
command in a Rust repository. The [glossary](GLOSSARY.md) defines each
technical term.

## Purpose

The command `cargo shallguard` connects numbered requirements in Markdown
documents to the Rust code that makes them true and to the tests that verify
them. The command can:

- examine the traceability of every requirement,
- format the requirement blocks,
- analyze the impact of a Git change,
- list the exact Cargo tests behind the verification anchors,
- collect LLVM execution evidence,
- build review capsules,
- run an optional semantic review. This feature is experimental.

## Installation

Install the published Cargo subcommand:

```bash
cargo install cargo-shallguard --version 0.1.2 --locked
```

Or install from a local copy of the repository:

```bash
git clone <repository-url> shallguard
cd shallguard
cargo install --path cargo-shallguard
```

Confirm the installed release from any directory:

```bash
cargo shallguard --version
```

The top-level [README](../README.md) also shows how to install from one fixed
Git commit. If you change the executable, run the source installation again.

## Repository setup and usage

Create the file `shallguard.toml` with the help of the
[configuration reference](CONFIGURATION.md). Then run the commands from any
directory inside the configured Cargo repository:

```bash
cargo shallguard fmt --check
cargo shallguard check
cargo shallguard impact --target origin/main \
  --json requirement-impact.json \
  --markdown requirement-impact.md
```

Two commands have more prerequisites:

- The `coverage` command needs the tool `cargo-llvm-cov`.
- The `review` command needs the selected provider program. The command can
  send bounded source capsules to that provider. The provider login and the
  data handling of the provider are outside this tool.

## Experimental features

An experimental feature is a feature that needs a large language model
(LLM). These features are experimental:

- the command `cargo shallguard review`, with its providers and its local
  inference options,
- the command `cargo shallguard review show`, which inspects the output of
  a review,
- the advisory pull-request workflow in `.github/workflows/shallguard-review.yml`.

An experimental feature can change or go away in any release, also in a
patch release. Its commands, options, and artifacts are not stable. Its
verdicts are advisory. The check, the format check, and the other
deterministic commands never depend on an experimental feature. The
`review` command prints a notice about this when it starts.

## What the deterministic gate does and does not prove

The check is the command `cargo shallguard check`. The check proves four
things:

- **Link integrity.** Every requirement resolves to its anchors. Every anchor
  resolves to its requirement.
- **Citation reality.** Every `[test]` claim names a real test. The test is not
  ignored, and it carries a verification anchor.
- **Evidence-class consistency.** The evidence class on the *Verified:* line
  agrees with the anchors that exist.
- **Monotone debt.** The committed baseline can only become smaller.

The check does not prove two things:

- **Test sharpness.** An `#[enforces]` attribute survives an edit that
  removes the behavior below it. A cited test can pass without a real test
  of the requirement, and a test that cannot fail is not detected. Execution
  coverage, human review, and semantic review address this layer.
- **Requirement quality.** A person decides if a requirement is a good
  requirement. [Issue #12](https://github.com/shallguard/shallguard-rs/issues/12)
  discusses this point.

## Evidence marks

Each requirement names its evidence class on its *Verified:* line with one
ASCII keyword:

| Keyword | Meaning | Emoji alias |
|---|---|---|
| `[test]` | An anchored automated test backs the requirement. | ✅ |
| `[e2e]` | An end-to-end or production validation backs the requirement. | 🔬 |
| `[review]` | Only a code review backs the requirement. | 👁 |
| `[pending]` | The evidence is pending. | ⏳ |

The keyword is the canonical form. It survives editors, copy and paste, and
diff tools, and `grep` finds it. The emoji is an optional alias with the same
meaning. The parser and the lint accept both forms. The command
`cargo shallguard fmt` adds the keyword before an emoji that has no keyword,
and it keeps the emoji next to the keyword, for example `[test] ✅`. A
document with only keywords stays as it is. The commands
`cargo shallguard fmt --check` and `cargo shallguard lint` accept a document
with an emoji that lacks its keyword. They do not report it. An existing
document therefore passes the checks without a change. To add the keywords
to an existing document, run `cargo shallguard fmt` once and commit the
result.

## Requirement ID concurrency

A requirement ID becomes stable when the change merges into the default
branch. It is not stable at draft time. Two branches can draft the same next
free `REQ-<AREA>-<NNN>` and collide at merge or rebase time. In that case:

- The branch that rebases renumbers its requirement to the next free ID.
- A renumber before the merge is safe. The check fails on a rename that
  reaches the document but not the anchors, or the reverse. A half-renamed
  requirement cannot pass the check.

## Portability

The tool finds the workspace root in a single-package project and in a
virtual Cargo workspace. The file `shallguard.toml` in your repository owns
all the policy:

- the requirement documents,
- the source ownership and the path prefixes,
- the area policies,
- the baseline location and the artifact location,
- the review defaults.
