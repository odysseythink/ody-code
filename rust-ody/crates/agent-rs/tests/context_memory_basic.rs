mod common;

use std::sync::Arc;

use agent_rs::context::{ContextMemory, USER_PROMPT_ORIGIN};
use agent_rs::records::nested::{ContextMessage, PromptOrigin};
use agent_rs::records::AgentRecord;
use common::TestAgent;
use kosong_rs::message::{ContentPart, Message, Role};

fn user_message(text: &str) -> ContextMessage {
    ContextMessage {
        message: Message::user_text(text),
        origin: Some(PromptOrigin::User),
        is_error: None,
    }
}

fn assistant_message(text: &str) -> ContextMessage {
    ContextMessage {
        message: Message::assistant(vec![ContentPart::Text { text: text.into() }], vec![]),
        origin: None,
        is_error: None,
    }
}

#[test]
fn memory_starts_empty_and_exposes_data() {
    let agent = Arc::new(TestAgent::new());
    let memory = ContextMemory::new(agent.clone());
    assert!(memory.history().is_empty());
    assert_eq!(memory.token_count(), 0);
    assert_eq!(memory.last_assistant_at(), None);
    let data = memory.data();
    assert!(data.history.is_empty());
    assert_eq!(data.token_count, 0);
}

#[test]
fn append_user_message_pushes_real_user_message_and_logs() {
    let agent = Arc::new(TestAgent::new());
    let mut memory = ContextMemory::new(agent.clone());
    memory.append_user_message(
        vec![ContentPart::Text {
            text: "hello".into(),
        }],
        USER_PROMPT_ORIGIN,
    );

    assert_eq!(memory.history().len(), 1);
    assert_eq!(memory.history()[0].message.role, Role::User);
    let records = agent.record_log.records.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert!(matches!(
        records[0],
        AgentRecord::ContextAppendMessage { .. }
    ));
}

#[test]
fn append_system_reminder_wraps_content() {
    let agent = Arc::new(TestAgent::new());
    let mut memory = ContextMemory::new(agent.clone());
    memory.append_system_reminder(
        "remember this",
        PromptOrigin::SystemTrigger {
            name: "host".into(),
        },
    );

    let history = memory.history();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].message.role, Role::User);
    let text = match &history[0].message.content[0] {
        ContentPart::Text { text } => text.as_str(),
        _ => panic!("expected text"),
    };
    assert_eq!(text, "<system-reminder>\nremember this\n</system-reminder>");
}

#[test]
fn clear_resets_history_and_emits_callbacks() {
    let agent = Arc::new(TestAgent::new());
    let mut memory = ContextMemory::new(agent.clone());
    memory.append_user_message(
        vec![ContentPart::Text { text: "a".into() }],
        USER_PROMPT_ORIGIN,
    );
    memory.clear();

    assert!(memory.history().is_empty());
    assert_eq!(memory.token_count(), 0);
    assert_eq!(memory.last_assistant_at(), None);
    let records = agent.record_log.records.lock().unwrap();
    assert!(records
        .iter()
        .any(|r| matches!(r, AgentRecord::ContextClear { .. })));
    let resets = agent.micro_compaction.0.lock().unwrap();
    assert_eq!(resets.as_slice(), &[0]);
    let injections = agent.injection.0.lock().unwrap();
    assert_eq!(injections.as_slice(), &["clear".to_string()]);
}

#[test]
fn messages_are_projected_and_healed() {
    let agent = Arc::new(TestAgent::new());
    let mut memory = ContextMemory::new(agent.clone());
    memory.append_user_message(
        vec![ContentPart::Text { text: "hi".into() }],
        USER_PROMPT_ORIGIN,
    );
    memory.append_user_message(
        vec![ContentPart::Text {
            text: "there".into(),
        }],
        USER_PROMPT_ORIGIN,
    );

    let messages = memory.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role, Role::User);
    assert_eq!(
        messages[0].content,
        vec![ContentPart::Text {
            text: "hi\n\nthere".into()
        }]
    );
}

#[test]
fn last_assistant_at_updated_on_assistant_push() {
    let agent = Arc::new(TestAgent::new());
    let mut memory = ContextMemory::new(agent.clone());
    memory.append_message(assistant_message("ok"));
    assert_eq!(memory.last_assistant_at(), Some(12345));
}
