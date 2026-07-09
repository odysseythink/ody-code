use agent_rs::tool::{ToolInfo, ToolSource};

#[test]
fn ts_tools_fixture_matches_rust_expectations() {
    let json = include_str!("fixtures/tools-ts.json");
    let infos: Vec<ToolInfo> = serde_json::from_str(json).unwrap();

    let active: Vec<_> = infos
        .iter()
        .filter(|i| i.active)
        .map(|i| i.name.as_str())
        .collect();
    assert!(active.contains(&"Read"));
    assert!(active.contains(&"Grep"));
    assert!(active.contains(&"custom_user_tool"));

    let custom = infos.iter().find(|i| i.name == "custom_user_tool").unwrap();
    assert_eq!(custom.source, ToolSource::User);
}
