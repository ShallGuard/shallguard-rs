# Requirement Assurance Tool: User Stories and Requirements

**Status:** Bootstrap specification for dogfooding and standalone extraction  
**Project:** ShallGuard  
**Component names:** `shallguard` deterministic core, `cargo shallguard` developer
CLI, and `shallguard` requirement anchors  
**Current implementation:** standalone ShallGuard repository  
**Target:** a repository-independent Rust requirement-assurance tool

This document specifies the requirement-assurance tool itself. It records the
behavior already implemented before extraction and the portable contracts to
be implemented in the standalone project. The intent is
to develop the next version under its own requirements: each implemented
requirement will gain enforcement and verification anchors before this document
is enrolled in the default traceability gate.

The tool does not claim to prove arbitrary natural-language requirements.
Traceability, impact, test execution, source coverage, static checks, and
semantic review are separate evidence dimensions and remain separately visible
in every report.

## Conventions

Every user story carries stakeholder intent followed by numbered system
requirements. A system requirement has this form:

> **REQ-\<AREA\>-\<NNN\>** — normative statement using RFC 2119 keywords
> (**SHALL**, **SHALL NOT**, or **MAY**), stated so that it is testable.
> *Enforced:* implementation reference or `not implemented` plan ·
> *Verified:* evidence (✅ automated test,  end-to-end validation,  code
> review only, or ⏳ pending)

IDs are stable, globally unique within the product, and never reused. Retired
requirements remain in the document and are marked retired. `not implemented`
means a normative vNext contract, not an exemption from eventual dogfooding.

The areas are:

| Area | Capability |
|---|---|
| `CLI` | Cargo subcommand and command-line behavior |
| `SPEC` | Requirement document grammar, linting, and formatting |
| `TRACE` | Source/test anchors and deterministic traceability |
| `BASE` | Historical-gap baseline and ratchet |
| `IMP` | Git change impact and dependency analysis |
| `TEST` | Cargo test identity resolution |
| `COV` | LLVM execution evidence |
| `CAP` | Per-requirement review capsules |
| `REV` | Local and CI semantic review |
| `STATIC` | Requirement-specific static checks |
| `PORT` | Standalone repository portability |
| `SEC` | Safety, trust boundaries, and data minimization |

## Architecture Preflight

### Artifact Classification

- **Type:** developer tool composed of a Cargo external subcommand, reusable
  deterministic library, and supporting procedural macros.
- **Owner/repository:** standalone ShallGuard project at
  `https://github.com/sigi64/shallguard`.
- **New artifact or extension:** extraction and generalization of an existing
  internal implementation.
- **User/operator/developer/service audience:** Rust developers, reviewers, CI
  jobs, and local coding agents; no service-to-service audience.
- **Runtime shape:** one-shot local or CI process that reads a Git checkout,
  invokes local build tools, writes auditable artifacts, and exits.

### Recommended Stack

- **Language/runtime:** stable Rust; no long-running async runtime required.
- **Frameworks/libraries:** `syn`/`proc-macro2` for syntax, Cargo metadata and
  test-harness protocols for build discovery, `serde` for versioned JSON/TOML,
  and SHA-256 for content identity. Keep Git, Cargo, LLVM, and model providers
  behind explicit adapters.
- **Interface:** `cargo shallguard`, a process-independent Rust library API, and
  versioned JSON artifacts. Human terminal output is presentation, not a stable
  machine API.
- **Packaging/container model:** installable Cargo binary plus publishable Rust
  library/macro crates; no runtime container or service image.
- **Testing model:** unit tests for parsers and policies, fixture-repository
  integration tests for Git/Cargo/LLVM behavior, provider protocol tests, and
  self-hosting checks against this specification.

### CI And Image Contract

- **CI guidance/source:** use a conventional Rust pipeline with formatting,
  linting, tests, documentation, and packaging checks.
- **Image names:** none.
- **Registry path:** no Docker registry path; Rust crate registry/publication
  scope remains an approval item.
- **Branch/dev image behavior:** not applicable.
- **Release/prod image behavior:** not applicable.
- **Versioning/tagging expectations:** Cargo manifest versions express release
  intent; libraries and the binary SHALL use compatible semantic versions.
- **Non-standard CI requirements:** jobs exercising coverage need a compatible
  Rust LLVM tools component; optional provider smoke tests need explicitly
  provisioned provider CLIs and SHALL NOT be deterministic merge gates.

### Dependency Contract

| Dependency | Owner | Protocol | Operations | Read/write | Credentials/config | Failure behavior |
|---|---|---|---|---|---|---|
| Git checkout | repository owner | Git CLI or library adapter | resolve revisions, merge base, diff, status, read objects | read-only | repository path and base selection | fail with the command and revision context |
| Cargo/rustc | Rust project/toolchain owner | Cargo metadata and test-harness CLI | discover packages/targets, build, enumerate and run tests | reads source; writes Cargo target artifacts | toolchain and feature/target configuration | preserve partial artifacts and report the failing identity |
| LLVM tools | Rust toolchain owner | profiling environment and `llvm-profdata`/`llvm-cov` | merge profiles and export source regions | writes isolated coverage work files | compatible toolchain component | mark execution evidence unavailable; never infer coverage |
| Requirement documents and Rust source | repository owner | Markdown plus Rust syntax | parse specifications and anchors | read-only except explicit `fmt` | repository configuration | reject malformed or ambiguous input |
| Codex/Claude-compatible provider CLI | provider/user | subprocess stdin/stdout JSON protocol | review one frozen capsule | provider process may write only its own external state; tool output is local | provider-managed authentication, optional model/local endpoint | record unavailable/invalid result without treating it as a semantic verdict |
| Local artifact/cache directory | developer or CI job | versioned JSON/Markdown files | checkpoint, resume, cache, publish | read/write within configured output roots | filesystem path | reject incompatible, corrupt, or path-escaping state |

### Data Strategy

- **Persistent state:** no database; durable state is limited to versioned
  artifacts, review checkpoints, and optional content-addressed cache entries.
- **Database ownership:** none.
- **Migration approach:** artifact schemas carry versions and readers either
  migrate explicitly or reject unsupported versions.
- **Cross-service DB access:** none.
- **Data consistency/replay requirements:** every reusable result is bound to
  source revisions, configuration, capsule content, provider identity, and
  schema version; resume/cache entries are revalidated before reuse.

### Operations

- **Config and secrets:** repository-local non-secret configuration; provider
  authentication remains owned by the provider CLI and is never copied into an
  artifact.
- **Service discovery:** not applicable.
- **Health checks:** not applicable; commands expose exit status and artifact
  completeness.
- **Metrics/logging/tracing:** concise progress on stderr and deterministic
  results on stdout/files; optional timing data in artifacts, no service
  telemetry endpoint.
- **Deployment target:** developer machines and CI runners.

### Documentation And Handoff

- **Existing docs read:** the original requirement-traceability guide and the
  requirement-assurance design series.
- **Docs to update before implementation:** standalone `README.md`,
  `AGENTS.md`/`CLAUDE.md`, `docs/USER_DOC.md`, `docs/TECHNICAL_DOC.md`,
  `docs/GLOSSARY.md`, and this specification.
- **Catalog updates:** none required for this standalone public tool.
- **Follow-up guidance:** no deployment or database onboarding is required.
- **Open questions or approval needed:** final repository/crate names and
  owner; internal-only versus public distribution and license; supported Rust
  versions and host platforms; configuration filename/schema; initial artifact
  compatibility commitment; whether CLI parsing remains manual or adopts a
  dedicated argument parser.

## Enrollment and Ratchet Policy

This document is selected by the repository-owned `shallguard.toml`. Every
implemented requirement is anchored, every automated citation resolves to its
exact test, the committed baseline is empty, and every area is hardened.
Requirements explicitly marked `not implemented` remain pending without false
anchors. Every subsequent behavior change must update its requirement,
enforcement anchor, and honest evidence together; the deterministic CI gate
rejects any traceability regression.

## CLI User Stories

### US-CLI-001: One Requirement-Assurance Command

**Status:** Implemented

**As a** Rust developer or CI author  
**I want** one discoverable Cargo subcommand for requirement assurance  
**So that** the same workflow is reproducible locally and in merge requests

**System Requirements:**

- **REQ-CLI-001** — The executable SHALL support Cargo external-subcommand
  invocation as `cargo shallguard` and direct binary invocation without changing
  command semantics, and its help output SHALL summarize the basic anchored
  development workflow and change-review workflow. *Enforced:*
  `cli:src/main.rs` (`normalized_args`), `cli:src/cli_help.rs` (`print`) · *Verified:* ✅
  `cli:src/cli_tests.rs` (`removes_cargo_external_subcommand_argument`),
  `cli:tests/external_subcommand.rs`
  (`installed_informational_commands_work_without_repository`)
- **REQ-CLI-002** — With no command, `cargo shallguard` SHALL run the deterministic
  traceability check, while `cargo shallguard review` SHALL default to Codex,
  the configured target branch, executable coverage, and configured artifact
  paths. *Enforced:* `cli:src/main.rs` (`main`, `run_review`),
  `cli:src/cli_review.rs` (`parse_review_args`) · *Verified:* ✅
  `cli:src/cli_tests.rs`
  (`leaves_repository_review_defaults_for_configuration`)
- **REQ-CLI-003** — Long-running coverage and review commands SHALL report the
  active requirement ID, concise requirement description, position, and
  elapsed time; interactive status MAY update one terminal line while
  redirected output SHALL retain durable progress heartbeats. Interactive
  terminal output SHALL use color for semantic outcomes and stored-review
  sections, labels, statuses, severities, identifiers, and paths when
  supported, while CI, redirected output, `NO_COLOR`, and `TERM=dumb` SHALL
  remain ANSI-free. *Enforced:* `cli:src/cli_color.rs`,
  `cli:src/cli_review_show.rs`, `cli:src/cli_progress.rs`,
  `src/review_progress.rs`,
  `src/review_workflow.rs` · *Verified:* ✅ `src/review_progress.rs`
  (`provider_status_includes_position_description_and_elapsed_time`),
  `cli:src/cli_color.rs` (`color_requires_an_interactive_non_ci_terminal`),
  `cli:src/cli_review_show_tests.rs`
  (`colors_review_sections_only_when_enabled`),
  `src/review_workflow.rs`
  (`coverage_requirement_progress_is_sorted_one_per_line_with_descriptions`)
- **REQ-CLI-004** — Generated bundles, coverage work, and local review output
  SHALL default beneath the configured artifact root and SHALL permit explicit
  output overrides. *Enforced:* `src/config.rs` (`RepositoryConfig`),
  `cli:src/main.rs` (`run_bundle`, `run_coverage`, `run_review`) ·
  *Verified:* ✅ `src/config.rs`
  (`loads_single_package_repository_configuration`),
  `cli:src/cli_tests.rs` (`leaves_bundle_output_for_repository_configuration`,
  `parses_local_review_options`)
- **REQ-CLI-005** — Machine-readable artifacts SHALL use versioned schemas and
  explicit paths or stdout, while terminal prose SHALL remain a human interface
  with no compatibility guarantee. *Enforced:* `src/impact.rs`,
  `src/test_index.rs`, `src/coverage.rs`, `src/bundle.rs`, `src/review.rs` ·
  *Verified:* ✅ `src/impact.rs`
  (`json_artifact_uses_versioned_schema_and_configuration`)

### US-CLI-002: Discover the Installed Version

**Status:** Implemented

**As a** ShallGuard user or support engineer  
**I want** a command that prints the installed CLI version  
**So that** I can identify the exact release without needing a configured repository

**System Requirements:**

- **REQ-CLI-006** — `cargo shallguard version` and `cargo shallguard --version`
  SHALL print `cargo-shallguard <version>` using the binary package version and
  exit successfully, and the `version`, `--version`, and `help` forms SHALL NOT
  require ShallGuard repository discovery or configuration. *Enforced:*
  `cli:src/main.rs` (`main`) · *Verified:* ✅
  `cli:tests/external_subcommand.rs`
  (`installed_informational_commands_work_without_repository`)

### US-CLI-003: Inspect a Stored Review Run

**Status:** Implemented

**As a** developer or reviewer  
**I want** to inspect a preserved local review from the CLI  
**So that** I can understand its verdicts and evidence without navigating artifact files manually

**System Requirements:**

- **REQ-CLI-007** — `cargo shallguard review show` SHALL read the existing local
  review at the configured review-output path, while `--output <directory>`
  SHALL select an explicit review-output directory, and SHALL print run status,
  provider, model, processed-response counts, separate semantic-verdict counts,
  unavailable-or-invalid count, and the artifact path in blocks separated by
  blank lines. *Enforced:*
  `src/review_show.rs` (`inspect_stored_review`),
  `cli:src/cli_review_show.rs` (`parse_review_show_args`,
  `render_stored_review`) · *Verified:* ✅ `src/review_show_tests.rs`
  (`reads_completed_current_attempt_and_preserves_the_artifact`),
  `cli:src/cli_review_show_tests.rs`
  (`renders_summary_and_requested_evidence_details`)
- **REQ-CLI-008** — Without a requirement filter, `review show` SHALL list every
  selected requirement with its verdict and confidence; positional `<REQ-ID>`
  operands and repeatable explicit `--requirement <REQ-ID>` filters SHALL
  instead print each requested requirement's clause reviews, findings with
  severity and citations, missing evidence, context limitations, failure
  information, and retained result path.
  *Enforced:* `cli:src/cli_review_show.rs` (`parse_review_show_args`,
  `render_stored_review`) · *Verified:* ✅ `src/review_show_tests.rs`
  (`reads_completed_current_attempt_and_preserves_the_artifact`,
  `partial_run_lists_pending_units_and_checks_requested_ids`),
  `cli:src/cli_review_show_tests.rs`
  (`parses_show_filters_and_rejects_run_options`,
  `treats_positional_requirement_ids_as_filters`,
  `renders_summary_and_requested_evidence_details`)
- **REQ-CLI-009** — `review show` SHALL support completed and partial runs,
  SHALL distinguish completed semantic verdicts from unavailable or invalid
  attempts, and SHALL use the manifest-selected current attempt rather than
  silently choosing an older attempt. *Enforced:* `src/review_show.rs`
  (`inspect_stored_review`), `cli:src/cli_review_show.rs`
  (`render_stored_review`) · *Verified:* ✅ `src/review_show_tests.rs`
  (`reads_completed_current_attempt_and_preserves_the_artifact`,
  `partial_run_lists_pending_units_and_checks_requested_ids`,
  `distinguishes_unavailable_and_invalid_attempts`)
- **REQ-CLI-010** — `review show` SHALL be strictly read-only, SHALL NOT invoke a
  model provider or any impact, coverage, or bundle stage, and SHALL validate
  artifact schema versions, manifest/result identity, digests, and contained
  paths before presenting stored content. *Enforced:* `src/review_show.rs`
  (`inspect_stored_review`), `cli:src/cli_review_show.rs` (`run`) · *Verified:*
  ✅ `src/review_show_tests.rs`
  (`reads_completed_current_attempt_and_preserves_the_artifact`,
  `rejects_tampered_result_digest`)
- **REQ-CLI-011** — Advisory `violated`, `insufficient_evidence`, and
  `not_impacted` verdicts SHALL NOT make `review show` fail; unreadable or
  invalid artifacts and absent requested requirement IDs SHALL return nonzero
  with actionable diagnostics, and `cargo shallguard review` SHALL direct users
  encountering an existing output directory to `review show` for inspection or
  `review --resume` for continuation. *Enforced:* `src/review_show.rs`
  (`inspect_stored_review`), `src/review_workflow.rs`
  (`require_new_review_output`), `cli:src/cli_review_show.rs`
  (`parse_review_show_args`, `run`, `render_stored_review`) · *Verified:* ✅
  `src/review_show_tests.rs`
  (`partial_run_lists_pending_units_and_checks_requested_ids`,
  `distinguishes_unavailable_and_invalid_attempts`,
  `rejects_tampered_result_digest`), `src/review_workflow.rs`
  (`existing_review_output_points_to_show_and_resume`),
  `cli:src/cli_review_show_tests.rs`
  (`parses_show_filters_and_rejects_run_options`,
  `treats_positional_requirement_ids_as_filters`,
  `renders_summary_and_requested_evidence_details`)

### US-CLI-004: Publish an Advisory Pull Request Review

**Status:** Implemented

**As a** pull request author or reviewer  
**I want** a bounded Copilot review summarized as Markdown on the pull request  
**So that** semantic feedback is easy to consume without replacing deterministic merge checks

**System Requirements:**

- **REQ-CLI-012** — `cargo shallguard review show --format markdown` SHALL
  render the same validated stored-review selection as the terminal format as
  deterministic, ANSI-free GitHub-flavored Markdown; it SHALL include a stable
  report marker, run metadata, progress, semantic-verdict counts, and selected
  requirement outcomes, SHALL include evidence details when requirement filters
  are supplied, and SHALL escape provider-controlled content so it cannot create
  active HTML or user mentions. *Enforced:*
  `cli:src/cli_review_show.rs` (`ReviewShowFormat`,
  `render_stored_review_markdown`) · *Verified:* ✅
  `cli:src/cli_review_show_tests.rs`
  (`parses_and_renders_markdown_without_terminal_sequences`,
  `markdown_details_escape_provider_controlled_content`)
- **REQ-REV-009** — Semantic review SHALL support a `copilot` provider adapter
  that submits the frozen prompt and response schema through standard input,
  runs the Copilot CLI non-interactively with tools, remote delegation, custom
  instructions, and color disabled, accepts an optional model, exposes only the
  provider environment allowlist including `COPILOT_` variables, and validates
  the response through the same strict schema and citation rules as other
  providers. *Enforced:* `src/review.rs` (`ReviewProvider`),
  `src/review_provider.rs` (`invoke_provider`, `command_spec`,
  `provider_environment_allowed`, `parse_provider_response`) · *Verified:* ✅
  `src/review_tests.rs` (`parses_provider_names`,
  `copilot_command_is_headless_and_toolless`,
  `provider_environment_excludes_unrelated_ci_secrets`,
  `extracts_copilot_structured_output`)
- **REQ-PORT-009** — The repository SHALL provide a separate GitHub Actions
  pull-request workflow that prepares deterministic impact and bundle artifacts
  without secrets, invokes Copilot only for same-repository pull requests from
  trusted base-revision code, uploads the Markdown report, and upserts an
  advisory pull-request comment; provider absence, semantic verdicts, or review
  failure SHALL NOT fail the workflow, while the existing deterministic
  `shallguard-dev fmt --check` and `shallguard-dev check` validation steps SHALL
  remain in the required Rust workflow. *Enforced:*
  `.github/workflows/shallguard-review.yml`, `.github/workflows/rust.yml` ·
  *Verified:* ✅ `cli:tests/github_advisory_workflow.rs`
  (`advisory_review_is_isolated_from_the_required_deterministic_gate`)

## Specification User Stories

### US-SPEC-001: Maintainable Requirement Documents

**Status:** Implemented

**As a** requirement author  
**I want** a strict but readable Markdown contract  
**So that** humans can maintain it and deterministic tools can interpret it

**System Requirements:**

- **REQ-SPEC-001** — An active requirement definition SHALL use the exact
  `REQ-<AREA>-<NNN>` header form, an em dash, and a statement containing
  `SHALL`, `SHALL NOT`, or `MAY`; requirement IDs SHALL be unique across every
  selected document. *Enforced:* `src/docs.rs` (`parse_text`),
  `src/requirement_format.rs` (`lint_block`), `src/check.rs` (`analyze`) ·
  *Verified:* ✅ `src/docs.rs` (`parses_requirements_and_segments`),
  `src/requirement_format.rs` (`rejects_missing_segments_and_evidence_status`)
- **REQ-SPEC-002** — Every active requirement SHALL contain exactly one
  enforcement segment followed by `·` and exactly one verification segment
  carrying at least one recognized evidence indicator. *Enforced:*
  `src/requirement_format.rs` (`lint_block`) · *Verified:* ✅
  `src/requirement_format.rs` (`rejects_missing_segments_and_evidence_status`)
- **REQ-SPEC-003** — Automated evidence SHALL cite a concrete Rust test file
  and SHOULD cite its test function; when a function is named, deterministic
  checking SHALL bind the claim to that exact path and function. *Enforced:*
  `src/docs.rs` (`Evidence`, `parse_chunk`), `src/check.rs` (`analyze`) ·
  *Verified:* ✅ `src/docs.rs` (`parses_requirements_and_segments`)
- **REQ-SPEC-004** — Retired requirement IDs SHALL remain reserved and MAY omit
  active enforcement and verification segments; retired IDs SHALL NOT satisfy
  live anchors. *Enforced:* `src/docs.rs` (`parse_chunk`),
  `src/requirement_format.rs` (`lint_block`), `src/check.rs` (`analyze`) ·
  *Verified:* ✅ `src/requirement_format.rs`
  (`permits_retired_requirements_without_evidence_segments`)
- **REQ-SPEC-005** — `cargo shallguard fmt` SHALL format only requirement list
  blocks, preserve surrounding Markdown, prove parsed semantic equivalence,
  write atomically, and refuse all writes when any selected document has a
  lint failure. *Enforced:* `src/requirement_format.rs` (`format`,
  `verify_semantic_equivalence`, `write_atomic`) · *Verified:* ✅
  `src/requirement_format.rs`
  (`formats_requirement_blocks_without_touching_surrounding_markdown`,
  `formatting_is_idempotent_and_semantically_equivalent`,
  `lint_failures_prevent_formatter_writes`)
- **REQ-SPEC-006** — `cargo shallguard fmt --check` and `cargo shallguard lint` SHALL
  perform non-mutating structural and canonical-format validation and SHALL
  return nonzero for malformed or non-canonical selected documents.
  *Enforced:* `cli:src/main.rs` (`parse_format_args`, `run_format`),
  `src/requirement_format.rs` (`check`) · *Verified:* ✅ `cli:src/cli_tests.rs`
  (`parses_requirement_format_modes_and_documents`,
  `rejects_unknown_requirement_format_flags`)

## Traceability User Stories

### US-TRACE-001: Honest Links from Requirements to Rust

**Status:** Implemented for the original repository shape

**As a** reviewer  
**I want** source and test references checked against Rust syntax  
**So that** comments, stale paths, or invented tests cannot masquerade as evidence

**System Requirements:**

- **REQ-TRACE-001** — Rust anchor discovery SHALL parse source with `syn` and
  SHALL ignore anchor-like text in comments and string literals. *Enforced:*
  `src/scan.rs` (`scan`, `walk_items`) · *Verified:* ✅ `src/scan_tests.rs`
  (`comments_are_never_anchors`, `anchor_text_inside_strings_is_invisible`)
- **REQ-TRACE-002** — `#[shallguard::enforces]` SHALL be recognized on
  supported Rust items, impl functions, struct fields, and enum variants, and
  the scanner SHALL retain each anchor's source scope and
  executable/structural kind.
  *Enforced:* `src/scan.rs` (`walk_items`, `collect_item_attrs`,
  `collect_fn_attrs`) · *Verified:* ✅ `src/scan_tests.rs`
  (`attribute_anchors_record_executable_and_structural_scopes`,
  `field_and_variant_attributes_are_anchors`,
  `enforces_attribute_on_items_and_impl_fns`)
- **REQ-TRACE-003** — `shallguard::enforces_here!("REQ-...")` SHALL be
  recognized in statement, item, match-arm, and nested macro positions and
  SHALL own the smallest enclosing executable block available to the syntax
  scanner.
  *Enforced:* `src/scan.rs` (`MacroVisitor`), `src/impact.rs`
  (`EnforcementCollector`) · *Verified:* ✅ `src/scan_tests.rs`
  (`enforces_here_macro_in_statement_and_item_position`,
  `enforces_here_nested_in_another_macro_body_is_found`), `src/impact.rs`
  (`branch_anchor_only_owns_its_enclosing_block`,
  `branch_anchor_without_braces_owns_its_match_arm`)
- **REQ-TRACE-004** — `#[shallguard::verifies]` SHALL count as automated
  evidence only on a syntactically recognized, non-ignored test function; an
  ordinary or ignored function carrying the attribute SHALL be reported
  invalid. *Enforced:*
  `src/scan.rs` (`collect_fn_attrs`) · *Verified:* ✅ `src/scan_tests.rs`
  (`verifies_attribute_needs_an_enabled_test`)
- **REQ-TRACE-005** — The checker SHALL fail for malformed documents,
  duplicate IDs, unknown live anchor IDs, or nonexistent cited Rust paths.
  *Enforced:* `src/check.rs` (`run`, `analyze`), `src/docs.rs` (`parse_doc`) ·
  *Verified:*  code review only
- **REQ-TRACE-006** — An implemented requirement SHALL have its exact ID on an
  enforcement anchor in every documented enforcement file, and an automated
  requirement SHALL resolve to a test carrying its exact verification anchor.
  *Enforced:* `src/check.rs` (`analyze`, `enforced_path_has_anchor`) ·
  *Verified:* ✅ `src/check_tests.rs`
  (`requires_an_anchor_in_every_documented_enforcement_file`)
- **REQ-TRACE-007** — Anchor relations SHALL be many-to-many: one site MAY
  claim multiple requirements and one requirement MAY have multiple
  enforcement or verification sites without losing individual site identity.
  *Enforced:* `src/scan.rs` (`Anchor`, `Anchors`), `src/check.rs` (`analyze`) ·
  *Verified:* ✅ `src/test_index_tests.rs`
  (`merges_repeated_attributes_on_one_test`)
- **REQ-TRACE-008** — The `shallguard` library SHALL expose enforcement,
  branch-enforcement, and verification anchors as `#[shallguard::enforces]`,
  `shallguard::enforces_here!`, and `#[shallguard::verifies]`, so consumers
  SHALL NOT need a direct dependency on the implementation macro crate.
  *Enforced:* `src/lib.rs` (`enforces`, `enforces_here`, `verifies`) ·
  *Verified:* ✅ `tests/public_anchor_api.rs`
  (`public_namespace_exposes_all_anchor_macros`)

### US-TRACE-002: Vacuity-Resistant Automated Evidence

**Status:** Implemented

**As a** maintainer  
**I want** ✅ evidence rejected when the cited test cannot fail  
**So that** a green traceability report cannot be earned by assertion-free
or constant tests

**System Requirements:**

- **REQ-TRACE-009** — A `#[verifies]` test body containing no failure path —
  no assertion macro, no `panic!`/`todo!`/`unreachable!` invocation, no
  `unwrap`/`expect`/`unwrap_err`/`expect_err` call, and no `?` operator in a
  `Result`-returning test — SHALL be reported as vacuous evidence.
  *Enforced:* `src/oracle.rs` (`classify`) · *Verified:* ✅ `src/oracle.rs`
  (`empty_and_assertion_free_bodies_are_vacuous`,
  `real_failure_paths_classify_as_present`,
  `question_mark_without_result_return_is_not_a_failure_path`,
  `failure_paths_inside_macro_arguments_are_seen`,
  `not_equals_comparisons_are_not_failure_paths`)
- **REQ-TRACE-010** — A constant assertion that provably always passes
  (`assert!(true)`, `assert_eq!(1, 1)`, `assert_ne!(0, 1)`) SHALL NOT count
  as a failure path; a constant assertion that always fails
  (`assert!(false)`, `assert_eq!(0, 1)`) SHALL count as an unconditional
  failure path. *Enforced:* `src/oracle.rs` (`assertion_is_trivial`) ·
  *Verified:* ✅ `src/oracle.rs` (`literal_only_assertions_are_trivial`,
  `always_failing_constant_asserts_are_failure_paths`)
- **REQ-TRACE-011** — An `assert_eq!` or `assert_ne!` SHALL count as
  vacuous only when both compared sides are literal; token-identical
  non-literal sides (impure calls, floating-point values) MAY fail at
  runtime and SHALL classify as evidence present. *Enforced:*
  `src/oracle.rs` (`assertion_is_trivial`) · *Verified:* ✅ `src/oracle.rs`
  (`identical_non_literal_sides_classify_as_present`)
- **REQ-TRACE-012** — `#[should_panic]` without an `expected` message on a
  `#[verifies]` test whose body offers no other failure path SHALL be
  reported as weak evidence. *Enforced:* `src/oracle.rs` (`classify`) ·
  *Verified:* ✅ `src/oracle.rs`
  (`bare_should_panic_is_weak_and_expected_is_present`),
  `src/check_evidence.rs` (`weak_anchors_are_reported_even_beside_solid_evidence`)
- **REQ-TRACE-013** — A requirement whose only ✅ citation resolves to a
  vacuous test SHALL be counted as lacking automated verification, and
  vacuous and weak findings SHALL flow through the ratcheted baseline as
  distinct gap kinds, so pre-existing cases in adopting repositories are
  grandfatherable and ratcheted; advisory weak-evidence findings SHALL NOT
  be recorded by baseline initialization. *Enforced:*
  `src/check_evidence.rs` (`evaluate_verification`), `src/check.rs`
  (`gap_is_hard`, `baseline_entries`), `src/baseline.rs` (`GapKind`) ·
  *Verified:* ✅ `src/check_evidence.rs`
  (`sole_vacuous_evidence_demotes_the_requirement`,
  `redundant_vacuous_evidence_keeps_the_requirement_anchored`),
  `src/check_tests.rs` (`vacuous_evidence_flows_through_the_baseline_like_other_kinds`,
  `weak_evidence_is_advisory_unless_strict_oracle`,
  `advisory_kinds_are_not_recorded_by_baseline_init`), `src/baseline.rs`
  (`evidence_gap_kinds_round_trip_through_baseline`)
- **REQ-TRACE-014** — An explicit `#[verifies("REQ-...", oracle = "<class>")]`
  opt-out SHALL suppress vacuity reporting for that test and SHALL be
  counted and listable in the check report; suppression SHALL NOT be
  silent. *Enforced:* `src/oracle.rs` (`classify`), `src/scan.rs`
  (`oracle_argument`), `src/check_report.rs` (`render_summary`) ·
  *Verified:* ✅ `src/oracle.rs` (`suppression_is_recorded_not_silent`),
  `src/check_report.rs` (`suppressed_oracles_are_listed_in_the_summary`),
  `src/scan_tests.rs` (`raw_string_oracle_classes_decode_to_their_value`)
- **REQ-TRACE-015** — Vacuity analysis SHALL be purely syntactic and
  deterministic, SHALL NOT execute tested code, and SHALL classify any
  construct the classifier does not fully understand as evidence present
  rather than vacuous. *Enforced:* `src/oracle.rs` (`classify`) ·
  *Verified:* ✅ `src/oracle.rs` (`unknown_constructs_classify_as_present`,
  `err_return_and_result_aliases_classify_as_present`,
  `third_party_assert_macros_classify_as_present`)
- **REQ-TRACE-016** — `#[verifies]` SHALL reject at compile time a test body
  containing no failure-path candidate tokens at all, or only constant
  `assert` -family invocations that provably always pass; the error SHALL
  name the offending requirement IDs and reference the evidence-honesty
  rules, and any body the token scan cannot fully classify SHALL compile —
  the deterministic check remains authoritative. *Enforced:* `src/lib.rs`
  (`verifies`) · *Verified:* ✅ `macros:tests/front_line.rs`
  (`front_line_rejects_vacuity_and_enforces_oracle_classes`)
- **REQ-TRACE-017** — The `oracle` opt-out SHALL accept only the closed
  value set `panic`, `compile`, and `external`: an unknown value SHALL be
  rejected at compile time with the accepted list, and the deterministic
  checker SHALL NOT treat an unknown class as a suppression and SHALL
  report it. *Enforced:* `src/lib.rs` (`verifies`), `src/scan.rs`
  (`collect_fn_attrs`, `oracle_argument`) · *Verified:* ✅
  `macros:tests/front_line.rs`
  (`front_line_rejects_vacuity_and_enforces_oracle_classes`),
  `src/scan_tests.rs` (`unknown_oracle_class_is_not_a_suppression`,
  `duplicate_and_non_string_oracle_values_are_invalid`), `src/oracle.rs`
  (`oracle_class_set_is_pinned`)

## Baseline and Ratchet User Stories

### US-BASE-001: Prevent New Debt without Rewriting History

**Status:** Implemented

**As a** maintainer adopting traceability incrementally  
**I want** existing gaps visible but frozen  
**So that** every new or modified requirement is complete and debt only decreases

**System Requirements:**

- **REQ-BASE-001** — A historical baseline entry SHALL identify only a
  requirement ID and gap kind and SHALL NOT contain a requirement-content
  fingerprint. *Enforced:* `src/baseline.rs` (`BaselineEntry`, `GapKey`) ·
  *Verified:* ✅ `src/baseline.rs` (`serialization_is_sorted_and_round_trips`)
- **REQ-BASE-002** — An exact historical gap MAY remain a visible warning, but
  any gap absent from the committed baseline SHALL be a hard regression.
  *Enforced:* `src/check.rs` (`apply_baseline`, `record_gap`) · *Verified:* ✅
  `src/check_tests.rs` (`exact_baseline_gap_is_known_warning`,
  `unbaselined_gap_is_a_regression`)
- **REQ-BASE-003** — Areas configured as fully hardened SHALL NOT accept
  baseline exceptions. *Enforced:* `src/check.rs` (`gap_is_hard`),
  `src/config.rs` (`RepositoryConfig::area_is_hard`) ·
  *Verified:* ✅ `src/check_tests.rs` (`hard_area_cannot_be_baselined`)
- **REQ-BASE-004** — A resolved or retired gap SHALL make its baseline entry
  stale and fail checking until `baseline prune` removes it; pruning SHALL
  remove only resolved entries. *Enforced:* `src/check.rs`
  (`apply_baseline`, `prune_baseline`) · *Verified:* ✅ `src/check_tests.rs`
  (`fixed_gap_makes_entry_stale`, `prune_mode_accepts_resolved_entry_for_removal`)
- **REQ-BASE-005** — Change impact SHALL reject manual baseline growth after
  initialization and SHALL reject modification of a requirement that still
  carries historical debt. *Enforced:* `src/impact.rs` (`compare_baseline`,
  `compare_requirement_documents`) · *Verified:* ✅ `src/impact.rs`
  (`changed_requirement_with_baseline_debt_is_policy_error`)

## Change Impact User Stories

### US-IMP-001: Requirement-Aware Merge Request Scope

**Status:** Implemented with conservative one-hop syntax dependencies

**As a** merge request reviewer  
**I want** changed Rust behavior mapped to affected requirements  
**So that** review and evidence focus on the contracts at risk

**System Requirements:**

- **REQ-IMP-001** — Impact analysis SHALL accept an exact base revision or the
  merge base of a target branch and SHALL compare it with the current working
  tree, including tracked modifications and deletions. *Enforced:*
  `src/impact.rs` (`analyze`, `resolve_revision`, `merge_base`,
  `changed_files`) · *Verified:*  code review only
- **REQ-IMP-002** — Git change parsing SHALL preserve non-UTF-8-safe field
  boundaries and rename source/destination identity by consuming NUL-terminated
  name-status output. *Enforced:* `src/impact.rs` (`parse_name_status`) ·
  *Verified:* ✅ `src/impact.rs`
  (`parses_nul_terminated_name_status_with_rename`)
- **REQ-IMP-003** — Rust item comparison SHALL use normalized syntax that
  ignores comments, formatting, documentation attributes, and traceability
  metadata while retaining behavior-bearing tokens. *Enforced:*
  `src/impact.rs` (`normalized_behavior_tokens`, `strip_item_docs`) ·
  *Verified:* ✅ `src/impact.rs`
  (`source_index_ignores_comments_but_finds_typed_anchors`,
  `behavior_tokens_exclude_trace_metadata`)
- **REQ-IMP-004** — A changed enforcement scope SHALL produce direct impact
  for its requirement, while a behavior-bearing changed Rust scope with no
  requirement association SHALL be reported as unclaimed. *Enforced:*
  `src/impact.rs` (`compare_scopes`, `report_as_unclaimed`) · *Verified:* ✅
  `src/impact.rs` (`changed_anchored_function_is_direct_impact`,
  `changed_unanchored_function_records_dependency_candidate`)
- **REQ-IMP-005** — Requirement document changes SHALL distinguish normative
  statement, enforcement evidence, and verification evidence changes and SHALL
  make the changed requirement directly impacted. *Enforced:* `src/impact.rs`
  (`compare_requirement_documents`, `requirement_change_reasons`) ·
  *Verified:* ✅ `src/impact.rs`
  (`requirement_change_classifies_each_segment`)
- **REQ-IMP-006** — The first dependency implementation SHALL propagate one
  conservative reverse syntax-dependency hop from changed local helpers,
  values, or types into anchored callers and SHALL label callable and
  structural impacts separately with non-certain confidence. *Enforced:*
  `src/impact_dependency.rs` (`analyze`, `propagate`) · *Verified:* ✅
  `src/impact_dependency_tests.rs`
  (`propagates_changed_helper_to_anchored_caller`,
  `classifies_changed_type_dependency_as_structural`)
- **REQ-IMP-007** — Impact output SHALL be emitted as a versioned artifact with
  base/head identity, configuration, impact class, reason, confidence, source
  location, unclaimed changes, and policy findings even when policy causes a
  nonzero exit. *Enforced:* `src/impact.rs` (`ImpactArtifact`), `cli:src/main.rs`
  (`run_impact`) · *Verified:* ✅ `src/impact.rs`
  (`json_artifact_uses_versioned_schema_and_configuration`)

## Test Identity User Stories

### US-TEST-001: Resolve Evidence to Executable Cargo Tests

**Status:** Implemented

**As a** coverage runner  
**I want** every verification anchor resolved to one exact Cargo identity  
**So that** the intended test, not a similarly named test, is executed

**System Requirements:**

- **REQ-TEST-001** — Test indexing SHALL use Cargo metadata to map each
  syntactic verification test to its owning package and library, binary, or
  integration-test target. *Enforced:* `src/test_index.rs` (`load_metadata`,
  `owning_package`, `select_target`) · *Verified:* ✅
  `src/test_index_tests.rs` (`maps_library_and_integration_source_targets`)
- **REQ-TEST-002** — Enumeration mode SHALL query each selected Cargo test
  harness using its list protocol and SHALL retain only executable test and
  benchmark identities. *Enforced:* `src/test_index.rs` (`enumerate_targets`,
  `enumerate_target`, `parse_harness_list`) · *Verified:* ✅
  `src/test_index_tests.rs` (`parses_only_tests_and_benchmarks_from_harness_output`)
- **REQ-TEST-003** — Resolution SHALL prefer an exact syntactic module/function
  name, MAY accept a unique function suffix, and SHALL report ambiguous or
  absent matches as deterministic findings. *Enforced:* `src/test_index.rs`
  (`resolve_candidate`) · *Verified:* ✅ `src/test_index_tests.rs`
  (`exact_syntactic_name_resolves_before_suffix_matching`,
  `unique_function_suffix_is_accepted`, `ambiguous_function_suffix_is_a_finding`)
- **REQ-TEST-004** — A resolved test identity SHALL include package, Cargo
  target kind/name, exact harness name, source path/function, and claimed
  requirement IDs. *Enforced:* `src/test_index.rs` (`CargoTestIdentity`,
  `IndexedVerificationTest`) · *Verified:*  code review only
- **REQ-TEST-005** — A reusable test catalog SHALL record source revision and
  working-tree state, and loading SHALL reject incompatible package filters or
  catalog identities rather than silently selecting another test. *Enforced:*
  `src/test_index.rs` (`HarnessCatalog`, `load_catalog`,
  `validate_package_filter`) · *Verified:* ✅ `src/test_index_tests.rs`
  (`validates_requested_package_names`)

## Executable Coverage User Stories

### US-COV-001: Requirement-Scoped Execution Evidence

**Status:** Implemented for LLVM source coverage; changed-region coverage planned

**As a** reviewer  
**I want** to know whether cited tests execute enforcement code  
**So that** a passing but irrelevant test is not mistaken for evidence

**System Requirements:**

- **REQ-COV-001** — Coverage collection SHALL select only exact resolved tests
  associated with requested automated requirements and SHALL list each selected
  requirement and test identity before execution. *Enforced:* `src/coverage.rs`
  (`generate`, `select_tests`), `src/review_workflow.rs`
  (`select_coverage_requirements`) · *Verified:* ✅
  `src/review_workflow.rs`
  (`coverage_selection_intersects_impact_automation_and_request`)
- **REQ-COV-002** — Selected tests SHALL run under Rust LLVM instrumentation
  with isolated raw profiles per exact test while reusing compatible build
  output across the run. *Enforced:* `src/coverage_llvm.rs` (`prepare`,
  `collect_test`, `clean_profiles`) · *Verified:*  code review only
- **REQ-COV-003** — LLVM export parsing SHALL retain workspace-local executable
  line/column regions and counts, deduplicate repeated instantiations, and
  reject unknown export forms or paths outside the repository. *Enforced:*
  `src/coverage_llvm.rs` (`parse_export`, `workspace_relative`) · *Verified:* ✅
  `src/coverage_llvm_tests.rs`
  (`parses_workspace_code_regions_and_deduplicates_instantiations`,
  `rejects_an_unknown_export_type`, `source_paths_must_stay_inside_the_workspace`)
- **REQ-COV-004** — Coverage mapping SHALL intersect executable LLVM regions
  with the source scope of each enforcement anchor using line and column
  boundaries. *Enforced:* `src/coverage.rs` (`enforcement_sites`,
  `apply_regions`, `ranges_overlap`) · *Verified:* ✅ `src/coverage_tests.rs`
  (`source_range_intersection_is_half_open`,
  `covered_llvm_regions_reach_the_owning_enforcement_scope`)
- **REQ-COV-005** — The artifact SHALL distinguish reached executable anchors,
  instrumented-but-unreached anchors, structural-only anchors, and execution
  errors; execution errors SHALL take precedence over a reach claim.
  *Enforced:* `src/coverage.rs` (`CoverageStatus`,
  `RequirementAccumulator::finish`) · *Verified:* ✅ `src/coverage_tests.rs`
  (`zero_count_region_is_instrumented_but_not_reached`,
  `declarations_are_structural_only`,
  `execution_errors_take_precedence_over_reach`)
- **REQ-COV-006** — Coverage JSON SHALL bind source revision, exact test
  identities, test outcomes, LLVM evidence, enforcement sites, and requirement
  status, and SHALL remain available when one or more selected tests fail.
  *Enforced:* `src/coverage.rs` (`CoverageArtifact`), `cli:src/main.rs`
  (`run_coverage`) · *Verified:*  code review only
- **REQ-COV-007** — A future patch-exercise result SHALL report whether cited
  tests execute changed executable regions inside impacted enforcement scopes
  and SHALL keep this result separate from whole-scope enforcement reach.
  *Enforced:* not implemented — changed-region coverage described in
  `docs/requirement-coverage-design.md` · *Verified:* ⏳ pending

## Review Capsule User Stories

### US-CAP-001: Deterministic, Bounded Review Input

**Status:** Implemented

**As a** human or model reviewer  
**I want** one complete but bounded capsule per impacted requirement  
**So that** review is focused, reproducible, and citable

**System Requirements:**

- **REQ-CAP-001** — Bundle generation SHALL produce one independently
  reviewable capsule for each selected impacted requirement and a manifest that
  maps requirement IDs to capsule files. *Enforced:* `src/bundle.rs`
  (`generate`, `BundleManifest`) · *Verified:*  code review only
- **REQ-CAP-002** — A capsule SHALL include the full normative statement and
  clauses, enforcement and verification declarations, impact reasons, related
  tests, available coverage, changed source, and current source for every
  enforcement anchor, including unchanged anchor heads. *Enforced:*
  `src/bundle.rs` (`build_capsule`, `enforcement_contexts`) · *Verified:* ✅
  `src/bundle.rs` (`extracts_normative_clauses_and_keeps_complete_segments`,
  `capsule_includes_unchanged_anchored_enforcement_source`)
- **REQ-CAP-003** — Every included source excerpt SHALL carry a repository path
  and line range suitable for citations; bounded or omitted context SHALL be
  reported explicitly through completeness metadata. *Enforced:*
  `src/bundle.rs` (`SourceExcerpt`, `ImplementationContext`,
  `EnforcementContext`) · *Verified:* ✅
  `src/bundle.rs` (`oversized_enforcement_scope_is_bounded_and_marked_incomplete`)
- **REQ-CAP-004** — Capsule and manifest schemas SHALL be versioned, and each
  manifest entry SHALL bind the serialized capsule bytes through a stable
  content digest. *Enforced:* `src/bundle.rs` (`ReviewCapsule`, `BundleManifest`,
  `capsule_digest`) · *Verified:* ✅ `src/bundle.rs`
  (`digest_is_stable_and_content_sensitive`,
  `verifies_serialized_capsule_content_against_manifest_digest`)
- **REQ-CAP-005** — Imported impact and coverage evidence SHALL be accepted
  only when their repository/revision identity is compatible with the bundle
  head. *Enforced:* `src/bundle.rs` (`read_impact`, `read_coverage`,
  `coverage_by_requirement`) ·
  *Verified:* ✅ `src/bundle.rs`
  (`selects_requirement_coverage_and_checks_head_identity`)
- **REQ-CAP-006** — Bundle generation SHALL be deterministic for identical
  source and inputs and SHALL exclude unrelated repository content unless it is
  explicitly related and bounded by the capsule schema. *Enforced:*
  `src/bundle.rs` (`generate`, `build_capsule`) · *Verified:*  code review only

## Semantic Review User Stories

### US-REV-001: Local and CI Agent Review

**Status:** Implemented for local Codex/Claude-compatible CLIs

**As a** developer or CI reviewer  
**I want** resumable requirement-by-requirement semantic review  
**So that** long model runs are useful without becoming an untrusted merge gate

**System Requirements:**

- **REQ-REV-001** — Review SHALL support Codex and Claude-compatible local CLI
  providers, SHALL submit one frozen capsule per invocation, and MAY select a
  provider-specific model or supported local inference endpoint. *Enforced:*
  `src/review.rs` (`ReviewProvider`, `review_capsule`),
  `src/review_provider.rs` (`command_spec`) ·
  *Verified:* ✅ `src/review_tests.rs` (`parses_provider_names`,
  `codex_command_is_ephemeral_and_read_only`,
  `claude_command_disables_tools_and_sessions`)
- **REQ-REV-002** — Provider execution SHALL be ephemeral and non-interactive,
  SHALL disable provider tools or filesystem mutation where supported, and
  SHALL pass only an allowlisted environment that excludes unrelated CI
  secrets. *Enforced:* `src/review_provider.rs` (`command_spec`,
  `sanitize_provider_environment`) · *Verified:* ✅ `src/review_tests.rs`
  (`codex_command_is_ephemeral_and_read_only`,
  `claude_command_disables_tools_and_sessions`,
  `provider_environment_excludes_unrelated_ci_secrets`)
- **REQ-REV-003** — A provider response SHALL satisfy a strict versioned schema
  bound to the exact requirement ID and capsule digest and SHALL review every
  supplied normative clause. *Enforced:* `src/review_schema.rs`,
  `src/review_validation.rs` (`validate_response`) · *Verified:* ✅
  `src/review_tests.rs`
  (`response_schema_binds_capsule_and_requirement_identity_exactly`,
  `rejects_missing_clause_review`)
- **REQ-REV-004** — Every finding and evidence claim SHALL cite only a path and
  line range made citable by the capsule; citations outside that allowlist
  SHALL invalidate the response. *Enforced:* `src/review_validation.rs`
  (`validate_citations`) · *Verified:* ✅ `src/review_tests.rs`
  (`validates_complete_result_with_supplied_citation`,
  `coverage_anchor_and_scope_are_citable_protocol_locations`,
  `rejects_citation_outside_capsule`)
- **REQ-REV-005** — Model verdicts SHALL remain advisory and SHALL be reported
  as separate satisfied, violated, insufficient-evidence, and not-impacted
  counts, distinct from deterministic impact-policy, test-execution, protocol,
  and provider-availability failures. *Enforced:* `src/review.rs`
  (`ReviewVerdict`, `ReviewVerdictCounts`, `ReviewFailureKind`),
  `src/review_progress.rs` (`review_completion_message`),
  `src/review_workflow.rs` (`ReviewWorkflowRun`),
  `cli:src/cli_review_show.rs`
  (`review_outcome_summary`) · *Verified:* ✅
  `src/review_progress.rs`
  (`completion_distinguishes_verdicts_from_unavailable_reviews`),
  `cli:src/cli_tests.rs`
  (`review_summary_separates_verdicts_from_unavailable_responses`),
  `src/review_workflow.rs`
  (`deterministic_or_provider_failure_fails_the_workflow`)
- **REQ-REV-006** — Each completed or failed requirement attempt SHALL be
  checkpointed atomically before the aggregate manifest and summary are
  refreshed, so partial progress survives interruption. *Enforced:*
  `src/review_state.rs` (`ReviewStore::write_checkpoint`), `src/review.rs`
  (`persist_review_artifact`) · *Verified:* ✅ `src/review_state_tests.rs`
  (`atomic_json_replaces_complete_document`), `src/review_tests.rs`
  (`aggregate_artifact_is_refreshed_from_running_to_completed`)
- **REQ-REV-007** — `--resume` SHALL reuse only completed checkpoints whose
  frozen run identity and result validate against the current bundle and SHALL
  reject incompatible or legacy output before modifying it. *Enforced:*
  `src/review_state.rs` (`ReviewStore::open`, `ReviewStore::checkpoint`) ·
  *Verified:* ✅
  `src/review_state_tests.rs`
  (`compatible_resume_reuses_only_revalidated_completed_checkpoint`,
  `incompatible_resume_is_rejected_before_work_starts`,
  `legacy_output_without_run_state_is_not_modified`)
- **REQ-REV-008** — Cross-run cache reuse SHALL use a content-derived key and
  SHALL revalidate cached identity, schema, citations, and result content
  before materialization; local and CI runs SHALL use the same validation
  rules. *Enforced:* `src/review_state.rs` (`ReviewStore::cache`,
  `ReviewStore::write_cache`, `read_cached_unit`), `src/review.rs`
  (`materialize_cached_review`) · *Verified:* ✅ `src/review_state_tests.rs`
  (`portable_cache_is_revalidated_before_reuse`)

## Static Checking User Stories

### US-STATIC-001: Machine-Expressible Requirement Predicates

**Status:** Planned

**As a** requirement owner  
**I want** selected clauses backed by deterministic static checks  
**So that** violations detectable without execution fail early and precisely

**System Requirements:**

- **REQ-STATIC-001** — The tool SHALL support explicit registration of one or
  more static checks against a requirement ID with checker kind, configuration,
  source scope, and stability metadata. *Enforced:* not implemented — static
  check registry described in `docs/requirement-static-checking-design.md` ·
  *Verified:* ⏳ pending
- **REQ-STATIC-002** — The first static backend SHALL support syntax-level Rust
  predicates over `syn`, while future HIR/MIR or Clippy integrations MAY add
  type- and control-flow-aware predicates without changing result semantics.
  *Enforced:* not implemented — backend interface described in
  `docs/requirement-static-checking-design.md` · *Verified:* ⏳ pending
- **REQ-STATIC-003** — Static-check results SHALL identify requirement, checker,
  outcome, source span, diagnostic, tool version, and configuration and SHALL
  remain a separate evidence dimension from tests, coverage, and model review.
  *Enforced:* not implemented — result schema described in
  `docs/requirement-static-checking-design.md` · *Verified:* ⏳ pending
- **REQ-STATIC-004** — A static checker SHALL become a hard merge gate only
  through explicit repository policy after its semantics and false-positive
  behavior are accepted; experimental or unavailable checks SHALL NOT be
  silently presented as passing. *Enforced:* not implemented — policy lifecycle
  described in `docs/requirement-static-checking-design.md` · *Verified:* ⏳
  pending

## Portability User Stories

### US-PORT-001: Standalone Use in Any Rust Repository

**Status:** Portable repository configuration implemented

**As a** maintainer of a Rust crate or workspace  
**I want** to adopt requirement assurance without repository-specific assumptions  
**So that** the method can be reused across repositories and evolved independently

**System Requirements:**

- **REQ-PORT-001** — Repository root and Cargo package topology SHALL be
  discovered from the invocation directory and Cargo metadata and SHALL NOT be
  derived from the tool crate's compile-time manifest location. *Enforced:*
  `src/workspace.rs` (`workspace_root`, `workspace_root_from`) · *Verified:* ✅
  `src/workspace.rs`
  (`discovers_single_package_and_virtual_workspace_roots`)
- **REQ-PORT-002** — The standalone tool SHALL support both an ordinary
  single-package Rust repository and a Cargo workspace, including virtual
  workspace roots. *Enforced:* `src/config.rs` (`RepositoryConfig::load`,
  `RepositoryConfig::documents`), `src/lib.rs` (`DocSpec`) · *Verified:* ✅
  `src/config.rs` (`loads_single_package_repository_configuration`,
  `loads_virtual_workspace_repository_configuration`),
  `cli:tests/external_subcommand.rs`
  (`installed_subcommand_checks_a_single_package_fixture`)
- **REQ-PORT-003** — Repository configuration SHALL declare requirement
  documents, owning packages/source roots, cross-package path prefixes, area
  labels, hardened policies, baseline path, artifact defaults, and optional
  providers without recompiling the tool. *Enforced:* `src/config.rs`
  (`RepositoryConfig`, `DocumentConfig`, `AreaConfig`, `ArtifactConfig`,
  `ReviewConfig`) · *Verified:* ✅ `src/config.rs`
  (`loads_single_package_repository_configuration`,
  `loads_virtual_workspace_repository_configuration`,
  `rejects_paths_that_escape_repository`),
  `cli:tests/external_subcommand.rs`
  (`installed_subcommand_checks_a_single_package_fixture`)
- **REQ-PORT-004** — The standalone implementation SHALL NOT hardcode consumer
  document paths, package names, area lists, removed paths, default
  branch names, or repository-specific minimum requirement counts. *Enforced:*
  `src/config.rs` (`RepositoryConfig`), `src/check.rs` (`analyze`),
  `src/docs.rs` (`resolve_path_span`), `src/impact.rs` (`analyze`),
  `src/impact_dependency.rs` (`analyze`) · *Verified:* ✅ `src/config.rs`
  (`loads_single_package_repository_configuration`,
  `loads_virtual_workspace_repository_configuration`),
  `cli:tests/external_subcommand.rs`
  (`installed_subcommand_checks_a_single_package_fixture`)
- **REQ-PORT-005** — Deterministic analysis SHALL live in reusable library APIs
  that accept explicit repository/configuration inputs and return typed results
  without exiting the process or writing terminal output; the CLI SHALL remain
  a thin adapter. *Enforced:* not implemented — extract process and presentation
  concerns from `cli:src/main.rs` · *Verified:* ⏳ pending
- **REQ-PORT-006** — Git, Cargo, LLVM, filesystem, and model-provider process
  execution SHALL be represented by replaceable adapters so core behavior can
  be fixture-tested and alternative implementations can be added without
  changing artifact contracts. *Enforced:* not implemented — command adapter
  interfaces · *Verified:* ⏳ pending
- **REQ-PORT-007** — Public artifact readers SHALL dispatch on schema version,
  SHALL reject unsupported versions with an actionable error, and SHALL provide
  an explicit migration path before a compatibility-breaking release.
  *Enforced:* not implemented — standalone artifact compatibility policy ·
  *Verified:* ⏳ pending
- **REQ-PORT-008** — The standalone repository SHALL dogfood this specification:
  implemented requirements SHALL be fully anchored before the document enters
  the default CI gate, and every subsequent behavior change SHALL update the
  requirement, enforcement anchor, and honest evidence in the same merge
  request. *Enforced:* `src/check.rs` (`analyze`), `cli:src/main.rs` (`main`) ·
  *Verified:* ✅ `cli:tests/external_subcommand.rs`
  (`repository_configuration_has_zero_traceability_debt`)

## Safety and Trust User Stories

### US-SEC-001: Safe Local and CI Execution

**Status:** Partially implemented

**As a** developer or CI administrator  
**I want** assurance analysis to respect repository and secret boundaries  
**So that** reviewing code does not introduce a new mutation or exfiltration path

**System Requirements:**

- **REQ-SEC-001** — Deterministic check, impact, test-index, coverage mapping,
  bundle, and review-input stages SHALL treat repository source and Git history
  as read-only; only explicit `fmt` MAY modify a requirement document.
  *Enforced:* `cli:src/main.rs`, `src/impact.rs`, `src/test_index.rs`,
  `src/coverage.rs`, `src/bundle.rs`, `src/review.rs` · *Verified:*  code
  review only
- **REQ-SEC-002** — Any path read from an artifact, capsule, cache, or provider
  response SHALL be normalized and SHALL NOT escape its configured root through
  an absolute path or parent traversal. *Enforced:* `src/review_state.rs`
  (`safe_output_path`), `src/review_validation.rs`, `src/bundle.rs` ·
  *Verified:* ✅ `src/review_state_tests.rs`
  (`safe_output_path_rejects_parent_and_absolute_paths`)
- **REQ-SEC-003** — A model provider SHALL receive only the selected capsule,
  review protocol, and allowlisted configuration; unrelated source, environment
  variables, credentials, and prior interactive session state SHALL NOT be
  included. *Enforced:* `src/review.rs` (`prepare_review`),
  `src/review_provider.rs` (`command_spec`, `sanitize_provider_environment`) ·
  *Verified:* ✅ `src/review_tests.rs`
  (`provider_environment_excludes_unrelated_ci_secrets`,
  `codex_command_is_ephemeral_and_read_only`,
  `claude_command_disables_tools_and_sessions`)
- **REQ-SEC-004** — Destructive cleanup SHALL remove only a validated generated
  artifact at the configured default location, SHALL preserve unknown
  directories, and SHALL be idempotent. *Enforced:* `src/bundle.rs`
  (`clean_bundle`), `cli:src/main.rs` (`run_clean`) · *Verified:* ✅
  `src/bundle.rs` (`clean_removes_only_a_valid_default_bundle_and_is_idempotent`,
  `clean_preserves_a_directory_without_a_shallguard_manifest`)
- **REQ-SEC-005** — Coverage, capsule, checkpoint, cache, and provider results
  SHALL carry sufficient identity and digest data to detect stale, corrupt, or
  substituted inputs before reuse; validation failure SHALL produce unavailable
  evidence rather than a fabricated pass. *Enforced:* `src/bundle.rs`,
  `src/review_validation.rs`, `src/review_state.rs` · *Verified:* ✅
  `src/bundle.rs` (`verifies_serialized_capsule_content_against_manifest_digest`),
  `src/review_state_tests.rs` (`portable_cache_is_revalidated_before_reuse`)

## Non-Goals

- Proving arbitrary natural-language requirements from source text.
- Replacing Rust compilation, Clippy, tests, or accountable human review.
- Treating line coverage as proof that assertions are correct.
- Making a model provider or network access mandatory for deterministic checks.
- Sending an entire repository to a model by default.
- Building a complete Rust call graph using `syn` alone.
- Automatically changing requirements, code, tests, or merge-request approval.

## Initial Dogfooding Completion Criteria

The bootstrap phase is complete when:

1. the standalone repository configuration can select this document and its
   owning source roots without compiled-in consumer constants;
2. every requirement marked implemented has at least one honest enforcement
   anchor;
3. every ✅ citation resolves to its exact, enabled `#[shallguard::verifies]` test;
4. the explicit traceability check reports zero unbaselined gaps;
5. `fmt --check`, unit/integration tests, Clippy, and the deterministic
   requirement gate pass in CI;
6. a self-change to ShallGuard can produce impact, coverage, capsule, and
   optional local review artifacts from these requirements.
