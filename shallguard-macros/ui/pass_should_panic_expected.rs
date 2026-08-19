use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
#[should_panic(expected = "zero floor")]
fn expected_panic_is_the_oracle() {
    scenario();
}

fn scenario() {}

fn main() {}
