use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use agent_rs::records::nested::{
    ContextMessage, ExecutableToolOutput, ExecutableToolResult, ExecutableToolSuccessResult,
    GoalActor, GoalBudgetLimits, GoalStatus, LoopRecordedEvent, PermissionMode, PromptOrigin,
    UsageRecordScope,
};
use agent_rs::records::types::AgentRecord;
use kosong_rs::message::{ContentPart, Message, Role, ToolCall, UrlPayload};
use kosong_rs::usage::TokenUsage;

fn main() {
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    std::fs::create_dir_all(&out_dir).expect("create fixtures dir");
    let path = out_dir.join("rust_records.jsonl");
    let file = File::create(&path).expect("create fixture file");
    let mut writer = BufWriter::new(file);

    let records = vec![
        AgentRecord::Metadata {
            time: Some(1_700_000_000_000),
            protocol_version: "1.3".into(),
            created_at: 1_700_000_000_000,
            app_version: Some("0.0.0".into()),
            resumed: Some(false),
        },
        AgentRecord::TurnPrompt {
            time: Some(1_700_000_000_001),
            input: vec![
                ContentPart::Text {
                    text: "Hello from Rust".into(),
                },
                ContentPart::ImageUrl {
                    image_url: UrlPayload {
                        url: "data:image/png;base64,iVBORw0KGgoAAAA==".into(),
                        id: Some("img_1".into()),
                    },
                },
            ],
            origin: PromptOrigin::User,
        },
        AgentRecord::ContextAppendMessage {
            time: Some(1_700_000_000_002),
            message: ContextMessage {
                message: Message {
                    role: Role::Assistant,
                    name: None,
                    content: vec![ContentPart::Text {
                        text: "Acknowledged".into(),
                    }],
                    tool_calls: vec![ToolCall {
                        call_type: "function".into(),
                        id: "call_1".into(),
                        name: "read".into(),
                        arguments: Some(r#"{"path":"README.md"}"#.into()),
                        extras: None,
                        stream_index: None,
                    }],
                    tool_call_id: None,
                    partial: None,
                },
                origin: Some(PromptOrigin::User),
                is_error: None,
            },
        },
        AgentRecord::ContextAppendLoopEvent {
            time: Some(1_700_000_000_003),
            event: LoopRecordedEvent::ToolResultEvent {
                parent_uuid: "p1".into(),
                tool_call_id: "call_1".into(),
                result: ExecutableToolResult::Success(ExecutableToolSuccessResult {
                    output: ExecutableToolOutput::Parts(vec![ContentPart::Text {
                        text: "file contents".into(),
                    }]),
                    is_error: None,
                    stop_turn: None,
                    message: None,
                }),
            },
        },
        AgentRecord::PermissionSetMode {
            time: Some(1_700_000_000_004),
            mode: PermissionMode::Yolo,
        },
        AgentRecord::UsageRecord {
            time: Some(1_700_000_000_005),
            model: "kimi-k2".into(),
            usage: TokenUsage {
                input_other: 12,
                output: 5,
                input_cache_read: 1,
                input_cache_creation: 0,
            },
            usage_scope: Some(UsageRecordScope::Turn),
        },
        AgentRecord::GoalCreate {
            time: Some(1_700_000_000_006),
            goal_id: "g1".into(),
            objective: "finish the fixture".into(),
            status: GoalStatus::Active,
            actor: GoalActor::User,
            budget_limits: GoalBudgetLimits {
                token_budget: Some(1_000_000),
                turn_budget: Some(100),
                wall_clock_budget_ms: None,
            },
        },
    ];

    for record in records {
        let line = serde_json::to_string(&record).expect("serialize record");
        writeln!(writer, "{}", line).expect("write record");
    }

    writer.flush().expect("flush fixture file");
    println!("Wrote {}", path.display());
}
