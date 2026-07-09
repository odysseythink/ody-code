use std::process::Command;

#[test]
fn turn_l3_binary_runs_end_turn_fixture() {
    let binary = env!("CARGO_BIN_EXE_turn_l3");
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../packages/integration-tests/src/parity/fixtures/turn/end-turn.json"
    );
    let output = Command::new(binary)
        .arg(fixture)
        .output()
        .expect("failed to run turn_l3 binary");

    assert!(
        output.status.success(),
        "turn_l3 failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let snapshot: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("binary output is not valid JSON");

    assert_eq!(snapshot["name"], "end-turn");
    let turns = snapshot["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0]["reason"], "completed");
    let events = snapshot["events"].as_array().unwrap();
    assert!(events.iter().any(|e| e["type"] == "turn.started"));
    assert!(events.iter().any(|e| e["type"] == "turn.ended"));
}
