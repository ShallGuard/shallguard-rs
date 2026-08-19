use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
fn equal_integer_spellings() {
    assert_eq!(1, 0x1);
}

fn main() {}
