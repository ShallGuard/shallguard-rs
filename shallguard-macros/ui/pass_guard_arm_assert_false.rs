use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
fn rejects_unexpected_success() {
    match parse("input") {
        Err(e) => check(e),
        Ok(_) => assert!(false, "expected parse error"),
    }
}

fn parse(_: &str) -> Result<(), String> {
    Err("boom".to_string())
}

fn check(_: String) {}

fn main() {}
