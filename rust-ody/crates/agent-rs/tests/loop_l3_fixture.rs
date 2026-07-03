use std::process::Command;

#[test]
fn loop_l3_binary_runs_end_turn_fixture() {
    let binary = env!("CARGO_BIN_EXE_loop_l3");
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../packages/integration-tests/src/parity/fixtures/loop/end-turn.json"
    );
    let output = Command::new(binary)
        .arg(fixture)
        .output()
        .expect("failed to run loop_l3 binary");

    assert!(
        output.status.success(),
        "loop_l3 failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let snapshot: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("binary output is not valid JSON");

    assert_eq!(snapshot["turnResult"]["stopReason"], "end_turn");
    assert_eq!(snapshot["turnResult"]["steps"], 1);
    assert_eq!(
        snapshot["recordedEvents"].as_array().map(|a| a.len()),
        Some(2)
    );
}

#[test]
fn loop_l3_binary_runs_single_tool_fixture() {
    let binary = env!("CARGO_BIN_EXE_loop_l3");
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../packages/integration-tests/src/parity/fixtures/loop/single-tool-call.json"
    );
    let output = Command::new(binary)
        .arg(fixture)
        .output()
        .expect("failed to run loop_l3 binary");

    assert!(
        output.status.success(),
        "loop_l3 failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let snapshot: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("binary output is not valid JSON");

    assert_eq!(snapshot["turnResult"]["stopReason"], "end_turn");
    assert_eq!(snapshot["turnResult"]["steps"], 2);
    let events = snapshot["recordedEvents"].as_array().unwrap();
    let tool_calls: Vec<_> = events.iter().filter(|e| e["type"] == "tool.call").collect();
    let tool_results: Vec<_> = events
        .iter()
        .filter(|e| e["type"] == "tool.result")
        .collect();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_results.len(), 1);
}
