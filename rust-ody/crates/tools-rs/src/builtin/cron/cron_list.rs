use serde_json::{json, Value};
use std::sync::Arc;

use super::CronManager;
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolOutput, ExecutableToolResult, ToolError,
    ToolExecution,
};
use crate::cron::cron_expr::{cron_to_human, parse_cron_expression, ParsedCronExpression};
use crate::cron::time_format::format_local_iso_with_offset;

const MS_PER_DAY: u64 = 24 * 3600 * 1000;
const PROMPT_PREVIEW_BYTES: usize = 200;

pub struct CronListTool<M: CronManager + 'static> {
    manager: Arc<M>,
}

impl<M: CronManager + 'static> CronListTool<M> {
    pub fn new(manager: Arc<M>) -> Self {
        Self { manager }
    }
}

fn truncate_prompt(prompt: &str, max_bytes: usize) -> String {
    if prompt.len() <= max_bytes {
        return prompt.to_string();
    }
    // Truncate to max_bytes (UTF-8 safe: cut at byte boundary)
    let truncated: String = prompt
        .chars()
        .scan(0usize, |acc, ch| {
            *acc += ch.len_utf8();
            if *acc <= max_bytes {
                Some(ch)
            } else {
                None
            }
        })
        .collect();
    format!("{}…(truncated)", truncated)
}

fn default_parsed(raw: &str) -> ParsedCronExpression {
    ParsedCronExpression {
        raw: raw.to_string(),
        minutes: vec![],
        hours: vec![],
        days_of_month: vec![],
        months: vec![],
        days_of_week: vec![],
        days_of_month_wildcard: true,
        days_of_week_wildcard: true,
    }
}

impl<M: CronManager + 'static> BuiltinTool for CronListTool<M> {
    fn name(&self) -> &str {
        "CronList"
    }

    fn description(&self) -> &str {
        "List all cron jobs currently scheduled in this session."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, _args: Value) -> Result<ToolExecution, ToolError> {
        let manager = Arc::clone(&self.manager);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: "List scheduled cron jobs".to_string(),
            matches_rule: None,
            display: None,
            approval_rule: "allow".to_string(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let manager = Arc::clone(&manager);
                Box::pin(async move {
                    let tasks = manager.list_tasks();
                    let now = manager.now_ms();

                    if tasks.is_empty() {
                        return ExecutableToolResult {
                            output: ExecutableToolOutput::Text(
                                "cron_jobs: 0\nNo cron jobs scheduled.".into(),
                            ),
                            message: None,
                            is_error: false,
                            stop_turn: None,
                        };
                    }

                    let mut output = format!("cron_jobs: {}\n", tasks.len());
                    let mut sorted = tasks.clone();
                    sorted.sort_by(|a, b| a.cron.cmp(&b.cron));
                    for task in &sorted {
                        let human_schedule = cron_to_human(
                            &parse_cron_expression(&task.cron)
                                .unwrap_or_else(|_| default_parsed(&task.cron)),
                        );

                        let prompt_json =
                            serde_json::to_string(&task.prompt).unwrap_or_else(|_| "\"\"".into());
                        let prompt_preview = truncate_prompt(&prompt_json, PROMPT_PREVIEW_BYTES);

                        let next_fire = manager.get_next_fire_for_task(&task.id);
                        let next_fire_str = next_fire
                            .map(|ms| format_local_iso_with_offset(ms))
                            .unwrap_or_else(|| "<no fire>".to_string());

                        let age_days =
                            (now.saturating_sub(task.created_at)) as f64 / MS_PER_DAY as f64;
                        let stale = manager.is_stale(task);

                        output.push_str("---\n");
                        output.push_str(&format!("id: {}\n", task.id));
                        output.push_str(&format!("cron: {}\n", task.cron));
                        output.push_str(&format!("humanSchedule: {}\n", human_schedule));
                        output.push_str(&format!("prompt: {}\n", prompt_preview));
                        output.push_str(&format!("nextFireAt: {}\n", next_fire_str));
                        output.push_str(&format!("recurring: {}\n", task.recurring));
                        output.push_str(&format!("ageDays: {:.2}\n", age_days));
                        output.push_str(&format!("stale: {}\n", stale));
                    }

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
