# Shallguard user guide

## Purpose

`cargo req-cov` connects numbered Markdown requirements to Rust enforcement
sites and verification tests. It can check traceability, format requirement
blocks, analyze Git change impact, enumerate exact Cargo tests, collect LLVM
execution evidence, build review capsules, and run optional semantic review.

## Local installation

```bash
git clone <repository-url> shallguard
cd shallguard
cargo install --path .
```

Until a remote repository and registry release exist, use the local checkout
directly. Re-run the installation after changing the executable.

## Workspace usage

From the Cargo workspace root being analyzed:

```bash
cargo req-cov fmt --check
cargo req-cov check
cargo req-cov impact --target origin/master \
  --json requirement-impact.json \
  --markdown requirement-impact.md
```

`coverage` requires `cargo-llvm-cov`. `review` additionally requires the
selected provider CLI and may send bounded source capsules to that provider;
provider authentication and data handling remain outside this tool.

## Current portability status

Workspace-root discovery works from single-package projects and virtual Cargo
workspaces. Full document/package ownership and repository policy currently use
illustrative defaults and will move to repository-local configuration in a
later milestone.
