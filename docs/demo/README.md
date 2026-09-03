# README demo recordings

This page explains how to make the animated recordings in the documentation
again.

The files `dev-workflow.gif`, `review-workflow.gif`, and
`semantic-review.gif` appear in the top-level `README.md`. The files
`migration-bootstrap.gif` and `migration-ratchet.gif` appear in
`docs/MIGRATION.md`. The script `regenerate-migration.sh` makes the
migration recordings.

The tool [VHS](https://github.com/charmbracelet/vhs) records each file from
the committed `.tape` script. The script runs inside a temporary copy of the
README quick-start example project. You can make the recordings again when
the output of the command changes.

The recordings type the commands into the `fish` shell. The shell shows the
`#` comments in gray and the commands in color. The recordings show files
with `bat`, which colors the syntax.

Run the script:

```bash
# needs vhs, ttyd, ffmpeg, fish, bat, and an installed cargo-shallguard on
# PATH; gifsicle is optional but keeps the GIFs small
./docs/demo/regenerate.sh

# record against a local (unpublished) ShallGuard checkout instead of the
# published crate:
SHALLGUARD_PATH=$PWD ./docs/demo/regenerate.sh
```

In a container or a sandbox without a Chromium SUID sandbox, also set the
variable `VHS_NO_SANDBOX=true`.

The script records `semantic-review.gif` only when `RECORD_SEMANTIC=1` is
set. The reasons are:

- The recording runs the configured Codex provider twice: one warm-up run
  and the recorded run.
- The recording needs a `codex` program with a login and the tool
  `cargo-llvm-cov`.
- The wording of the agent verdict changes between runs.

Run it with:

```bash
RECORD_SEMANTIC=1 ./docs/demo/regenerate.sh
```

Note for the `claude` provider: ShallGuard starts it with `--bare`, which
skips the user settings. On a machine with a login through `claude /login`
and no `ANTHROPIC_API_KEY`, the provider then reports "Not logged in". For
this reason, the recordings use Codex.
