# Shallguard glossary

| Term | Meaning |
|---|---|
| Requirement | A stable `REQ-<AREA>-<NNN>` normative statement in a selected Markdown document. |
| Enforcement anchor | `#[enforces]` or `enforces_here!`, linking Rust behavior to a requirement. |
| Verification anchor | `#[verifies]` on an enabled Rust test that provides evidence for a requirement. |
| Traceability | The deterministic relationship among requirements, implementation anchors, and evidence anchors. |
| Coverage | LLVM execution evidence showing whether selected verification tests reached enforcement scopes; not proof of correctness. |
| Impact | A direct, transitive, or structural relationship between a Git change and a requirement. |
| Capsule | A bounded, deterministic bundle of requirement text, source, changes, and available evidence for review. |
| Baseline | A repository-owned, monotonic inventory of accepted historical traceability gaps. |
| Semantic review | Advisory human or model judgment over a frozen capsule; separate from deterministic gates. |
