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
- run an optional semantic review.

## Installation

Install the published Cargo subcommand:

```bash
cargo install cargo-shallguard --version 0.1.1 --locked
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

## What the deterministic gate does and does not prove

The check is the command `cargo shallguard check`. The check proves five
things:

- **Link integrity.** Every requirement resolves to its anchors. Every anchor
  resolves to its requirement.
- **Citation reality.** Every ✅ claim names a real test. The test is not
  ignored, and it carries a verification anchor.
- **Evidence-class consistency.** The evidence class on the *Verified:* line
  agrees with the anchors that exist.
- **Monotone debt.** The committed baseline can only become smaller.
- **An evidence floor.** The check rejects a cited test that cannot fail.
  Such a test has no failure path, or it has only assertions that always
  pass. The compiler rejects the certain cases. The check reports the other
  cases as findings. The report counts and lists every `oracle` opt-out.

The check does not prove two things:

- **Test sharpness.** An `#[enforces]` attribute survives an edit that
  removes the behavior below it. A test above the evidence floor can still
  pass without a real test of the requirement. Execution coverage, human
  review, and semantic review address this layer.
- **Requirement quality.** A person decides if a requirement is a good
  requirement. [Issue #12](https://github.com/sigi64/shallguard/issues/12)
  discusses this point.

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
