use std::process::Command;

#[test]
fn session_mode_l3_plan_enter_exit() {
    let binary = env!("CARGO_BIN_EXE_session_mode_l3");
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../packages/integration-tests/src/parity/fixtures/session-mode/plan-enter-exit.json"
    );

    let output = Command::new(binary)
        .arg(fixture)
        .output()
        .expect("Failed to run session_mode_l3 binary");

    assert!(
        output.status.success(),
        "golden binary failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();

    // Verify expected events: enter + exit
    assert!(
        !lines.is_empty(),
        "Expected at least one event line, got none"
    );

    let first: serde_json::Value =
        serde_json::from_str(lines[0]).expect("first line not valid JSON");
    assert_eq!(first["type"], "session_mode.enter");
    assert_eq!(first["id"], "plan-fixture-1");

    let last: serde_json::Value =
        serde_json::from_str(lines.last().unwrap()).expect("last line not valid JSON");
    assert_eq!(last["type"], "session_mode.exit");
}
