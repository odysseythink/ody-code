mod common;

use std::sync::Arc;

use agent_rs::context::{ContextMemory, USER_PROMPT_ORIGIN};
use agent_rs::records::nested::{CompactionResult, ContextMessage, PromptOrigin};
use common::TestAgent;
use kosong_rs::message::{ContentPart, Message, Role};

fn user_msg(text: &str) -> ContextMessage {
    ContextMessage {
        message: Message::user_text(text),
        origin: Some(PromptOrigin::User),
        is_error: None,
    }
}

fn user_msg_with_origin(text: &str, origin: PromptOrigin) -> ContextMessage {
    ContextMessage {
        message: Message::user_text(text),
        origin: Some(origin),
        is_error: None,
    }
}

#[test]
fn undo_removes_real_user_prompts_and_updates_token_accounting() {
    let agent = Arc::new(TestAgent::new());
    let mut memory = ContextMemory::new(agent.clone());
    memory.append_user_message(
        vec![ContentPart::Text { text: "a".into() }],
        USER_PROMPT_ORIGIN,
    );
    memory.append_user_message(
        vec![ContentPart::Text { text: "b".into() }],
        USER_PROMPT_ORIGIN,
    );

    memory.undo(1);

    assert_eq!(memory.history().len(), 1);
    assert_eq!(memory.history()[0].message.role, Role::User);
    let text = match &memory.history()[0].message.content[0] {
        ContentPart::Text { text } => text.as_str(),
        _ => panic!("expected text"),
    };
    assert_eq!(text, "a");
    let injections = agent.injection.0.lock().unwrap();
    assert!(injections.iter().any(|s| s.starts_with("removed:")));
}

#[test]
fn undo_skips_injection_and_stops_at_compaction_summary() {
    // When restoring, hitting the compaction_summary boundary is not an error.
    let agent = Arc::new(TestAgent::restoring(0));
    let mut memory = ContextMemory::new(agent.clone());
    memory.append_message(user_msg_with_origin(
        "summary",
        PromptOrigin::CompactionSummary,
    ));
    memory.append_message(user_msg_with_origin(
        "u1",
        PromptOrigin::Injection {
            variant: "test".into(),
        },
    ));
    memory.append_message(user_msg("u2"));

    memory.undo(1);

    // Injection is skipped, u2 is removed, and the compaction_summary boundary is retained.
    assert_eq!(memory.history().len(), 2);
    assert_eq!(
        memory.history()[0].origin,
        Some(PromptOrigin::CompactionSummary)
    );
    assert_eq!(
        memory.history()[1].origin,
        Some(PromptOrigin::Injection {
            variant: "test".into()
        })
    );
}

#[test]
fn undo_with_non_positive_count_is_noop() {
    let agent = Arc::new(TestAgent::new());
    let mut memory = ContextMemory::new(agent.clone());
    memory.append_message(user_msg("x"));
    memory.undo(0);
    memory.undo(-1);
    assert_eq!(memory.history().len(), 1);
}

#[test]
fn apply_compaction_prepends_summary_and_resets_counts() {
    let agent = Arc::new(TestAgent::new());
    let mut memory = ContextMemory::new(agent.clone());
    memory.append_message(user_msg("a"));
    memory.append_message(user_msg("b"));
    memory.append_message(user_msg("c"));

    memory.apply_compaction(CompactionResult {
        summary: "summary text".into(),
        compacted_count: 2,
        tokens_before: 10,
        tokens_after: 3,
    });

    let history = memory.history();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].message.role, Role::Assistant);
    assert_eq!(history[0].origin, Some(PromptOrigin::CompactionSummary));
    assert_eq!(memory.token_count(), 3);
    assert_eq!(memory.token_count_covered_message_count(), 2);
    let injections = agent.injection.0.lock().unwrap();
    assert!(injections.iter().any(|s| s == "compacted:2"));
}

#[test]
fn apply_compaction_with_zero_compacted_count_keeps_history() {
    let agent = Arc::new(TestAgent::new());
    let mut memory = ContextMemory::new(agent.clone());
    memory.append_message(user_msg("a"));

    memory.apply_compaction(CompactionResult {
        summary: "summary".into(),
        compacted_count: 0,
        tokens_before: 1,
        tokens_after: 5,
    });

    assert_eq!(memory.history().len(), 2);
    assert_eq!(memory.token_count(), 5);
}
