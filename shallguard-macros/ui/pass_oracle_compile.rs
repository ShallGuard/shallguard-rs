use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001", oracle = "compile")]
#[test]
fn compile_oracle() {
    scenario();
}

fn scenario() {}

fn main() {}
