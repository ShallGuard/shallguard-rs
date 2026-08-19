use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
fn comparison_is_not_a_macro() {
    let flag = 3;
    let _ = flag != 4;
}

fn main() {}
