# Requirement Change Impact Design

**Status:** Partially implemented (item/block-level plus one-hop syntax graph v1)

This document describes how ShallGuard maps a merge request to the
requirements that the merge request affects. A merge request (MR) is a
proposed change that a reviewer examines before it is merged. A requirement is
a stable `REQ-<AREA>-<NNN>` statement in a selected Markdown document. An
impact is a relationship between a Git change and a requirement. This document
is a component of
[Requirement Assurance Design](requirement-assurance-design.md).

An anchor is a mark in Rust code that links the code to a requirement. An
enforcement anchor is `#[shallguard::enforces]` or
`shallguard::enforces_here!`. It links Rust behavior to a requirement. A
verification anchor is `#[shallguard::verifies]` on an enabled Rust test. It
gives evidence for a requirement.

## Implementation checkpoint

The first deterministic part of this design is available through this
command. Deterministic means that the same input always gives the same
output:

```bash
cargo shallguard impact \
  --base <merge-base> \
  --json requirement-impact.json \
  --markdown requirement-impact.md
```

The merge base is the last commit that the MR branch and the target branch
have in common. If you omit `--base`, the command uses the value of
`CI_MERGE_REQUEST_DIFF_BASE_SHA`. As an alternative, `--target <branch>`
computes the merge base of that branch and `HEAD`. A pipeline is the set of
jobs that continuous integration (CI) runs for one commit. In a branch
pipeline, `CI_DEFAULT_BRANCH` selects `origin/<default-branch>`
automatically. If you omit `--json`, the command writes the JSON to the
standard output.

This part implements these functions:

- The tool validates the merge base. The tool finds the changed files and
  recognizes renamed files.
- The tool compares the working tree and the untracked files with the base.
  The tool does not check out the base. The working tree is the set of files
  on disk. An untracked file is a file that Git does not track yet.
- The tool compares the requirement statement, the Enforced field, the
  Verified field, and the retirement of each requirement. The comparison
  ignores whitespace differences.
- The tool compares each Rust item in a normalized form. The comparison
  ignores comments and formatting. A Rust item is a declaration, for example a
  function, a type, or a constant.
- The tool classifies impact at item level into the classes `direct`,
  `evidence`, `anchor`, `specification`, `structural`, and `transitive`. The
  tool also has a fallback class for a parse failure. For
  `shallguard::enforces_here!`, the tool intersects the changed lines with the
  smallest block that encloses the anchor.
- The tool indexes the source of the base revision and the head revision.
  From each changed local function, method, type, constant, or static, the
  tool follows one reverse-dependency hop to the enforcement scopes that have
  anchors. A reverse-dependency hop goes from a changed item to one item that
  uses it.
- The tool resolves local paths, module-qualified paths, `Self::` associated
  calls, and references to types and constructors. This resolution is
  conservative. The tool reports each of these results with the confidence
  `possible`. A changed callable gets the class `transitive`. A changed type,
  constant, or static gets the class `structural`.
- The tool writes deterministic JSON output and Markdown output.
- The tool reports runtime items that have no requirement association.
- The tool reports a hard finding when a change adds a baseline entry after
  the first baseline exists. A baseline is a repository-owned list of accepted
  historical traceability gaps. The tool also reports a hard finding when a
  change edits a requirement that still has baseline debt. The first baseline
  gets a warning that a reviewer must examine, because the base revision has
  no policy file yet.

The artifact is the output file of the command. The artifact records
`scope_precision: rust-item+anchor-block` and `dependency_depth: 1`. These
functions are follow-up work:

- general mapping of hunks at expression level;
- structural identity at field level;
- resolution of imports and aliases;
- receiver-method dispatch and trait dispatch;
- collapse of nodes with a high fan-out;
- dependency edges that the compiler resolves.

## 1. Goals

- Compare the merge base and the head revision at the level of Rust syntax
  nodes. A syntax node is one element of the parsed source, for example a
  function, a block, or an expression.
- Associate each changed node with its direct requirements and its possible
  transitive requirements.
- Detect changed requirements, changed enforcement sites, and changed
  verification tests.
- Report each change that affects behavior and has no requirement
  association.
- Produce deterministic JSON that coverage jobs and review jobs can read.
- Work without compilation, without network access, and without a large
  language model (LLM).

## 2. Non-goals

- The tool does not do complete name resolution with `syn`. The tool does not
  build a complete call graph with `syn`. The Rust library `syn` parses Rust
  source code.
- The tool does not prove that a change violates a requirement. The tool does
  not prove that a change satisfies a requirement.
- The tool does not decide whether an unclaimed change is incorrect.
- The tool does not replace `git diff`, the compiler checks, or the semantic
  review. A semantic review is an advisory judgment by a person or a model.
- The tool does not treat a formatting-only change as a behavioral impact.

## 3. Inputs

A Cargo workspace is a set of Rust crates that Cargo builds together. A crate
is one Rust package. The tool needs these inputs:

- the root of the workspace;
- the merge-base commit;
- the head source tree;
- the requirement documents;
- the syntactic enforcement anchors and verification anchors.

The tool can also use these optional inputs:

- the Cargo metadata;
- an explicit selection of features and targets;
- a precomputed dependency graph;
- the results of ordinary tests and static checks.

In a GitLab MR pipeline, the preferred base is
`CI_MERGE_REQUEST_DIFF_BASE_SHA`. A local caller can supply `--base <sha>`.
The tool must record the selected base. If the tool cannot read the base, the
tool must fail with a clear error.

## 4. Data model

The examples below describe the intended schema. The exact Rust ownership and
the serialization details can differ.

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

A named item gets an identity that the tool derives from these parts:

```text
crate :: module path :: containing items :: item kind :: item name
```

Two examples follow:

```text
example_core::router::tasks::config_manager::update_goal_weights
example_app::controller::Context::task_gc_enabled
```

An anonymous block or expression uses the nearest named ancestor plus a
structural path. The structural path locates the node inside the normalized
syntax tree of that ancestor. The tool uses structural paths only inside one
revision comparison. Structural paths are not long-term public identifiers.

### 4.2 Syntax hashes

A syntax hash is a digest of the tokens of a node. The tool computes the
syntax hash from normalized tokens as follows:

- The tool discards whitespace and comments.
- The tool keeps literal values, identifiers, operators, attributes, and
  delimiters.
- The tool normalizes the spelling of a raw identifier only when the meaning
  stays the same.
- The tool keeps macro token streams unchanged.
- The tool excludes source coordinates.

If the syntax hashes are equal, the tool classifies a moved or reformatted
node as non-semantic. The tool must still report a moved anchor when a
documentation path becomes stale.

## 5. Building revision indexes

### 5.1 Head index

The tool builds the head index from the workspace files on disk. The tool
parses each Rust file with `syn`. The tool uses the span locations to index
these elements:

- modules and items;
- the members of impl blocks and traits;
- fields and variants;
- the blocks, statements, match arms, and expressions that local impact needs;
- `#[shallguard::enforces]`, `shallguard::enforces_here!`, and `#[shallguard::verifies]` anchors.

### 5.2 Base index

The tool reads the base files without a change to the worktree. For example,
the tool can use `git show <base>:<path>`. Deleted files and renamed files must
stay indexable from the base revision.

Only the files that the diff and the anchor graph use need a full index at
expression level. At first, the tool can index the other source files at item
level.

### 5.3 Parse failure

If the tool cannot parse the base revision or the head revision of a changed
Rust file, the tool must do these steps:

- record a deterministic finding;
- use file-level impact for the requirements that have anchors in that file;
- mark all conclusions for the file as conservative;
- never treat the file as unimpacted without a finding.

## 6. Diff processing

The tool obtains a diff between the merge base and the head. The diff
recognizes renamed files and has zero context lines. The tool keeps this
information:

- the added and deleted line ranges;
- the old and new paths;
- the rename similarity information;
- the classification of binary files and generated files;
- the changes to requirement documents.

A hunk is one changed region of a diff. The tool maps each hunk to the
smallest syntax node that intersects it in the applicable revision:

- The tool maps a deletion against the base index.
- The tool maps an addition against the head index.
- The tool maps a replacement against both indexes.
- A file deletion impacts every base anchor in the file.
- The tool inspects a file addition for new anchors and unclaimed code.

A change is a formatting-only or comment-only change when two conditions are
true. The matched node before the change and the matched node after the
change have the same normalized syntax hash. No requirement text and no anchor
changed.

## 7. Anchor scopes

The tool finds direct impact from the intersection of a change with a source
scope. Each anchor owns one scope:

- an item anchor owns the complete item;
- a field anchor or a variant anchor owns that declaration;
- `shallguard::enforces_here!` owns the smallest executable block that
  encloses it;
- a verification anchor owns the complete test function.

If a `shallguard::enforces_here!` invocation is not the first executable
statement in its block, the tool reports an advisory placement finding. The
impact engine still uses the block as the conservative scope.

A change to the arguments of an anchor is always semantic for impact. This is
true also when the code around the anchor is unchanged.

## 8. Impact classification

Each `(requirement, change)` relation has one primary class and one
confidence.

| Class | Meaning | Initial confidence |
|-------|---------|--------------------|
| `specification` | The requirement statement or evidence changed | Certain |
| `direct` | A semantic change intersects an enforcement scope | Certain |
| `evidence` | A cited or anchored verification test changed | Certain |
| `anchor` | A change added, removed, moved, or retargeted an anchor | Certain |
| `structural` | A referenced type, variant, constant, or static changed | Possible |
| `transitive` | A callable dependency of an enforcement site changed | Possible |
| `file_fallback` | Parse failure. The tool used a file-level association | Possible |

Impact is conservative by design. A false positive is better than a safety
requirement that the review omits without a report.

## 9. Dependency propagation

### 9.1 Initial syntactic propagation

An edge is a dependency link between two items. The v1 implementation derives
conservative edges from these sources:

- the call syntax of direct functions and associated functions;
- references to local constants, statics, types, constructors, and variants;
- module-qualified paths;
- associated constructor calls inside the same crate.

The explicit composition of related requirements is follow-up work in the
graph layer.

Each edge from syntax has the confidence `possible`. Such an edge is not a
resolved fact. The tool accepts an unqualified name only when the name
resolves in the current module. The tool does not resolve imports, aliases,
receiver-method calls, trait dispatch, or dynamic dispatch. The tool resolves
an associated call with an explicit type or with `Self::` from the syntax.

At first, the tool limits the propagation to one reverse-dependency hop. With
a longer propagation, a common helper quickly marks most requirements as
impacted.

### 9.2 Compiler-backed propagation

HIR is the high-level intermediate representation of the Rust compiler. A
later index based on HIR can add resolved `DefId` edges, knowledge of the
active `cfg` flags, types, and trait implementations. Such edges must record
the toolchain and the feature configuration. The reason is that these edges
describe one compilation view and not all possible builds.

### 9.3 Shared infrastructure

A node with a very high fan-out has many dependents. The tool reports a
change to such a node as a shared-infrastructure impact. The tool does not
emit an unbounded list of requirements. The report includes these items:

- the total number of downstream requirements;
- the affected areas;
- the subset with the highest risk or with direct anchors;
- a complete machine-readable list in the artifact.

## 10. Unclaimed changes

An unclaimed change is a semantic Rust change that has no direct, structural,
or transitive association with a requirement.

The tool excludes these changes at first:

- tests and test fixtures;
- formatting and comments;
- generated code;
- the internal traceability tooling itself;
- build metadata with no runtime effect.

The tool gives these categories a higher priority at first:

- changed behavior of a public or crate-visible function;
- changed branching, matching, error propagation, or return expressions;
- changed configuration fields or defaults;
- changed protocol types or database types;
- changed metrics, logging, retry, timeout, or safety constants;
- unsafe code or code related to concurrency.

An unclaimed change is not an invalid change. A reviewer must decide which of
these statements is true:

- The change is implementation-only.
- The change belongs to an existing requirement.
- The change needs a new requirement or an updated requirement.

## 11. Requirement-document changes

A requirement chunk is the block of text for one requirement. The impact
engine compares the normalized requirement chunks of the base and the head.
The engine distinguishes these change types:

- a change of the normative statement;
- a change of an enforcement reference;
- a change of an evidence reference or an evidence class;
- a retirement;
- a formatting-only change.

A normative change automatically impacts all enforcement sites and
verification sites of that requirement. If a change adds or modifies a
requirement without the required anchors in the head revision, the result
stays a deterministic `shallguard` error.

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

The companion Markdown summary is short and links to the source locations.
The JSON contains the complete data. The JSON is the input to the later
stages.

## 13. Exit policy

The impact analysis fails only when it cannot produce a trustworthy artifact.
Examples of such a failure are:

- an invalid base revision;
- a corrupt diff;
- a requirement graph that failed to load;
- a request for an unsupported artifact schema;
- an output that the tool cannot write.

Impacted requirements, possible propagation, and unclaimed changes are data.
They are not process failures. A policy can gate on selected findings as a
separate decision.

## 14. Performance and caching

- Parse at full expression depth only the changed files and the files that
  anchors or dependencies need.
- Cache file indexes by `(blob hash, parser version)`. A blob hash is the Git
  identifier of the content of a file.
- Cache requirement chunks by the blob hash of the document.
- Use Git blob hashes to reuse results across rebases.
- Limit the depth and the fan-out of the dependency propagation.
- Keep the JSON deterministic so that the artifact digests are stable.

## 15. Testing strategy

Unit tests cover these functions:

- span-to-node mapping;
- normalization and syntax hashing;
- anchor-scope association;
- requirement-chunk comparison;
- each impact class;
- unclaimed-change classification;
- parse-failure fallback.

A repository fixture is a small test repository. Repository-fixture tests
cover these cases:

- function modification;
- branch modification under `shallguard::enforces_here!`;
- moved or renamed files and items;
- deleted anchors;
- changed verification tests;
- changed requirements;
- helper propagation;
- formatting-only diffs;
- active and inactive `cfg` code;
- macro-heavy code.

A golden test compares the output with a stored expected file. Golden JSON
tests pin the public artifact schema and the ordering.

## 16. Known limitations

- The parser `syn` sees the source syntax before macro expansion and without
  type resolution.
- The initial call graph is incomplete. It can contain false edges.
- All `cfg` branches are visible in the syntax, also when the build does not
  include them.
- An enforcement site in another repository needs separate indexes and trust
  rules.
- Changes to non-Rust configuration, SQL, protobuf, and deployment files need
  dedicated parsers. Without such parsers, they cannot get impact analysis at
  node level.
- A direct impact finding sets the scope of a review. It does not show a
  defect.
