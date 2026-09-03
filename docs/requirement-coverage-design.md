# Requirement Coverage Design

**Status:** Partially implemented (per-test enforcement reach v1)

This document describes how ShallGuard maps test execution and source-based
coverage onto requirements, enforcement scopes, and merge request changes.
Coverage is LLVM execution evidence. It shows which source regions a test
executed. LLVM is the compiler back end that the Rust compiler uses. A merge
request (MR) is a proposed change that a reviewer examines before it is
merged.

A requirement is a normative statement with a stable `REQ-<AREA>-<NNN>` ID in
a selected Markdown document. An enforcement anchor is
`#[shallguard::enforces]` or `shallguard::enforces_here!`. It links Rust
behavior to a requirement. An enforcement scope is the source code that an
enforcement anchor marks. A verification anchor is `#[shallguard::verifies]`
on an enabled Rust test. It gives evidence for a requirement. A verification
test is a test with a verification anchor.

ShallGuard works in a Cargo workspace. Cargo is the build tool for Rust. A
crate is one Rust package. A Cargo workspace is a set of crates that Cargo
builds together.

This document is a component of
[Requirement Assurance Design](requirement-assurance-design.md). It uses the
output of
[Requirement Change Impact](requirement-change-impact-design.md).

## Implementation checkpoint

Exact identity resolution finds the one Cargo test for each verification
anchor. This resolution is available with the following command:

```bash
cargo shallguard test-index --enumerate \
  --json requirement-tests.json --markdown requirement-tests.md \
  --catalog-output harness-tests.json

# Limit local enumeration to one package during local development.
cargo shallguard test-index --enumerate \
  --package example-core --json requirement-tests.json
```

Reach is execution evidence. It shows that a verification test executed at
least one region in an enforcement scope. Executable enforcement reach is
available with the following command:

```bash
# Run every resolved verification test. The set can include integration
# tests. The run then needs the same service prerequisites as the normal
# test jobs.
cargo shallguard coverage \
  --json requirement-coverage.json --markdown requirement-coverage.md

# Fast and focused local run. You can repeat both flags.
cargo shallguard coverage \
  --package example-core --requirement REQ-CF-009 \
  --json requirement-coverage.json --markdown requirement-coverage.md
```

The coverage command requires `cargo-llvm-cov`. `cargo-llvm-cov` is a Cargo
tool that collects LLVM coverage data. The command resolves the selected tests
live. It removes stale coverage build data and stale profile data. A profile
is the raw counter data that an instrumented test writes during execution. The
command then runs each exact test in an isolated profile cycle.

Between tests, the command clears only the `.profraw` data. Thus the command
reuses the instrumented Cargo artifacts. The command hashes each LLVM JSON
export. It then projects the export only onto the requirements that the test
claims.

A function or method with `#[shallguard::enforces]` maps to its body. A
`shallguard::enforces_here!` anchor maps to the smallest enclosing block that
the tool can recover. A const initializer or a static initializer is a runtime
site only when LLVM emits a region for it. Fields, variants, traits, types,
and other declarations are structural. The tool excludes structural sites from
the executable-site denominators. A denominator is the total count that the
tool divides a reached count by.

LLVM intersections use line ranges and column ranges that start at 1. The Rust
compiler can produce several copies of one generic function. LLVM then emits
duplicate regions for one source location. The tool collapses these duplicate
regions into one source region. That region keeps the maximum execution count.

The command writes a versioned artifact with the schema
`shallguard.requirement-coverage/v1`. An artifact is a file that the command
writes. The artifact records these items:

- the source revision and the dirty state of the working tree;
- the tool versions;
- the build selection and the target selection;
- the exact result of each test;
- the digest of the normalized export of workspace regions for each test;
- the status of each requirement;
- the counts of reached, instrumented, and total sites;
- every syntactic scope;
- the count of covered regions in each scope;
- the tests that reached each scope.

A working tree is dirty when it has changes that are not committed. A test
failure or an infrastructure failure makes the command fail. The command
writes the artifacts before it fails. Partial evidence and not-reached
evidence remain advisory. Advisory evidence does not make the command fail.

The resolver is the part of the tool that finds the exact identity of a test.
The resolver combines these inputs:

- file paths and inline-module paths that it derives from the syntax;
- Cargo metadata;
- target ownership;
- the `--list` output of each selected test harness.

A test harness is the compiled test binary for one Cargo target. The resolver
resolves library targets, binary targets, and integration-test targets. An
exact syntactic name wins. The resolver accepts a function-name suffix only
when the harness contains one candidate. A missing target, a missing test, and
an ambiguous suffix are findings. The command still writes the JSON artifact.
The command then exits with a nonzero code.

Live enumeration can write a reusable catalog with `--catalog-output`. For
reproducible offline use, `--catalog <harness-tests.json>` accepts a catalog
with the versioned schema `shallguard.test-harness-catalog/v1`. The tool
rejects a catalog when its commit differs from the current commit. The tool
also rejects a catalog when its feature configuration differs from the current
default-feature view of the workspace.

Catalog input and catalog output require a clean working tree. Dirty local
source must use live enumeration. The generated test index records `HEAD`, the
state of the working tree, the selected packages, the Cargo targets, and the
harness source. `HEAD` is the commit that Git has checked out. Git is the
version control system.

The repository exercises test identity resolution against both workspace
shapes. The first shape is the library harness of the example core crate. The
second shape is the library of the example application plus its `basic` and
`telemetry` integration harnesses. Coverage v1 has end-to-end fixtures for 2
cases. The first case is an annotated router function. The second case is a
`shallguard::enforces_here!` branch block. A fixture is a small prepared input
that a test uses.

These items remain follow-up work:

- impact-driven selection;
- changed-region exercise and branch exercise;
- timeouts;
- coverage wiring in continuous integration (CI).

CI is the automated system that builds and tests each change.

## 1. Purpose

Ordinary source coverage tells which compiled regions a test run executed.
Requirement coverage adds 2 relations that already exist in this workspace:

```text
verifying test -> requirement -> enforcement scope
```

The system keeps the attribution for each test. Thus the system can answer
these questions:

- Did a test with `#[shallguard::verifies("REQ-X")]` execute code that
  enforces `REQ-X`?
- Which enforcement sites did the test reach?
- Did the tests of the requirement execute the executable regions that this
  MR changed?
- Which relevant branches did no test exercise?

These questions are about evidence. The answers do not prove that the test
assertions fully express the requirement.

## 2. Goals

- Attribute execution profiles to exact verification tests.
- Map LLVM source regions to syntactic enforcement scopes.
- Calculate reach for each requirement and exercise of changed regions.
- Remove duplicate runs of a test that verifies several requirements.
- Distinguish executable, structural, external, and pending evidence.
- Produce deterministic JSON and a short report for people.
- In MR mode, run only the evidence that is relevant to impacted requirements.
- Support a later layer for requirement-directed mutation testing.

Mutation testing makes small changes to the source code. It then examines
whether the tests detect each change.

## 3. Non-goals

The design does not do these things:

- claim that line execution or branch execution proves semantic correctness;
- require every structural requirement to have runtime coverage;
- replace the existing `#[shallguard::verifies]` and evidence-citation checks;
- require 100 percent line coverage or branch coverage for every enforcement
  item;
- attribute a combined workspace profile to individual tests;
- instrument production binaries;
- treat a failure of the coverage tool as a requirement violation.

## 4. Evidence dimensions

The report keeps the following dimensions separate:

| Dimension | Definition |
|-----------|------------|
| `trace` | The requirement has valid enforcement links and valid verification links |
| `pass` | The selected verification tests completed successfully |
| `reach` | A verification test executed at least one region in an enforcement scope |
| `site_reach` | The count of reached enforcement scopes divided by the count of executable scopes |
| `patch_exercise` | The changed executable regions that relevant verification tests reached |
| `branch_exercise` | The changed instrumented decisions that tests exercised in the relevant directions |
| `sensitivity` | The requirement-directed mutants that verification tests detected |

No single aggregate percentage combines these dimensions.

## 5. Inputs

- The requirement graph at the head revision.
- The file `requirement-impact.json` in MR mode.
- The source scopes of the enforcement anchors.
- The identities of the verification tests.
- The Cargo workspace metadata and the selected feature configuration.
- The LLVM source-based coverage export.
- Optional ordinary test results from another CI job.

The coverage artifact must record the exact source revision and the build
configuration. The tool must not merge profiles from different revisions. The
tool must not merge profiles from incompatible builds.

## 6. Canonical test identity

File names and function names are easy to read in documentation. However,
they are not always sufficient to invoke one Cargo test. The runtime identity
is:

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

The resolver combines these inputs:

- the evidence file and the test function from `Verified:`;
- the syntactic module path;
- the Cargo target metadata;
- the enumerated test list of the test harness.

Resolution must be exact. A suffix match is acceptable only when it produces
one candidate. An ambiguous match is a finding. An ambiguous match prevents
coverage attribution for that test.

The v1 artifact that contains these identities has the schema
`shallguard.requirement-test-index/v1`.

Parameterized tests and generated tests need a stable logical parent identity.
This identity is not implemented yet. Until then, the tool either treats each
generated harness test separately or classifies the evidence as suite-level
evidence, not exact-test evidence.

## 7. Selecting tests

### 7.1 Full mode

Full mode selects every resolved `#[shallguard::verifies]` test that is not
ignored. Full mode produces an assurance baseline for the workspace.

### 7.2 MR mode

MR mode selects the union of the tests for requirements with these impact
classes:

- `direct`;
- `specification`;
- `evidence`;
- `anchor`;
- `structural` or `transitive` with high confidence.

A policy can select the tests for possible-impact requirements. Otherwise the
tool reports those tests without execution. One test can verify several
requirements. Thus the tool removes duplicate tests before execution.

### 7.3 Ordinary CI reuse

An ordinary combined test result can show that the workspace test run passed.
It cannot show which test executed which region. Thus requirement coverage
needs its own profile for each test. This is true even when a normal test job
already ran.

## 8. Coverage execution

Rust source-based coverage instruments compiled functions and branches. The
instrumented code writes raw profiles during execution. The tool then maps the
counters back to source regions. The implementation can drive
`cargo llvm-cov`. Or the implementation can invoke the compiler and the LLVM
tools directly. In both cases, the exported LLVM JSON is the interchange
input.

The tool does these steps for each unique selected test:

1. Build the relevant target with coverage instrumentation.
2. Run exactly that test.
3. Write the profile data to a unique path. The path contains the test
   identity and the process identity.
4. Merge only the profiles that belong to that test invocation.
5. Export the source-region data and the branch data as JSON.
6. Associate the profile with the canonical test identity.
7. Record the result independently. The result is pass, fail, timeout, or
   infrastructure error.

We recommend that the tool reuses build artifacts across tests when the
coverage tool guarantees compatible instrumentation. Test processes remain separate. Thus
each profile remains attributable to one test.

## 9. Mapping coverage to requirements

### 9.1 Source normalization

LLVM and the source index must agree on canonical paths that are relative to
the workspace. Resolve symbolic links and build-directory prefixes once at
ingestion. The tool classifies generated paths and macro-expansion paths
separately.

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

The intersection uses line ranges and column ranges. It does not use line
presence alone. A region can touch only the anchor attribute or only the macro
invocation. Such a region is not execution of the enforced behavior.

### 9.3 Enforcement-scope classes

| Enforcement scope | Coverage treatment |
|-------------------|--------------------|
| Function body or method body | Executable. The tool maps regions and branches. |
| Const initializer or static initializer | Executable when LLVM emits a region. Otherwise structural. |
| Field declaration | Structural. The tool does not expect it to execute. |
| Enum variant declaration | Structural. The tool does not expect it to execute. |
| `shallguard::enforces_here!` block | Executable statements in the enclosing block |
| Trait declaration or type declaration | Structural, unless an anchor marks a concrete executable child. |

The tool excludes structural scopes from the runtime denominators. The
evidence for a structural scope remains traceability, compilation, static
analysis, or review.

### 9.4 Macro complications

An anchor macro expands to no runtime code. Thus the tool cannot use the line
of the macro as a coverage counter. The source index supplies the enclosing
scope. Other macros generate code. For that code, coverage can point at the
invocation location or at the expansion location. The compiler mapping decides
which location. The tool reports and excludes generated regions that it cannot
map. The tool does not guess their location.

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

In the current v1 artifact, `Covered` has a precise meaning. The tool
instrumented all selected syntactically executable enforcement scopes. Also,
at least one test that claims the requirement reached each scope. `Covered`
does not mean that the requirement is proven. MR selection will later add
explicit records for not-selected items and for unresolved inputs. It will not
omit them silently.

Site-level details remain available. This is true even when a policy combines
them into a summary status.

## 11. Output schema

The primary artifact is `requirement-coverage.json`. This is an example:

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

`executable_sites` counts syntactic enforcement scopes. For each site,
`instrumented_regions` and `covered_regions` count deduplicated LLVM source
regions. They do not count raw lines. The report names the unit each time it
shows a number.

## 12. Reporting

We recommend that the MR summary puts the missing evidence first. High
percentages are not the important information. This is an example:

```text
REQ-HRS-002 - directly impacted
  tests:                 2 passed
  executable sites:      2/2 reached
  changed regions:       6/7 reached
  attention:             changed false branch not exercised
```

Do not rank requirements by one combined score. For example, a safety
requirement can have one unreached critical error path. That requirement is
more important than a large observational requirement with high line
coverage.

## 13. Gating policy

Initial MR coverage is advisory. These deterministic failures remain:

- a selected cited test fails;
- the tool cannot resolve an exact evidence test that it resolved before;
- a profile belongs to the wrong revision or the wrong build, and the report
  would then be misleading;
- the tool cannot validate the coverage artifact.

Not-reached findings and partial-patch findings are advisory. They stay
advisory until an area defines its own policy and its expected exceptions. A
structural requirement never fails because it has no runtime coverage.

An area can later adopt hard requirement coverage. Its policy must then state
these items:

- the required classes of enforcement sites;
- the required test set;
- the exclusions, with a rationale for each;
- the branch expectations;
- the timeout behavior and the infrastructure behavior.

## 14. Requirement-directed mutation testing

Coverage establishes reach. It does not establish the strength of the
assertions. A later mutation stage can operate only on changed enforcement
scopes. It can run only the verification tests of the requirement that owns
the scope.

The candidate mutations include these changes:

- invert a condition;
- remove an error return;
- remove a filter call or a validation call;
- change a bound or a default;
- replace zero with a configured value;
- swap the order of a validation and a state change;
- remove the behavior of one match arm.

A mutant is a copy of the source code with one mutation applied. A mutant is
useful only when it compiles and represents a plausible violation. The tool
excludes equivalent mutants, uncompilable mutants, and duplicate mutants from
the denominator.

The results remain advisory. This is an example:

```text
REQ-HRS-002 sensitivity: 4/5 mutants killed
survivor: removal of unmanaged-domain filter did not fail cited tests
```

A large language model (LLM) can propose requirement-specific mutations.
However, deterministic tooling must materialize, compile, run, and classify
the mutations. A claim from a model alone is not mutation evidence.

## 15. Runtime-marker alternative

The anchor macros can inject `requirement_hit(REQ_ID)` calls in a dedicated
instrumentation build. This approach gives direct counters for requirement
hits. The design defers this approach for these reasons:

- it changes the instrumented code;
- it is coarse for item-level anchors;
- it cannot execute fields and variants in a natural way;
- it complicates attribution across async tasks and concurrent tests;
- it duplicates information that source coverage and syntactic scopes
  already give.

Reconsider runtime markers only if LLVM region mapping cannot cover important
anchor forms in a reliable way.

## 16. CI and cost control

- In MR mode, execute only the unique tests for impacted requirements.
- Group build preparation by Cargo target and feature set.
- Reuse instrumented artifacts. Never merge attribution profiles across
  tests.
- Apply a timeout for each test and a timeout for each job.
- Cache profiles by source, test identity, toolchain, features, and
  test-input digest. Do this only when the tests are deterministic enough for
  reuse.
- Let safety areas adopt coverage before areas with lower risk.
- If the MR cost is excessive, keep full-workspace coverage as a scheduled
  job or a manually started job.

Application integration tests can require service dependencies. The coverage
job must reuse the same declared prerequisites as the normal test job. The
coverage job must not skip those tests silently.

## 17. Testing the coverage tool

Fixture tests must demonstrate these cases:

- one test that verifies multiple requirements;
- multiple tests that verify one requirement;
- profile separation for each test;
- reached and unreached anchored functions;
- `shallguard::enforces_here!` block mapping;
- structural anchors on fields and variants;
- changed-region intersection;
- branch coverage;
- fallback for macro-generated code;
- failed, timed-out, ignored, and unresolved tests;
- moved files and normalized paths;
- rejection of an incompatible profile.

Golden JSON tests pin the schema and the ordering. A golden test compares the
output with a stored expected file. A small instrumented example crate gives
end-to-end validation. This validation does not depend on application
dependencies.

## 18. References

- [Rust compiler instrumentation-based coverage](https://doc.rust-lang.org/rustc/instrument-coverage.html)
- [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov)
