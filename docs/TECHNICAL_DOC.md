# Shallguard technical documentation

## Architecture

The repository is a Rust workspace with a root package containing both the
`req_trace` library and `cargo-req-cov` executable, plus the independent
`shallguard-macros` procedural macro package.

- The library parses requirement Markdown and Rust syntax and produces typed
  reports and versioned artifacts.
- The CLI discovers the invoked Cargo workspace and adapts library results to
  terminal output and files.
- The macro crate validates requirement ID syntax at compile time while
  emitting annotated Rust items unchanged.
- Git, Cargo, LLVM, and optional model providers are invoked as local
  subprocesses. There is no long-running runtime or network service.

The detailed architecture preflight and dependency contract live in
[the requirements specification](USER_STORIES_AND_REQUIREMENTS.md#architecture-preflight).

## Build and validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo package --allow-dirty
```

An installed-binary smoke test must run `cargo req-cov` from a separate fixture
workspace so compile-time source paths cannot accidentally become repository
discovery inputs.

## State and data

The tool owns no database. It writes only explicitly selected artifacts,
coverage work files, review checkpoints, and cache entries. Artifacts bind
results to revisions, configuration, schema versions, and content digests.

## Release status

The packages are currently `publish = false` and licensed under MIT. Final
package names, MSRV, remote repository metadata, and registry automation must
be resolved before publication.
