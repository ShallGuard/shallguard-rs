//! Compile-time acceptance tests for the vacuity front line.
//!
//! trybuild asserts by panicking when its `TestCases` drop, so the
//! oracle of these tests genuinely lives outside their bodies — the
//! visible `oracle = "compile"` opt-out below is the escape hatch
//! working as specified, and the checker counts and lists it.

use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-TRACE-016", oracle = "compile")]
#[test]
fn definitely_vacuous_bodies_fail_to_compile() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("ui/vacuous_empty.rs");
    cases.compile_fail("ui/vacuous_constant_assert.rs");
    cases.pass("ui/pass_unwrap.rs");
    cases.pass("ui/pass_question_mark.rs");
    cases.pass("ui/pass_should_panic_expected.rs");
    cases.pass("ui/pass_result_tail.rs");
    cases.pass("ui/pass_idempotency_assert.rs");
}

#[shallguard::verifies("REQ-TRACE-017", oracle = "compile")]
#[test]
fn oracle_classes_are_a_closed_set() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("ui/oracle_unknown.rs");
    cases.pass("ui/pass_oracle_panic.rs");
    cases.pass("ui/pass_oracle_compile.rs");
    cases.pass("ui/pass_oracle_external.rs");
}
