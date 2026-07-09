use agent_rs::records::nested::{ContextMessage, PromptOrigin, SessionModeKind};
use agent_rs::replay::{AgentReplayRecord, ReplayBuilder};
use kosong_rs::message::Message;

#[test]
fn replay_builder_records_messages_tagged_with_runtime_mode() {
    let mut rb = ReplayBuilder::new();
    // Initially normal mode (None = normal)
    rb.set_mode(None);

    let msg = ContextMessage {
        message: Message::user_text("hello"),
        origin: Some(PromptOrigin::User),
        is_error: Some(false),
    };
    rb.push_message(&msg);

    let result = rb.build_result();
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0],
        AgentReplayRecord::Message {
            message: msg.clone(),
            mode: None, // normal
        }
    );
}

#[test]
fn replay_builder_records_mode_transitions() {
    let mut rb = ReplayBuilder::new();

    rb.set_mode(Some(SessionModeKind::Plan));
    rb.push_session_mode_updated(true, Some(SessionModeKind::Plan));

    rb.set_mode(None);
    rb.push_session_mode_updated(false, Some(SessionModeKind::Plan));

    let result = rb.build_result();
    assert_eq!(result.len(), 2);
}

#[test]
fn replay_builder_build_result_for_mode_filters() {
    let mut rb = ReplayBuilder::new();

    // normal message
    rb.set_mode(None);
    rb.push_message(&ContextMessage {
        message: Message::user_text("normal msg"),
        origin: Some(PromptOrigin::User),
        is_error: Some(false),
    });

    // plan message
    rb.set_mode(Some(SessionModeKind::Plan));
    rb.push_message(&ContextMessage {
        message: Message::user_text("plan msg"),
        origin: Some(PromptOrigin::User),
        is_error: Some(false),
    });

    let plan_msgs = rb.build_result_for_mode(Some(SessionModeKind::Plan));
    assert_eq!(plan_msgs.len(), 1);

    let normal_msgs = rb.build_result_for_mode(None);
    assert_eq!(normal_msgs.len(), 1);
}

#[test]
fn replay_builder_remove_last_messages() {
    let mut rb = ReplayBuilder::new();
    rb.set_mode(None);

    let msg1 = ContextMessage {
        message: Message::user_text("first"),
        origin: Some(PromptOrigin::User),
        is_error: Some(false),
    };
    let msg2 = ContextMessage {
        message: Message::user_text("second"),
        origin: Some(PromptOrigin::User),
        is_error: Some(false),
    };
    rb.push_message(&msg1);
    rb.push_message(&msg2);

    rb.remove_last_messages(&[msg2.clone()]);

    let result = rb.build_result();
    assert_eq!(result.len(), 1);
}
