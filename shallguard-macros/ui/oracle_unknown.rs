use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001", oracle = "vibes")]
#[test]
fn unknown_oracle() {
    scenario();
}

fn scenario() {}

fn main() {}
