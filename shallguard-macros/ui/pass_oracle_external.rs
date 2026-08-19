use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001", oracle = "external")]
#[test]
fn external_oracle() {
    scenario();
}

fn scenario() {}

fn main() {}
