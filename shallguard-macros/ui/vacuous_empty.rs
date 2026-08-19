use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
fn cannot_fail() {
    let _ = 1 + 1;
}

fn main() {}
