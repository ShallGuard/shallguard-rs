# shallguard-macros

This crate implements the procedural macros of
[ShallGuard](https://github.com/shallguard/shallguard-rs).

Your application must depend on the `shallguard` crate. That crate exports
the public anchor API:

```rust
#[shallguard::enforces("REQ-RD-001")]
fn enforce_contract() {}

#[shallguard::verifies("REQ-RD-001")]
#[test]
fn contract_is_enforced() {}
```

Do not add a direct dependency on `shallguard-macros`. The only exception is
work on ShallGuard itself.
