use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use kaos_rs::kaos::Kaos;
use kosong_rs::message::ContentPart;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[cfg(test)]
use crate::background::manager::BackgroundManager;
use crate::background::tasks::ProcessBackgroundTask;
use crate::background::types::BackgroundTaskId;
use crate::context::types::PromptOrigin;
#[cfg(test)]
use crate::cron::manager::{CronManager, CronManagerOptions};
use crate::cron::task::CronTaskInit;
use crate::turn::fixture_agent::FixtureAgent;
use crate::turn::turn_flow::TurnFlow;
use crate::turn::types::TurnEndResult;

#[derive(Debug, Deserialize)]
pub struct BackgroundCronFixture {
    pub name: String,
    #[serde(default)]
    pub initial_goal: Option<FixtureInitialGoal>,
    #[serde(default)]
    pub loop_control: Option<FixtureLoopControl>,
    #[serde(default)]
    pub responses: Vec<crate::turn::fixture_agent::FixtureResponse>,
    #[serde(default)]
    pub tools: Vec<crate::turn::fixture_agent::FixtureToolDef>,
    pub actions: Vec<BackgroundCronAction>,
}

#[derive(Debug, Deserialize)]
pub struct FixtureInitialGoal {
    pub status: String,
    #[serde(default)]
    pub budget: FixtureBudget,
}

#[derive(Debug, Default, Deserialize)]
pub struct FixtureBudget {
    #[serde(rename = "tokenBudget")]
    pub token_budget: Option<i64>,
    #[serde(rename = "turnBudget")]
    pub turn_budget: Option<i64>,
    #[serde(rename = "wallClockBudgetMs")]
    pub wall_clock_budget_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct FixtureLoopControl {
    pub max_steps: Option<u32>,
    pub max_retry_attempts: Option<u32>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "kind")]
pub enum FixtureOrigin {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "system_trigger")]
    SystemTrigger { name: String },
    #[serde(rename = "hook_result")]
    HookResult {
        event: String,
        blocked: Option<bool>,
    },
}

impl From<FixtureOrigin> for PromptOrigin {
    fn from(o: FixtureOrigin) -> Self {
        match o {
            FixtureOrigin::User => PromptOrigin::User,
            FixtureOrigin::SystemTrigger { name } => PromptOrigin::SystemTrigger { name },
            FixtureOrigin::HookResult { event, blocked } => {
                PromptOrigin::HookResult { event, blocked }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum BackgroundCronAction {
    Prompt {
        input: Vec<ContentPart>,
        origin: FixtureOrigin,
    },
    Steer {
        input: Vec<ContentPart>,
        origin: FixtureOrigin,
    },
    Cancel {
        turn_id: Option<i64>,
        reason: Option<String>,
    },
    Wait,
    AdvanceClockTo {
        epoch_ms: i64,
    },
    CronAdd {
        cron: String,
        prompt: String,
        #[serde(default)]
        recurring: Option<bool>,
    },
    CronRemoveLast,
    CronTick,
    BackgroundRunProcess {
        args: Vec<String>,
        description: String,
    },
    BackgroundWaitLast {
        timeout_ms: u64,
    },
    BackgroundStopLast {
        reason: Option<String>,
    },
}

#[derive(Debug, Serialize)]
pub struct BackgroundCronSnapshot {
    pub name: String,
    pub turns: Vec<TurnSummary>,
    pub events: Vec<JsonValue>,
    pub records: Vec<JsonValue>,
    pub context_inputs: Vec<ContextInputSummary>,
    pub cron_tasks: Vec<CronTaskSummary>,
    pub background_tasks: Vec<BackgroundTaskSummary>,
    pub telemetry: Vec<TelemetrySummary>,
}

#[derive(Debug, Serialize)]
pub struct TurnSummary {
    pub turn_id: i64,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_by_user_prompt_hook: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ContextInputSummary {
    pub text: String,
    pub origin_kind: String,
}

#[derive(Debug, Serialize)]
pub struct TelemetrySummary {
    pub event: String,
    pub properties: JsonValue,
}

#[derive(Debug, Serialize)]
pub struct CronTaskSummary {
    pub id: String,
    pub cron: String,
    pub prompt: String,
    pub recurring: bool,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_fired_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct BackgroundTaskSummary {
    pub task_id: String,
    pub kind: String,
    pub description: String,
    pub status: String,
    pub started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

fn kaos_env() -> kaos_rs::environment::Environment {
    kaos_rs::environment::Environment {
        os_kind: "Linux".into(),
        os_arch: "x86_64".into(),
        os_version: "test".into(),
        shell_name: "bash".into(),
        shell_path: "/bin/bash".into(),
    }
}

pub async fn run_fixture(
    fixture: BackgroundCronFixture,
    agent: Arc<FixtureAgent>,
    flow: Arc<TurnFlow>,
) -> anyhow::Result<BackgroundCronSnapshot> {
    let mut turns: Vec<TurnSummary> = Vec::new();
    let mut last_cron_id: Option<String> = None;
    let mut last_background_id: Option<String> = None;

    for action in fixture.actions {
        match action {
            BackgroundCronAction::Prompt { input, origin } => {
                flow.prompt(input, origin.into());
            }
            BackgroundCronAction::Steer { input, origin } => {
                flow.steer(input, origin.into());
            }
            BackgroundCronAction::Cancel { turn_id, reason } => {
                flow.cancel(turn_id, reason);
            }
            BackgroundCronAction::Wait => {
                if let Ok(end) = flow.wait_for_current_turn(None).await {
                    turns.push(turn_summary(&end));
                }
            }
            BackgroundCronAction::AdvanceClockTo { epoch_ms } => {
                agent.advance_clock_to(epoch_ms);
            }
            BackgroundCronAction::CronAdd {
                cron,
                prompt,
                recurring,
            } => {
                let mgr = agent
                    .cron
                    .lock()
                    .unwrap()
                    .clone()
                    .context("cron manager not installed")?;
                let now = agent.clock().wall_now();
                let task = mgr.store.lock().unwrap().add(
                    CronTaskInit {
                        cron,
                        prompt,
                        recurring,
                    },
                    now,
                );
                last_cron_id = Some(task.id.clone());
            }
            BackgroundCronAction::CronRemoveLast => {
                if let Some(id) = last_cron_id.take() {
                    let mgr = agent
                        .cron
                        .lock()
                        .unwrap()
                        .clone()
                        .context("cron manager not installed")?;
                    mgr.remove_tasks(&[id]);
                }
            }
            BackgroundCronAction::CronTick => {
                let mgr = agent
                    .cron
                    .lock()
                    .unwrap()
                    .clone()
                    .context("cron manager not installed")?;
                mgr.tick();
            }
            BackgroundCronAction::BackgroundRunProcess {
                args,
                description: _desc,
            } => {
                let mgr = agent
                    .background
                    .lock()
                    .unwrap()
                    .clone()
                    .context("background manager not installed")?;
                let kaos = Kaos::new(kaos_env(), std::env::current_dir()?);
                let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                let task = ProcessBackgroundTask::new(kaos, args_ref)
                    .with_id(BackgroundTaskId::new("process-unset"));
                let id = mgr.register_task(Box::new(task));
                last_background_id = Some(id);
            }
            BackgroundCronAction::BackgroundWaitLast { timeout_ms } => {
                if let Some(id) = &last_background_id {
                    let mgr = agent
                        .background
                        .lock()
                        .unwrap()
                        .clone()
                        .context("background manager not installed")?;
                    let _ = mgr.wait(id, Duration::from_millis(timeout_ms)).await;
                }
            }
            BackgroundCronAction::BackgroundStopLast { reason } => {
                if let Some(id) = &last_background_id {
                    let mgr = agent
                        .background
                        .lock()
                        .unwrap()
                        .clone()
                        .context("background manager not installed")?;
                    let _ = mgr.stop(id, reason).await;
                }
            }
        }
    }

    let captures = agent.captures.lock().unwrap();
    let cron_mgr = agent.cron.lock().unwrap();
    let bg_mgr = agent.background.lock().unwrap();

    let cron_tasks: Vec<CronTaskSummary> = cron_mgr
        .as_ref()
        .map(|m| {
            m.store
                .lock()
                .unwrap()
                .list()
                .iter()
                .map(|t| CronTaskSummary {
                    id: t.id.clone(),
                    cron: t.cron.clone(),
                    prompt: t.prompt.clone(),
                    recurring: t.recurring.unwrap_or(true),
                    created_at: t.created_at,
                    last_fired_at: t.last_fired_at,
                })
                .collect()
        })
        .unwrap_or_default();

    let background_tasks: Vec<BackgroundTaskSummary> = bg_mgr
        .as_ref()
        .map(|m| {
            m.list(false, None)
                .into_iter()
                .map(|info| BackgroundTaskSummary {
                    task_id: info.id.to_string(),
                    kind: format!("{:?}", info.kind).to_lowercase(),
                    description: info.description,
                    status: format!("{:?}", info.status).to_lowercase(),
                    started_at: info.started_at.timestamp_millis(),
                    ended_at: info.finished_at.map(|d| d.timestamp_millis()),
                    stop_reason: info.stop_reason,
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(BackgroundCronSnapshot {
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
        cron_tasks,
        background_tasks,
        telemetry: captures
            .telemetry_events
            .iter()
            .map(|(event, props)| TelemetrySummary {
                event: event.clone(),
                properties: props.clone(),
            })
            .collect(),
    })
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
        stop_reason: end
            .stop_reason
            .as_ref()
            .map(|r| format!("{:?}", r).to_lowercase()),
        blocked_by_user_prompt_hook: Some(end.blocked_by_user_prompt_hook),
    }
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

fn origin_kind(origin: &PromptOrigin) -> String {
    let value = serde_json::to_value(origin).unwrap_or_default();
    value
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::fixture_agent::FixtureAgent;
    use crate::turn::TurnFlow;
    use std::sync::Arc;

    fn sample_json() -> &'static str {
        r#"
        {
            "name": "driver-cron-fire",
            "responses": [
                {
                    "toolCalls": [],
                    "finishReason": "completed",
                    "rawFinishReason": "stop",
                    "usage": { "inputOther": 0, "output": 0, "inputCacheRead": 0, "inputCacheCreation": 0 }
                }
            ],
            "actions": [
                { "op": "advance_clock_to", "epoch_ms": 0 },
                { "op": "cron_add", "cron": "* * * * *", "prompt": "ping", "recurring": true },
                { "op": "advance_clock_to", "epoch_ms": 120000 },
                { "op": "cron_tick" },
                { "op": "wait" }
            ]
        }
        "#
    }

    #[tokio::test]
    async fn driver_parses_and_runs_cron_fire_fixture() {
        let fixture: BackgroundCronFixture = serde_json::from_str(sample_json()).unwrap();
        let agent = Arc::new(FixtureAgent::new(fixture.responses.clone(), vec![]));
        let flow = Arc::new(TurnFlow::new(agent.clone()));
        let background = Arc::new(BackgroundManager::new(agent.clone(), flow.clone(), None));
        let cron = CronManager::new(
            agent.clone(),
            flow.clone(),
            None,
            CronManagerOptions {
                clocks: Some(agent.clock()),
                poll_interval_ms: Some(0),
            },
        );
        agent.install_managers(background, cron.clone());

        let snapshot = run_fixture(fixture, agent.clone(), flow.clone())
            .await
            .unwrap();

        assert_eq!(snapshot.name, "driver-cron-fire");
        assert!(!snapshot.cron_tasks.is_empty());
        assert!(snapshot
            .events
            .iter()
            .any(|e| { e.get("type").and_then(|t| t.as_str()) == Some("cron.fired") }));
    }
}
