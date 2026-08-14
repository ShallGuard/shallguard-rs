# Shallguard

Shallguard is requirement-assurance tooling for Cargo projects. The repository
contains the reusable `req-trace` library, the
`cargo-req-cov` Cargo subcommand, and the `shallguard-macros` anchor crate.

This is a local extraction workspace. Final crate names, repository hosting,
and registry publication are intentionally deferred. The project is licensed
under the [MIT License](LICENSE).

## Build and test

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Install the Cargo subcommand

From this repository:

```bash
cargo install --path .
```

Run the installed command from the Rust repository being analyzed:

```bash
cargo req-cov check
cargo req-cov fmt --check
```

The executable discovers the invoked Cargo workspace through `cargo metadata`.
The current document/package ownership model and default policy use
illustrative workspace values; making those inputs repository-configurable is
the next portability milestone.

Commands that collect executable coverage additionally require
`cargo-llvm-cov`. Semantic review requires a supported provider CLI such as
Codex or Claude; deterministic checking does not require a model provider.

## Local path integration

Until the crates are published, another Cargo workspace can depend on the
anchor macro crate through a local path such as
`../shallguard/shallguard-macros`. Install this repository's root package to
provide `cargo req-cov`.

See [User documentation](docs/USER_DOC.md),
[technical documentation](docs/TECHNICAL_DOC.md), and the
[requirements specification](docs/USER_STORIES_AND_REQUIREMENTS.md).
