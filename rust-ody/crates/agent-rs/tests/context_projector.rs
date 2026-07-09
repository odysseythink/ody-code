use agent_rs::context::projector::{drop_orphan_tool_results, project};
use agent_rs::context::types::{ContextMessage, PromptOrigin};
use kosong_rs::message::{ContentPart, Message, Role, ToolCall};

fn user(text: &str) -> ContextMessage {
    ContextMessage {
        message: Message::user_text(text),
        origin: Some(PromptOrigin::User),
        is_error: None,
    }
}

fn user_with_parts(parts: Vec<ContentPart>) -> ContextMessage {
    ContextMessage {
        message: Message {
            role: Role::User,
            name: None,
            content: parts,
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        },
        origin: Some(PromptOrigin::User),
        is_error: None,
    }
}

fn system_reminder(text: &str) -> ContextMessage {
    ContextMessage {
        message: Message::user_text(text),
        origin: Some(PromptOrigin::Injection {
            variant: "host".into(),
        }),
        is_error: None,
    }
}

fn assistant_text(text: &str) -> ContextMessage {
    ContextMessage {
        message: Message::assistant(vec![ContentPart::Text { text: text.into() }], vec![]),
        origin: None,
        is_error: None,
    }
}

fn assistant_with_call(id: &str, text: &str) -> ContextMessage {
    ContextMessage {
        message: Message::assistant(
            if text.is_empty() {
                vec![]
            } else {
                vec![ContentPart::Text { text: text.into() }]
            },
            vec![ToolCall {
                call_type: "function".into(),
                id: id.into(),
                name: "SomeTool".into(),
                arguments: Some("{}".into()),
                extras: None,
                stream_index: None,
            }],
        ),
        origin: None,
        is_error: None,
    }
}

fn tool_result(id: &str, text: &str) -> ContextMessage {
    ContextMessage {
        message: Message {
            role: Role::Tool,
            name: None,
            content: vec![ContentPart::Text { text: text.into() }],
            tool_calls: vec![],
            tool_call_id: Some(id.into()),
            partial: None,
        },
        origin: None,
        is_error: None,
    }
}

fn project_and_heal(history: Vec<ContextMessage>) -> Vec<Message> {
    drop_orphan_tool_results(project(&history))
}

#[test]
fn drops_orphan_tool_result() {
    let history = vec![tool_result("orphan", "orphaned"), user("continue")];
    let projected = project_and_heal(history);
    assert!(!projected.iter().any(|m| m.role == Role::Tool));
    assert_eq!(
        projected.iter().map(|m| m.role).collect::<Vec<_>>(),
        vec![Role::User]
    );
}

#[test]
fn keeps_tool_result_with_preceding_call() {
    let history = vec![
        assistant_with_call("ok", ""),
        tool_result("ok", "ok"),
        user("next"),
    ];
    let projected = project_and_heal(history);
    assert_eq!(
        projected.iter().map(|m| m.role).collect::<Vec<_>>(),
        vec![Role::Assistant, Role::Tool, Role::User]
    );
    assert_eq!(projected[1].tool_call_id, Some("ok".into()));
}

#[test]
fn drops_only_orphan_and_keeps_valid_exchange() {
    let history = vec![
        tool_result("orphan", "orphaned result at head"),
        assistant_with_call("ok", "calling tool"),
        tool_result("ok", "ok"),
    ];
    let projected = project_and_heal(history);
    assert_eq!(
        projected.iter().map(|m| m.role).collect::<Vec<_>>(),
        vec![Role::Assistant, Role::Tool]
    );
    assert_eq!(projected[1].tool_call_id, Some("ok".into()));
}

#[test]
fn drops_tool_result_appearing_before_its_call() {
    let history = vec![
        tool_result("late", "too early"),
        assistant_with_call("late", ""),
    ];
    let projected = project_and_heal(history);
    assert_eq!(
        projected.iter().map(|m| m.role).collect::<Vec<_>>(),
        vec![Role::Assistant]
    );
}

#[test]
fn preserves_tool_message_without_tool_call_id() {
    let history = vec![ContextMessage {
        message: Message {
            role: Role::Tool,
            name: None,
            content: vec![ContentPart::Text {
                text: "tool-like output".into(),
            }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        },
        origin: None,
        is_error: None,
    }];
    let projected = project_and_heal(history);
    assert_eq!(
        projected.iter().map(|m| m.role).collect::<Vec<_>>(),
        vec![Role::Tool]
    );
}

#[test]
fn project_alone_does_not_heal_windowed_slice() {
    let slice = vec![tool_result("outside", "result only")];
    let projected = project(&slice);
    assert_eq!(
        projected.iter().map(|m| m.role).collect::<Vec<_>>(),
        vec![Role::Tool]
    );
}

#[test]
fn merges_adjacent_real_user_messages() {
    let history = vec![user("hello"), user("world")];
    let projected = project(&history);
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].role, Role::User);
    assert_eq!(
        projected[0].content,
        vec![ContentPart::Text {
            text: "hello\n\nworld".into()
        }]
    );
}

#[test]
fn does_not_merge_user_with_non_user_origin() {
    let history = vec![user("hello"), system_reminder("reminder")];
    let projected = project(&history);
    assert_eq!(projected.len(), 2);
}

#[test]
fn merges_text_and_appends_non_text_parts() {
    let history = vec![
        user_with_parts(vec![
            ContentPart::Text { text: "a".into() },
            ContentPart::ImageUrl {
                image_url: kosong_rs::message::UrlPayload {
                    url: "u1".into(),
                    id: None,
                },
            },
        ]),
        user_with_parts(vec![
            ContentPart::Text { text: "b".into() },
            ContentPart::ImageUrl {
                image_url: kosong_rs::message::UrlPayload {
                    url: "u2".into(),
                    id: None,
                },
            },
        ]),
    ];
    let projected = project(&history);
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].content.len(), 3);
    assert_eq!(
        projected[0].content[0],
        ContentPart::Text {
            text: "a\n\nb".into()
        }
    );
}

#[test]
fn strips_origin_and_is_error_from_projected_messages() {
    let history = vec![user("x")];
    let projected = project(&history);
    assert!(projected[0].name.is_none());
}

#[test]
fn filters_partial_and_empty_assistant_placeholders() {
    let history = vec![
        ContextMessage {
            message: Message::assistant(vec![], vec![]),
            origin: None,
            is_error: None,
        },
        ContextMessage {
            message: Message {
                role: Role::Assistant,
                name: None,
                content: vec![],
                tool_calls: vec![],
                tool_call_id: None,
                partial: Some(true),
            },
            origin: None,
            is_error: None,
        },
        user("real"),
    ];
    let projected = project(&history);
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].role, Role::User);
}
