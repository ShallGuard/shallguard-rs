# Changelog

All notable changes to ShallGuard are documented in this file. The project
follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.2] - 2026-09-04

- Added ASCII evidence marks `[test]`, `[e2e]`, `[review]`, and `[pending]`
  as the canonical form on `*Verified:*` lines. The emoji ✅, 🔬, 👁, and ⏳
  stay valid as optional aliases. `cargo shallguard fmt` adds the keyword
  before an emoji and keeps the emoji. `fmt --check`, `lint`, and `check`
  accept an emoji without its keyword, so an existing document passes
  without a change (#14).
- Rewrote every document in Simplified Technical English and added
  `docs/WRITING_STYLE.md` with the mandatory rules.
- Marked every feature that needs a language model as experimental: the
  `review` command with its providers, `review show`, and the advisory
  pull-request workflow. The `review` command prints a notice, and the help
  output labels the commands. An experimental feature can change in any
  release.
- Moved the repository to `shallguard/shallguard-rs` and updated every
  manifest and link.
- Restructured the README as a landing page, added animated workflow demos
  with a recording pipeline, and added the friction log `docs/FRICTION.md`.
- Added the planned requirements REQ-TRACE-009 to REQ-TRACE-017 for the
  evidence floor. Their implementation stays on the branch
  `feature/evidence-floor` and is not part of this release.

## [0.1.1] - 2026-08-17

- Added repository-independent `version`, `--version`, and expanded `help`
  commands, and made `cargo-shallguard` the default workspace binary.
- Added read-only `review show` inspection for completed and partial review
  runs, with positional requirement filters, colored terminal sections, and
  deterministic GitHub-flavored Markdown reports.
- Improved semantic-review progress and completion output so verdicts,
  unavailable responses, evidence gaps, and stored artifact locations are
  clearly distinguished.
- Added a sandboxed GitHub Copilot CLI provider and an advisory GitHub Actions
  workflow that publishes bounded semantic-review results on pull requests
  without replacing the deterministic ShallGuard merge gate.
- Added an installable AI-agent skill describing ShallGuard's
  requirements-first development workflow and evidence rules.

## [0.1.0] - 2026-08-14

- Initial crates.io release of the `shallguard` analysis library,
  `shallguard-macros` anchor implementation, and `cargo-shallguard` Cargo
  subcommand.
- Deterministic requirement parsing, traceability checks, ratcheted baselines,
  impact analysis, test indexing, executable coverage, review bundles, and
  optional semantic review.
- Public `shallguard::enforces`, `shallguard::verifies`, and
  `shallguard::enforces_here!` anchor API.

[Unreleased]: https://github.com/shallguard/shallguard-rs/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/shallguard/shallguard-rs/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/shallguard/shallguard-rs/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/shallguard/shallguard-rs/releases/tag/v0.1.0
