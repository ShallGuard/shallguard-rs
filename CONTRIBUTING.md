# Contributing to ShallGuard

Thank you for helping improve ShallGuard. Contributions may include code,
tests, requirements, documentation, design proposals, bug reports, and English
wording improvements.

Please follow the [Code of Conduct](CODE_OF_CONDUCT.md) in all project spaces.

## Project direction

ShallGuard exists to keep software behavior understandable and auditable as
LLM-assisted development makes producing code faster. Features that improve the
development process in this environment are especially welcome, provided that
they preserve deterministic checks, honest evidence, and human control over
the specification.

ShallGuard also welcomes proposals to extend its principles beyond Rust. In
particular, integrations for Python, JavaScript, TypeScript, Go, and other
languages with reliable test-discovery and coverage tooling are in scope.

## Requirements-first development

ShallGuard dogfoods its own requirements workflow. Every new feature or
intentional behavior change must be specified and delivered as one coherent
change:

1. Add or update a user story and testable requirement in
   [`docs/USER_STORIES_AND_REQUIREMENTS.md`](docs/USER_STORIES_AND_REQUIREMENTS.md)
   before the final implementation pass.
2. Implement the behavior and anchor the code that enforces the requirement
   with `#[shallguard::enforces]` or `shallguard::enforces_here!`.
3. Add a test that exercises the contract and anchor it with
   `#[shallguard::verifies]`.
4. Claim automated evidence only after that test exists and would fail if the
   contract were violated.
5. Run the deterministic ShallGuard checks before submitting the change.

Do not add baseline entries for new work. The committed baseline is empty, and
all requirement areas are hardened. Requirement IDs are stable: retire an ID
instead of deleting or reusing it.

Refactoring, formatting, and documentation corrections that do not change
product behavior do not need artificial product requirements. Changes to
normative requirement wording need special care: a purely editorial change
must preserve meaning, while a semantic change must be treated as a behavior
change and kept consistent with its code and evidence.

## Use the ShallGuard agent skill

When working with a coding agent, we strongly recommend installing and using
the repository's [ShallGuard skill](docs/skill/SKILL.md). See the
[installation instructions](README.md#ai-agent-skill) for Codex and Claude
Code. Tell the agent to use the skill before it changes behavior, requirements,
anchors, verification tests, or code near an existing anchor.

The skill teaches agents to read the requirement before changing its enforcing
code, follow the requirements-first sequence, preserve stable IDs, anchor
honest evidence, and fix checker failures at their source. This helps prevent
an agent from making a change appear successful by weakening the specification
or bypassing traceability.

In particular, an agent must never:

- Delete or move an anchor merely to silence a checker or review finding.
- Weaken, broaden, or reword a requirement merely to match the implementation
  or make a deterministic check or semantic review pass.
- Fabricate a test citation, verification anchor, or other evidence.
- Downgrade evidence or add baseline debt merely to avoid implementing or
  testing the required behavior.

When an implementation conflicts with a requirement, fix the implementation or
its evidence. If the intended product behavior has genuinely changed, update
the requirement explicitly as a reviewed specification decision and keep its
implementation and evidence consistent in the same change. A passing tool
result is never more important than preserving the intended contract.

## LLM-assisted contributions

LLM-assisted code and prose are welcome. Much of ShallGuard's English text has
itself been drafted, reworded, or corrected with LLM assistance because English
is not the maintainer's native language.

The person submitting a contribution remains accountable for its correctness,
security, licensing, and maintainability, regardless of which tools helped
produce it. In particular:

- Generated tests, citations, requirement anchors, and review findings must be
  checked before they are presented as evidence.
- Secrets, private source code, and other restricted information must not be
  sent to external model providers without authorization.
- Model verdicts remain advisory. Deterministic checks and human review retain
  authority over acceptance.
- Deterministic ShallGuard checks must not require network access or a model.

Using an LLM is neither a reason to reject a contribution nor a substitute for
understanding it.

## Additional language support

A language integration does not need to imitate Rust attributes or macros, but
it should preserve ShallGuard's core guarantees:

- Stable requirement identities.
- Traceable enforcement and verification evidence.
- Honest, ecosystem-native test discovery and execution.
- Integration with the ecosystem's coverage tooling where available.
- Deterministic, machine-readable checks and versioned artifacts.
- A clear distinction between execution coverage and proof of correctness.
- No network or model dependency in deterministic gates.

Before implementing a substantial language integration, a design proposal
should explain its enforcement-anchor mechanism, test identity model, coverage
strategy, configuration and artifact compatibility, and deterministic trust
boundary. The integration itself must follow the requirements-first workflow.

Repository organization and build orchestration for multiple languages are not
yet predetermined. They should be discussed before adopting a layout or adding
a new build system. The proposal should address:

- Whether ShallGuard remains one multi-language repository or uses separate
  repositories for language-specific integrations.
- Which functionality and artifact schemas are shared, and which parts should
  be implemented as ecosystem-native packages or tools.
- How Cargo and language-specific build systems are invoked locally and in CI,
  including whether a common top-level task runner is useful.
- How compiler, runtime, package-manager, and dependency versions are pinned
  and reproduced from a clean checkout.
- How unit, integration, fixture, coverage, and cross-language compatibility
  tests are organized and owned.
- How CI selects the checks affected by a change without allowing one language
  integration to hide failures in another.
- How packages and shared artifact schemas are versioned, released, and tested
  for backward compatibility.

The goal is not to impose one universal build system. Contributors in each
ecosystem should be able to use familiar tools, while maintainers retain a
clear, reproducible way to run the complete repository test suite and verify
that all language integrations implement compatible ShallGuard semantics.

## Development checks

Run the checks relevant to the change. The complete repository check is:

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo +1.89.0 check --workspace --all-targets --locked
cargo shallguard-dev fmt --check
cargo shallguard-dev check
```

After editing the requirement specification, always run both ShallGuard
commands without restricting them to one document. This lets
`shallguard.toml` select and cross-check the complete specification.

## Rust conventions

- Prefer simple, deterministic APIs and explicit configuration.
- Do not use `Result::unwrap()`; reserve `expect("BUG: ...")` for invariants.
- Keep deterministic library behavior separate from terminal presentation and
  provider execution.
- Split focused modules before a source file grows beyond 1,000 lines.
- Use versioned schemas for machine-readable artifacts.
- Do not add network access or a model dependency to deterministic checks.

## Pull requests

Keep changes focused and explain the user problem they solve. Identify the
requirements added or changed, describe the evidence, and mention important
limitations or follow-up work. For a substantial feature or language
integration, opening a design discussion before implementation is encouraged.

Do not claim that tests, coverage, or an LLM verdict prove more than they
actually demonstrate. Clear pending evidence is preferable to a false claim of
completed verification.
