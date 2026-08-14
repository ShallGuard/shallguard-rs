# ShallGuard

ShallGuard is requirement-assurance tooling for Cargo projects. The workspace
contains the reusable `shallguard` library, the `cargo-shallguard` Cargo
subcommand, and the `shallguard-macros` anchor crate.

The project is licensed under the [MIT License](LICENSE).

## Build and test

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Install the Cargo subcommand

From this repository:

```bash
cargo install --path cargo-shallguard
```

Run the installed command from the Rust repository being analyzed:

```bash
cargo shallguard check
cargo shallguard fmt --check
```

The executable discovers the invoked Cargo repository through `cargo metadata`
and reads repository-owned policy from `shallguard.toml`. It supports ordinary
single-package repositories and virtual Cargo workspaces.

Commands that collect executable coverage additionally require
`cargo-llvm-cov`. Semantic review requires a supported provider CLI such as
Codex or Claude; deterministic checking does not require a model provider.

## Git integration before crates.io

Pin the macros and CLI to the same immutable commit:

```toml
[dependencies]
shallguard-macros = { git = "https://github.com/sigi64/shallguard.git", rev = "<published-sha>" }
```

```bash
cargo install \
  --git https://github.com/sigi64/shallguard.git \
  --rev <published-sha> \
  --package cargo-shallguard
```

Copy and adapt the [repository configuration example](docs/CONFIGURATION.md)
before running the command.

See [User documentation](docs/USER_DOC.md),
[technical documentation](docs/TECHNICAL_DOC.md), and the
[requirements specification](docs/USER_STORIES_AND_REQUIREMENTS.md).
