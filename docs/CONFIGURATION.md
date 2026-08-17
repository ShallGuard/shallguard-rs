# Repository configuration

ShallGuard reads `shallguard.toml` from the Cargo repository root discovered
from the invocation directory. All paths are repository-relative; `.` is valid
as a document source root for a single-package repository.

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

For a virtual workspace, add one `[[documents]]` entry per specification and
set `source_root` to the owning package directory, such as `crates/router`.
Prefix mappings may point to other package roots. ShallGuard scans `src/` and
`tests/` beneath every selected source root and mapped prefix.

The configuration loader rejects unknown fields, unsupported schemas,
absolute or parent-traversing paths, duplicate documents, missing documents or
source roots, invalid area identifiers, and unsupported review-provider names.

## Baseline lifecycle

Repositories adopting ShallGuard around existing code may create the baseline
once and commit it:

```bash
cargo shallguard baseline init
```

The baseline is a ratchet, not an allowlist to extend. New gaps fail checking.
After adding honest enforcement or verification anchors, remove only resolved
entries and commit the result with the anchors:

```bash
cargo shallguard baseline prune
cargo shallguard check
```

Set an area's `hard_enforcement` or `hard_verification` policy to `true` once
that dimension has no historical gaps. Hardened areas cannot be baselined.
