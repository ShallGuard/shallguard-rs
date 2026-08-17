#[shallguard::enforces("REQ-CLI-001")]
pub(super) fn print() {
    println!(
        r#"ShallGuard keeps product requirements connected to Rust implementation and real tests.
It provides deterministic traceability and impact checks, executable coverage,
auditable review capsules, and optional semantic review.

Development workflow:
  1. Add or update a REQ-<AREA>-<NNN> contract in a configured Markdown document.
  2. Anchor the implementation with #[shallguard::enforces(...)] or
     shallguard::enforces_here!(...).
  3. Exercise the contract in a real test marked #[shallguard::verifies(...)].
  4. Run cargo test, cargo shallguard fmt --check, and cargo shallguard check.
     The check rejects missing or stale anchors and traceability ratchet regressions.

Review workflow:
  1. Run cargo shallguard impact --target <branch> to map the change to affected
     requirements and their exact verification tests.
  2. Run cargo shallguard review --target <branch> for the end-to-end workflow:
     impact analysis, configured executable coverage, bounded review capsules,
     and semantic verdicts from the configured provider.
  3. Inspect the versioned artifacts and resolve deterministic failures. Semantic
     verdicts are advisory and remain a human review input.

Usage:
  cargo shallguard [check] [<doc.md> ...]
  cargo shallguard version
  cargo shallguard --version
  cargo shallguard fmt [--check] [<doc.md> ...]
  cargo shallguard lint [<doc.md> ...]
  cargo shallguard clean
  cargo shallguard baseline <check|init|prune>
  cargo shallguard impact <--base <revision>|--target <branch>> [--json <path>] [--markdown <path>]
  cargo shallguard bundle --impact <impact.json> [--coverage <coverage.json>] [options]
  cargo shallguard test-index <--enumerate|--catalog <path>> [options]
  cargo shallguard coverage [--package <crate>] [--requirement <REQ-ID>] [options]
  cargo shallguard review [--base <revision>|--target <branch>] [options]

Review options:
  --provider <name>          Local model CLI [default: shallguard.toml, then codex]
  --base <revision>          Compare against an exact revision
  --target <branch>          Compare against its merge base [default: shallguard.toml]
  --with-coverage            Run impacted verification tests [configured default]
  --without-coverage         Skip executable coverage collection
  --bundle <directory>       Generated bundle, or existing bundle in replay mode
                             [default: shallguard.toml artifact root]
  --output <directory>       New auditable result directory
                             [default: shallguard.toml artifact root]
  --resume                   Continue the compatible run in --output
  --cache-dir <directory>    Reuse validated responses across run directories
  --model <identifier>       Provider-specific model override
  --local-provider <name>    Codex on-device inference: ollama or lmstudio
  --requirement <REQ-ID>     Review only this requirement; repeatable
  --timeout-seconds <n>      Per-requirement timeout [default: shallguard.toml, then 300]

`fmt` lints and formats requirement blocks; `fmt --check` and `lint` never write.
`clean` removes only the validated bundle at the configured artifact location.
Model verdicts are advisory. Provider or schema failures return nonzero."#
    );
}
