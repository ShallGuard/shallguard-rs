# shallguard-macros

This is the procedural-macro implementation crate for
[ShallGuard](https://github.com/sigi64/shallguard).

Applications should depend on `shallguard`, which re-exports the public anchor
API:

```rust
#[shallguard::enforces("REQ-RD-001")]
fn enforce_contract() {}

#[shallguard::verifies("REQ-RD-001")]
#[test]
fn contract_is_enforced() {}
```

Do not add a direct dependency on `shallguard-macros` unless you are developing
ShallGuard itself.
