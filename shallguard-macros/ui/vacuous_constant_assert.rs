use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
fn constant_assertion() {
    assert!(true);
}

fn main() {}
