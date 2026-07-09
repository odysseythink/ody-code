use agent_rs::usage::UsageStatus;

#[test]
fn ts_usage_fixture_matches_rust_expectations() {
    let json = include_str!("fixtures/usage-ts.json");
    let status: UsageStatus = serde_json::from_str(json).unwrap();

    let by_model = status.by_model.as_ref().unwrap();
    let kimi = by_model.get("kimi-k2").unwrap();
    assert_eq!(kimi.input_other, 13);
    assert_eq!(kimi.output, 7);
    assert_eq!(kimi.input_cache_read, 2);
    assert_eq!(kimi.input_cache_creation, 1);
    assert_eq!(status.total.unwrap().output, 7);
    assert_eq!(status.current_turn.unwrap().output, 2);
}
