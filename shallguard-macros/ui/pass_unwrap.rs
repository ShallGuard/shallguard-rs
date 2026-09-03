use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
fn unwrap_is_a_failure_path() {
    "7".parse::<u32>().unwrap();
}

fn main() {}
