# ShallGuard contributor guidance

This repository contains a one-shot Rust developer tool, reusable analysis
library, and procedural anchor macros. It has no service runtime, database,
container image, Consul/Vault integration, or deployment surface.

## Commands

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo shallguard-dev fmt --check
cargo shallguard-dev check
cargo install --path cargo-shallguard
```

## Requirement workflow

The product specification is
`docs/USER_STORIES_AND_REQUIREMENTS.md`. It is selected by `shallguard.toml`;
all implemented requirements are anchored, the committed baseline is empty,
and every area is protected by the ratcheted CI gate.

- Add or update a requirement for behavior changes.
- Anchor enforcement with `#[enforces]` or `enforces_here!`.
- Anchor honest automated evidence with `#[verifies]`.
- Do not add baseline entries; new implemented behavior must arrive fully anchored.
- Never claim automated evidence without a test that exercises the contract.
- Keep requirement IDs stable; retire them instead of reusing them.

## Rust conventions

- Prefer simple, deterministic APIs and explicit configuration.
- Do not use `Result::unwrap()`; reserve `expect("BUG: ...")` for invariants.
- Keep deterministic library behavior separate from terminal presentation and
  provider execution.
- Keep files below 1,000 lines by splitting focused modules before extending
  oversized migrated files.
- Use versioned machine-readable artifact schemas.
- Do not add network access or a model dependency to deterministic checks.
