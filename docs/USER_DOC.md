# ShallGuard user guide

## Purpose

`cargo shallguard` connects numbered Markdown requirements to Rust enforcement
sites and verification tests. It can check traceability, format requirement
blocks, analyze Git change impact, enumerate exact Cargo tests, collect LLVM
execution evidence, build review capsules, and run optional semantic review.

## Local installation

```bash
git clone <repository-url> shallguard
cd shallguard
cargo install --path cargo-shallguard
```

Until a remote repository and registry release exist, use the local checkout
directly. Re-run the installation after changing the executable.

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

## Portability

Workspace-root discovery works from single-package projects and virtual Cargo
workspaces. Requirement documents, source ownership, path prefixes, area
policies, baseline, artifact locations, and review defaults are all owned by
the consuming repository's `shallguard.toml`.
