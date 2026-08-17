# Releasing ShallGuard

The workspace packages use independent versions. Release only packages whose
published contents or dependency requirements changed, from a clean checkout
of the reviewed release commit. Unchanged packages keep their existing version
and are not republished.

## Prepare

1. Increment the version of each package being released. Update an internal
   dependency requirement only when the release needs a newer version of that
   dependency; otherwise retain the existing published version.
2. Move the relevant entries in [`CHANGELOG.md`](../CHANGELOG.md) from
   `Unreleased` into the dated release section.
3. Run the repository validation commands from
   [`TECHNICAL_DOC.md`](TECHNICAL_DOC.md#build-and-validation).
4. Inspect the archive of every package being released. For example:

   ```bash
   cargo package --locked --list -p shallguard
   cargo package --locked --list -p cargo-shallguard
   ```

## Publish

Crates.io packages are immutable. Publish only changed packages, in dependency
order, and wait for each dependency release to become visible before publishing
its consumer. For a release that changes `shallguard` and `cargo-shallguard`:

```bash
cargo publish --locked -p shallguard
cargo publish --locked -p cargo-shallguard
```

If `shallguard-macros` changes, publish it before `shallguard`. If only
`cargo-shallguard` changes and its current `shallguard` requirement remains
valid, publish only `cargo-shallguard`.

After all packages are available, verify a fresh installation in a directory
outside this repository:

```bash
cargo install cargo-shallguard --version 0.1.1 --locked
cargo shallguard --help
```

Create and push the version tag only after the registry release succeeds. If a
package is published incorrectly, release a corrected patch version; an
existing crates.io version cannot be replaced.
