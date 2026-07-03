use serde_json::{json, Value};
use std::sync::Arc;

use super::{CronManager, SessionCronTaskInit};
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolOutput, ExecutableToolResult, ToolError,
    ToolExecution,
};
use crate::cron::cron_expr::{
    compute_next_cron_run, cron_to_human, has_fire_within_years, parse_cron_expression,
};
use crate::cron::jitter::{
    jittered_next_cron_run_ms, one_shot_jittered_next_cron_run_ms, JitterConfig,
};
use crate::cron::time_format::format_local_iso_with_offset;

const MAX_CRON_JOBS_PER_SESSION: usize = 50;
const MAX_PROMPT_BYTES: usize = 8192;
/// One-shot tasks must have their first fire within this many days from now.
const ONE_SHOT_MAX_FUTURE_DAYS: u64 = 350;

pub struct CronCreateTool<M: CronManager + 'static> {
    manager: Arc<M>,
}

impl<M: CronManager + 'static> CronCreateTool<M> {
    pub fn new(manager: Arc<M>) -> Self {
        Self { manager }
    }
}

impl<M: CronManager + 'static> BuiltinTool for CronCreateTool<M> {
    fn name(&self) -> &str {
        "CronCreate"
    }

    fn description(&self) -> &str {
        "Schedule a prompt to be enqueued at a future time. Use for both recurring schedules and one-shot reminders."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "cron": {
                    "type": "string",
                    "description": "5-field cron expression in local time: \"M H DoM Mon DoW\""
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt to enqueue at each fire time.",
                    "minLength": 1,
                    "maxLength": 8192
                },
                "recurring": {
                    "type": "boolean",
                    "description": "true = fire on every cron match; false = fire once then auto-delete.",
                    "default": true
                }
            },
            "required": ["cron", "prompt"],
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let cron_raw = args["cron"].as_str().unwrap_or("").trim().to_string();
        let prompt = args["prompt"].as_str().unwrap_or("").to_string();
        let recurring = args
            .get("recurring")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // --- Validation ---

        // 1. Normalize whitespace in cron expression
        let cron_normalized: String = cron_raw.split_whitespace().collect::<Vec<_>>().join(" ");
        if cron_normalized.is_empty() || cron_normalized.split_whitespace().count() != 5 {
            return Err(ToolError::InvalidArgs(format!(
                "Invalid cron expression: '{}'. Must be 5 fields.",
                cron_raw
            )));
        }

        // 2. Parse cron expression
        let parsed = parse_cron_expression(&cron_normalized)
            .map_err(|e| ToolError::InvalidArgs(format!("Invalid cron expression: {}", e)))?;

        // 3. Reject if no fire within 5 years
        let now_ms = self.manager.now_ms();
        if !has_fire_within_years(&parsed, 5, now_ms) {
            return Err(ToolError::InvalidArgs(
                "Cron expression has no fire within the next 5 years.".into(),
            ));
        }

        // 4. Session cap check
        let current_count = self.manager.list_tasks().len();
        if current_count >= MAX_CRON_JOBS_PER_SESSION {
            return Err(ToolError::InvalidArgs(format!(
                "Session cron limit reached ({}). Remove existing jobs first.",
                MAX_CRON_JOBS_PER_SESSION
            )));
        }

        // 5. Prompt byte-length cap
        if prompt.len() > MAX_PROMPT_BYTES {
            return Err(ToolError::InvalidArgs(format!(
                "Prompt too long: {} bytes (max {}).",
                prompt.len(),
                MAX_PROMPT_BYTES
            )));
        }
        if prompt.is_empty() {
            return Err(ToolError::InvalidArgs("Prompt must not be empty.".into()));
        }

        // 6. One-shot "rolled to next year" guard
        if !recurring {
            let max_future_ms = ONE_SHOT_MAX_FUTURE_DAYS * 24 * 3600 * 1000;
            if let Some(next_fire) = compute_next_cron_run(&parsed, now_ms) {
                if next_fire > now_ms + max_future_ms {
                    return Err(ToolError::InvalidArgs(
                        "One-shot task's first fire is too far in the future (max 350 days)."
                            .into(),
                    ));
                }
            }
        }

        let manager = Arc::clone(&self.manager);
        let human_schedule = cron_to_human(&parsed);
        let recurring_flag = recurring;

        Ok(ToolExecution {
            accesses: Default::default(),
            description: format!(
                "Schedule {} cron job: {}",
                if recurring_flag {
                    "recurring"
                } else {
                    "one-shot"
                },
                human_schedule
            ),
            matches_rule: None,
            display: None,
            approval_rule: "allow".to_string(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let manager = Arc::clone(&manager);
                let c = cron_normalized.clone();
                let p = prompt.clone();
                let sched = human_schedule.clone();
                let rec = recurring_flag;
                let parsed = parsed.clone();
                Box::pin(async move {
                    let now = manager.now_ms();
                    let task = manager.add_task(SessionCronTaskInit {
                        cron: c.clone(),
                        prompt: p.clone(),
                        recurring: rec,
                    });

                    // Compute jittered next fire time
                    let ideal = compute_next_cron_run(&parsed, now);
                    let jitter_config = JitterConfig::default();
                    let next_fire_at = if let Some(ideal_ms) = ideal {
                        if rec {
                            jittered_next_cron_run_ms(&parsed, ideal_ms, &task.id, &jitter_config)
                        } else {
                            one_shot_jittered_next_cron_run_ms(&task.id, ideal_ms, &jitter_config)
                        }
                    } else {
                        now
                    };

                    let next_fire_str = format_local_iso_with_offset(next_fire_at);

                    let _kind = if rec { "recurring" } else { "one-shot" };
                    let output = format!(
                        "Cron job created.\nid: {}\ncron: {}\nhumanSchedule: {}\nprompt: {}\nnextFireAt: {}\nrecurring: {}\nageDays: 0.00\nstale: false",
                        task.id, task.cron, sched, p, next_fire_str, rec
                    );

                    ExecutableToolResult {
                        output: ExecutableToolOutput::Text(output),
                        message: None,
                        is_error: false,
                        stop_turn: None,
                    }
                })
            }),
        })
    }
}
