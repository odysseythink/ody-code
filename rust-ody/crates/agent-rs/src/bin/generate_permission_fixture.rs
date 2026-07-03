//! Binary that generates a permission-scenarios-rust.json fixture for TS parity testing.
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Serialize, Deserialize)]
struct FixtureScenario {
    name: String,
    description: String,
    mode: String,
    #[serde(rename = "toolName")]
    tool_name: String,
    rules: Vec<FixtureRule>,
    #[serde(rename = "expectedDecision")]
    expected_decision: String,
    #[serde(rename = "expectedMessageContains")]
    expected_message_contains: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FixtureRule {
    decision: String,
    scope: String,
    pattern: String,
}

fn main() {
    let scenarios = vec![
        FixtureScenario {
            name: "yolo-mode-approve".into(),
            description: "Yolo mode approves any tool".into(),
            mode: "yolo".into(),
            tool_name: "Bash".into(),
            rules: vec![],
            expected_decision: "approve".into(),
            expected_message_contains: None,
        },
        FixtureScenario {
            name: "auto-mode-approve".into(),
            description: "Auto mode approves any tool".into(),
            mode: "auto".into(),
            tool_name: "Bash".into(),
            rules: vec![],
            expected_decision: "approve".into(),
            expected_message_contains: None,
        },
        FixtureScenario {
            name: "manual-fallback-ask".into(),
            description: "Manual mode with no rules asks".into(),
            mode: "manual".into(),
            tool_name: "Bash".into(),
            rules: vec![],
            expected_decision: "ask".into(),
            expected_message_contains: None,
        },
        FixtureScenario {
            name: "deny-rule-blocks".into(),
            description: "User deny rule blocks Write".into(),
            mode: "manual".into(),
            tool_name: "Write".into(),
            rules: vec![FixtureRule {
                decision: "deny".into(),
                scope: "user".into(),
                pattern: "Write".into(),
            }],
            expected_decision: "deny".into(),
            expected_message_contains: Some("denied by permission rule".into()),
        },
        FixtureScenario {
            name: "allow-rule-approves".into(),
            description: "User allow rule approves Read".into(),
            mode: "manual".into(),
            tool_name: "Read".into(),
            rules: vec![FixtureRule {
                decision: "allow".into(),
                scope: "user".into(),
                pattern: "Read".into(),
            }],
            expected_decision: "approve".into(),
            expected_message_contains: None,
        },
    ];

    let out_dir = "tests/fixtures";
    fs::create_dir_all(out_dir).unwrap();
    let json = serde_json::to_string_pretty(&scenarios).unwrap();
    fs::write(format!("{}/permission-scenarios-rust.json", out_dir), json).unwrap();
    eprintln!(
        "Wrote permission fixture to {}/permission-scenarios-rust.json",
        out_dir
    );
}
