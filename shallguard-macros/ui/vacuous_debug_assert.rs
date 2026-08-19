use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
fn constant_debug_assertion() {
    debug_assert!(true);
}

fn main() {}
