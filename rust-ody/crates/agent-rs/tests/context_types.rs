use agent_rs::context::{AgentContextData, USER_PROMPT_ORIGIN};
use agent_rs::records::nested::{ContextMessage, PromptOrigin};

#[test]
fn user_prompt_origin_constant_matches_user_variant() {
    assert_eq!(USER_PROMPT_ORIGIN, PromptOrigin::User);
}

#[test]
fn agent_context_data_serializes_with_camel_case_token_count() {
    let data = AgentContextData {
        history: vec![ContextMessage {
            message: kosong_rs::message::Message::user_text("hi"),
            origin: Some(PromptOrigin::User),
            is_error: None,
        }],
        token_count: 42,
    };
    let json = serde_json::to_string(&data).unwrap();
    assert!(json.contains("\"tokenCount\":42"), "got {}", json);
}
