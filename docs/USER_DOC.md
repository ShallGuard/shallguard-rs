# ShallGuard user guide

## Purpose

`cargo shallguard` connects numbered Markdown requirements to Rust enforcement
sites and verification tests. It can check traceability, format requirement
blocks, analyze Git change impact, enumerate exact Cargo tests, collect LLVM
execution evidence, build review capsules, and run optional semantic review.

## Installation

Install the published Cargo subcommand:

```bash
cargo install cargo-shallguard --version 0.1.1 --locked
```

To install from a local checkout instead:

```bash
git clone <repository-url> shallguard
cd shallguard
cargo install --path cargo-shallguard
```

Confirm the installed release from any directory:

```bash
cargo shallguard --version
```

The top-level [README](../README.md) also shows how to pin an immutable Git
revision. Re-run a source installation after changing the executable.

## Repository setup and usage

Create `shallguard.toml` using the
[configuration reference](CONFIGURATION.md), then run from anywhere within the
configured Cargo repository:

```bash
cargo shallguard fmt --check
cargo shallguard check
cargo shallguard impact --target origin/main \
  --json requirement-impact.json \
  --markdown requirement-impact.md
```

`coverage` requires `cargo-llvm-cov`. `review` additionally requires the
selected provider CLI and may send bounded source capsules to that provider;
provider authentication and data handling remain outside this tool.

## What the deterministic gate does and does not prove

`cargo shallguard check` proves five things:

- **Link integrity** — every requirement resolves to its anchors and every
  anchor to its requirement.
- **Citation reality** — every ✅ claim names a real, non-ignored, anchored
  test.
- **Evidence-class consistency.**
- **Monotone debt** — the committed baseline can only shrink.
- **An evidence floor** — a cited test that cannot fail (no failure path,
  or only provably always-passing assertions) is rejected, at compile time
  for the certain cases and as check findings otherwise; every `oracle`
  opt-out is counted and listed in the report.

It does not prove that an anchored test is sharp — an `#[enforces]`
attribute survives edits that gut the behavior it annotates, and a test
above the vacuity floor can still pass without exercising the contract;
executable coverage, human review, and semantic review address that layer.
It also does not prove that a SHALL statement is well-chosen —
requirement quality stays with the human reviewer, as discussed in
[issue #12](https://github.com/sigi64/shallguard/issues/12).

## Requirement ID concurrency

Requirement IDs are stable forever only once merged. When two branches
draft the same next-free `REQ-<AREA>-<NNN>` and collide at merge or
rebase, the rebasing branch renumbers its requirement to the next free ID;
IDs become stable at merge to the default branch, not at draft time.
Renumbering before merge is safe because `cargo shallguard check` fails on
any rename missed between the document and the anchors — a half-renamed
requirement cannot pass the gate.

## Portability

Workspace-root discovery works from single-package projects and virtual Cargo
workspaces. Requirement documents, source ownership, path prefixes, area
policies, baseline, artifact locations, and review defaults are all owned by
the consuming repository's `shallguard.toml`.
