use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001", oracle = "panic")]
#[test]
fn oracle_lives_outside_the_body() {
    scenario();
}

fn scenario() {}

fn main() {}
