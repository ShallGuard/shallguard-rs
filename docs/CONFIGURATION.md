# Repository configuration

This page describes the file `shallguard.toml`. The file holds the ShallGuard
policy of one repository. The [glossary](GLOSSARY.md) defines each technical
term.

## Location and format

ShallGuard finds the root of the Cargo repository from the directory where
you run the command. It reads `shallguard.toml` from that root. All paths in
the file are relative to the repository root. In a single-package repository,
`.` is a valid source root for a document.

This is a complete example:

```toml
schema = 1
minimum_requirements = 1
baseline = ".shallguard/baseline.toml"
verify_outlier_threshold = 6
allow_missing_paths = []

[[documents]]
path = "docs/USER_STORIES_AND_REQUIREMENTS.md"
source_root = "."

# Optional aliases used by paths such as `core:src/lib.rs` in a document.
[prefixes]
core = "crates/core"

[areas.CLI]
label = "Command Line"
hard_enforcement = true
hard_verification = true

[areas.PORT]
label = "Portability"
hard_enforcement = true
hard_verification = true

[artifacts]
root = "target/shallguard"

# Every review setting is optional. CLI flags take precedence.
[review]
target = "origin/main"
provider = "codex" # codex, claude, or copilot
with_coverage = true
timeout_seconds = 300
# model = "provider-specific-model"
# local_provider = "ollama" # ollama or lmstudio, with Codex
```

A virtual Cargo workspace is a repository with several crates and no root
crate. In a virtual workspace, add one `[[documents]]` entry for each
requirement document. Set `source_root` to the directory of the crate that
owns the document, for example `crates/router`. A prefix can point at
another crate directory. ShallGuard scans the `src/` and `tests/`
directories below every source root and every prefix.

The configuration loader rejects these errors:

- an unknown field,
- an unsupported schema number,
- an absolute path, or a path that goes to a parent directory,
- a document that appears twice,
- a document or a source root that does not exist,
- an invalid area identifier,
- an unsupported review provider name.

## Baseline lifecycle

A repository that adopts ShallGuard around existing code has gaps. A gap is a
requirement without an enforcement anchor or without automated evidence.
Create the baseline once, and commit it:

```bash
cargo shallguard baseline init
```

The baseline is a ratchet. It is not a list of exceptions that you extend.
Each new gap fails the check. After you add honest enforcement anchors or
verification anchors, remove only the resolved entries. Commit the result
together with the anchors:

```bash
cargo shallguard baseline prune
cargo shallguard check
```

An area has two policy fields, `hard_enforcement` and `hard_verification`.
Set a field to `true` when the area has no gap of that kind. A hard area
cannot go into the baseline.

