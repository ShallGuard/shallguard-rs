use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
fn fails_via_err_return() -> Result<(), String> {
    run_checks()
}

fn run_checks() -> Result<(), String> {
    Ok(())
}

fn main() {}
