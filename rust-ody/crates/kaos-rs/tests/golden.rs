use std::path::PathBuf;

use kaos_rs::golden::{run_fixture_file_async, FixtureFile};

fn fixture_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../../packages/integration-tests/src/parity/fixtures/kaos");
    path.push(name);
    path
}

async fn assert_fixture(name: &str) {
    let path = fixture_path(name);
    let content = tokio::fs::read_to_string(&path).await.unwrap();
    let fixture: FixtureFile = serde_json::from_str(&content).unwrap();
    let actual = run_fixture_file_async(path.to_str().unwrap())
        .await
        .unwrap();
    for case in &fixture.cases {
        let actual_result = actual
            .get(&case.name)
            .unwrap_or_else(|| panic!("missing result for case {}", case.name));
        if let Some(ref error) = actual_result.error {
            let expected_error = case
                .expected
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    panic!(
                        "fixture {} case '{}' got error {:?} but expected no error",
                        name, case.name, error
                    )
                });
            assert_eq!(
                error, expected_error,
                "fixture {} case '{}' error mismatch",
                name, case.name
            );
        } else if let Some(ref result) = actual_result.result {
            let expected_result = case.expected.get("result").unwrap_or(&case.expected);
            assert_eq!(
                result,
                expected_result,
                "fixture {} case '{}' result mismatch\ngot: {}\nexpected: {}",
                name,
                case.name,
                serde_json::to_string_pretty(result).unwrap(),
                serde_json::to_string_pretty(expected_result).unwrap(),
            );
        } else {
            panic!(
                "fixture {} case '{}' has neither result nor error",
                name, case.name
            );
        }
    }
}

#[tokio::test]
async fn l1_paths_match_fixture() {
    assert_fixture("l1-paths.json").await;
}

#[tokio::test]
async fn l1_text_decode_match_fixture() {
    assert_fixture("l1-text-decode.json").await;
}

#[tokio::test]
async fn l1_glob_patterns_match_fixture() {
    assert_fixture("l1-glob-patterns.json").await;
}

#[tokio::test]
async fn l1_file_io_match_fixture() {
    assert_fixture("l1-file-io.json").await;
}

#[tokio::test]
async fn l1_directory_ops_match_fixture() {
    assert_fixture("l1-directory-ops.json").await;
}

#[tokio::test]
async fn l1_process_ops_match_fixture() {
    if cfg!(windows) {
        return;
    }
    assert_fixture("l1-process-ops.json").await;
}
