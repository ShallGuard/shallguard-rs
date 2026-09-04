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

/// A custom test harness whose attribute name ends in `test`, as a
/// re-export of the built-in `#[test]` attribute.
mod harness {
    pub use core::prelude::v1::test as container_test;
}

#[shallguard::verifies("REQ-TRACE-004")]
#[harness::container_test]
fn custom_test_attribute_is_accepted_by_the_macro() {
    assert!(public_enforcement_anchor());
}
