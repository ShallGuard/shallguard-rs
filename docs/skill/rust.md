# ShallGuard in Rust

This file completes `SKILL.md` for a Rust repository. Read `SKILL.md`
first. This file gives the command, the anchor syntax, and the rules that
exist only in Rust.

## The command

The command is `cargo shallguard <command>`. Where `SKILL.md` writes
`shallguard check`, run `cargo shallguard check`.

Install the command with the same version as the `shallguard` crate that
the repository pins:

```bash
cargo install cargo-shallguard --version <pinned-version> --locked
```

When the pinned version changes, run `cargo shallguard install-skill`
after the install. It replaces this skill with the skill of that release.

The command finds the repository with `cargo metadata`. It supports a
single package and a virtual Cargo workspace. In a virtual workspace, set
`source_root` of each document to the directory of the package that owns
the document. ShallGuard scans `src/` and `tests/` below every source root
and every prefix.

Add the `shallguard` crate as a dependency only to the crates that carry
anchors. The crate exports the anchor macros. You do not need a direct
dependency on the macro crate.

## Anchor syntax

- **Enforcement anchor.** The attribute `#[shallguard::enforces("REQ-X-NNN")]`
  on an item: a function, a struct, an enum, a const, or a static. The
  statement macro `shallguard::enforces_here!("REQ-X-NNN")` inside a block.
- **Verification anchor.** The attribute
  `#[shallguard::verifies("REQ-X-NNN", ...)]` on a test function.

Only these forms are anchors. A `REQ-...` string in a comment, in a doc
comment, or in a string literal is not an anchor.

## Rules that exist only in Rust

- **Field and variant anchors.** A struct field or an enum variant can carry
  `#[enforces("...")]`. This works only if the struct or enum itself carries
  `#[enforces]` as its **first** attribute, above any `#[derive]`. The
  container attribute can be bare or can list IDs. The container attribute
  removes the field-level anchors before the derives see them. A wrong order
  breaks the compilation.
- Use `enforces_here!("REQ-X-NNN")` for a branch, a match arm, or a sequence
  of statements. An attribute cannot reach these places. Put the macro as
  the first statement inside the block. If a match arm is a bare expression,
  wrap it in `{ }`. The macro expands to nothing.
- `#[verifies("REQ-X-NNN", ...)]` is valid only on a function with a
  recognized test attribute: `#[test]`, `#[tokio::test]`, or any attribute
  whose name ends in `test`, for example `#[my_harness::container_test]`.
  It **rejects a test with `#[ignore]`** at compile time and in the check.
- The compiler validates the format of each anchor ID. A typo like
  `REQ-HSR-002` is a compile error. The command `cargo build` is a cheap
  first sanity check.
- The citation in a `*Verified:*` line names the test file and the test
  function, for example:

  ```markdown
  *Verified:* [test] `cli:tests/session.rs` (`rejects_expired_token`)
  ```

## Comments next to anchors

There are three placements. Use one, or use the first together with the
second or the third.

**A block before the anchor.** Use it for the summary of what the item
guarantees. In a doc comment, the empty line that separates the block from
an existing comment is a line with only `///`. In a line comment, it is an
empty source line.

```rust
/// Registration of a new provider.
/// Checks for duplicates to prevent tracking corruption.
///
/// Requirements:
/// The first registration makes an eligible target `Ok`. The health
/// fact and the quality timing do not change.
#[enforces("REQ-OP-066")]
pub fn register_provider(&mut self, provider_id: ProviderId) {
```

```rust
if has_prefix {
    // The provider is ready and has a free prefix.

    // Requirements:
    // Registration is the provider-presence fact behind the operational
    // state. Health is left to the quality handlers.
    enforces_here!("REQ-OP-066");
    target.register_provider(provider_id);
}
```

**A comment after an ID inside the attribute.** Use it when an anchor
claims several requirements and each ID has its own one-line reason. Put
a comma after the last ID when a comment follows it.

**A comment above a group of IDs inside the attribute.** Use it when one
reason covers several IDs. Both forms can mix in one attribute. Only `//`
and `/* */` comments are valid inside an attribute. A `///` line inside
an attribute is a compile error. The compiler removes the comments before
the macro reads the IDs, and `rustfmt` keeps them.

```rust
#[enforces(
    // Weighted split that converges per goal and reacts to receivers
    // appearing, disappearing, and goal changes.
    "REQ-CR-008",
    "REQ-CR-009",
    // Batched per cycle. A move never drops a downstream connection.
    "REQ-OP-004",
    "REQ-EH-016", // the cap damps cascade patterns across targets
)]
pub fn get_patch(&mut self, configuration: &OptimizerConfiguration) -> Patch {
```

## Tools of the analysis pipeline

- The `coverage` command needs the tool `cargo-llvm-cov`.
- The `review` command needs the provider program: `codex`, `claude`, or
  `copilot`. The provider login and the data handling of the provider are
  outside ShallGuard.
