# cargo-shallguard

`cargo-shallguard` is the Cargo subcommand of
[ShallGuard](https://github.com/sigi64/shallguard). ShallGuard connects
written requirements to the Rust code and the tests of a repository.

Install the command from crates.io:

```bash
cargo install cargo-shallguard --locked
```

Confirm the installed version from any directory:

```bash
cargo shallguard --version
```

Then run the checks in a repository that has a `shallguard.toml` file:

```bash
cargo shallguard fmt --check
cargo shallguard check
```

The [ShallGuard repository](https://github.com/sigi64/shallguard) has the
configuration reference, the complete command guide, and the library API.
