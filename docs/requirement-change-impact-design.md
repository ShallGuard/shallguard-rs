# Requirement Change Impact Design

**Status:** Partially implemented (item/block-level plus one-hop syntax graph v1)

This document specifies how an MR is mapped to impacted requirements. It is a
component of [Requirement Assurance Design](requirement-assurance-design.md).

## Implementation checkpoint

The first deterministic slice is available through:

```bash
cargo shallguard impact \
  --base <merge-base> \
  --json requirement-impact.json \
  --markdown requirement-impact.md
```

If `--base` is omitted, the command uses
`CI_MERGE_REQUEST_DIFF_BASE_SHA`. Alternatively, `--target <branch>` computes
the merge base with `HEAD`. In a branch pipeline, `CI_DEFAULT_BRANCH` selects
`origin/<default-branch>` automatically. JSON defaults to stdout when
`--json` is omitted.

Implemented in this slice:

- merge-base validation and rename-aware changed-file discovery;
- working-tree and untracked-file comparison without checking out the base;
- whitespace-normalized requirement statement, Enforced, Verified, and
  retirement comparison;
- normalized Rust item comparison with comments and formatting excluded;
- item-level `direct`, `evidence`, `anchor`, `specification`, `structural`,
  `transitive`, and parse-fallback classification, with changed-line
  intersection restricting `enforces_here!` to its smallest enclosing block;
- base/head source indexing and one reverse-dependency hop from changed local
  functions, methods, types, constants, and statics to anchored enforcement
  scopes;
- conservative resolution of local and module-qualified paths, `Self::`
  associated calls, and type/constructor references, all reported with
  `possible` confidence; changed callables are `transitive`, while changed
  types, constants, and statics are `structural`;
- deterministic JSON and Markdown output;
- unclaimed runtime-item reporting;
- hard findings for post-bootstrap baseline additions and edits to requirements
  that retain baseline debt. The first baseline is a warning requiring review
  because the base revision has no policy file yet.

The artifact records `scope_precision: rust-item+anchor-block` and
`dependency_depth: 1`. General expression-level hunk mapping, field-level
structural identity, import/alias resolution, receiver-method and trait
dispatch, high-fan-out collapsing, and compiler-resolved dependency edges
remain follow-up work.

## 1. Goals

- Compare a merge base and head revision at Rust syntax-node granularity.
- Associate changed nodes with direct and possible transitive requirements.
- Detect changed requirements, enforcement sites, and verification tests.
- Report behavior-bearing changes with no requirement association.
- Produce deterministic JSON that can feed coverage and review jobs.
- Work without compilation, network access, or an LLM.

## 2. Non-goals

- Complete name resolution or call-graph construction using `syn`.
- Proving that a change violates or satisfies a requirement.
- Deciding whether an unclaimed change is necessarily incorrect.
- Replacing `git diff`, compiler checks, or semantic review.
- Treating formatting-only changes as behavioral impact.

## 3. Inputs

Required inputs:

- workspace root;
- merge-base commit;
- head source tree;
- requirement documents;
- syntactic enforcement and verification anchors.

Optional inputs:

- Cargo metadata;
- explicit feature and target selection;
- a precomputed dependency graph;
- ordinary test and static-check results.

In GitLab MR pipelines, the preferred base is
`CI_MERGE_REQUEST_DIFF_BASE_SHA`. A local caller may supply `--base <sha>`.
The tool must record the selected base and fail clearly if it cannot read it.

## 4. Data model

The examples below describe the intended schema; exact Rust ownership and
serialization details may differ.

```rust
struct SourceNode {
    id: NodeId,
    revision: Revision,
    kind: NodeKind,
    symbol: Option<SymbolId>,
    file: PathBuf,
    span: SourceSpan,
    parent: Option<NodeId>,
    syntax_hash: Digest,
}

struct EnforcementSite {
    requirement: RequirementId,
    node: NodeId,
    scope: SourceSpan,
    anchor_span: SourceSpan,
}

struct VerificationSite {
    requirement: RequirementId,
    node: NodeId,
    test: CargoTestIdentity,
    scope: SourceSpan,
}

struct Change {
    kind: ChangeKind,
    base_node: Option<NodeId>,
    head_node: Option<NodeId>,
    changed_spans: Vec<SourceSpan>,
    semantic: bool,
}
```

### 4.1 Node identity

Named items use an identity derived from:

```text
crate :: module path :: containing items :: item kind :: item name
```

Examples:

```text
example_core::router::tasks::config_manager::update_goal_weights
example_app::controller::Context::task_gc_enabled
```

Anonymous blocks and expressions use the nearest named ancestor plus a
structural path within its normalized syntax tree. Structural paths are only
used within a revision comparison and are not long-term public identifiers.

### 4.2 Syntax hashes

The syntax hash is computed from normalized tokens:

- discard whitespace and comments;
- retain literal values, identifiers, operators, attributes, and delimiters;
- normalize raw-identifier spelling only where semantically equivalent;
- retain macro token streams unchanged;
- exclude source coordinates.

Equal syntax hashes classify a moved or reformatted node as non-semantic. The
tool must still report moved anchors when documentation paths become stale.

## 5. Building revision indexes

### 5.1 Head index

The head index is built from workspace files on disk. Each Rust file is parsed
with `syn`, using span locations to index:

- modules and items;
- impl and trait members;
- fields and variants;
- blocks, statements, arms, and expressions needed for local impact;
- `#[enforces]`, `enforces_here!`, and `#[verifies]` anchors.

### 5.2 Base index

Base files are read without changing the worktree, for example through
`git show <base>:<path>`. Deleted and renamed files must remain indexable from
the base revision.

Only files relevant to the diff and anchor graph need full expression-level
indexes. Other source files may initially be indexed at item level.

### 5.3 Parse failure

If either revision of a changed Rust file cannot be parsed:

- record a deterministic finding;
- fall back to file-level impact for requirements anchored in that file;
- mark all conclusions for the file as conservative;
- never silently treat the file as unimpacted.

## 6. Diff processing

Obtain a rename-aware, zero-context diff between merge base and head. Preserve:

- added and deleted line ranges;
- old and new paths;
- rename similarity information;
- binary/generated classification;
- requirement-document changes.

Each hunk is mapped to the smallest intersecting syntax node in the appropriate
revision:

- deletions map against the base index;
- additions map against the head index;
- replacements map against both;
- file deletion impacts every base anchor in the file;
- file addition is inspected for new anchors and unclaimed code.

A change is formatting/comment-only when the matched before and after nodes
have the same normalized syntax hash and no requirement text or anchor changed.

## 7. Anchor scopes

Direct impact is based on source-scope intersection:

- an item anchor owns the complete item;
- a field or variant anchor owns that declaration;
- `enforces_here!` owns its smallest enclosing executable block;
- a verification anchor owns the complete test function.

If an `enforces_here!` invocation is not the first executable statement in its
block, report an advisory placement finding. The impact engine still uses the
block conservatively.

Changes to the anchor arguments themselves are always semantic for impact
purposes, even if the surrounding implementation is unchanged.

## 8. Impact classification

Every `(requirement, change)` relation has one primary class and a confidence.

| Class | Meaning | Initial confidence |
|-------|---------|--------------------|
| `specification` | The requirement statement or evidence changed | Certain |
| `direct` | A semantic change intersects an enforcement scope | Certain |
| `evidence` | A cited or anchored verification test changed | Certain |
| `anchor` | An anchor was added, removed, moved, or retargeted | Certain |
| `structural` | A referenced type, variant, constant, or static changed | Possible |
| `transitive` | A callable dependency of an enforcement site changed | Possible |
| `file_fallback` | Parsing failed; file-level association was used | Possible |

Impact is intentionally conservative. False positives are preferable to
silently omitting a safety requirement from review.

## 9. Dependency propagation

### 9.1 Initial syntactic propagation

The v1 implementation derives conservative edges from:

- direct function and associated-function call syntax;
- references to local constants, statics, types, constructors, and variants;
- module-qualified paths;
- associated constructor calls within the same crate.

Explicit related-requirement composition remains a graph-layer follow-up.

Every syntax-derived edge is `possible`, not a resolved fact. An unqualified
name is accepted only when it resolves in the current module. Imports, aliases,
receiver-method calls, trait dispatch, and dynamic dispatch are not resolved;
an associated call with an explicit type or `Self::` is resolved syntactically.

Limit propagation to one reverse-dependency hop initially. Longer propagation
quickly makes common helpers mark most requirements as impacted.

### 9.2 Compiler-backed propagation

A later HIR-based index may add resolved `DefId` edges, active `cfg` knowledge,
types, and trait implementations. Such edges must record toolchain and feature
configuration because they describe one compilation view, not all possible
builds.

### 9.3 Shared infrastructure

Changes to very high-fan-out nodes are reported as shared-infrastructure impact
rather than emitting an unbounded requirement list. The report includes:

- total downstream requirements;
- affected areas;
- the highest-risk or directly anchored subset;
- a machine-readable complete list in the artifact.

## 10. Unclaimed changes

An unclaimed change is a semantic Rust change for which no direct, structural,
or transitive requirement association is found.

Initial exclusions:

- tests and test fixtures;
- formatting and comments;
- generated code;
- the internal traceability tooling itself;
- build metadata with no runtime effect.

Initial higher-priority categories:

- changed public or crate-visible function behavior;
- changed branching, matching, error propagation, or return expressions;
- changed configuration fields/defaults;
- changed protocol or database types;
- changed metrics, logging, retry, timeout, or safety constants;
- unsafe or concurrency-related code.

Unclaimed does not mean invalid. It means a reviewer must decide whether the
change is implementation-only, belongs to an existing requirement, or needs a
new/updated requirement.

## 11. Requirement-document changes

The impact engine compares normalized requirement chunks between base and head.
It distinguishes:

- normative-statement change;
- enforcement-reference change;
- evidence-reference or evidence-class change;
- retirement;
- formatting-only change.

A normative change automatically impacts all enforcement and verification sites
for that requirement. Adding or modifying a requirement without the required
head-revision anchors remains a deterministic `shallguard` error.

## 12. Output schema

The primary artifact is `requirement-impact.json`.

```json
{
  "schema": "shallguard.requirement-impact/v1",
  "repository": "workspace",
  "base_commit": "0123456789abcdef",
  "head_commit": "fedcba9876543210",
  "configuration": {
    "features": [],
    "targets": ["workspace"],
    "dependency_depth": 1
  },
  "requirements": [
    {
      "id": "REQ-HRS-002",
      "area": "HRS",
      "impact": [
        {
          "class": "direct",
          "confidence": "certain",
          "change_id": "change-17",
          "reason": "changed region intersects enforcement scope",
          "site": {
            "file": "example-core/src/router/tasks/config_manager.rs",
            "symbol": "update_goal_weights_with_scope",
            "line": 1306
          }
        }
      ]
    }
  ],
  "unclaimed_changes": [],
  "findings": []
}
```

The companion Markdown summary is concise and links to source locations. JSON
contains the complete data and is the input to later stages.

## 13. Exit policy

Impact analysis fails only when it cannot produce a trustworthy artifact, for
example:

- invalid base revision;
- corrupt diff;
- requirement graph failed to load;
- unsupported artifact schema requested;
- output cannot be written.

Impacted requirements, possible propagation, and unclaimed changes are data,
not process failures. Policy may separately choose to gate on selected findings.

## 14. Performance and caching

- Parse only changed files and files needed for anchors/dependencies at full
  expression depth.
- Cache file indexes by `(blob hash, parser version)`.
- Cache requirement chunks by document blob hash.
- Use Git blob hashes to reuse results across rebases.
- Bound dependency propagation depth and fan-out.
- Keep JSON deterministic so artifact digests are stable.

## 15. Testing strategy

Unit tests cover:

- span-to-node mapping;
- normalization and syntax hashing;
- anchor-scope association;
- requirement-chunk comparison;
- each impact class;
- unclaimed-change classification;
- parse-failure fallback.

Repository-fixture tests cover:

- function modification;
- branch modification under `enforces_here!`;
- moved/renamed files and items;
- deleted anchors;
- changed verification tests;
- changed requirements;
- helper propagation;
- formatting-only diffs;
- active and inactive `cfg` code;
- macro-heavy code.

Golden JSON tests pin the public artifact schema and ordering.

## 16. Known limitations

- `syn` observes source syntax before macro expansion and without type
  resolution.
- The initial call graph is incomplete and may contain false edges.
- `cfg` branches are all visible syntactically even when not built.
- Cross-repository enforcement sites require separate indexes and trust rules.
- Non-Rust configuration, SQL, protobuf, and deployment changes need dedicated
  parsers before they can receive node-level impact analysis.
- A direct impact finding establishes review scope, not a defect.
