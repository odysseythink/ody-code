use agent_rs::usage::UsageStatus;

#[test]
fn rust_usage_fixture_round_trips() {
    let json = include_str!("fixtures/usage-rust.json");
    let status: UsageStatus = serde_json::from_str(json).unwrap();

    let by_model = status.by_model.as_ref().unwrap();
    let kimi = by_model.get("kimi-k2").unwrap();
    assert_eq!(kimi.input_other, 13);
    assert_eq!(kimi.output, 7);
    assert_eq!(kimi.input_cache_read, 2);
    assert_eq!(kimi.input_cache_creation, 1);
    assert_eq!(status.current_turn.as_ref().unwrap().output, 2);

    let re = serde_json::to_string_pretty(&status).unwrap();
    let status2 = serde_json::from_str(&re).unwrap();
    assert_eq!(status, status2);
}
