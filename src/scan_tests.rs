//! Unit tests for anchor scanning and oracle-argument parsing.

use super::*;

fn scan_text(text: &str) -> Anchors {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "shallguard-scan-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let sub = dir.join("src");
    std::fs::create_dir_all(&sub).expect("BUG: temp dir creation failed");
    std::fs::write(sub.join("sample.rs"), text).expect("BUG: temp write failed");
    let anchors = scan(&dir, &["src"]).expect("BUG: scan failed");
    std::fs::remove_dir_all(&dir).ok();
    anchors
}

#[shallguard::verifies("REQ-TRACE-001")]
#[test]
fn comments_are_never_anchors() {
    let anchors = scan_text(
        "\
// Enforces: REQ-HRS-003 - comments no longer anchor anything.
fn a() {}

// REQ-HRS-004: bare comment, not even a reference.
fn b() {}
",
    );
    assert!(anchors.enforcement.is_empty());
    assert!(anchors.references.is_empty());
    assert!(anchors.verification.is_empty());
    assert!(anchors.invalid.is_empty());
}

#[shallguard::verifies("REQ-TRACE-003")]
#[test]
fn enforces_here_macro_in_statement_and_item_position() {
    let anchors = scan_text(
        "\
shallguard::enforces_here!(\"REQ-DYN-016\");

fn a(configured: usize) -> usize {
if configured == 0 {
    shallguard::enforces_here!(\"REQ-CM-038\", \"REQ-SAFE-004\");
    return 1;
}
match configured {
    1 => {
        shallguard::enforces_here!(\"REQ-CM-039\");
        1
    }
    n => n,
}
}
",
    );
    let all: Vec<&[String]> = anchors
        .enforcement
        .iter()
        .map(|a| a.ids.as_slice())
        .collect();
    assert_eq!(anchors.enforcement.len(), 3, "{all:?}");
    assert!(
        anchors
            .enforcement
            .iter()
            .any(|a| a.ids == vec!["REQ-CM-038", "REQ-SAFE-004"])
    );
    assert!(
        anchors
            .enforcement
            .iter()
            .any(|a| a.ids == vec!["REQ-CM-039"])
    );
    assert!(
        anchors
            .enforcement
            .iter()
            .any(|a| a.ids == vec!["REQ-DYN-016"])
    );
    let branch = anchors
        .enforcement
        .iter()
        .find(|anchor| anchor.ids.contains(&"REQ-CM-038".to_string()))
        .expect("branch anchor exists");
    assert_eq!(branch.scope_kind, EnforcementScopeKind::Block);
    assert!(branch.scope.is_some());
    let item = anchors
        .enforcement
        .iter()
        .find(|anchor| anchor.ids == ["REQ-DYN-016"])
        .expect("item anchor exists");
    assert_eq!(item.scope_kind, EnforcementScopeKind::Unmapped);
    assert!(item.scope.is_none());
    assert!(anchors.references.contains_key("REQ-CM-038"));
}

#[shallguard::verifies("REQ-TRACE-002")]
#[test]
fn attribute_anchors_record_executable_and_structural_scopes() {
    let anchors = scan_text(
        "\
#[shallguard::enforces(\"REQ-CM-001\")]
fn executable() {
do_work();
}

struct Config {
#[shallguard::enforces(\"REQ-CF-001\")]
value: usize,
}

#[shallguard::enforces(\"REQ-CF-002\")]
const DEFAULT: usize = 1;

#[shallguard::enforces(\"REQ-CF-003\")]
pub use other::Thing;
",
    );

    let function = anchors
        .enforcement
        .iter()
        .find(|anchor| anchor.ids == ["REQ-CM-001"])
        .expect("function anchor exists");
    assert_eq!(function.scope_kind, EnforcementScopeKind::FunctionBody);
    assert_eq!(function.scope.expect("function has scope").start_line, 2);

    let field = anchors
        .enforcement
        .iter()
        .find(|anchor| anchor.ids == ["REQ-CF-001"])
        .expect("field anchor exists");
    assert_eq!(field.scope_kind, EnforcementScopeKind::Structural);

    let constant = anchors
        .enforcement
        .iter()
        .find(|anchor| anchor.ids == ["REQ-CF-002"])
        .expect("constant anchor exists");
    assert_eq!(constant.scope_kind, EnforcementScopeKind::ConstInitializer);

    let import = anchors
        .enforcement
        .iter()
        .find(|anchor| anchor.ids == ["REQ-CF-003"])
        .expect("use anchor exists");
    assert_eq!(import.scope_kind, EnforcementScopeKind::Structural);
}

#[shallguard::verifies("REQ-TRACE-003")]
#[test]
fn enforces_here_nested_in_another_macro_body_is_found() {
    let anchors = scan_text(
        "\
async fn actor_loop() {
tokio::select! {
    event = rx.recv() => {
        match event {
            Command::Create => {
                shallguard::enforces_here!(\"REQ-CM-037\");
            }
            _ => {}
        }
    }
    _ = tick.tick() => {
        shallguard::enforces_here!(\"REQ-OP-048\", \"REQ-OP-046\");
    }
}
}
",
    );
    assert_eq!(anchors.enforcement.len(), 2);
    assert!(
        anchors
            .enforcement
            .iter()
            .any(|a| a.ids == vec!["REQ-CM-037"])
    );
    assert!(
        anchors
            .enforcement
            .iter()
            .any(|a| a.ids == vec!["REQ-OP-048", "REQ-OP-046"])
    );
}

#[test]
fn enforces_here_without_id_is_invalid() {
    let anchors = scan_text(
        "\
fn a() {
shallguard::enforces_here!();
}
",
    );
    assert!(anchors.enforcement.is_empty());
    assert_eq!(anchors.invalid.len(), 1);
    assert!(
        anchors.invalid[0]
            .message
            .contains("without a requirement ID")
    );
}

#[shallguard::verifies("REQ-TRACE-002")]
#[test]
fn field_and_variant_attributes_are_anchors() {
    let anchors = scan_text(
        "\
#[shallguard::enforces]
struct Config {
#[shallguard::enforces(\"REQ-CM-036\")]
max_creating_providers: usize,
other: u32,
}

#[shallguard::enforces(\"REQ-CM-048\")]
enum Input {
#[shallguard::enforces(\"REQ-CM-049\")]
Evict {
    #[shallguard::enforces(\"REQ-CM-050\")]
    id: u32,
},
}
",
    );
    let all: Vec<&str> = anchors
        .enforcement
        .iter()
        .flat_map(|a| a.ids.iter().map(String::as_str))
        .collect();
    assert!(all.contains(&"REQ-CM-036"));
    assert!(all.contains(&"REQ-CM-048"));
    assert!(all.contains(&"REQ-CM-049"));
    assert!(all.contains(&"REQ-CM-050"));
}

#[shallguard::verifies("REQ-TRACE-001")]
#[test]
fn anchor_text_inside_strings_is_invisible() {
    let anchors = scan_text(
        r##"
fn a() {
let _s = "shallguard::enforces_here!(\"REQ-HRS-001\") - not an anchor";
let _r = r#"#[shallguard::enforces("REQ-HRS-001")] - not an anchor"#;
}
/* shallguard::enforces_here!("REQ-HRS-001"); - inside a block comment, invisible */
"##,
    );
    assert!(anchors.enforcement.is_empty());
    assert!(anchors.references.is_empty());
}

#[shallguard::verifies("REQ-TRACE-014", "REQ-TRACE-017")]
#[test]
fn unknown_oracle_class_is_not_a_suppression() {
    let anchors = scan_text(
        "\
#[cfg(test)]
mod tests {
#[shallguard::verifies(\"REQ-RD-006\", oracle = \"trustme\")]
#[test]
fn cannot_fail() {}
}
",
    );
    // The anchor stays, but the invalid class does not suppress
    // vacuity analysis and is reported.
    assert_eq!(anchors.verification.len(), 1);
    assert!(matches!(
        anchors.verification[0].oracle,
        crate::oracle::OracleClass::Vacuous(_)
    ));
    assert_eq!(anchors.invalid.len(), 1);
    assert!(anchors.invalid[0].message.contains("unknown oracle class"));
}

#[shallguard::verifies("REQ-TRACE-014")]
#[test]
fn raw_string_oracle_classes_decode_to_their_value() {
    let anchors = scan_text(
        "\
#[cfg(test)]
mod tests {
#[shallguard::verifies(\"REQ-RD-006\", oracle = r\"compile\")]
#[test]
fn compile_oracle() {}
}
",
    );
    assert_eq!(anchors.verification.len(), 1);
    assert_eq!(
        anchors.verification[0].oracle,
        crate::oracle::OracleClass::Suppressed("compile".to_string())
    );
    assert!(anchors.invalid.is_empty());
}

#[shallguard::verifies("REQ-TRACE-004")]
#[test]
fn verifies_attribute_needs_an_enabled_test() {
    let anchors = scan_text(
        "\
#[shallguard::verifies(\"REQ-RD-006\")]
#[test]
fn valid_test() {}

#[shallguard::verifies(\"REQ-RD-007\")]
fn not_a_test() {}

#[shallguard::verifies(\"REQ-RD-008\")]
#[test]
#[ignore]
fn ignored_test() {}
",
    );
    let ids: Vec<&str> = anchors.verified_ids().collect();
    assert_eq!(ids, vec!["REQ-RD-006"]);
    assert_eq!(anchors.verification[0].test_fn, "valid_test");
    assert_eq!(anchors.invalid.len(), 2);
}

#[shallguard::verifies("REQ-TRACE-002")]
#[test]
fn enforces_attribute_on_items_and_impl_fns() {
    let anchors = scan_text(
        "\
#[shallguard::enforces(\"REQ-HRS-002\", \"REQ-SAFE-001\")]
fn site() {}

struct S;
impl S {
#[shallguard::enforces(\"REQ-RD-007\")]
fn method(&self) {}
}

mod inner {
#[shallguard::enforces(\"REQ-DYN-016\")]
pub struct Gate;
}
",
    );
    let all: Vec<&str> = anchors
        .enforcement
        .iter()
        .flat_map(|a| a.ids.iter().map(String::as_str))
        .collect();
    assert!(all.contains(&"REQ-HRS-002"));
    assert!(all.contains(&"REQ-SAFE-001"));
    assert!(all.contains(&"REQ-RD-007"));
    assert!(all.contains(&"REQ-DYN-016"));
    // Attributes never count as verification evidence.
    assert!(anchors.verification.is_empty());
}

#[test]
fn qualified_and_wrapped_attributes_are_found_with_lines() {
    let anchors = scan_text(
        "\
mod outer {
mod tests {
#[shallguard::verifies(
\"REQ-RD-007\",
\"REQ-RD-008\",
)]
#[tokio::test]
async fn t2() {}
}
}
",
    );
    assert_eq!(anchors.verification.len(), 1);
    assert_eq!(anchors.verification[0].ids.len(), 2);
    assert_eq!(anchors.verification[0].line, 3);
    assert_eq!(anchors.verification[0].inline_modules, ["outer", "tests"]);
}

#[shallguard::verifies("REQ-TRACE-017")]
#[test]
fn duplicate_and_non_string_oracle_values_are_invalid() {
    let anchors = scan_text(
        "\
#[cfg(test)]
mod tests {
    #[shallguard::verifies(\"REQ-RD-006\", oracle = \"panic\", oracle = \"vibes\")]
    #[test]
    fn duplicated() {}

    #[shallguard::verifies(\"REQ-RD-007\", oracle = 3)]
    #[test]
    fn non_string() {}
}
",
    );
    // Neither malformed opt-out suppresses; both are reported.
    assert_eq!(anchors.verification.len(), 2);
    assert!(
        anchors
            .verification
            .iter()
            .all(|anchor| matches!(anchor.oracle, crate::oracle::OracleClass::Vacuous(_)))
    );
    assert_eq!(anchors.invalid.len(), 2);
    assert!(
        anchors.invalid[0]
            .message
            .contains("duplicate oracle argument")
    );
    assert!(
        anchors.invalid[1]
            .message
            .contains("must be a string literal")
    );
}
