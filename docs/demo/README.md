# README demo recordings

`dev-workflow.gif`, `review-workflow.gif`, and `semantic-review.gif` are
embedded in the top-level
`README.md`. They are recorded with [VHS](https://github.com/charmbracelet/vhs)
from the committed `.tape` scripts, inside a throwaway copy of the README
quick-start example project, so they can be regenerated exactly whenever the
CLI output changes:

The recordings type into `fish` (its live syntax highlighting renders the
`#` narration comments gray and commands in color) and display files with
`bat` for syntax highlighting.

```bash
# needs vhs, ttyd, ffmpeg, fish, bat, and an installed cargo-shallguard on
# PATH; gifsicle is optional but keeps the GIFs small
./docs/demo/regenerate.sh

# record against a local (unpublished) ShallGuard checkout instead of the
# published crate:
SHALLGUARD_PATH=$PWD ./docs/demo/regenerate.sh
```

In containers or sandboxes without a Chromium SUID sandbox, additionally
export `VHS_NO_SANDBOX=true`.

`semantic-review.gif` is only re-recorded when `RECORD_SEMANTIC=1` is set,
because it invokes the configured codex provider (twice: a warm-up run plus
the recorded run), requires a logged-in `codex` CLI and `cargo-llvm-cov`,
and the agent verdict wording varies between runs:

```bash
RECORD_SEMANTIC=1 ./docs/demo/regenerate.sh
```

Note for the `claude` provider: ShallGuard invokes it with `--bare`, which
skips user settings — on machines authenticated through `claude /login`
(subscription OAuth, no `ANTHROPIC_API_KEY`) the provider currently reports
"Not logged in", so these recordings use codex.
