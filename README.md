# ShallGuard

ShallGuard is requirement-assurance tooling for Cargo projects. The workspace
contains the reusable `shallguard` library, the `cargo-shallguard` Cargo
subcommand, and its internal procedural-macro crate.

The project is licensed under the [MIT License](LICENSE).

## Build and test

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo shallguard-dev fmt --check
cargo shallguard-dev check
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

This repository defines `cargo shallguard-dev` as a local alias for its in-tree
binary; CI uses the same command. Its committed baseline is empty and every
requirement area is hardened, so newly introduced traceability gaps fail
immediately.

Commands that collect executable coverage additionally require
`cargo-llvm-cov`. Semantic review requires a supported provider CLI such as
Codex or Claude; deterministic checking does not require a model provider.

## Git integration before crates.io

Pin the library and CLI to the same immutable commit:

```toml
[dependencies]
shallguard = { git = "https://github.com/sigi64/shallguard.git", rev = "<published-sha>" }
```

Anchor requirements through the library's public namespace:

```rust
#[shallguard::enforces("REQ-RD-001")]
fn enforce_contract() {
    shallguard::enforces_here!("REQ-RD-001");
}

#[shallguard::verifies("REQ-RD-001")]
#[test]
fn contract_is_enforced() {}
```

```bash
cargo install \
  --git https://github.com/sigi64/shallguard.git \
  --rev <published-sha> \
  --locked cargo-shallguard
```

Copy and adapt the [repository configuration example](docs/CONFIGURATION.md)
before running the command.

See [User documentation](docs/USER_DOC.md),
[technical documentation](docs/TECHNICAL_DOC.md), and the
[requirements specification](docs/USER_STORIES_AND_REQUIREMENTS.md).
