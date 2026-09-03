#!/usr/bin/env bash
# Regenerate the README demo GIFs with VHS (https://github.com/charmbracelet/vhs).
#
# Requirements on PATH: vhs, ttyd, ffmpeg, cargo, cargo-shallguard, plus
# fish (the recorded shell; its live highlighting renders the narration
# comments gray) and bat (syntax-highlighted file output).
# Inside a container without a Chromium sandbox, export VHS_NO_SANDBOX=true.
#
# The recordings run inside a small throwaway project (worker-scheduler, the
# README quick-start example) so every table and error message fits one screen.
# Set SHALLGUARD_PATH to a local ShallGuard checkout to record against
# unpublished changes; otherwise the published crate version is used.
#
# semantic-review.gif is only re-recorded when RECORD_SEMANTIC=1 is set: it
# invokes the codex provider twice (one warm-up run, one recorded run), which
# requires a logged-in codex CLI plus cargo-llvm-cov and spends model tokens.
# The recorded agent verdict is nondeterministic in wording; re-record until
# the verdict scene reads well.
set -euo pipefail

demo_dir="$(cd "$(dirname "$0")" && pwd)"
work_dir="$(mktemp -d)"
project="$work_dir/worker-scheduler"
trap 'rm -rf "$work_dir"' EXIT

mkdir -p "$project/src" "$project/docs"

if [[ -n "${SHALLGUARD_PATH:-}" ]]; then
    dependency="shallguard = { path = \"$SHALLGUARD_PATH\" }"
else
    dependency='shallguard = "0.1.1"'
fi

cat > "$project/Cargo.toml" <<EOF
[package]
name = "worker-scheduler"
version = "0.1.0"
edition = "2021"

[dependencies]
$dependency
EOF

cat > "$project/shallguard.toml" <<'EOF'
schema = 1
minimum_requirements = 1
baseline = ".shallguard/baseline.toml"
verify_outlier_threshold = 6

[[documents]]
path = "docs/REQUIREMENTS.md"
source_root = "."

[areas.HRS]
label = "Worker Scheduling"
hard_enforcement = true
hard_verification = true

[artifacts]
root = "target/shallguard"

[review]
target = "master"
provider = "codex"
with_coverage = true
timeout_seconds = 300
EOF

cat > "$project/docs/REQUIREMENTS.md" <<'EOF'
# Worker Scheduler: Requirements

## US-1: Operators never lose the whole fleet

As a fleet operator, I want a guaranteed worker floor so that a bad
configuration can never idle every machine.

**System Requirements:**

- **REQ-HRS-001** — The scheduler SHALL never emit a zero worker floor.
  *Enforced:* `src/lib.rs` (`floor`) · *Verified:* [test] `src/lib.rs`
  (`floor_never_returns_zero`)
- **REQ-HRS-002** — Worker resolution SHALL apply the configured floor in
  every scheduling mode. *Enforced:* `src/lib.rs` (`resolve`) · *Verified:* [test]
  `src/lib.rs` (`resolve_applies_floor_in_every_mode`)
EOF

cat > "$project/src/lib.rs" <<'EOF'
//! Worker scheduler: resolves how many workers a fleet node runs.

pub enum Mode {
    Fixed(usize),
    Auto,
}

/// A zero floor would idle the whole fleet (REQ-HRS-001 is the contract).
#[shallguard::enforces("REQ-HRS-001")]
pub fn floor(configured: usize) -> usize {
    configured.max(1)
}

#[shallguard::enforces("REQ-HRS-002")]
pub fn resolve(mode: Mode) -> usize {
    match mode {
        Mode::Fixed(n) => floor(n),
        Mode::Auto => floor(detected_parallelism()),
    }
}

fn detected_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shallguard::verifies("REQ-HRS-001")]
    #[test]
    fn floor_never_returns_zero() {
        assert_eq!(floor(0), 1);
    }

    #[shallguard::verifies("REQ-HRS-002")]
    #[test]
    fn resolve_applies_floor_in_every_mode() {
        assert_eq!(resolve(Mode::Fixed(0)), 1);
        assert!(resolve(Mode::Auto) >= 1);
    }
}
EOF

printf '/target\nCargo.lock\n' > "$project/.gitignore"

git_commit() {
    git -C "$project" -c user.name="Demo" -c user.email="demo@example.com" \
        commit -q "$@"
}

# Optional post-processing keeps the README lightweight: re-time to a low
# frame rate with a small undithered palette, then a lossy gifsicle pass.
# The "heavy" profile squeezes the long semantic recording harder.
shrink_gif() {
    local gif="$1" profile="${2:-normal}" fps=10 lossy=100 colors=""
    if [[ "$profile" == heavy ]]; then
        fps=8
        lossy=160
        colors="--colors 32"
    fi
    if command -v ffmpeg > /dev/null; then
        ffmpeg -v error -y -i "$gif" -vf \
            "fps=$fps,split[a][b];[a]palettegen=max_colors=48[p];[b][p]paletteuse=dither=none" \
            "$gif.tmp.gif" && mv "$gif.tmp.gif" "$gif"
    fi
    if command -v gifsicle > /dev/null; then
        # shellcheck disable=SC2086
        gifsicle -O3 --lossy=$lossy $colors "$gif" -o "$gif.tmp" && mv "$gif.tmp" "$gif"
    fi
}

cd "$project"
git init -q -b master
cargo shallguard baseline init
git add -A
git_commit -m "Initial scheduler with anchored requirements"

# The colleague's branch reviewed in review-workflow.tape and
# semantic-review.tape. The Burst arm deliberately bypasses floor(): anchors
# and tests stay green, so only the semantic review catches the violation.
git checkout -q -b feature/burst-mode
python3 - <<'PY'
src = open('src/lib.rs').read()
src = src.replace("""    Fixed(usize),
    Auto,
}""", """    Fixed(usize),
    Auto,
    Burst(usize),
}""")
src = src.replace("""        Mode::Auto => floor(detected_parallelism()),
""", """        Mode::Auto => floor(detected_parallelism()),
        Mode::Burst(n) => n.saturating_mul(2),
""")
open('src/lib.rs', 'w').write(src)
PY
git add -A
git -c user.name="Colleague" -c user.email="colleague@example.com" \
    commit -qm "Add burst scheduling mode"

# Warm every cache so the recordings show real, instant command output.
git checkout -q master
cargo build -q
cargo shallguard check > /dev/null

vhs "$demo_dir/dev-workflow.tape"
git checkout -q -- .

git checkout -q feature/burst-mode
cargo test --no-run -q > /dev/null 2>&1
cargo shallguard check > /dev/null
vhs "$demo_dir/review-workflow.tape"

cp dev-workflow.gif review-workflow.gif "$demo_dir/"
shrink_gif "$demo_dir/dev-workflow.gif"
shrink_gif "$demo_dir/review-workflow.gif"
echo "Regenerated $demo_dir/dev-workflow.gif and $demo_dir/review-workflow.gif"

if [[ -n "${RECORD_SEMANTIC:-}" ]]; then
    # Warm-up run: builds the instrumented llvm-cov binaries and verifies the
    # provider works, so the recorded run finishes within the tape's window.
    rm -f impact.json impact.md
    cargo shallguard clean > /dev/null 2>&1 || true
    rm -rf target/shallguard/local-review
    cargo shallguard review > /dev/null
    cargo shallguard clean > /dev/null
    rm -rf target/shallguard/local-review
    vhs "$demo_dir/semantic-review.tape"
    cp semantic-review.gif "$demo_dir/"
    shrink_gif "$demo_dir/semantic-review.gif" heavy
    echo "Regenerated $demo_dir/semantic-review.gif"
fi
