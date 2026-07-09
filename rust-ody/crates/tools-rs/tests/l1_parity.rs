#[test]
fn runs_all_fixtures() {
    let fixtures = vec![
        "path-policy.json",
        "rule-match.json",
        "schema-validation.json",
        "tool-accesses.json",
        "result-builder.json",
        "file-type.json",
        "rg-locator.json",
        "list-directory.json",
    ];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("packages/integration-tests/src/parity/fixtures/tools-rs");
    for name in fixtures {
        let path = root.join(name);
        assert!(path.exists(), "missing fixture {}", name);
        let out = tools_rs::golden::run_fixture_file(path.to_str().unwrap());
        assert!(!out.is_empty(), "fixture {} produced no output", name);
    }
}
