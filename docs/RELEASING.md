# Releasing ShallGuard

Crates.io releases contain three version-aligned packages. Publish them from a
clean checkout of the reviewed release commit in dependency order.

## Prepare

1. Set the same release version in all three `Cargo.toml` files and update
   their internal dependency versions.
2. Move the relevant entries in [`CHANGELOG.md`](../CHANGELOG.md) from
   `Unreleased` into the dated release section.
3. Run the repository validation commands from
   [`TECHNICAL_DOC.md`](TECHNICAL_DOC.md#build-and-validation).
4. Inspect each archive before publishing:

   ```bash
   cargo package --locked --list -p shallguard-macros
   cargo package --locked --list -p shallguard
   cargo package --locked --list -p cargo-shallguard
   ```

## Publish

Crates.io packages are immutable. Publish each package only after the previous
one is visible in the registry:

```bash
cargo publish --locked -p shallguard-macros
cargo publish --locked -p shallguard
cargo publish --locked -p cargo-shallguard
```

After all packages are available, verify a fresh installation in a directory
outside this repository:

```bash
cargo install cargo-shallguard --version 0.1.0 --locked
cargo shallguard --help
```

Create and push the version tag only after the registry release succeeds. If a
package is published incorrectly, release a corrected patch version; an
existing crates.io version cannot be replaced.
