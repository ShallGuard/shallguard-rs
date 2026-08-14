# Requirement Traceability Baseline Design

**Status:** Implemented; initial debt inventory pending

This document specifies how existing traceability gaps are grandfathered while
new gaps are rejected immediately. Modified baselined requirements are handled
by the merge-request impact check described below.
It is a component of
[Requirement Assurance Design](requirement-assurance-design.md).

The baseline complements the existing area ratchet. It does not weaken areas
that are already hard.

## 1. Problem

Several requirement areas still report anchor or automated-evidence gaps as
warnings while their historical requirements are audited. An area-only ratchet
has two undesirable properties during that migration:

- a new requirement can add another warning in an unhardened area;
- a modified historical requirement needs explicit MR-level scrutiny when it
  retains an old exception.

A count threshold is not sufficient. One old gap could be fixed while a new gap
is introduced, leaving the count unchanged.

The baseline identifies the exact accepted gap. It intentionally contains no
content hashes that authors would need to maintain.

## 2. Goals

- Preserve known historical gaps temporarily.
- Reject every new gap regardless of requirement area.
- Let MR impact analysis reject edits to requirements with inherited gaps.
- Reject regressions after a gap has been fixed.
- Make baseline debt visible, reviewable, and monotonically removable.
- Keep already-hard areas fully hard.
- Require no baseline churn for ordinary requirement edits or Markdown reflow.

## 3. Non-goals

- Automatically accepting a new exception.
- Hiding known gaps from normal reports.
- Replacing the area ratchet.
- Fingerprinting requirement text, implementation code, or test bodies.
- Proving that a historical evidence claim is honest.
- Detecting behavior added without any requirement; that remains an impact and
  review concern.

## 4. Baseline file

The proposed file is `.shallguard/baseline.toml`:

```toml
schema = 1

[[gap]]
requirement = "REQ-AUTH-001"
kind = "enforcement-anchor"

[[gap]]
requirement = "REQ-AUTH-001"
kind = "verification-anchor"

[[gap]]
requirement = "REQ-CF-009"
kind = "evidence-citation"
```

Each entry permits one missing traceability dimension for one active
requirement.

Recognized initial kinds:

| Kind | Meaning |
|------|---------|
| `enforcement-anchor` | Enforced file has no matching enforcement anchor |
| `verification-anchor` | Automated requirement has no matching `#[shallguard::verifies]` test |
| `evidence-citation` | Automated evidence lacks a concrete resolvable test citation |

An entry does not suppress unknown IDs, duplicate requirements, dead paths,
malformed anchors, or invalid evidence tests.

## 5. No stored fingerprint

Baseline identity is only `(requirement ID, gap kind)`. This keeps the file
stable and ensures that editing requirement prose never creates hash-update
work.

The trade-off is explicit: a head-only check cannot tell whether a baselined
requirement was modified. The MR impact phase compares base and head
requirement chunks directly. If a changed requirement still has any baselined
gap, that MR check fails and asks the author to complete its traceability. This
preserves the modified-requirement ratchet without persistent hashes.

## 6. Check algorithm

For every current traceability gap:

1. Determine requirement ID and gap kind.
2. If the requirement's area is already hard, report the normal hard error;
   baseline entries cannot exempt it.
3. Look up an exact baseline entry by `(requirement, kind)`.
4. No entry means a new regression and is a hard error.
5. An exact match is known debt and remains a warning.

Then inspect every baseline entry:

1. Missing requirement means stale baseline and is a hard error.
2. Retired requirement means stale baseline and is a hard error.
3. Gap no longer exists means the entry is stale and is a hard error asking the
   author to remove it.
4. Duplicate or unknown gap kind is a hard baseline-format error.
5. Entry for an already-hard area is a hard policy error.

Stale entries must fail rather than remain harmless. Otherwise a fixed
requirement could later regress under its old exception.

## 7. State transitions

| Previous state | Head state | Result |
|----------------|------------|--------|
| Baselined gap | Same gap | Warning: known debt |
| Baselined gap | Requirement changed in an MR | MR impact check requires the gap to be fixed |
| Baselined gap | Gap fixed, entry remains | Hard: remove stale entry |
| Baselined gap | Gap fixed, entry removed | Pass |
| No gap | New gap | Hard regression |
| No requirement | New incomplete requirement | Hard regression |
| Active requirement | Retired, entry remains | Hard: remove stale entry |
| Warning area | Area promoted hard | Baseline entries forbidden |

## 8. Maintenance commands

The normal tool supports only monotonic maintenance:

```text
shallguard baseline check
shallguard baseline prune
```

`baseline prune` may remove entries for gaps that are now fixed or requirements
that are retired. It must never add entries.

The implementation also has a bootstrap-only `baseline init` command. It uses
create-new file semantics and refuses to run once a baseline file exists.

The initial baseline is created once as part of introducing this policy and is
reviewed as a complete debt inventory. There is intentionally no routine
`baseline update` command: adding or refreshing an exception must be a visible
manual policy change.

## 9. Reporting

Known debt and regressions are reported separately:

```text
traceability baseline
  known gaps:         N
  resolved gaps:      3 (remove stale entries)
  new regressions:    0
```

Every known-gap warning includes:

- requirement ID and title;
- gap kind;
- owning document location;
- marker that the gap is grandfathered;
- remediation target.

The area coverage table remains unchanged so baseline debt cannot disappear
from the visible totals.

## 10. Interaction with MR impact

The baseline is evaluated against head state independently of Git diff, so it
cannot be bypassed by an unusual base selection.

Impact analysis adds context:

- changed baselined requirement, which is a deterministic hard finding until
  its inherited gaps are fixed;
- source change in a baselined enforcement file;
- test change related to baselined automated evidence;
- baseline file modification.

Any baseline modification is surfaced prominently in the MR summary and review
bundle. An LLM may review the rationale, but deterministic baseline matching is
authoritative.

## 11. Merge and conflict behavior

The baseline is sorted by requirement ID then gap kind. Stable formatting keeps
parallel cleanup changes mergeable.

If two branches fix the same gap, both may remove the same entry; ordinary Git
conflict resolution is sufficient. A branch that changes a requirement while
another fixes its gap must rebase and complete traceability under the new head
state.

## 12. Rollout

1. Generate the proposed initial entries from current warning gaps.
2. Review counts by area and gap kind.
3. Verify every entry matches one current warning and no hard-area requirement.
4. Commit baseline and enforcement logic together.
5. Make new and stale entries hard in the same MR.
6. Continue area-by-area anchor work; prune entries as each gap is fixed.
7. Remove the baseline mechanism when the file becomes empty, or keep the empty
   file as an explicit no-debt policy marker.

## 13. Testing strategy

Unit and fixture tests cover:

- exact known gap remains a warning;
- new gap fails;
- fixed gap with stale entry fails;
- removed stale entry passes;
- retired and missing requirements fail;
- duplicate entries and unknown kinds fail;
- hard-area entries fail;
- `baseline prune` only removes entries;
- deterministic sorting and serialization.

The Phase 1 MR-impact fixtures separately cover modified baselined
requirements.

One repository integration test pins the expected known-gap totals so accidental
large baseline changes are conspicuous in review.

## 14. Security and integrity

The baseline is trusted repository policy and is reviewed like CI configuration.
It must not be downloaded from CI artifacts or generated from the current gaps
at runtime. The checker reads only the committed file and validates every field.

An MR that changes the baseline should receive a dedicated finding even when all
entries are syntactically valid.
