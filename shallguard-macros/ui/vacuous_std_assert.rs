use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
fn standard_qualified_constant_assertion() {
    std::assert!(true);
}

#[shallguard::verifies("REQ-ZZ-002")]
#[test]
fn core_qualified_constant_assertion() {
    core::assert_eq!(1, 0x1);
}

fn main() {}
