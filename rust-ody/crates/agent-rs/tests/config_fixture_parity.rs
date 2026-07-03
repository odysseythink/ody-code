use agent_rs::config::AgentConfigData;

#[test]
fn rust_config_fixture_round_trips() {
    let json = include_str!("fixtures/config-rust.json");
    let data: AgentConfigData = serde_json::from_str(json).unwrap();
    assert_eq!(data.cwd, "/fixture/cwd");
    assert_eq!(data.model_alias, Some("kimi-k2".into()));
    assert_eq!(data.profile_name, Some("fixture".into()));
    assert_eq!(data.thinking_level, "high");
    assert_eq!(data.system_prompt, "fixture system prompt");
    assert!(data.model_capabilities.thinking);
}
