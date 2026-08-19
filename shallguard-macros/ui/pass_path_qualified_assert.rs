use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
fn third_party_macro_reusing_a_std_name() {
    harness::assert!(true);
}

mod harness {
    macro_rules! assert {
        ($value:expr) => {
            if !crate::harness::holds($value) {
                panic!("snapshot mismatch");
            }
        };
    }
    pub(crate) use assert;

    pub fn holds(_: bool) -> bool {
        true
    }
}

fn main() {}
