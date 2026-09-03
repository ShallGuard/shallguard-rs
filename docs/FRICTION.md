# Friction log

This file records each moment when the gate or the workflow made the work
harder. A contributor adds one line at the moment of the annoyance, without
an explanation. Each entry later gets exactly one resolution: bug, lint
idea, SKILL.md rule, feature-removal candidate, or accepted cost. Add the
resolution to the end of the line. Never delete a line.

Add the friction entry in the same change set as the work that caused it.
This file exists to prevent one failure: a contributor who works around the
friction in silence.

- 2026-08-19 `review`: a stale local-review directory blocks
  feature-by-feature review work; it must be removed by hand between
  runs → bug ([#7](https://github.com/shallguard/shallguard-rs/issues/7))
- 2026-08-19 `cli`: only `review show` produces colored output; every
  other command is monochrome, so reports read inconsistently → bug
  ([#8](https://github.com/shallguard/shallguard-rs/issues/8))
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
