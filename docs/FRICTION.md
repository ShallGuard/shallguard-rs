# Friction log

Contract: one line per entry, written at the moment of annoyance, no
rationalizing. Each entry eventually resolves to exactly one of:
bug / lint idea / SKILL.md rule / feature-removal candidate /
accepted-cost — append the resolution to the line, never delete it.
Friction is appended in the same change set as the work that produced
it; working around friction silently is the failure mode this file
exists to prevent.

- 2026-08-19 `review`: a stale local-review directory blocks
  feature-by-feature review work; it must be removed by hand between
  runs → bug ([#7](https://github.com/sigi64/shallguard/issues/7))
- 2026-08-19 `cli`: only `review show` produces colored output; every
  other command is monochrome, so reports read inconsistently → bug
  ([#8](https://github.com/sigi64/shallguard/issues/8))
- 2026-08-19 `check`: a test written ahead of its requirement's ⏳→✅ flip
  cannot carry its `#[verifies]` anchor without a stale-anchor warning, so
  the anchor and the flip must land in the same change → accepted-cost
  (the warning is what forces flip discipline)
- 2026-08-19 `anchors`: a proc-macro crate cannot carry enforcement anchors
  for its own compile-time behavior (it cannot invoke its own macros), so
  macro contracts anchor the re-export in `src/lib.rs` instead
  (REQ-TRACE-008 precedent) → accepted-cost
- 2026-08-19 `lints`: the vacuity predicates exist twice (macro token
  scan, classifier) plus a suppression parser; the macro-rejects-a-subset
  invariant is guarded only by shared regression fixtures → lint idea
  (a shared proc-macro2-only crate would create one authority, but that
  is a new publishable crate — maintainer decision)
- 2026-08-19 `test-index`: tests split into `#[path = "*_tests.rs"]` files
  resolve by unique-suffix fallback instead of exact syntactic name (the
  file-derived module guess differs from the libtest name); pre-existing
  convention across the repo, breaks only on a cross-file test-name
  collision → accepted-cost (revisit if an Ambiguous resolution appears)
- 2026-08-19 `impact`: src/impact.rs was already ~2,300 lines and the
  round-3 fix added ~55 more (baseline_addition_finding + test) instead of
  splitting first; the module needs a real decomposition pass → lint idea
  (candidates: baseline comparison, dependency analysis, scope collection)
