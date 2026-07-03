use crate::turn::types::{PromptOrigin, TurnAgent};
use kosong_rs::provider::AbortSignal;
use std::sync::Mutex;

pub const DEFAULT_NORMAL_TASK_COMPACTION_RATIO: f64 = 0.5;

#[derive(Debug, Clone, Default)]
pub struct TodoCounts {
    pub done: usize,
    pub in_progress: usize,
    pub pending: usize,
}

pub fn count_todos(content: &str) -> Option<TodoCounts> {
    let mut done = 0usize;
    let mut in_progress = 0usize;
    let mut pending = 0usize;
    let mut found_any = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.contains("[done]") {
            done += 1;
            found_any = true;
        } else if trimmed.contains("[in_progress]") {
            in_progress += 1;
            found_any = true;
        } else if trimmed.contains("[pending]") {
            pending += 1;
            found_any = true;
        }
    }

    if found_any {
        Some(TodoCounts {
            done,
            in_progress,
            pending,
        })
    } else {
        None
    }
}

pub struct NormalModeTaskCheckpoint {
    last_done_count: Mutex<Option<usize>>,
    sent_reminders: Mutex<bool>,
}

impl NormalModeTaskCheckpoint {
    pub fn new() -> Self {
        Self {
            last_done_count: Mutex::new(None),
            sent_reminders: Mutex::new(false),
        }
    }

    pub fn reset(&self) {
        *self.last_done_count.lock().unwrap() = None;
        *self.sent_reminders.lock().unwrap() = false;
    }

    pub async fn before_step(
        &self,
        agent: std::sync::Arc<dyn TurnAgent>,
        signal: AbortSignal,
    ) -> Result<(), anyhow::Error> {
        let session_mode = agent.session_mode();
        if session_mode.is_active() {
            self.reset();
            return Ok(());
        }

        // Send E2E / test-review reminders if enabled
        {
            let mut sent = self.sent_reminders.lock().unwrap();
            if !*sent {
                if agent.config().e2e_enabled() {
                    agent.context().append_system_reminder(
                        "Consider running E2E tests to validate your changes.",
                        PromptOrigin::SystemTrigger {
                            name: "e2e_reminder".into(),
                        },
                    );
                    *sent = true;
                }
                if agent.config().test_review_enabled() {
                    agent.context().append_system_reminder(
                        "Consider requesting a code review before proceeding.",
                        PromptOrigin::SystemTrigger {
                            name: "review_reminder".into(),
                        },
                    );
                    *sent = true;
                }
            }
        }

        let ratio = agent
            .config()
            .loop_control()
            .and_then(|c| c.normal_task_compaction_ratio)
            .unwrap_or(DEFAULT_NORMAL_TASK_COMPACTION_RATIO);
        if ratio <= 0.0 {
            return Ok(());
        }

        // Check todo tool data for boundary crossing
        let store = agent.tools().store_data();
        let todo_data = store.get("todos");
        let todo_str = todo_data.and_then(|v| v.as_str()).unwrap_or("");
        let counts = match count_todos(todo_str) {
            Some(c) => c,
            None => return Ok(()),
        };

        let crossed_boundary = self
            .last_done_count
            .lock()
            .unwrap()
            .map(|last| counts.done > last)
            .unwrap_or(false);
        let more_work = counts.in_progress > 0 || counts.pending > 0;
        *self.last_done_count.lock().unwrap() = Some(counts.done);

        if !crossed_boundary || !more_work {
            return Ok(());
        }

        let max_context_tokens = agent.config().model_capabilities().max_context_tokens;
        if max_context_tokens <= 0 {
            return Ok(());
        }
        if agent.context().token_count_with_pending() as f64 >= max_context_tokens as f64 * ratio {
            let agent_clone = agent.clone();
            agent
                .full_compaction()
                .compact_checkpoint(agent_clone, signal)
                .await?;
        }
        Ok(())
    }
}

impl Default for NormalModeTaskCheckpoint {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_todo_statuses() {
        let content = "- [done] Fix auth\n- [in_progress] Add tests\n- [pending] Write docs\n";
        let counts = count_todos(content).unwrap();
        assert_eq!(counts.done, 1);
        assert_eq!(counts.in_progress, 1);
        assert_eq!(counts.pending, 1);
    }

    #[test]
    fn no_todos_returns_none() {
        assert!(count_todos("just some text").is_none());
    }
}
