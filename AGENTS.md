# ShallGuard contributor guidance

This repository contains a Rust developer tool, a reusable analysis library,
and procedural anchor macros. The tool runs once and exits. The repository
has no service runtime, no database, no container image, no Consul or Vault
integration, and no deployment surface.

## Commands

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo +1.89.0 check --workspace --all-targets --locked
cargo shallguard-dev fmt --check
cargo shallguard-dev check
cargo install --path cargo-shallguard
```

## Requirement workflow

The product specification is `docs/USER_STORIES_AND_REQUIREMENTS.md`. The
file `shallguard.toml` selects it. All implemented requirements have
anchors. The committed baseline is empty. The CI gate protects every area,
and the gate is a ratchet.

- Add or update a requirement for each behavior change.
- Anchor the enforcement with `#[shallguard::enforces]` or
  `shallguard::enforces_here!`.
- Anchor honest automated evidence with `#[shallguard::verifies]`.
- Do not add baseline entries. New behavior must arrive with all its
  anchors.
- Never claim automated evidence without a test that verifies the
  requirement.
- Keep requirement IDs stable. Retire an ID instead of a reuse.

## Documentation style (mandatory)

Write ALL documents in **ASD-STE100** (Simplified Technical English), for
readers **without prior knowledge**. The rules are in the shared writing
style page of the specification repository, linked from
`docs/WRITING_STYLE.md`. Exception: a requirement statement keeps its
RFC 2119 form (SHALL, SHALL NOT, MAY), because ShallGuard needs it.

## Rust conventions

- Prefer simple APIs with the same result each time, and explicit
  configuration.
- Do not use `Result::unwrap()`. Use `expect("BUG: ...")` only for an
  invariant.
- Keep the library behavior separate from the terminal presentation and
  from the provider execution.
- Keep each file below 1,000 lines. Split a large migrated file into focused
  modules before you extend it.
- Use versioned schemas for machine-readable artifacts.
- Do not add network access or a model dependency to a check.
