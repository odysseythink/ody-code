use agent_rs::agent_loop::types::{LoopTurnStopReason, TurnResult};
use kosong_rs::usage::TokenUsage;
use serde_json;

#[test]
fn turn_result_serializes_like_ts() {
    let result = TurnResult {
        stop_reason: LoopTurnStopReason::EndTurn,
        steps: 1,
        usage: TokenUsage {
            input_other: 10,
            output: 5,
            input_cache_read: 0,
            input_cache_creation: 0,
        },
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"stopReason\":\"end_turn\""), "{}", json);
    assert!(json.contains("\"steps\":1"), "{}", json);
}
