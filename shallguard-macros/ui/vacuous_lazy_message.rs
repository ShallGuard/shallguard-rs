use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
fn unwrap_in_a_never_evaluated_message() {
    assert_eq!(1, 1, "state: {}", inspect().unwrap());
}

fn inspect() -> Result<u32, String> {
    Ok(7)
}

fn main() {}
