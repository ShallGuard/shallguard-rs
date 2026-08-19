#!/usr/bin/env bash
# Regenerate the migration demo GIFs (docs/MIGRATION.md) with VHS.
#
# Requirements on PATH: vhs, ttyd, ffmpeg, cargo, cargo-shallguard, plus
# fish and bat (see regenerate.sh); gifsicle is optional but keeps the
# GIFs small. Inside a container without a Chromium sandbox, export
# VHS_NO_SANDBOX=true.
#
# The recordings run inside a throwaway legacy-style project
# (fleet-scheduler): code without requirements, one thin test, plus an
# agent-drafted requirement document with honest ⏳ evidence. Set
# SHALLGUARD_PATH to a local ShallGuard checkout to record against
# unpublished changes; otherwise the published crate version is used.
set -euo pipefail

demo_dir="$(cd "$(dirname "$0")" && pwd)"
work_dir="$(mktemp -d)"
project="$work_dir/fleet-scheduler"
trap 'rm -rf "$work_dir"' EXIT

mkdir -p "$project/src" "$project/docs"

if [[ -n "${SHALLGUARD_PATH:-}" ]]; then
    dependency="shallguard = { path = \"$SHALLGUARD_PATH\" }"
else
    dependency='shallguard = "0.1.1"'
fi

cat > "$project/Cargo.toml" <<EOF
[package]
name = "fleet-scheduler"
version = "0.4.2"
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

[areas.SCH]
label = "Worker Scheduling"
hard_enforcement = false
hard_verification = false

[areas.NET]
label = "Reconnect Policy"
hard_enforcement = false
hard_verification = false

[artifacts]
root = "target/shallguard"
EOF

cat > "$project/docs/REQUIREMENTS.md" <<'EOF'
# Fleet Scheduler: Recovered Requirements

Drafted by an agent from the existing code, tests, and history.
Every SHALL statement below was reviewed by a human before enrollment.

## US-1: Operators never lose the whole fleet

**System Requirements:**

- **REQ-SCH-001** — The scheduler SHALL never emit a zero worker floor.
  *Enforced:* `src/lib.rs` (`floor`) · *Verified:* ⏳ pending
- **REQ-SCH-002** — Worker resolution SHALL apply the configured floor in
  every scheduling mode. *Enforced:* `src/lib.rs` (`resolve`) · *Verified:* ⏳
  pending

## US-2: Reconnects must not stampede the pool

**System Requirements:**

- **REQ-NET-001** — Reconnect backoff SHALL be capped at 60 seconds.
  *Enforced:* `src/lib.rs` (`backoff_delay`) · *Verified:* ⏳ pending
EOF

cat > "$project/src/lib.rs" <<'EOF'
//! Fleet scheduler — grown over four years, no written requirements.

pub enum Mode {
    Fixed(usize),
    Auto,
}

pub fn floor(configured: usize) -> usize {
    configured.max(1)
}

pub fn resolve(mode: Mode) -> usize {
    match mode {
        Mode::Fixed(n) => floor(n),
        Mode::Auto => floor(detected_parallelism()),
    }
}

/// Reconnect backoff in seconds, doubling per retry.
pub fn backoff_delay(retry: u32) -> u64 {
    (1u64 << retry.min(6)).min(60)
}

fn detected_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_never_returns_zero() {
        assert_eq!(floor(0), 1);
    }
}
EOF

printf '/target\nCargo.lock\n' > "$project/.gitignore"

# The "agent's" ratchet step recorded in migration-ratchet.tape: anchor the
# enforcement site, add a real capping test, flip the evidence honestly.
cat > "$work_dir/apply_ratchet.py" <<'EOF'
src = open('src/lib.rs').read()
src = src.replace("""/// Reconnect backoff in seconds, doubling per retry.
pub fn backoff_delay(retry: u32) -> u64 {""",
"""/// Reconnect backoff in seconds, doubling per retry.
#[shallguard::enforces("REQ-NET-001")]
pub fn backoff_delay(retry: u32) -> u64 {""")
src = src.replace("""    fn floor_never_returns_zero() {
        assert_eq!(floor(0), 1);
    }
}""",
"""    fn floor_never_returns_zero() {
        assert_eq!(floor(0), 1);
    }

    #[shallguard::verifies("REQ-NET-001")]
    #[test]
    fn backoff_is_capped_at_sixty_seconds() {
        assert_eq!(backoff_delay(0), 1);
        assert_eq!(backoff_delay(31), 60);
        assert!(backoff_delay(u32::MAX) <= 60);
    }
}""")
open('src/lib.rs', 'w').write(src)
doc = open('docs/REQUIREMENTS.md').read()
doc = doc.replace("""- **REQ-NET-001** — Reconnect backoff SHALL be capped at 60 seconds.
  *Enforced:* `src/lib.rs` (`backoff_delay`) · *Verified:* ⏳ pending""",
"""- **REQ-NET-001** — Reconnect backoff SHALL be capped at 60 seconds.
  *Enforced:* `src/lib.rs` (`backoff_delay`) · *Verified:* ✅ `src/lib.rs`
  (`backoff_is_capped_at_sixty_seconds`)""")
open('docs/REQUIREMENTS.md', 'w').write(doc)
EOF

shrink_gif() {
    local gif="$1"
    if command -v ffmpeg > /dev/null; then
        ffmpeg -v error -y -i "$gif" -vf \
            "fps=10,split[a][b];[a]palettegen=max_colors=48[p];[b][p]paletteuse=dither=none" \
            "$gif.tmp.gif" && mv "$gif.tmp.gif" "$gif"
    fi
    if command -v gifsicle > /dev/null; then
        gifsicle -O3 --lossy=100 "$gif" -o "$gif.tmp" && mv "$gif.tmp" "$gif"
    fi
}

cd "$project"
git init -q -b master
git add -A
git -c user.name="Demo" -c user.email="demo@example.com" \
    commit -qm "Legacy scheduler with drafted requirements"

# Warm every cache so the recordings show real, instant command output;
# the ratchet edit is applied once so its test build is also warm.
cargo build -q
python3 "$work_dir/apply_ratchet.py"
cargo test --no-run -q > /dev/null 2>&1
git checkout -q -- .

vhs "$demo_dir/migration-bootstrap.tape"

# The bootstrap recording created the baseline live; commit it so the
# ratchet recording starts from the adopted state.
git add .shallguard
git -c user.name="Demo" -c user.email="demo@example.com" \
    commit -qm "Record adoption baseline"

vhs "$demo_dir/migration-ratchet.tape"

cp migration-bootstrap.gif migration-ratchet.gif "$demo_dir/"
shrink_gif "$demo_dir/migration-bootstrap.gif"
shrink_gif "$demo_dir/migration-ratchet.gif"
echo "Regenerated $demo_dir/migration-bootstrap.gif and $demo_dir/migration-ratchet.gif"
