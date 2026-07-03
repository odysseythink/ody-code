use std::env;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use agent_rs::compaction::full::FullCompaction;
use agent_rs::compaction::strategy::{CompactionStrategy, DefaultCompactionStrategy};
use agent_rs::compaction::types::CompactionBeginData;
use agent_rs::context::tokens::estimate_tokens_for_message;
use agent_rs::context::types::{ContextMessage, PromptOrigin};
use agent_rs::records::nested::CompactionSource;
use agent_rs::turn::fixture_agent::FixtureAgent;
use agent_rs::turn::types::*;
use kosong_rs::message::{ContentPart, Message, Role};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Deserialize)]
struct Fixture {
    name: String,
    history: Vec<FixtureContextMessage>,
    strategy: FixtureStrategy,
    begin: FixtureBegin,
    generate_one_off_result: JsonValue,
}

#[derive(Debug, Deserialize)]
struct FixtureContextMessage {
    role: String,
    #[serde(default)]
    name: Option<String>,
    content: Vec<JsonValue>,
    #[serde(default)]
    origin: Option<JsonValue>,
}

#[derive(Debug, Deserialize)]
struct FixtureStrategy {
    max_size: i64,
}

#[derive(Debug, Deserialize)]
struct FixtureBegin {
    source: String,
    instruction: Option<String>,
}

fn parse_role(s: &str) -> Role {
    match s {
        "assistant" => Role::Assistant,
        "tool" => Role::Tool,
        _ => Role::User,
    }
}

#[derive(Debug, Serialize)]
struct Snapshot {
    name: String,
    history: Vec<JsonValue>,
    records: Vec<JsonValue>,
    events: Vec<JsonValue>,
    token_count: i64,
}

#[tokio::main]
async fn main() {
    let path = env::args().nth(1).expect("fixture path required");
    let raw = fs::read_to_string(&path).expect("read fixture");
    let fixture: Fixture = serde_json::from_str(&raw).expect("parse fixture");

    let agent = Arc::new(FixtureAgent::new(vec![], vec![]));
    {
        let mut history = agent.history.lock().unwrap();
        for msg in &fixture.history {
            history.push(ContextMessage {
                message: Message {
                    role: parse_role(&msg.role),
                    name: msg.name.clone(),
                    content: msg
                        .content
                        .iter()
                        .filter_map(|v| {
                            let obj = v.as_object()?;
                            Some(ContentPart::Text {
                                text: obj.get("text")?.as_str()?.into(),
                            })
                        })
                        .collect(),
                    tool_calls: vec![],
                    tool_call_id: None,
                    partial: None,
                },
                origin: msg
                    .origin
                    .as_ref()
                    .map(|v| serde_json::from_value(v.clone()).unwrap_or(PromptOrigin::User)),
                is_error: None,
            });
        }
    }

    let result: CompactGenerateResult =
        serde_json::from_value(fixture.generate_one_off_result).unwrap();
    agent
        .generate_one_off_responses
        .lock()
        .unwrap()
        .push(result);

    let strategy: Arc<dyn CompactionStrategy> = Arc::new(DefaultCompactionStrategy::new(
        move || fixture.strategy.max_size,
        None,
    ));
    let compaction = Arc::new(FullCompaction::new(strategy));

    let source: CompactionSource = match fixture.begin.source.as_str() {
        "manual" => CompactionSource::Manual,
        _ => CompactionSource::Auto,
    };

    // Start the compaction
    compaction.begin(
        agent.clone(),
        CompactionBeginData {
            source,
            instruction: fixture.begin.instruction,
        },
    );

    // Wait for worker to complete (run on the tokio runtime)
    if compaction.is_compacting() {
        compaction
            .block(agent.clone(), kosong_rs::provider::AbortSignal::new())
            .await;
    }

    // Small yield to let any remaining tasks complete
    tokio::time::sleep(Duration::from_millis(5)).await;

    // Collect snapshot
    let snapshot = {
        let captures = agent.captures.lock().unwrap();
        let history_guard = agent.history.lock().unwrap();
        let snapshot = Snapshot {
            name: fixture.name.clone(),
            history: history_guard
                .iter()
                .map(|cm| serde_json::to_value(cm).unwrap())
                .collect(),
            records: captures
                .records
                .iter()
                .map(|r| serde_json::to_value(r).unwrap())
                .collect(),
            events: captures
                .events
                .iter()
                .map(|e| serde_json::to_value(e).unwrap())
                .collect(),
            token_count: history_guard
                .iter()
                .map(|cm| estimate_tokens_for_message(&cm.message))
                .sum(),
        };
        drop(history_guard);
        snapshot
    };

    // The outer Arc<FullCompaction> goes out of scope after this, which is fine
    let output = serde_json::to_string(&snapshot).unwrap();
    println!("{}", output);
}
