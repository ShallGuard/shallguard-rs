---
name: shallguard
description: Use when working in a Rust repository that uses ShallGuard requirement traceability — recognizable by a shallguard.toml at the repo root, #[enforces]/#[verifies] anchors in code, or a numbered-requirement USER_STORIES_AND_REQUIREMENTS.md. Triggers include adding/modifying/removing behavior in such a repo, editing a requirements document, responding to "cargo shallguard check" failures, anchoring tests, adopting ShallGuard in a new repository, or running requirement impact/coverage/review analysis.
---

# ShallGuard requirement traceability

This manual is for a coding agent that works in a repository that uses
ShallGuard. It is self-contained.

ShallGuard checks the link between three things: numbered requirements in
Markdown documents, the Rust code that makes each requirement true, and the
tests that verify it. In a repository that uses ShallGuard, the requirement
documents are the specification of the codebase. Every behavior change must
keep the three-way link intact. The command `cargo shallguard check` is the
gate. This manual calls it the check.

You detect ShallGuard by the file `shallguard.toml` at the root of the Cargo
repository. That file names the requirement documents and their source
roots. Read it first. It tells you which document owns which code.

Install the command with the same version as the `shallguard` crate that the
repository pins:

```bash
cargo install cargo-shallguard --version <pinned-version> --locked
```

## The model

- **Requirement.** A requirement has the ID `REQ-<AREA>-<NNN>` and lives in
  a selected Markdown document. It has three parts: a testable statement in
  RFC 2119 form (SHALL / SHALL NOT / MAY), an `*Enforced:*` line that names
  the file and the symbol that implements it, and a `*Verified:*` line that
  names the evidence class. An ID is stable forever. A retired ID is never
  used again.
- **Enforcement anchor.** The attribute `#[enforces("REQ-X-NNN")]` on a code
  item, or the statement macro `enforces_here!("REQ-X-NNN")` inside a block.
- **Verification anchor.** The attribute `#[verifies("REQ-X-NNN")]` on a
  test function.
- **Evidence marks** on a `*Verified:*` line: `[test]` an anchored
  automated test, `[e2e]` an end-to-end or production validation,
  `[review]` a code review only, `[pending]` pending. The emoji ✅, 🔬, 👁,
  and ⏳ are optional aliases. `cargo shallguard fmt` adds the keyword
  before an emoji and keeps the emoji, for example `[test] ✅`. The
  commands `fmt --check` and `lint` accept an emoji without a keyword. Only
  `[test]` requires a `#[verifies]` anchor and a citation with a file and
  a function. The check compares the `[test]` claim with the anchor.

ShallGuard finds anchors in the syntax of the code. **A comment is never an
anchor.** A `REQ-...` string in a comment or in a string literal does
nothing. It does not satisfy traceability, and the check does not validate
it.

## Workflow: requirements first

For **new behavior**, do these steps in this order, all in the same change
set:

1. **Write the requirement before the implementation.** Use the next free
   `REQ-<AREA>-NNN` in the document that owns the area. Write the SHALL
   statement, the `*Enforced:*` target, and `*Verified:* [pending]`. You
   can explore before you write the requirement. The final implementation
   pass starts from the written requirement. New behavior without a
   requirement is an incomplete change. If a merge or a rebase collides on
   the number, the branch that rebases takes the next free ID. An ID becomes
   stable when the change merges into the default branch, not at draft
   time. The check catches a rename that reaches the document but not the
   anchors.
2. **Implement the behavior and anchor the enforcement site.**
3. **Write the test and anchor it with `#[verifies]`.** Only then change
   `[pending]` to `[test]`, with a citation that names the exact test file
   and function:

   ```markdown
   *Verified:* [test] `prefix:tests/file.rs` (`test_fn_name`)
   ```

4. **Run the gate** before you report the work as done. The gate is
   described below.

**If you change code near an anchor,** read the requirement first. If the
behavior changes, update the requirement text in the same change. If the
code only moves, move the anchor with it.

**If you remove behavior,** mark the requirement as `*(retired ...)*` in the
document and remove its anchors. Never delete the entry. Never use the ID
again.

## Anchor rules

- Put `#[enforces("REQ-X-NNN")]` on an item: a function, a struct, an enum,
  a const, or a static. One item can enforce several requirements. One
  requirement can have anchors at several sites.
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
  It **rejects a test with `#[ignore]`** at
  compile time and in the check. There is no statement form for
  verification.
- The check flags one test that claims 6 or more requirements as an
  outlier.
- The compiler validates the format of each anchor ID. A typo like
  `REQ-HSR-002` is a compile error. The command `cargo build` is a cheap
  first sanity check.
- Put `#[enforces]` on the item that implements the SHALL statement. Do not
  put it on the nearest public function.

## Evidence honesty

These rules are hard rules.

- **Read the test before you anchor it.** Put `#[verifies]` only on a test
  that FAILS when the requirement is violated. A test that only runs the
  enforcing code path is not enough.
- Never write `[test]` in a document without a `#[verifies]` anchor that
  resolves
  and a citation that names the exact test file and function. Never invent
  a citation that looks plausible.
- If no targeted test exists, use the honest class: `[e2e]`, `[review]`, or
  `[pending]`. A `[pending]` is a to-do item, not a failure. A false
  `[test]` is a failure.
- Never weaken the text of a requirement, and never make it vague, to make
  a check pass.

## The gate

Run these commands before you report the work as done:

```bash
cargo shallguard check          # full traceability cross-check
cargo shallguard fmt --check    # after any requirement-document edit
cargo shallguard fmt            # fix doc formatting (never hand-wrap
                                # requirement blocks yourself)
```

- Run `check` with **no document argument**, so that `shallguard.toml`
  selects every document. A run with one document scans only the crates of
  that document, and it reports cross-crate anchors as missing.
- The `fmt` command refuses to write if a selected document is malformed.
  It changes only requirement list blocks. It verifies that the format
  change did not change the meaning of a statement or of the evidence.
- After you fix or retire a gap that is in the baseline, run
  `cargo shallguard baseline prune`. Commit the removal together with the
  fix. A stale baseline entry is itself a failure.
- If the repository has no ShallGuard CI job yet, the local run IS the
  gate. Never skip it because "CI will catch it".
- If the repository has a friction log at `docs/FRICTION.md`, use it. When
  the gate or the workflow causes friction while you work, add a one-line
  entry in the same change set. Never work around friction in silence.

## Never do

- Never edit the baseline file `.shallguard/baseline.toml` by hand. Never
  try to add an entry. There is no command that adds an entry. New debt
  cannot be accepted, only fixed. The baseline is a ratchet.
- Never remove an anchor, delete or reword a requirement, or downgrade
  evidence only to remove a check failure. Fix what the failure points at.
- Never mark an anchored test with `#[ignore]`.
- Never invent or guess a requirement ID. An ID in code that no document
  defines is a hard failure.
- Never use a retired requirement ID again.

## Respond to `check` failures

| Failure | Correct response |
|---|---|
| An ID in code is not defined in any document | Add the requirement, or fix the typo. Never delete the anchor without a reason. |
| Two requirements have the same ID | Renumber the newer one and its anchors. |
| A `[test]` claim has no anchored test | Write and anchor the test, or downgrade honestly to `[pending]`. |
| A `[test]` citation names no real test file or function | Complete the citation with the real file and function. |
| A cited path does not exist | Fix the citation to the real file. Never invent a path. |
| The cited enforcement file has no anchor with the ID | Anchor the real enforcement site. Never move the anchor to a file that only mentions the code. |
| A baseline entry is stale | Run `cargo shallguard baseline prune` and commit the removal. |
| The document does not parse | Fix the document structure. The command `fmt --check` points at the structural error. |
| `fmt --check` reports a document as non-canonical | Run `cargo shallguard fmt`. It also adds the keyword before an emoji evidence alias, which `fmt --check` does not require. |

## Adopt ShallGuard in a new repository

1. Create `shallguard.toml` at the root of the Cargo repository. This is the
   minimal shape. All paths are relative to the repository. The loader
   rejects unknown fields:

   ```toml
   schema = 1
   minimum_requirements = 1
   baseline = ".shallguard/baseline.toml"
   verify_outlier_threshold = 6

   [[documents]]
   path = "docs/USER_STORIES_AND_REQUIREMENTS.md"
   source_root = "."

   # Optional path aliases used in *Enforced:*/*Verified:* citations,
   # e.g. `core:src/lib.rs`.
   [prefixes]
   core = "crates/core"

   [areas.CLI]
   label = "Command Line"
   hard_enforcement = true
   hard_verification = true

   [artifacts]
   root = "target/shallguard"
   ```

   In a virtual workspace, add one `[[documents]]` entry for each
   requirement document. Set `source_root` to the directory of the package
   that owns the document. ShallGuard scans `src/` and `tests/` below every
   source root and every prefix.
2. Write the requirement document. Use numbered `REQ-<AREA>-NNN` entries
   with SHALL statements and with Enforced and Verified lines. Run
   `cargo shallguard fmt` to normalize the structure.
3. Anchor the existing code and tests honestly. Follow the rules above. Add
   `shallguard` as a dependency only to the crates that carry anchors.
4. If the existing code has gaps that you cannot avoid, create the baseline
   **once** with `cargo shallguard baseline init` and commit it. An empty
   baseline is best. Each entry is debt. The entry blocks edits of the text
   of that requirement until the gap is resolved.
5. Set `hard_enforcement` or `hard_verification` of an area to `true` as
   soon as the area has no gap of that kind. A hard area cannot go into the
   baseline. The ratchet only moves forward.
6. Add `cargo shallguard check` and `cargo shallguard fmt --check` to CI
   when a build image with `cargo-shallguard` is available. Until then, the
   local run is the gate.

## Analysis and review pipeline

Run these commands only on request. They are for change review and for
evidence audits. They are not gates. Do not run them on every edit. The
`coverage` and `review` commands are expensive. The `review` and
`review show` commands are experimental: they need a language model
provider, and they can change in any release.

```bash
# Which requirements does this diff touch? Direct, transitive (one
# syntax-derived reverse-dependency hop), and structural impacts.
cargo shallguard impact --base <merge-base> \
  --json requirement-impact.json --markdown requirement-impact.md

# Resolve every #[verifies] anchor to an exact Cargo test identity.
cargo shallguard test-index --enumerate --json requirement-tests.json

# Run only the anchored tests under cargo-llvm-cov and check the covered
# regions actually intersect the enforcement scopes. Requires
# cargo-llvm-cov. "Covered" means reached, not correct.
cargo shallguard coverage --requirement REQ-<AREA>-NNN \
  --json requirement-coverage.json

# One-command local semantic review (experimental): impact -> impacted-test selection ->
# coverage -> capsule bundle -> LLM review. Defaults: --target
# origin/master (falls back per repo config), --with-coverage,
# --provider codex; "claude" and "copilot" are the other providers. May send bounded
# source capsules to a hosted model service — confirm that is acceptable
# for the repository before running; use --local-provider
# ollama|lmstudio for on-device Codex inference.
cargo shallguard review

# Housekeeping: continue an interrupted review, reuse validated
# responses, remove the default generated bundle.
cargo shallguard review --resume
cargo shallguard review --cache-dir .cache/shallguard-review
cargo shallguard clean
```

The verdicts from `review` are advisory. The gates `check` and
`fmt --check` never depend on network access or on a model.
