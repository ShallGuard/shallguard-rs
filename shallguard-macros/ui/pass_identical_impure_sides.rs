use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-ZZ-001")]
#[test]
fn iterator_is_not_stuck() {
    let mut it = [1, 2].into_iter();
    assert_eq!(it.next(), it.next());
}

fn main() {}
