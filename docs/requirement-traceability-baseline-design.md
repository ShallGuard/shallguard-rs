# Requirement Traceability Baseline Design

**Status:** Implemented; initial debt inventory pending

This document specifies how ShallGuard accepts the existing traceability gaps
and rejects each new gap immediately. Traceability is the relationship between
a requirement, the code that enforces it, and the tests that verify it. A gap
is a missing anchor or missing evidence for a requirement. An anchor is a mark
in the Rust code that links the code or a test to a requirement. The baseline
records the accepted gaps. The baseline is a file that the repository owns.

The gaps that the baseline accepts are the debt. The check is the ShallGuard
tool run that examines the traceability. A hard error makes the check fail. A
warning does not make the check fail.

The merge request (MR) impact check handles a requirement that has a gap in
the baseline and that a change modifies. A merge request (MR) is a proposed
change that a reviewer examines before it is merged. The section "Interaction
with MR impact" describes this check.

This document is a component of
[Requirement Assurance Design](requirement-assurance-design.md).

The baseline adds to the existing area ratchet. An area is a group of
requirements that share the same ID prefix. The area ratchet is the existing
mechanism that promotes an area from warnings to hard errors. In a hard area,
the check reports each gap as a hard error. The baseline does not weaken an
area that is already hard.

## 1. Problem

Several requirement areas still report gaps as warnings. These gaps are
missing anchors or missing automated evidence. The areas report warnings
while a person audits their historical requirements. During this migration, a
ratchet that works only on areas has 2 unwanted properties:

- A new requirement can add one more warning in an area that is not hard.
- A modified historical requirement needs an explicit examination at the MR
  level when it keeps an old accepted gap.

A limit on the number of gaps is not sufficient. A change can fix one old gap
and add one new gap at the same time. The number of gaps then does not
change.

The baseline identifies the exact accepted gap. The baseline contains no
content hashes. This is intentional. A content hash is a fingerprint of a
text. Authors would have to update the hashes after each edit.

## 2. Goals

- Keep the known historical gaps for a limited time.
- Reject every new gap in every requirement area.
- Let the MR impact analysis reject an edit to a requirement that has an
  inherited gap. An inherited gap is a gap that the baseline accepts.
- Reject a gap that comes back after a person fixed it.
- Make the debt in the baseline visible. Let a reviewer examine it. Let the
  debt only decrease.
- Keep each area that is already hard fully hard.
- Do not require a change to the baseline after an ordinary edit to a
  requirement or after a change of the line breaks in the Markdown.

## 3. Non-goals

The design does not do these things:

- Accept a new baseline entry automatically.
- Hide a known gap from the normal reports.
- Replace the area ratchet.
- Make a fingerprint of the requirement text, the implementation code, or the
  test bodies.
- Prove that a historical evidence claim is honest.
- Detect behavior that a change adds without a requirement. This stays a
  concern for the impact analysis and for the review.

## 4. Baseline file

The proposed file is `.shallguard/baseline.toml`. TOML is a plain text
format for configuration files. This is an example of the file:

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

Each entry permits one kind of gap for one active requirement.

The check recognizes these initial kinds:

| Kind | Meaning |
|------|---------|
| `enforcement-anchor` | The enforced file has no enforcement anchor that matches the requirement |
| `verification-anchor` | Automated requirement has no matching `#[shallguard::verifies]` test |
| `evidence-citation` | The automated evidence does not cite a concrete test that the check can find |

An entry does not hide these errors:

- an unknown requirement ID
- a duplicate requirement
- a dead path
- a malformed anchor
- an invalid evidence test

## 5. No stored fingerprint

The identity of a baseline entry is only `(requirement ID, gap kind)`. This
keeps the file stable. An edit to the prose of a requirement never creates
work to update a hash.

This design has an explicit cost. Git is the version control system that
stores the repository. The base is the version of the repository before a
change. The head is the version of the repository with the change. A check
that examines only the head cannot know whether a change modified a
requirement that has a gap in the baseline.

The MR impact phase compares the requirement text in the base and in the head
directly. If a changed requirement still has a gap in the baseline, the MR
check fails. The check asks the author to complete the traceability of the
requirement. This keeps the ratchet for modified requirements without stored
hashes.

## 6. Check algorithm

The check does these steps for every current gap:

1. Find the requirement ID and the gap kind.
2. If the area of the requirement is already hard, report the normal hard
   error. A baseline entry cannot exempt the gap.
3. Find the exact baseline entry by `(requirement, kind)`.
4. If there is no entry, the gap is a new regression. A regression is a gap
   that has no entry in the baseline. Report a hard error.
5. If there is an exact match, the gap is known debt. Report a warning.

A stale entry is an entry that no longer matches a real gap. Then the check
examines every baseline entry:

1. If the requirement does not exist, the entry is stale. Report a hard
   error.
2. If the requirement is retired, the entry is stale. Report a hard error.
3. If the gap no longer exists, the entry is stale. Report a hard error that
   asks the author to remove the entry.
4. If the entry is a duplicate, or if the gap kind is unknown, report a hard
   error for the baseline format.
5. If the entry is for an area that is already hard, report a hard policy
   error.

A stale entry must cause a failure. It must not stay in the file as a
harmless entry. If a stale entry stays, a fixed requirement can get the same
gap again later. The old entry then hides the new gap.

## 7. State transitions

| Previous state | Head state | Result |
|----------------|------------|--------|
| Gap in the baseline | The same gap | Warning. The gap is known debt. |
| Gap in the baseline | An MR changes the requirement | The MR impact check requires a fix for the gap. |
| Gap in the baseline | The gap is fixed and the entry remains | Hard error. Remove the stale entry. |
| Gap in the baseline | The gap is fixed and the entry is removed | Pass |
| No gap | A new gap | Hard error. The gap is a regression. |
| No requirement | A new requirement that has a gap | Hard error. The gap is a regression. |
| Active requirement | The requirement is retired and the entry remains | Hard error. Remove the stale entry. |
| Warning area | The area becomes hard | Baseline entries are not permitted. |

## 8. Maintenance commands

The normal tool supports only monotonic maintenance. Monotonic means that the
maintenance can remove an entry but can never add an entry. These are the
commands:

```text
shallguard baseline check
shallguard baseline prune
```

The `baseline prune` command can remove an entry for a gap that is now fixed.
It can also remove an entry for a requirement that is retired. It must never
add an entry.

The implementation also has a `baseline init` command for the first setup
only. The command creates a new file. If a baseline file already exists, the
command refuses to run.

A person creates the initial baseline once, when the project introduces this
policy. A reviewer examines the initial baseline as a complete inventory of
the debt. There is no routine `baseline update` command. This is intentional.
A person who adds or refreshes a baseline entry must make a visible manual
change to the policy.

## 9. Reporting

The check reports known debt and regressions separately. This is an example
of the report:

```text
traceability baseline
  known gaps:         N
  resolved gaps:      3 (remove stale entries)
  new regressions:    0
```

Every warning for a known gap includes these items:

- the requirement ID and the title
- the gap kind
- the location in the document that owns the requirement
- a marker that says that the gap is an accepted historical gap
- the target of the remediation

The area coverage table does not change. Thus the baseline debt cannot
disappear from the visible totals.

## 10. Interaction with MR impact

The check evaluates the baseline against the head state. It does not use the
Git diff for this. A Git diff is the list of differences between the base and
the head. Thus an unusual choice of the base cannot bypass the baseline.

The impact analysis adds this context:

- a changed requirement that has a gap in the baseline. This is a
  deterministic hard finding until the inherited gaps are fixed.
  Deterministic means that the same input always gives the same result.
- a change to a source file that enforces a requirement with a gap in the
  baseline
- a change to a test that is related to automated evidence with a gap in the
  baseline
- a modification of the baseline file

The MR summary and the review bundle show every modification of the baseline
prominently. A large language model (LLM) is an AI model that reads and
writes text. An LLM can review the reason for the modification. The
deterministic matching of the baseline is authoritative.

## 11. Merge and conflict behavior

The tool sorts the baseline by requirement ID and then by gap kind. The
stable format lets Git merge parallel cleanup changes.

A branch is a separate line of changes in Git. If 2 branches fix the same
gap, both can remove the same entry. The ordinary Git conflict resolution is
sufficient.

One branch can change a requirement while another branch fixes the gap of
that requirement. The first branch must then rebase. A rebase moves the
changes of a branch on top of the newest state. The first branch must then
complete the traceability under the new head state.

## 12. Rollout

1. Generate the proposed initial entries from the current warning gaps.
2. Review the counts by area and by gap kind.
3. Verify that every entry matches one current warning. Verify that no entry
   matches a requirement in a hard area.
4. Commit the baseline and the enforcement logic together.
5. Make new entries and stale entries hard errors in the same MR.
6. Continue the anchor work area by area. Prune an entry when its gap is
   fixed.
7. Remove the baseline mechanism when the file becomes empty. As an
   alternative, keep the empty file as an explicit marker that there is no
   debt.

## 13. Testing strategy

A fixture test uses a prepared example input. Unit tests and fixture tests
cover these cases:

- An exact known gap stays a warning.
- A new gap fails.
- A fixed gap with a stale entry fails.
- A removed stale entry passes.
- A retired requirement and a missing requirement fail.
- A duplicate entry and an unknown kind fail.
- An entry for a hard area fails.
- The `baseline prune` command only removes entries.
- The sorting and the serialization are deterministic. Serialization is the
  conversion of the entries to the text of the file.

The MR impact fixtures from Phase 1 cover modified requirements with a gap in
the baseline. These fixtures are separate.

One integration test in the repository pins the expected totals of known
gaps. Thus a large accidental change to the baseline is easy to see in a
review.

## 14. Security and integrity

The baseline is trusted policy of the repository. Reviewers examine it in the
same way as the continuous integration (CI) configuration. The tool must not
download the baseline from CI artifacts. The tool must not generate the
baseline from the current gaps at run time. The check reads only the
committed file. The check validates every field.

We recommend a dedicated finding for an MR that changes the baseline. This
applies also when all entries have a valid syntax.
