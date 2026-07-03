use agent_rs::context::tokens::{
    estimate_tokens, estimate_tokens_for_content_part, estimate_tokens_for_message,
    estimate_tokens_for_messages,
};
use kosong_rs::message::{ContentPart, Message, Role, ToolCall};

#[test]
fn estimate_ascii_tokens_ceils_division_by_four() {
    assert_eq!(estimate_tokens("hello"), 2);
    assert_eq!(estimate_tokens(""), 0);
    assert_eq!(estimate_tokens("abcd"), 1);
    assert_eq!(estimate_tokens("abcde"), 2);
}

#[test]
fn estimate_non_ascii_counts_one_per_char() {
    assert_eq!(estimate_tokens("你好"), 2);
    assert_eq!(estimate_tokens("a你b"), 2);
}

#[test]
fn estimate_message_counts_role_content_and_tool_calls() {
    let message = Message {
        role: Role::User,
        name: None,
        content: vec![ContentPart::Text {
            text: "hello".into(),
        }],
        tool_calls: vec![ToolCall {
            call_type: "function".into(),
            id: "call_1".into(),
            name: "ToolName".into(),
            arguments: Some("{\"x\":1}".into()),
            extras: None,
            stream_index: None,
        }],
        tool_call_id: None,
        partial: None,
    };
    assert_eq!(estimate_tokens_for_message(&message), 1 + 2 + 2 + 2);
}

#[test]
fn estimate_messages_sums_individual_messages() {
    let m1 = Message::user_text("hello");
    let m2 = Message::assistant(
        vec![ContentPart::Text {
            text: "world".into(),
        }],
        vec![],
    );
    // user(hello)=1+2=3, assistant(world)=3+2=5
    assert_eq!(estimate_tokens_for_messages(&[m1, m2]), 8);
}

#[test]
fn estimate_think_part_counts_think_text() {
    let part = ContentPart::Think {
        think: "think".into(),
        encrypted: None,
    };
    assert_eq!(estimate_tokens_for_content_part(&part), 2);
}

#[test]
fn estimate_non_text_part_is_zero() {
    let part = ContentPart::ImageUrl {
        image_url: kosong_rs::message::UrlPayload {
            url: "http://x".into(),
            id: None,
        },
    };
    assert_eq!(estimate_tokens_for_content_part(&part), 0);
}

#[test]
fn estimate_tool_call_with_none_arguments_uses_null() {
    let message = Message {
        role: Role::Assistant,
        name: None,
        content: vec![],
        tool_calls: vec![ToolCall {
            call_type: "function".into(),
            id: "c".into(),
            name: "N".into(),
            arguments: None,
            extras: None,
            stream_index: None,
        }],
        tool_call_id: None,
        partial: None,
    };
    assert_eq!(estimate_tokens_for_message(&message), 3 + 1 + 1);
}
