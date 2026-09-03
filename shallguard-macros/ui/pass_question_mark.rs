use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
fn question_mark_is_a_failure_path() -> Result<(), std::num::ParseIntError> {
    let _: u32 = "7".parse()?;
    Ok(())
}

fn main() {}
