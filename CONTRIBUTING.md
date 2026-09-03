# Contributing to ShallGuard

Thank you for your help with ShallGuard. This page tells you how to
contribute. A contribution can be code, tests, requirements, documentation,
a design proposal, a bug report, or an English wording improvement. The
[glossary](docs/GLOSSARY.md) defines each technical term.

Follow the [Code of Conduct](CODE_OF_CONDUCT.md) in all project spaces.

## Project direction

ShallGuard exists to keep software behavior understandable and auditable.
Development with a large language model (LLM) makes code faster to produce.
A feature that improves the development process in this environment is
welcome. The feature must keep three properties:

- The checks give the same result each time.
- The evidence is honest.
- A person controls the specification.

ShallGuard also welcomes proposals that extend its principles beyond Rust.
Integrations for Python, JavaScript, TypeScript, Go, Java, C#/.NET, and
other languages are in scope. The language must have reliable tools for test
discovery and for coverage.

## Requirements-first development

ShallGuard uses its own requirements workflow. Every new feature and every
intended behavior change must arrive as one change with these parts:

1. Add or update a user story and a testable requirement in
   [`docs/USER_STORIES_AND_REQUIREMENTS.md`](docs/USER_STORIES_AND_REQUIREMENTS.md).
   Do this before the final implementation pass.
2. Implement the behavior. Anchor the code that makes the requirement true
   with `#[shallguard::enforces]` or `shallguard::enforces_here!`.
3. Add a test that verifies the requirement. Anchor the test with
   `#[shallguard::verifies]`.
4. Claim automated evidence only after that test exists. The test must fail
   when the requirement is violated.
5. Run the ShallGuard checks before you submit the change.

Do not add baseline entries for new work. The committed baseline is empty,
and all requirement areas are hard. Requirement IDs are stable. Retire an ID
instead of a deletion or a reuse.

A refactor, a format change, or a documentation correction does not change
the behavior of the product. Such a change does not need an artificial
requirement. A change to the text of a requirement needs special care. An
editorial change must keep the meaning. A change of meaning is a behavior
change. Keep it consistent with the code and the evidence.

## Documentation style (mandatory)

Write all documents in ASD-STE100 (Simplified Technical English). Write for
a reader without previous knowledge. The rules are in
[`docs/WRITING_STYLE.md`](docs/WRITING_STYLE.md). The most important rules
are:

- Use short sentences with one topic. Use the active voice and the present
  tense.
- Use "must" for a rule and "can" for a possibility. Do not use "should".
- Explain each technical term the first time you use it, or link to the
  glossary.
- Use one word for one thing.

One exception applies. A requirement statement keeps its RFC 2119 form with
SHALL, SHALL NOT, or MAY. ShallGuard needs that form.

## Use the ShallGuard agent skill

If you work with a coding agent, install and use the
[ShallGuard skill](docs/skill/SKILL.md). The
[README](README.md#ai-agent-skill) gives the installation steps for Codex
and Claude Code. Tell the agent to use the skill before it changes behavior,
requirements, anchors, verification tests, or code near an anchor.

The skill teaches the agent to read the requirement before it changes the
code that makes the requirement true. The agent then follows the
requirements-first sequence, keeps the IDs stable, anchors honest evidence,
and fixes each check failure at its source. This prevents an agent from a
false success through a weaker specification or a bypass of traceability.

An agent must never:

- Delete or move an anchor only to remove a check finding or a review
  finding.
- Weaken, widen, or reword a requirement only to match the implementation
  or to make a check or a semantic review pass.
- Invent a test citation, a verification anchor, or other evidence.
- Downgrade evidence or add baseline debt only to avoid the implementation
  or the test of the required behavior.

When an implementation conflicts with a requirement, fix the implementation
or its evidence. If the intended product behavior has changed, update the
requirement as a reviewed specification decision. Keep the implementation
and the evidence consistent in the same change. A passing tool result is
never more important than the intended requirement.

## LLM-assisted contributions

Code and text with LLM help are welcome. Much of the English text of
ShallGuard was drafted, reworded, or corrected with LLM help, because
English is not the native language of the maintainer.

The person who submits a contribution is responsible for its correctness,
security, licensing, and maintainability. This applies whichever tools
helped to produce it. In particular:

- Examine each generated test, citation, requirement anchor, and review
  finding before you present it as evidence.
- Do not send secrets, private source code, or other restricted information
  to an external model provider without authorization.
- Model verdicts are advisory. The checks and the human review decide about
  acceptance.
- The ShallGuard checks must not need network access or a model.

The use of an LLM is not a reason to reject a contribution. It is also not a
replacement for an understanding of the contribution.

## Additional language support

A language integration does not need to imitate the Rust attributes or
macros. It must keep the core guarantees of ShallGuard:

- Stable requirement identities.
- Traceable enforcement evidence and verification evidence.
- Honest test discovery and test execution with the native tools of the
  ecosystem.
- Integration with the coverage tools of the ecosystem, where they exist.
- Machine-readable checks with the same result each time, and versioned
  artifacts.
- A clear difference between execution coverage and proof of correctness.
- No network dependency and no model dependency in a gate.

Before you implement a large language integration, write a design proposal.
The proposal must explain the enforcement-anchor mechanism, the test
identity model, the coverage strategy, the compatibility of configuration
and artifacts, and the trust boundary of the checks. The integration itself
must follow the requirements-first workflow.

The repository layout and the build orchestration for several languages are
not decided yet. Discuss them before you adopt a layout or add a build
system. The proposal must answer these questions:

- Does ShallGuard stay one repository for all languages, or does each
  language integration get its own repository?
- Which functionality and which artifact schemas are shared? Which parts
  are packages or tools native to the ecosystem?
- How do Cargo and the language-specific build systems run, on a local
  machine and in CI? Is a common top-level task runner useful?
- How are the versions of compilers, runtimes, package managers, and
  dependencies pinned and reproduced from a clean copy of the repository?
- How are unit, integration, fixture, coverage, and cross-language
  compatibility tests organized, and who owns them?
- How does CI select the checks that a change affects, without a way for one
  language integration to hide failures in another?
- How are packages and shared artifact schemas versioned, released, and
  tested for backward compatibility?

The goal is not one universal build system. A contributor in each ecosystem
must be able to use familiar tools. A maintainer must keep a clear,
reproducible way to run the complete test suite of the repository and to
verify that all language integrations implement compatible ShallGuard
semantics.

## Development checks

Run the checks that apply to your change. The complete check of the
repository is:

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo +1.89.0 check --workspace --all-targets --locked
cargo shallguard-dev fmt --check
cargo shallguard-dev check
```

After an edit of the requirement specification, always run both ShallGuard
commands without a document argument. Then `shallguard.toml` selects the
complete specification and cross-checks it.

## Rust conventions

- Prefer simple APIs with the same result each time, and explicit
  configuration.
- Do not use `Result::unwrap()`. Use `expect("BUG: ...")` only for an
  invariant.
- Keep the library behavior separate from the terminal presentation and from
  the provider execution.
- Split a source file into focused modules before it grows beyond 1,000
  lines.
- Use versioned schemas for machine-readable artifacts.
- Do not add network access or a model dependency to a check.

## Pull requests

Keep each change focused. Explain the user problem that the change solves.
Name the requirements that you added or changed. Describe the evidence.
Mention important limits and follow-up work. For a large feature or a
language integration, open a design discussion before the implementation.

Do not claim that tests, coverage, or an LLM verdict prove more than they
show. A clear pending mark is better than a false claim of complete
verification.

When the gate or the workflow causes friction during your work, add a
one-line entry to [docs/FRICTION.md](docs/FRICTION.md) in the same change
set. Do not work around the friction in silence.
