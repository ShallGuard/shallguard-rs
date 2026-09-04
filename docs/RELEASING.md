# Releasing ShallGuard

This page describes how a maintainer publishes a ShallGuard release to
crates.io. The [glossary](GLOSSARY.md) defines each technical term.

Each package in the workspace has its own version. Release only the packages
whose published contents or dependency requirements changed. Release from a
clean copy of the reviewed release commit. A package without changes keeps
its version, and you do not publish it again.

## Prepare

1. Increase the version of each package that you release. Change an internal
   dependency requirement only when the release needs a newer version of that
   dependency. In all other cases, keep the published version.
2. Move the related entries in [`CHANGELOG.md`](../CHANGELOG.md) from the
   `Unreleased` section into a dated release section.
3. Run the validation commands in
   [`TECHNICAL_DOC.md`](TECHNICAL_DOC.md#build-and-validation).
4. Examine the archive of each package that you release. For example:

   ```bash
   cargo package --locked --list -p shallguard
   cargo package --locked --list -p cargo-shallguard
   ```

## Publish

A package version on crates.io cannot change after publication. Publish only
the changed packages, in dependency order. Wait until each dependency is
visible on crates.io before you publish the package that uses it. For a
release that changes `shallguard` and `cargo-shallguard`:

```bash
cargo publish --locked -p shallguard
cargo publish --locked -p cargo-shallguard
```

If `shallguard-macros` changed, publish it before `shallguard`. If only
`cargo-shallguard` changed, and its current `shallguard` requirement is
still valid, publish only `cargo-shallguard`.

After all packages are available, verify a fresh installation from a
directory outside this repository:

```bash
cargo install cargo-shallguard --version 0.1.2 --locked
cargo shallguard --help
```

Create and push the version tag only after the registry accepted the
release. If you publish a package with an error, release a corrected patch
version. You cannot replace an existing crates.io version.
