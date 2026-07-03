mod common;

use std::sync::Arc;

use agent_rs::context::ContextMemory;
use agent_rs::records::nested::{
    ContextMessage, ExecutableToolErrorResult, ExecutableToolOutput, ExecutableToolResult,
    ExecutableToolSuccessResult, LoopRecordedEvent, PromptOrigin,
};
use common::TestAgent;
use kosong_rs::message::{ContentPart, Message, Role, ToolCall};

fn user_msg(text: &str) -> ContextMessage {
    ContextMessage {
        message: Message::user_text(text),
        origin: Some(PromptOrigin::User),
        is_error: None,
    }
}

fn assistant_with_call(id: &str) -> ContextMessage {
    ContextMessage {
        message: Message::assistant(
            vec![],
            vec![ToolCall {
                call_type: "function".into(),
                id: id.into(),
                name: "read".into(),
                arguments: Some("{}".into()),
                extras: None,
                stream_index: None,
            }],
        ),
        origin: None,
        is_error: None,
    }
}

#[test]
fn append_loop_event_step_begin_end_tracks_open_steps_and_usage() {
    let agent = Arc::new(TestAgent::new());
    let mut memory = ContextMemory::new(agent.clone());

    memory.append_loop_event(LoopRecordedEvent::StepBegin {
        uuid: "s1".into(),
        turn_id: "t1".into(),
        step: 1,
    });
    assert!(memory.has_open_steps());
    assert_eq!(memory.history().len(), 1);

    memory.append_loop_event(LoopRecordedEvent::ContentPartEvent {
        uuid: "p1".into(),
        turn_id: "t1".into(),
        step: 1,
        step_uuid: "s1".into(),
        part: ContentPart::Text { text: "hi".into() },
    });

    memory.append_loop_event(LoopRecordedEvent::StepEnd {
        uuid: "s1".into(),
        turn_id: "t1".into(),
        step: 1,
        usage: Some(kosong_rs::usage::TokenUsage {
            input_cache_read: 1,
            input_cache_creation: 2,
            input_other: 3,
            output: 4,
        }),
        finish_reason: None,
        llm_first_token_latency_ms: None,
        llm_stream_duration_ms: None,
        provider_finish_reason: None,
        raw_finish_reason: None,
    });

    assert!(!memory.has_open_steps());
    assert_eq!(memory.token_count(), 10);
    assert_eq!(memory.token_count_covered_message_count(), 1);
}

#[test]
fn append_loop_event_tool_call_and_result_closes_exchange() {
    let agent = Arc::new(TestAgent::new());
    let mut memory = ContextMemory::new(agent.clone());

    memory.append_loop_event(LoopRecordedEvent::StepBegin {
        uuid: "s1".into(),
        turn_id: "t1".into(),
        step: 1,
    });
    memory.append_loop_event(LoopRecordedEvent::ToolCallEvent {
        uuid: "tc1".into(),
        turn_id: "t1".into(),
        step: 1,
        step_uuid: "s1".into(),
        tool_call_id: "call_1".into(),
        name: "read".into(),
        args: serde_json::json!({"path":"README.md"}),
        description: None,
        display: None,
    });

    memory.append_message(user_msg("deferred until tool result"));

    memory.append_loop_event(LoopRecordedEvent::ToolResultEvent {
        parent_uuid: "s1".into(),
        tool_call_id: "call_1".into(),
        result: ExecutableToolResult::Success(ExecutableToolSuccessResult {
            output: ExecutableToolOutput::Text("ok".into()),
            is_error: None,
            stop_turn: None,
            message: None,
        }),
    });

    let history = memory.history();
    assert_eq!(history.len(), 3); // assistant step, tool result, deferred user
    assert_eq!(history[2].message.role, Role::User);
}

#[test]
fn tool_result_error_formatting_prefixes_system_error() {
    let agent = Arc::new(TestAgent::new());
    let mut memory = ContextMemory::new(agent.clone());

    memory.append_loop_event(LoopRecordedEvent::StepBegin {
        uuid: "s1".into(),
        turn_id: "t1".into(),
        step: 1,
    });
    memory.append_loop_event(LoopRecordedEvent::ToolCallEvent {
        uuid: "tc1".into(),
        turn_id: "t1".into(),
        step: 1,
        step_uuid: "s1".into(),
        tool_call_id: "call_1".into(),
        name: "read".into(),
        args: serde_json::json!({}),
        description: None,
        display: None,
    });
    memory.append_loop_event(LoopRecordedEvent::ToolResultEvent {
        parent_uuid: "s1".into(),
        tool_call_id: "call_1".into(),
        result: ExecutableToolResult::Error(ExecutableToolErrorResult {
            output: ExecutableToolOutput::Text("file not found".into()),
            is_error: true,
            stop_turn: None,
            message: None,
        }),
    });

    let history = memory.history();
    assert_eq!(history[1].message.role, Role::Tool);
    let text = match &history[1].message.content[0] {
        ContentPart::Text { text } => text.as_str(),
        _ => panic!("expected text"),
    };
    assert!(text.starts_with("<system>ERROR: Tool execution failed.</system>"));
    assert!(text.contains("file not found"));
    assert_eq!(history[1].is_error, Some(true));
}

#[test]
fn tool_result_empty_error_uses_combined_status() {
    let agent = Arc::new(TestAgent::new());
    let mut memory = ContextMemory::new(agent.clone());

    memory.append_loop_event(LoopRecordedEvent::StepBegin {
        uuid: "s1".into(),
        turn_id: "t1".into(),
        step: 1,
    });
    memory.append_loop_event(LoopRecordedEvent::ToolCallEvent {
        uuid: "tc1".into(),
        turn_id: "t1".into(),
        step: 1,
        step_uuid: "s1".into(),
        tool_call_id: "call_1".into(),
        name: "read".into(),
        args: serde_json::json!({}),
        description: None,
        display: None,
    });
    memory.append_loop_event(LoopRecordedEvent::ToolResultEvent {
        parent_uuid: "s1".into(),
        tool_call_id: "call_1".into(),
        result: ExecutableToolResult::Error(ExecutableToolErrorResult {
            output: ExecutableToolOutput::Text("".into()),
            is_error: true,
            stop_turn: None,
            message: None,
        }),
    });

    let history = memory.history();
    let text = match &history[1].message.content[0] {
        ContentPart::Text { text } => text.as_str(),
        _ => panic!("expected text"),
    };
    assert_eq!(
        text,
        "<system>ERROR: Tool execution failed. Tool output is empty.</system>"
    );
}

#[test]
fn reset_runtime_state_clears_open_tracking_without_touching_history() {
    let agent = Arc::new(TestAgent::new());
    let mut memory = ContextMemory::new(agent.clone());
    memory.append_loop_event(LoopRecordedEvent::StepBegin {
        uuid: "s1".into(),
        turn_id: "t1".into(),
        step: 1,
    });
    memory.append_loop_event(LoopRecordedEvent::ToolCallEvent {
        uuid: "tc1".into(),
        turn_id: "t1".into(),
        step: 1,
        step_uuid: "s1".into(),
        tool_call_id: "call_1".into(),
        name: "read".into(),
        args: serde_json::json!({}),
        description: None,
        display: None,
    });
    memory.append_message(user_msg("deferred"));

    memory.reset_runtime_state();

    assert!(!memory.has_open_steps());
    // The deferred user message and pending tool-call tracking are dropped; the
    // assistant placeholder remains in history.
    assert_eq!(memory.history().len(), 1);
}
