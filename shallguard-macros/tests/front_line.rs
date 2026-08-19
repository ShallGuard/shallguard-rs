//! Compile-time acceptance tests for the vacuity front line and the
//! closed oracle set.
//!
//! trybuild asserts by panicking when its `TestCases` drop, so the
//! oracle of this test genuinely lives outside its body — the visible
//! `oracle = "compile"` opt-out below is the escape hatch working as
//! specified, and the checker counts and lists it. One shared harness
//! keeps the fixture crate to a single cargo build.

use shallguard_macros as shallguard;

#[shallguard::verifies("REQ-TRACE-016", "REQ-TRACE-017", oracle = "compile")]
#[test]
fn front_line_rejects_vacuity_and_enforces_oracle_classes() {
    let cases = trybuild::TestCases::new();
    // Definitely-vacuous bodies fail to compile (REQ-TRACE-016).
    cases.compile_fail("ui/vacuous_empty.rs");
    cases.compile_fail("ui/vacuous_constant_assert.rs");
    cases.compile_fail("ui/vacuous_debug_assert.rs");
    cases.compile_fail("ui/vacuous_not_equals.rs");
    cases.compile_fail("ui/vacuous_lazy_message.rs");
    // Fallible shapes compile: the zero-false-positive contract.
    cases.pass("ui/pass_unwrap.rs");
    cases.pass("ui/pass_question_mark.rs");
    cases.pass("ui/pass_should_panic_expected.rs");
    cases.pass("ui/pass_result_tail.rs");
    cases.pass("ui/pass_idempotency_assert.rs");
    cases.pass("ui/pass_guard_arm_assert_false.rs");
    cases.pass("ui/pass_identical_impure_sides.rs");
    cases.pass("ui/pass_assert_ne_hex.rs");
    cases.pass("ui/pass_path_qualified_assert.rs");
    // The oracle opt-out is a closed set (REQ-TRACE-017).
    cases.compile_fail("ui/oracle_unknown.rs");
    cases.pass("ui/pass_oracle_panic.rs");
    cases.pass("ui/pass_oracle_compile.rs");
    cases.pass("ui/pass_oracle_external.rs");
}
