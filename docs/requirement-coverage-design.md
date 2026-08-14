# Requirement Coverage Design

**Status:** Partially implemented (per-test enforcement reach v1)

This document specifies how test execution and source-based coverage are
projected onto requirements, enforcement scopes, and MR changes. It is a
component of [Requirement Assurance Design](requirement-assurance-design.md)
and consumes
[Requirement Change Impact](requirement-change-impact-design.md).

## Implementation checkpoint

Exact verification-test identity resolution is available through:

```bash
cargo shallguard test-index --enumerate \
  --json requirement-tests.json --markdown requirement-tests.md \
  --catalog-output harness-tests.json

# Limit local enumeration to one package while developing.
cargo shallguard test-index --enumerate \
  --package example-core --json requirement-tests.json
```

Executable enforcement reach is available through:

```bash
# Run every resolved verification test. This can include integration tests
# and therefore needs the same service prerequisites as the normal test jobs.
cargo shallguard coverage \
  --json requirement-coverage.json --markdown requirement-coverage.md

# Fast, focused local run. Both flags are repeatable.
cargo shallguard coverage \
  --package example-core --requirement REQ-CF-009 \
  --json requirement-coverage.json --markdown requirement-coverage.md
```

The coverage command requires `cargo-llvm-cov`. It resolves the selected tests
live, removes stale coverage build/profile data, and runs each exact test in an
isolated profile cycle. Only `.profraw` data is cleared between tests, so the
instrumented Cargo artifacts are reused. Each LLVM JSON export is hashed and
then projected only onto the requirements claimed by that test.

`#[enforces]` functions and methods map to their body; `enforces_here!` maps to
its smallest recoverable enclosing block. Const/static initializers are
runtime sites only when LLVM emits a region. Fields, variants, traits, types,
and other declarations are structural and excluded from executable-site
denominators. LLVM intersections use one-based line and column ranges, and
duplicate monomorphized regions collapse to one source region with the maximum
execution count.

The versioned `shallguard.requirement-coverage/v1` artifact records the
revision and dirty state, tool versions, build/target selection, exact test
result and normalized workspace-region export digest, requirement status,
reached/instrumented/total site counts, every syntactic scope, covered-region
counts, and the tests that reached it. Test and infrastructure failures make
the command fail after the artifacts are written. Partial/not-reached evidence
remains advisory.

The resolver combines syntax-derived file and inline-module paths, Cargo
metadata, target ownership, and each selected test harness's `--list` output.
It resolves library, binary, and integration-test targets. An exact syntactic
name wins; a function-name suffix is accepted only when the harness contains
one candidate. Missing targets, missing tests, and ambiguous suffixes are
findings, the JSON artifact is still written, and the command exits nonzero.

Live enumeration may emit a reusable catalog with `--catalog-output`. For
reproducible offline use, `--catalog <harness-tests.json>` accepts the
versioned `shallguard.test-harness-catalog/v1` schema. A catalog is rejected
when its commit or feature configuration differs from the current default-
feature workspace view. Catalog input and output require a clean working tree;
dirty local source must use live enumeration. The generated test index records
`HEAD`, working-tree state, selected packages, Cargo targets, and harness
source.

Test identity resolution has been exercised against both workspace shapes:
the example core library harness and example application's library plus `basic` and
`telemetry` integration harnesses. Coverage v1 has end-to-end fixtures for an
annotated router function and an `enforces_here!` branch block. Impact-driven
selection, changed-region/branch exercise, timeouts, and coverage CI wiring
remain follow-up work.

## 1. Purpose

Ordinary source coverage answers which compiled regions a test run executed.
Requirement coverage adds two relations already present in this workspace:

```text
verifying test -> requirement -> enforcement scope
```

By preserving per-test attribution, the system can answer:

- Did a test claiming `#[verifies("REQ-X")]` execute code that enforces
  `REQ-X`?
- Which enforcement sites did it reach?
- Did the requirement's tests execute the executable regions changed by this
  MR?
- Which relevant branches remain unexercised?

These are evidence questions. They do not prove that assertions fully express
the requirement.

## 2. Goals

- Attribute execution profiles to exact verification tests.
- Map LLVM source regions to syntactic enforcement scopes.
- Calculate requirement-level reach and changed-region exercise.
- Deduplicate tests that verify several requirements.
- Distinguish executable, structural, external, and pending evidence.
- Produce deterministic JSON and a concise human report.
- Run only evidence relevant to impacted requirements in MR mode.
- Support a later requirement-directed mutation-testing layer.

## 3. Non-goals

- Claiming that line or branch execution proves semantic correctness.
- Requiring every structural requirement to have runtime coverage.
- Replacing the existing `#[verifies]` and evidence-citation checks.
- Requiring 100 percent line or branch coverage for every enforcement item.
- Attributing a combined workspace profile to individual tests.
- Instrumenting production binaries.
- Treating a coverage-tool failure as a requirement violation.

## 4. Evidence dimensions

The report keeps the following dimensions separate:

| Dimension | Definition |
|-----------|------------|
| `trace` | Requirement has valid enforcement and verification links |
| `pass` | Selected verification tests completed successfully |
| `reach` | A verification test executed at least one region in an enforcement scope |
| `site_reach` | Enforcement scopes reached divided by executable scopes |
| `patch_exercise` | Changed executable regions reached by relevant verification tests |
| `branch_exercise` | Changed instrumented decisions exercised in relevant directions |
| `sensitivity` | Requirement-directed mutants detected by verification tests |

No single aggregate percentage combines these dimensions.

## 5. Inputs

- Head-revision requirement graph.
- `requirement-impact.json` in MR mode.
- Enforcement source scopes.
- Verification test identities.
- Cargo workspace metadata and selected feature configuration.
- LLVM source-based coverage export.
- Optional ordinary test results from another CI job.

The coverage artifact must record the exact source revision and build
configuration. Profiles from different revisions or incompatible builds must
not be merged.

## 6. Canonical test identity

File and function names are documentation-friendly but are not always enough to
invoke one Cargo test. The runtime identity is:

```rust
struct CargoTestIdentity {
    package: String,
    target_kind: TestTargetKind,
    target_name: String,
    fully_qualified_name: String,
}
```

Examples:

```text
package=example-core
target_kind=lib
target_name=example_core
fully_qualified_name=router::tasks::tests::config_manager_tests::test_...

package=example-app
target_kind=integration
target_name=basic
fully_qualified_name=test_auth_negative_positive
```

The resolver combines:

- the evidence file and test function from `Verified:`;
- the syntactic module path;
- Cargo target metadata;
- the test harness's enumerated test list.

Resolution must be exact. A suffix match is acceptable only when it produces one
candidate; ambiguity is a finding and prevents per-test coverage attribution.

The v1 artifact containing these identities is
`shallguard.requirement-test-index/v1`.

Parameterized/generated tests need a stable logical parent identity. Until that
is implemented, each generated harness test is treated separately or the
evidence is classified as suite-level rather than exact-test evidence.

## 7. Selecting tests

### 7.1 Full mode

Full mode selects every resolved, non-ignored `#[verifies]` test and produces a
workspace assurance baseline.

### 7.2 MR mode

MR mode selects the union of tests for requirements classified as:

- `direct`;
- `specification`;
- `evidence`;
- `anchor`;
- high-confidence `structural` or `transitive`.

Possible-impact tests may be selected by policy or reported without execution.
Tests are deduplicated before execution because one test may verify several
requirements.

### 7.3 Ordinary CI reuse

An ordinary combined test result may establish that the workspace test passed,
but it cannot establish per-test execution attribution. Requirement coverage
therefore needs its own individual profiles even when a normal test job already
ran.

## 8. Coverage execution

Rust source-based coverage instruments compiled functions and branches, writes
raw profiles during execution, and maps counters back to source regions. The
implementation may drive `cargo llvm-cov` or invoke the compiler and LLVM tools
directly; the exported LLVM JSON is the interchange input.

For each unique selected test:

1. Build the relevant target with coverage instrumentation.
2. Run exactly that test.
3. Write profile data to a unique path containing test and process identity.
4. Merge only the profiles belonging to that test invocation.
5. Export source-region and branch data as JSON.
6. Associate the profile with the canonical test identity.
7. Record pass, fail, timeout, or infrastructure error independently.

Build artifacts should be reused across tests when the coverage tool guarantees
compatible instrumentation. Test processes remain separate so their profiles
remain attributable.

## 9. Mapping coverage to requirements

### 9.1 Source normalization

LLVM and the source index must agree on canonical workspace-relative paths.
Resolve symlinks and build-directory prefixes once at ingestion. Generated and
macro-expansion paths are classified separately.

### 9.2 Region intersection

For a verification test `T` and requirement `R`:

```text
covered(T, R) = covered_regions(T) intersect executable_scopes(R)
```

For an MR change set `D`:

```text
patch_covered(T, R, D) = covered_regions(T)
                         intersect executable_scopes(R)
                         intersect changed_executable_regions(D)
```

Intersection uses line and column ranges, not line presence alone. A region
touching only the anchor attribute or macro invocation is not execution of the
enforced behavior.

### 9.3 Enforcement-scope classes

| Enforcement scope | Coverage treatment |
|-------------------|--------------------|
| Function/method body | Executable; region and branch mapping |
| Const/static initializer | Executable when LLVM emits a region; otherwise structural |
| Field declaration | Structural, not expected to execute |
| Enum variant declaration | Structural, not expected to execute |
| `enforces_here!` block | Executable statements in the enclosing block |
| Trait/type declaration | Structural unless a concrete executable child is anchored |

Structural scopes are excluded from runtime denominators. Their evidence remains
traceability, compilation, static checks, or review.

### 9.4 Macro complications

An anchor macro expands to no runtime code, so its own line cannot be used as a
coverage counter. The source index supplies the enclosing scope. For code
generated by other macros, coverage may point at invocation or expansion
locations depending on compiler mapping. Unmappable generated regions are
reported and excluded rather than guessed.

## 10. Requirement result

Each requirement receives a structured result:

```rust
enum CoverageStatus {
    Covered,
    PartiallyCovered,
    NotReached,
    StructuralOnly,
    NoExecutableEvidence,
    TestFailed,
    InfrastructureError,
}
```

In the current v1 artifact, `Covered` means all selected syntactically
executable enforcement scopes were instrumented and at least one claimed test
reached each scope. It does not mean the requirement was proven. MR selection
will later add explicit not-selected and unresolved-input records rather than
silently omitting them.

Site-level details remain available even when a policy rolls them into a summary
status.

## 11. Output schema

The primary artifact is `requirement-coverage.json`.

```json
{
  "schema": "shallguard.requirement-coverage/v1",
  "repository": "workspace",
  "head_commit": "fedcba9876543210",
  "working_tree_dirty": false,
  "rust_toolchain": "rustc 1.xx.x",
  "coverage_tool": "cargo-llvm-cov x.y.z",
  "tests": [
    {
      "identity": {
        "package": "example-core",
        "target_kind": "lib",
        "target_name": "example_core",
        "fully_qualified_name": "router::...::test_..."
      },
      "requirements": ["REQ-HRS-002"],
      "result": "passed",
      "export_digest": "sha256:..."
    }
  ],
  "requirements": [
    {
      "id": "REQ-HRS-002",
      "status": "partially_covered",
      "executable_sites": {
        "reached": 1,
        "instrumented": 2,
        "total": 2
      },
      "structural_sites": 0,
      "unmapped_sites": 0,
      "sites": [
        {
          "file": "example-core/src/router/tasks/config_manager.rs",
          "anchor_line": 1318,
          "scope_kind": "block",
          "scope": {
            "start_line": 1318,
            "start_column": 9,
            "end_line": 1324,
            "end_column": 10
          },
          "instrumented_regions": 7,
          "covered_regions": 6,
          "reached_by": ["example-core:lib:example_core:router::...::test_..."]
        }
      ]
    }
  ],
  "infrastructure_findings": []
}
```

`executable_sites` counts syntactic enforcement scopes. Per-site
`instrumented_regions` and `covered_regions` count deduplicated LLVM source
regions, not raw lines. The report names the unit whenever a number is shown.

## 12. Reporting

The MR summary should emphasize missing evidence, not celebrate percentages:

```text
REQ-HRS-002 - directly impacted
  tests:                 2 passed
  executable sites:      2/2 reached
  changed regions:       6/7 reached
  attention:             changed false branch not exercised
```

Do not rank requirements by one combined score. A safety requirement with one
unreached critical error path is more important than a large observational
requirement with high line coverage.

## 13. Gating policy

Initial MR coverage is advisory. Deterministic failures remain:

- selected cited test fails;
- exact evidence test cannot be resolved after it previously resolved;
- profile belongs to the wrong revision/build and would make the report
  misleading;
- the coverage artifact cannot be validated.

Not-reached and partial-patch findings are advisory until area-specific policy
and expected exceptions are defined. Structural requirements never fail for
lack of runtime coverage.

If an area later adopts hard requirement coverage, its policy must state:

- required enforcement-site classes;
- required test set;
- exclusions with rationale;
- branch expectations;
- timeout and infrastructure behavior.

## 14. Requirement-directed mutation testing

Coverage establishes reach, not assertion strength. A later mutation stage may
operate only on changed enforcement scopes and run only the owning requirement's
verification tests.

Candidate mutations include:

- invert a condition;
- remove an error return;
- remove a filter or validation call;
- change a bound or default;
- replace zero with a configured value;
- reorder validation and mutation;
- drop one match arm behavior.

A mutant is useful only when it compiles and represents a plausible violation.
Equivalent, uncompilable, and duplicate mutants are excluded from the
denominator.

Results remain advisory:

```text
REQ-HRS-002 sensitivity: 4/5 mutants killed
survivor: removal of unmanaged-domain filter did not fail cited tests
```

LLMs may propose requirement-specific mutations, but deterministic tooling must
materialize, compile, run, and classify them. Model claims alone are not mutation
evidence.

## 15. Runtime-marker alternative

The anchor macros could inject `requirement_hit(REQ_ID)` calls in a dedicated
instrumentation build. This would provide direct requirement-hit counters but is
deferred because it:

- changes instrumented code;
- is coarse for item-level anchors;
- cannot naturally execute fields and variants;
- complicates attribution across async tasks and concurrent tests;
- duplicates information available from source coverage and syntactic scopes.

Runtime markers should be reconsidered only if LLVM region mapping cannot
reliably cover important anchor forms.

## 16. CI and cost control

- Execute only unique tests for impacted requirements in MR mode.
- Group build preparation by Cargo target and feature set.
- Reuse instrumented artifacts but never merge attribution profiles across
  tests.
- Apply per-test and per-job timeouts.
- Cache profiles by source, test identity, toolchain, features, and test-input
  digest only when tests are deterministic enough for reuse.
- Allow safety areas to opt into coverage before lower-risk areas.
- Keep full-workspace coverage as a scheduled or manually triggered job if MR
  cost is excessive.

Application integration tests may require service dependencies. The coverage job
must reuse the same declared prerequisites as the normal test job rather than
silently skipping those tests.

## 17. Testing the coverage tool

Fixture tests must demonstrate:

- one test verifying multiple requirements;
- multiple tests verifying one requirement;
- per-test profile separation;
- reached and unreached anchored functions;
- `enforces_here!` block mapping;
- structural field/variant anchors;
- changed-region intersection;
- branch coverage;
- macro-generated code fallback;
- failed, timed-out, ignored, and unresolved tests;
- moved files and normalized paths;
- incompatible profile rejection.

Golden JSON tests pin schema and ordering. A small instrumented example crate
provides end-to-end validation independent of application dependencies.

## 18. References

- [Rust compiler instrumentation-based coverage](https://doc.rust-lang.org/rustc/instrument-coverage.html)
- [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov)
