# Changelog

All notable changes to ShallGuard are documented in this file. The project
follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

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

[Unreleased]: https://github.com/sigi64/shallguard/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/sigi64/shallguard/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/sigi64/shallguard/releases/tag/v0.1.0
