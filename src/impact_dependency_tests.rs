use std::collections::BTreeSet;
use std::path::Path;

use super::*;

fn index_fixture(text: &str) -> RevisionIndex {
    let mut index = RevisionIndex::default();
    index_text(text, Path::new("crate/src/lib.rs"), &mut index);
    index
}

fn changed_function(name: &str) -> ChangedDefinition {
    ChangedDefinition {
        change_id: "change-0001".to_string(),
        file: "crate/src/lib.rs".to_string(),
        symbol: format!("crate::fn:{name}"),
        line: 1,
        definitions: BTreeSet::from([Definition {
            module: vec!["crate".to_string()],
            owner: None,
            name: name.to_string(),
            kind: DefinitionKind::Callable,
        }]),
        associated_requirements: BTreeSet::new(),
    }
}

#[shallguard_macros::verifies("REQ-IMP-006")]
#[test]
fn propagates_changed_helper_to_anchored_caller() {
    let index = index_fixture(
        r#"
        fn helper() {}

        #[enforces("REQ-ZZ-001")]
        fn apply() { helper(); }
        "#,
    );
    let analysis = propagate(
        &[changed_function("helper")],
        [index, RevisionIndex::default()],
    );
    assert_eq!(analysis.impacts.len(), 1);
    assert_eq!(analysis.impacts[0].requirement, "REQ-ZZ-001");
    assert_eq!(analysis.impacts[0].class, DependencyClass::Transitive);
    assert!(analysis.claimed_changes.contains("change-0001"));
}

#[shallguard_macros::verifies("REQ-IMP-006")]
#[test]
fn classifies_changed_type_dependency_as_structural() {
    let index = index_fixture(
        r#"
        struct Config { value: u8 }

        #[enforces("REQ-ZZ-001")]
        fn apply(config: Config) { consume(config); }
        "#,
    );
    let change = ChangedDefinition {
        change_id: "change-0001".to_string(),
        file: "crate/src/lib.rs".to_string(),
        symbol: "crate::struct:Config".to_string(),
        line: 2,
        definitions: BTreeSet::from([Definition {
            module: vec!["crate".to_string()],
            owner: None,
            name: "Config".to_string(),
            kind: DefinitionKind::Type,
        }]),
        associated_requirements: BTreeSet::new(),
    };
    let analysis = propagate(&[change], [index, RevisionIndex::default()]);
    assert_eq!(analysis.impacts.len(), 1);
    assert_eq!(analysis.impacts[0].class, DependencyClass::Structural);
    assert_eq!(analysis.impacts[0].requirement, "REQ-ZZ-001");
}

#[test]
fn branch_anchor_does_not_claim_call_outside_its_block() {
    let index = index_fixture(
        r#"
        fn helper() {}

        fn apply(flag: bool) {
            if flag {
                enforces_here!("REQ-ZZ-001");
                guarded();
            }
            helper();
        }
        "#,
    );
    let analysis = propagate(
        &[changed_function("helper")],
        [index, RevisionIndex::default()],
    );
    assert!(analysis.impacts.is_empty());
    assert!(analysis.claimed_changes.is_empty());
}

#[test]
fn resolves_self_associated_function_in_same_impl() {
    let index = index_fixture(
        r#"
        struct Gate;

        impl Gate {
            fn helper() {}

            #[enforces("REQ-ZZ-001")]
            fn apply() { Self::helper(); }
        }
        "#,
    );
    let change = ChangedDefinition {
        change_id: "change-0001".to_string(),
        file: "crate/src/lib.rs".to_string(),
        symbol: "crate::impl:Gate::fn:helper".to_string(),
        line: 4,
        definitions: BTreeSet::from([Definition {
            module: vec!["crate".to_string()],
            owner: Some("Gate".to_string()),
            name: "helper".to_string(),
            kind: DefinitionKind::Callable,
        }]),
        associated_requirements: BTreeSet::new(),
    };
    let analysis = propagate(&[change], [index, RevisionIndex::default()]);
    assert_eq!(analysis.impacts.len(), 1);
    assert_eq!(analysis.impacts[0].requirement, "REQ-ZZ-001");
}

#[test]
fn direct_requirement_is_not_duplicated_as_transitive() {
    let index = index_fixture(
        r#"
        fn helper() {}

        #[enforces("REQ-ZZ-001")]
        fn apply() { helper(); }
        "#,
    );
    let mut change = changed_function("helper");
    change
        .associated_requirements
        .insert("REQ-ZZ-001".to_string());
    let analysis = propagate(&[change], [index, RevisionIndex::default()]);
    assert!(analysis.impacts.is_empty());
}
