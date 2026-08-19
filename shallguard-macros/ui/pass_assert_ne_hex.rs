use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
fn textually_different_literals_may_be_equal() {
    assert_ne!(1, 0x1);
}

fn main() {}
