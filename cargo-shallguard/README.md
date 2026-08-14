# cargo-shallguard

`cargo-shallguard` is the Cargo subcommand for
[ShallGuard](https://github.com/sigi64/shallguard), a requirement-assurance
tool for Rust repositories.

Install the command from crates.io:

```bash
cargo install cargo-shallguard --locked
```

Then run it from a repository containing `shallguard.toml`:

```bash
cargo shallguard fmt --check
cargo shallguard check
```

See the [ShallGuard repository](https://github.com/sigi64/shallguard) for the
configuration reference, complete command guide, and library API.
