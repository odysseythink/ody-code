use agent_rs::tool::{ToolInfo, ToolSource};

#[test]
fn rust_tools_fixture_round_trips() {
    let json = include_str!("fixtures/tools-rust.json");
    let infos: Vec<ToolInfo> = serde_json::from_str(json).unwrap();

    let active: Vec<_> = infos
        .iter()
        .filter(|i| i.active)
        .map(|i| i.name.as_str())
        .collect();
    assert!(active.contains(&"Read"));
    assert!(active.contains(&"Grep"));
    assert!(active.contains(&"custom_user_tool"));

    let read = infos.iter().find(|i| i.name == "Read").unwrap();
    assert_eq!(read.source, ToolSource::Builtin);

    let custom = infos.iter().find(|i| i.name == "custom_user_tool").unwrap();
    assert_eq!(custom.source, ToolSource::User);

    let re = serde_json::to_string_pretty(&infos).unwrap();
    let infos2: Vec<ToolInfo> = serde_json::from_str(&re).unwrap();
    assert_eq!(infos, infos2);
}
