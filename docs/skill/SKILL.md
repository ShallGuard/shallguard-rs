---
name: shallguard
description: Use when working in a Rust repository that uses ShallGuard requirement traceability — recognizable by a shallguard.toml at the repo root, #[enforces]/#[verifies] anchors in code, or a numbered-requirement USER_STORIES_AND_REQUIREMENTS.md. Triggers include adding/modifying/removing behavior in such a repo, editing a requirements document, responding to "cargo shallguard check" failures, anchoring tests, adopting ShallGuard in a new repository, or running requirement impact/coverage/review analysis.
---

# ShallGuard requirement traceability

ShallGuard machine-checks the link between numbered Markdown requirements,
Rust enforcement sites, and verification tests. In a repo that uses it, the
requirements documents are the specification of the codebase and every
behavior change must keep the three-way link intact. `cargo shallguard check`
is the gate.

Detect usage: `shallguard.toml` at the Cargo repository root. That file names
the requirement documents and their source roots; read it first to learn
which documents own which code.

Install the CLI matching the repo's pinned `shallguard` macro-crate version:

```bash
cargo install cargo-shallguard --version <pinned-version> --locked
```

## The model

- **Requirement**: `REQ-<AREA>-<NNN>` in a selected Markdown document. A
  testable RFC 2119 statement (SHALL / SHALL NOT / MAY) plus an
  `*Enforced:*` line (file + symbol implementing it) and a `*Verified:*`
  line (evidence class). IDs are stable forever: retired, never reused.
- **Enforcement anchor**: `#[enforces("REQ-X-NNN")]` attribute or
  `enforces_here!("REQ-X-NNN")` statement macro in code.
- **Verification anchor**: `#[verifies("REQ-X-NNN")]` on a test function.
- **Evidence classes** on `*Verified:*` lines: `✅` (automated, anchored
  test), `🔬` (end-to-end/production validation), `👁` (code review only),
  `⏳` (pending). Only `✅` requires — and is cross-checked against — a
  `#[verifies]` anchor and a concrete file+function citation.

Anchors are found syntactically. **Comments are never anchors**: a
`REQ-...` string in a comment or string literal is inert — it neither
satisfies traceability nor is validated.

## Workflow: requirements first

For **new behavior**, in this order, all in the same change set:

1. **Draft the requirement before implementing.** Next free
   `REQ-<AREA>-NNN` in the owning document: SHALL statement, `*Enforced:*`
   target, `*Verified:*` ⏳ (pending). Exploration/spikes may precede the
   draft, but the final implementation pass starts from the written
   requirement. New behavior without a requirement is an incomplete change.
2. **Implement and anchor the enforcement site.**
3. **Write the test, anchor it with `#[verifies]`**, and only then flip
   ⏳ to ✅ with a citation naming the exact test file and function:

   ```markdown
   *Verified:* ✅ `prefix:tests/file.rs` (`test_fn_name`)
   ```

4. **Run the gate** (below) before claiming the work done.

**Modifying code near an anchor**: read the referenced requirement FIRST.
Behavior changes → update the requirement text in the same change. Code
merely moves → the anchor moves with it.

**Removing behavior**: mark the requirement `*(retired ...)*` in the
document (never delete the entry, never reuse the ID) and remove its
anchors.

## Anchor rules

- `#[enforces("REQ-X-NNN")]` on items: functions, structs, enums, consts,
  statics. One item may enforce several requirements; one requirement may
  be anchored at several sites.
- **Field/variant anchors**: individual struct fields and enum variants may
  carry `#[enforces("...")]`, but only if the containing struct/enum itself
  carries `#[enforces]` (bare, or with IDs) as its **first** attribute —
  above any `#[derive]`. The container attribute strips the field-level
  anchors before derives see them; wrong ordering breaks compilation.
- `enforces_here!("REQ-X-NNN")` for branches, match arms, and statement
  sequences — sites an attribute cannot reach. Place it as the first
  statement inside the relevant block; wrap a bare match-arm expression in
  `{ }` if needed. It expands to nothing.
- `#[verifies("REQ-X-NNN", ...)]` is valid only on functions carrying a
  recognized test attribute (`#[test]`, `#[tokio::test]`, or any attribute
  path ending in `test`). It **rejects `#[ignore]`d tests** at compile time
  and in the checker. There is no statement form for verification.
- A single test claiming 6 or more requirements is flagged as an outlier.
- Anchor ID format is validated at build time — a typo like `REQ-HSR-002`
  is a compile error, so `cargo build` is a cheap first sanity check.
- Place `#[enforces]` on the item that actually implements the SHALL
  statement, not on the nearest public function.

## Evidence honesty (hard rules)

- **Read the test before anchoring it.** `#[verifies]` goes only on a test
  that would FAIL if the requirement were violated — not one that merely
  executes the enforcing code path.
- Never write ✅ in a document without a resolving `#[verifies]` anchor and
  a citation naming the exact test file and function. Never fabricate a
  plausible-looking citation.
- No targeted test? Use the honest class: 🔬, 👁, or ⏳. A ⏳ is a to-do
  item, not a failure; a false ✅ is a failure.
- Never weaken or vague-ify a requirement's text to make a check pass.

## The gate — run before claiming done

```bash
cargo shallguard check          # full traceability cross-check
cargo shallguard fmt --check    # after any requirement-document edit
cargo shallguard fmt            # fix doc formatting (never hand-wrap
                                # requirement blocks yourself)
```

- Run `check` with **no document arguments** so `shallguard.toml` selects
  every document: a single-document run scans only that document's crates
  and falsely reports cross-crate anchors as missing.
- `fmt` refuses to write if any selected document is malformed, touches
  only requirement list blocks, and verifies formatting changed no
  statement or evidence semantics.
- After fixing or retiring a baselined gap:
  `cargo shallguard baseline prune`, then commit the removal together with
  the fix. A stale baseline entry is itself a failure.
- If the consuming repo has no ShallGuard CI job yet, the local run IS the
  gate — never skip it because "CI will catch it".

## Never do

- Never edit the baseline file (`.shallguard/baseline.toml`) by hand and
  never try to add entries — there is no baseline-update command; new debt
  cannot be accepted, only fixed. The baseline is a ratchet.
- Never remove an anchor, delete or reword a requirement, or downgrade
  evidence just to silence a checker failure — fix what the failure points
  at.
- Never mark an anchored test `#[ignore]`.
- Never invent or guess a REQ ID — an ID referenced in code that no
  document defines is a hard failure.
- Never reuse a retired requirement ID.

## Responding to `check` failures

| Failure | Correct response |
|---|---|
| ID in code not defined in any document | Add the requirement, or fix the typo — never delete the anchor blindly |
| Duplicate requirement IDs | Renumber the newer one and its anchors |
| ✅ claim with no anchored test | Write and anchor the test, or downgrade honestly to ⏳ |
| ✅ citation names no concrete test file/function | Complete the citation with the real file and function |
| Cited path does not exist | Fix the citation to the real file — never fabricate a path |
| Enforced file carries no anchor with the ID | Anchor the real enforcement site — never move the anchor to a file that merely mentions the code |
| Stale baseline entry | `cargo shallguard baseline prune`, commit the removal |
| Document stops parsing | Fix the document structure; `fmt --check` pinpoints structural lint errors |

## Adopting ShallGuard in a new repository

1. Create `shallguard.toml` at the Cargo repository root. Minimal shape
   (all paths repository-relative; loader rejects unknown fields):

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

   For a virtual workspace, add one `[[documents]]` entry per
   specification with `source_root` set to the owning package directory.
   ShallGuard scans `src/` and `tests/` beneath every source root and
   mapped prefix.
2. Write the requirement document: numbered `REQ-<AREA>-NNN` entries with
   SHALL statements and Enforced/Verified lines; run
   `cargo shallguard fmt` to normalize structure.
3. Anchor existing code and tests honestly (see rules above). Only add
   `shallguard` as a dependency of crates that carry anchors.
4. If adopting around existing code with unavoidable historical gaps,
   create the baseline **once** and commit it:
   `cargo shallguard baseline init`. Prefer an empty baseline; each entry
   is debt that blocks editing that requirement's text until resolved.
5. Set an area's `hard_enforcement` / `hard_verification` to `true` as
   soon as that dimension has no historical gaps. Hardened areas cannot be
   baselined and the ratchet only moves forward.
6. Wire `cargo shallguard check` and `cargo shallguard fmt --check` into
   CI once a build image with `cargo-shallguard` is available; until then
   the local run is the gate.

## Analysis and review pipeline (on request only)

These commands are for change review and evidence audits — not gates. Do
not run them on every edit; `coverage` and `review` are expensive.

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

# One-command local semantic review: impact -> impacted-test selection ->
# coverage -> capsule bundle -> LLM review. Defaults: --target
# origin/master (falls back per repo config), --with-coverage,
# --provider codex; "claude" is the other provider. May send bounded
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

Model verdicts from `review` are advisory. Deterministic gates
(`check`, `fmt --check`) never depend on network access or a model.
