#[shallguard::enforces("REQ-TRACE-008")]
fn public_enforcement_anchor() -> bool {
    true
}

#[shallguard::verifies("REQ-TRACE-008")]
#[test]
fn public_namespace_exposes_all_anchor_macros() {
    shallguard::enforces_here!("REQ-TRACE-008");
    assert!(public_enforcement_anchor());
}
