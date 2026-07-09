use std::env;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

use agent_rs::agent_loop::types::LoopTurnStopReason;
use agent_rs::records::nested::{GoalBudgetLimits, GoalStatus, PromptOrigin};
use agent_rs::turn::fixture_agent::{FixtureAgent, FixtureResponse, FixtureTool, FixtureToolDef};
use agent_rs::turn::types::{LoopControl, TurnEndResult};
use agent_rs::turn::TurnFlow;
use anyhow::{Context, Error};
use kosong_rs::message::ContentPart;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Deserialize)]
struct Fixture {
    name: String,
    #[serde(default)]
    initial_goal: Option<FixtureInitialGoal>,
    #[serde(default)]
    loop_control: Option<FixtureLoopControl>,
    actions: Vec<FixtureAction>,
    responses: Vec<FixtureResponse>,
    #[serde(default)]
    tools: Vec<FixtureToolDef>,
}

#[derive(Debug, Deserialize)]
struct FixtureInitialGoal {
    status: String,
    #[serde(default)]
    budget: FixtureBudget,
}

#[derive(Debug, Default, Deserialize)]
struct FixtureBudget {
    #[serde(rename = "tokenBudget")]
    token_budget: Option<i64>,
    #[serde(rename = "turnBudget")]
    turn_budget: Option<i64>,
    #[serde(rename = "wallClockBudgetMs")]
    wall_clock_budget_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct FixtureLoopControl {
    max_steps: Option<u32>,
    max_retry_attempts: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
enum FixtureAction {
    #[serde(rename = "prompt")]
    Prompt {
        input: Vec<ContentPart>,
        origin: JsonValue,
    },
    #[serde(rename = "steer")]
    Steer {
        input: Vec<ContentPart>,
        origin: JsonValue,
    },
    #[serde(rename = "cancel")]
    Cancel {
        turn_id: Option<i64>,
        reason: Option<String>,
    },
    #[serde(rename = "wait")]
    Wait,
}

#[derive(Debug, Serialize)]
struct Snapshot {
    name: String,
    turns: Vec<TurnSummary>,
    events: Vec<JsonValue>,
    records: Vec<JsonValue>,
    context_inputs: Vec<ContextInputSummary>,
    telemetry: Vec<TelemetrySummary>,
    goal_state: Option<GoalStateSummary>,
    compaction_events: Vec<JsonValue>,
    compaction_records: Vec<JsonValue>,
}

#[derive(Debug, Serialize)]
struct TurnSummary {
    turn_id: i64,
    reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocked_by_user_prompt_hook: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ContextInputSummary {
    text: String,
    origin_kind: String,
}

#[derive(Debug, Serialize)]
struct TelemetrySummary {
    event: String,
    properties: JsonValue,
}

#[derive(Debug, Serialize)]
struct GoalStateSummary {
    status: String,
    turns_used: i64,
    tokens_used: i64,
}

fn parse_origin(origin: JsonValue) -> PromptOrigin {
    match origin.get("kind").and_then(|v| v.as_str()) {
        Some("user") => PromptOrigin::User,
        Some("system_trigger") => PromptOrigin::SystemTrigger {
            name: origin
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .into(),
        },
        Some("hook_result") => PromptOrigin::HookResult {
            event: origin
                .get("event")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .into(),
            blocked: origin.get("blocked").and_then(|v| v.as_bool()),
        },
        _ => PromptOrigin::User,
    }
}

fn origin_kind(origin: &PromptOrigin) -> String {
    let value = serde_json::to_value(origin).unwrap_or_default();
    value
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string()
}

fn content_text(parts: &[ContentPart]) -> String {
    parts
        .iter()
        .filter_map(|p| match p {
            ContentPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn format_stop_reason(reason: LoopTurnStopReason) -> String {
    serde_json::to_string(&reason)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args: Vec<String> = env::args().collect();
    let fixture_path = args
        .get(1)
        .map(PathBuf::from)
        .context("usage: turn_l3 <fixture.json>")?;

    let file = File::open(&fixture_path)
        .with_context(|| format!("failed to open {}", fixture_path.display()))?;
    let fixture: Fixture = serde_json::from_reader(file)
        .with_context(|| format!("failed to parse {}", fixture_path.display()))?;

    let mut agent = FixtureAgent::new(fixture.responses, vec![]);

    if let Some(ctrl) = fixture.loop_control {
        agent.loop_control = Some(LoopControl {
            max_steps_per_turn: ctrl.max_steps,
            max_retries_per_step: ctrl.max_retry_attempts,
            reserved_context_size: None,
            ..Default::default()
        });
    }

    let agent = Arc::new(agent);

    let tools: Vec<_> = fixture
        .tools
        .into_iter()
        .map(|def| Arc::new(FixtureTool::new(def, agent.clone())) as Arc<_>)
        .collect();
    agent.tools.lock().unwrap().extend(tools);

    if let Some(goal) = fixture.initial_goal {
        let status = match goal.status.as_str() {
            "active" => GoalStatus::Active,
            "paused" => GoalStatus::Paused,
            "blocked" => GoalStatus::Blocked,
            "complete" => GoalStatus::Complete,
            _ => GoalStatus::Active,
        };
        agent.set_goal(
            status,
            GoalBudgetLimits {
                token_budget: goal.budget.token_budget,
                turn_budget: goal.budget.turn_budget,
                wall_clock_budget_ms: goal.budget.wall_clock_budget_ms,
            },
        );
    }

    let flow = TurnFlow::new(agent.clone());
    let mut turns: Vec<TurnSummary> = Vec::new();

    for action in fixture.actions {
        match action {
            FixtureAction::Prompt { input, origin } => {
                flow.prompt(input, parse_origin(origin));
            }
            FixtureAction::Steer { input, origin } => {
                flow.steer(input, parse_origin(origin));
            }
            FixtureAction::Cancel { turn_id, reason } => {
                flow.cancel(turn_id, reason);
            }
            FixtureAction::Wait => {
                if let Ok(end) = flow.wait_for_current_turn(None).await {
                    turns.push(turn_summary(&end));
                }
            }
        }
    }

    let captures = agent.captures.lock().unwrap();

    let compaction_event_prefixes = [
        "compaction.started",
        "compaction.cancelled",
        "compaction.blocked",
        "compaction.completed",
    ];
    let compaction_events: Vec<JsonValue> = captures
        .events
        .iter()
        .filter(|e| {
            let s = serde_json::to_string(e).unwrap_or_default();
            compaction_event_prefixes.iter().any(|p| s.contains(p))
        })
        .map(|e| serde_json::to_value(e).unwrap())
        .collect();

    let compaction_record_prefixes = [
        "full_compaction.",
        "micro_compaction.",
        "context.apply_compaction",
    ];
    let compaction_records: Vec<JsonValue> = captures
        .records
        .iter()
        .filter(|r| {
            let s = serde_json::to_string(r).unwrap_or_default();
            compaction_record_prefixes.iter().any(|p| s.contains(p))
        })
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();

    let snapshot = Snapshot {
        name: fixture.name,
        turns,
        events: captures
            .events
            .iter()
            .map(|e| serde_json::to_value(e).unwrap())
            .collect(),
        records: captures
            .records
            .iter()
            .map(|r| serde_json::to_value(r).unwrap())
            .collect(),
        context_inputs: captures
            .context_inputs
            .iter()
            .map(|(parts, origin)| ContextInputSummary {
                text: content_text(parts),
                origin_kind: origin_kind(origin),
            })
            .collect(),
        telemetry: captures
            .telemetry_events
            .iter()
            .map(|(event, props)| TelemetrySummary {
                event: event.clone(),
                properties: props.clone(),
            })
            .collect(),
        goal_state: agent
            .goal_status
            .lock()
            .unwrap()
            .as_ref()
            .map(|g| GoalStateSummary {
                status: format!("{:?}", g.status).to_lowercase(),
                turns_used: g.turns_used,
                tokens_used: g.tokens_used,
            }),
        compaction_events,
        compaction_records,
    };

    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}

fn turn_summary(end: &TurnEndResult) -> TurnSummary {
    TurnSummary {
        turn_id: end.event.turn_id,
        reason: format!("{:?}", end.event.reason).to_lowercase(),
        error: end
            .event
            .error
            .as_ref()
            .map(|e| serde_json::to_value(e).unwrap()),
        stop_reason: end.stop_reason.map(format_stop_reason),
        blocked_by_user_prompt_hook: Some(end.blocked_by_user_prompt_hook),
    }
}
