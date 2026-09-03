use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
fn identical_sides_with_fallible_arguments() {
    assert_eq!(compute().unwrap(), compute().unwrap());
}

fn compute() -> Result<u32, String> {
    Ok(7)
}

fn main() {}
