# Contributing to ShallGuard for Rust

Thank you for your help with ShallGuard. This page tells you how to
contribute to the Rust implementation. The rules that apply to every
ShallGuard repository are in the
[shared contributing guide](https://github.com/shallguard/spec/blob/master/CONTRIBUTING.md)
of the specification repository. Read that guide first. It covers the
project direction, the requirements-first workflow, the mandatory writing
style, the work with a coding agent, the responsibility for a contribution
with LLM help, and the support for more languages. The
[glossary](docs/GLOSSARY.md) defines each technical term of this
repository.

Follow the [Code of Conduct](CODE_OF_CONDUCT.md) in all project spaces.

## Requirements-first development in this repository

The product specification is
[`docs/USER_STORIES_AND_REQUIREMENTS.md`](docs/USER_STORIES_AND_REQUIREMENTS.md).
Every new feature and every intended behavior change must arrive as one
change with these parts:

1. Add or update a user story and a testable requirement in the
   specification. Do this before the final implementation pass.
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

## Documentation style (mandatory)

Write all documents in ASD-STE100 (Simplified Technical English). The rules
are in the shared
[writing style page](https://github.com/shallguard/spec/blob/master/WRITING_STYLE.md).
One exception applies. A requirement statement keeps its RFC 2119 form with
SHALL, SHALL NOT, or MAY. ShallGuard needs that form.

## Use the ShallGuard agent skill

If you work with a coding agent, install and use the
[ShallGuard skill](docs/skill/SKILL.md). The
[README](README.md#ai-agent-skill) gives the installation steps for Codex
and Claude Code. Tell the agent to use the skill before it changes behavior,
requirements, anchors, verification tests, or code near an anchor. The
shared contributing guide lists what an agent must never do.

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
Mention important limits and follow-up work. For a large feature, open a
design discussion before the implementation.

Do not claim that tests, coverage, or an LLM verdict prove more than they
show. A clear pending mark is better than a false claim of complete
verification.

When the gate or the workflow causes friction during your work, add a
one-line entry to [docs/FRICTION.md](docs/FRICTION.md) in the same change
set. Do not work around the friction in silence.
