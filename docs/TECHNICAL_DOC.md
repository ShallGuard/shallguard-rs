# ShallGuard technical documentation

This page describes the structure of the ShallGuard repository. It is for a
developer who changes ShallGuard itself. The [glossary](GLOSSARY.md) defines
each technical term.

## Architecture

The repository is a Rust workspace with three packages:

- the `shallguard` library at the root,
- the `cargo-shallguard` executable package,
- the internal `shallguard-macros` procedural-macro package.

Each package has one role:

- The library parses the requirement Markdown and the Rust syntax. It
  produces typed reports and versioned artifacts.
- The executable finds the Cargo workspace from which you run it. It turns
  the library results into terminal output and files.
- The library exports the anchor macros under its public `shallguard::`
  namespace. The internal macro crate validates the syntax of each
  requirement ID at compile time. It emits the annotated Rust item without a
  change.
- Git, Cargo, LLVM, and the optional model providers of the experimental
  review command run as local subprocesses. There is no long-running process and no network service.

The [requirements specification](USER_STORIES_AND_REQUIREMENTS.md#architecture-preflight)
holds the detailed architecture preflight and the dependency contract.

## Build and validation

Run these commands to validate the repository:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo +1.89.0 check --workspace --all-targets --locked
cargo shallguard-dev fmt --check
cargo shallguard-dev check
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo package --locked -p shallguard-macros
cargo package --locked --list -p shallguard
cargo package --locked --list -p cargo-shallguard
```

The package verification of `shallguard` and `cargo-shallguard` needs their
dependencies with the matching version in the target registry. Release the
packages in dependency order: the macros, then the library, then the
executable. The [release procedure](RELEASING.md) describes the complete
manual steps.

A smoke test of the installed binary must run `cargo shallguard` from a
separate fixture workspace. This makes sure that a source path from compile
time cannot become an input for repository discovery.

## State and data

The tool owns no database. It writes only these files:

- the artifacts that you select,
- the work files of the coverage command,
- the checkpoints of the review command,
- the cache entries.

Each artifact binds its result to a revision, a configuration, a schema
version, and a content digest.

## Release status

The packages use the MIT license. Their manifests permit publication only to
crates.io. Rust 1.89 is the tested minimum supported Rust version.
