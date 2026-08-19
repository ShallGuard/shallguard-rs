use shallguard_macros as shallguard;

struct Holder {
    value: (),
}

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
fn assertion_in_struct_field_position() {
    let _ = Holder {
        value: assert!(true),
    };
}

fn main() {}
