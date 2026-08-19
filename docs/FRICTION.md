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
