use super::*;

#[test]
fn parses_workspace_code_regions_and_deduplicates_instantiations() {
    let json = br#"
    {
      "type": "llvm.coverage.json.export",
      "data": [{
        "functions": [
          {
            "filenames": ["/workspace/crate/src/lib.rs", "/registry/dependency.rs"],
            "regions": [
              [10, 5, 12, 6, 0, 0, 0, 0],
              [20, 1, 20, 8, 7, 1, 0, 0],
              [30, 1, 30, 8, 9, 0, 0, 1]
            ]
          },
          {
            "filenames": ["/workspace/crate/src/lib.rs"],
            "regions": [[10, 5, 12, 6, 3, 0, 0, 0]]
          }
        ]
      }]
    }
    "#;

    let parsed = parse_export(Path::new("/workspace"), json).expect("LLVM fixture parses");
    let regions = parsed.regions_for("crate/src/lib.rs");

    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].execution_count, 3);
    assert_eq!(regions[0].range.start_line, 10);
    assert!(parsed.regions_for("dependency.rs").is_empty());
}

#[test]
fn rejects_an_unknown_export_type() {
    let error = parse_export(
        Path::new("/workspace"),
        br#"{"type":"something.else","data":[]}"#,
    )
    .expect_err("unknown LLVM schema must fail");

    assert!(error.to_string().contains("unexpected LLVM export type"));
}

#[test]
fn source_paths_must_stay_inside_the_workspace() {
    assert_eq!(
        workspace_relative(
            Path::new("/workspace"),
            Path::new("/workspace/crate/src/lib.rs")
        ),
        Some("crate/src/lib.rs".to_string())
    );
    assert_eq!(
        workspace_relative(Path::new("/workspace"), Path::new("/other/src/lib.rs")),
        None
    );
    assert_eq!(
        workspace_relative(Path::new("/workspace"), Path::new("../outside.rs")),
        None
    );
}
