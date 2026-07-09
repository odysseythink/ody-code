use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use async_trait::async_trait;
use kosong_rs::message::ContentPart;
use serde_json::Value as JsonValue;
use tokio::sync::watch;

use crate::agent_loop::types::ExecutableToolResult;
use crate::records::nested::{ExecutableToolOutput, ExecutableToolSuccessResult};
use crate::turn::canonical_args::canonical_telemetry_args;

const REMINDER_TEXT_1: &str = "\n\n<system-reminder>\nYou are repeating the exact same tool call with identical parameters. Please carefully analyze the previous result. If the task is not yet complete, try a different method or parameters instead of repeating the same call.\n</system-reminder>";

fn make_reminder_text_2(tool_name: &str, repeat_count: usize, args: &JsonValue) -> String {
    let args_str = canonical_telemetry_args(args);
    format!(
        "\n\n<system-reminder>\n\
         You have repeatedly called the same tool with identical parameters many times.\n\
         Repeated tool call detected:\n\
         - tool: {tool_name}\n\
         - repeated_times: {repeat_count}\n\
         - arguments: {args_str}\n\
         The previous repeated calls did not make progress. Do not call this exact same tool with the exact same arguments again.\n\
         Carefully inspect the latest tool result and choose a different next action, different parameters, or finish the task if enough evidence has been gathered.\n\
         </system-reminder>"
    )
}

fn dedup_placeholder_result() -> ExecutableToolResult {
    ExecutableToolResult::Success(ExecutableToolSuccessResult {
        output: ExecutableToolOutput::Text("".into()),
        is_error: None,
        stop_turn: None,
        message: None,
    })
}

fn lost_result_error() -> ExecutableToolResult {
    ExecutableToolResult::Success(ExecutableToolSuccessResult {
        output: ExecutableToolOutput::Text(
            "Tool call deduplicated but original result was lost".into(),
        ),
        is_error: Some(true),
        stop_turn: None,
        message: None,
    })
}

fn append_reminder(result: ExecutableToolResult, reminder_text: &str) -> ExecutableToolResult {
    match result {
        ExecutableToolResult::Success(mut s) => {
            s.output = append_to_output(s.output, reminder_text);
            ExecutableToolResult::Success(s)
        }
        ExecutableToolResult::Error(mut e) => {
            e.output = append_to_output(e.output, reminder_text);
            ExecutableToolResult::Error(e)
        }
    }
}

fn append_to_output(output: ExecutableToolOutput, reminder_text: &str) -> ExecutableToolOutput {
    match output {
        ExecutableToolOutput::Text(text) => ExecutableToolOutput::Text(text + reminder_text),
        ExecutableToolOutput::Parts(mut parts) => {
            if let Some(ContentPart::Text { text }) = parts.last_mut() {
                *text = format!("{}{}", text, reminder_text);
            } else {
                parts.push(ContentPart::Text {
                    text: reminder_text.into(),
                });
            }
            ExecutableToolOutput::Parts(parts)
        }
    }
}

fn make_key(tool_name: &str, args: &JsonValue) -> String {
    format!("{} {}", tool_name, canonical_telemetry_args(args))
}

#[async_trait]
pub trait Dedup: Send + Sync {
    fn begin_step(&self);
    fn end_step(&self);
    fn check_same_step(
        &self,
        tool_call_id: &str,
        tool_name: &str,
        args: &JsonValue,
    ) -> Option<ExecutableToolResult>;
    async fn finalize_result(
        &self,
        tool_call_id: &str,
        tool_name: &str,
        args: &JsonValue,
        result: ExecutableToolResult,
    ) -> ExecutableToolResult;
}

pub struct NoopDedup;

#[async_trait]
impl Dedup for NoopDedup {
    fn begin_step(&self) {}
    fn end_step(&self) {}
    fn check_same_step(
        &self,
        _tool_call_id: &str,
        _tool_name: &str,
        _args: &JsonValue,
    ) -> Option<ExecutableToolResult> {
        None
    }
    async fn finalize_result(
        &self,
        _tool_call_id: &str,
        _tool_name: &str,
        _args: &JsonValue,
        result: ExecutableToolResult,
    ) -> ExecutableToolResult {
        result
    }
}

/// Detects and suppresses repetitive tool calls within a single turn.
///
/// Two behaviours are layered:
/// - Same-step dedup: a duplicate `(toolName, args)` issued in the same LLM step
///   reuses the original call's result instead of executing the tool twice.
/// - Cross-step dedup: when the exact same call is repeated consecutively
///   across steps, the result returned to the model is suffixed with a system
///   reminder at specific streak thresholds (3, 5, and 8) to nudge the model
///   to try a different approach.
pub struct ToolCallDeduplicator {
    inner: Mutex<DedupInner>,
}

struct DedupInner {
    step_senders: HashMap<String, watch::Sender<Option<ExecutableToolResult>>>,
    step_calls: Vec<String>,
    original_call_index: HashMap<String, usize>,
    synthetic_call_ids: HashSet<String>,
    call_key_by_call_id: HashMap<String, String>,
    duplicate_receivers: HashMap<String, watch::Receiver<Option<ExecutableToolResult>>>,
    consecutive_key: Option<String>,
    consecutive_count: usize,
}

impl Default for DedupInner {
    fn default() -> Self {
        Self {
            step_senders: HashMap::new(),
            step_calls: Vec::new(),
            original_call_index: HashMap::new(),
            synthetic_call_ids: HashSet::new(),
            call_key_by_call_id: HashMap::new(),
            duplicate_receivers: HashMap::new(),
            consecutive_key: None,
            consecutive_count: 0,
        }
    }
}

impl ToolCallDeduplicator {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(DedupInner::default()),
        }
    }
}

#[async_trait]
impl Dedup for ToolCallDeduplicator {
    fn begin_step(&self) {
        let mut inner = self.inner.lock().unwrap();
        for sender in inner.step_senders.values() {
            let _ = sender.send(Some(lost_result_error()));
        }
        inner.step_senders.clear();
        inner.step_calls.clear();
        inner.original_call_index.clear();
        inner.synthetic_call_ids.clear();
        inner.call_key_by_call_id.clear();
        inner.duplicate_receivers.clear();
    }

    fn end_step(&self) {
        let mut inner = self.inner.lock().unwrap();
        let step_calls = inner.step_calls.clone();
        for key in step_calls {
            if Some(&key) == inner.consecutive_key.as_ref() {
                inner.consecutive_count += 1;
            } else {
                inner.consecutive_key = Some(key);
                inner.consecutive_count = 1;
            }
        }
    }

    fn check_same_step(
        &self,
        tool_call_id: &str,
        tool_name: &str,
        args: &JsonValue,
    ) -> Option<ExecutableToolResult> {
        let key = make_key(tool_name, args);
        let mut inner = self.inner.lock().unwrap();
        let index = inner.step_calls.len();
        inner.step_calls.push(key.clone());
        inner
            .call_key_by_call_id
            .insert(tool_call_id.into(), key.clone());

        if let Some(sender) = inner.step_senders.get(&key).cloned() {
            inner.synthetic_call_ids.insert(tool_call_id.into());
            let rx = sender.subscribe();
            inner.duplicate_receivers.insert(tool_call_id.into(), rx);
            return Some(dedup_placeholder_result());
        }

        let (tx, _rx) = watch::channel(None);
        inner.step_senders.insert(key, tx);
        inner.original_call_index.insert(tool_call_id.into(), index);
        None
    }

    async fn finalize_result(
        &self,
        tool_call_id: &str,
        tool_name: &str,
        args: &JsonValue,
        result: ExecutableToolResult,
    ) -> ExecutableToolResult {
        let (key, is_synthetic) = {
            let mut inner = self.inner.lock().unwrap();
            let key = match inner.call_key_by_call_id.remove(tool_call_id) {
                Some(k) => k,
                None => return result,
            };
            let is_synthetic = inner.synthetic_call_ids.remove(tool_call_id);
            (key, is_synthetic)
        };

        if is_synthetic {
            let mut rx = {
                let mut inner = self.inner.lock().unwrap();
                match inner.duplicate_receivers.remove(tool_call_id) {
                    Some(rx) => rx,
                    None => return result,
                }
            };
            return await_result(&mut rx).await;
        }

        let index = {
            let mut inner = self.inner.lock().unwrap();
            match inner.original_call_index.remove(tool_call_id) {
                Some(i) => i,
                None => return result,
            }
        };

        let streak = {
            let inner = self.inner.lock().unwrap();
            let mut last_key = inner.consecutive_key.clone();
            let mut streak = inner.consecutive_count;
            for i in 0..=index {
                if let Some(k) = inner.step_calls.get(i) {
                    if Some(k) == last_key.as_ref() {
                        streak += 1;
                    } else {
                        last_key = Some(k.clone());
                        streak = 1;
                    }
                }
            }
            streak
        };

        let mut final_result = result;
        if streak == 3 {
            final_result = append_reminder(final_result, REMINDER_TEXT_1);
        } else if streak == 5 || streak == 8 {
            final_result =
                append_reminder(final_result, &make_reminder_text_2(tool_name, streak, args));
        }

        {
            let inner = self.inner.lock().unwrap();
            if let Some(sender) = inner.step_senders.get(&key) {
                let _ = sender.send(Some(final_result.clone()));
            }
        }

        final_result
    }
}

async fn await_result(
    rx: &mut watch::Receiver<Option<ExecutableToolResult>>,
) -> ExecutableToolResult {
    loop {
        if let Some(result) = rx.borrow_and_update().clone() {
            return result;
        }
        if rx.changed().await.is_err() {
            return lost_result_error();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_result(text: &str) -> ExecutableToolResult {
        ExecutableToolResult::Success(ExecutableToolSuccessResult {
            output: ExecutableToolOutput::Text(text.into()),
            is_error: None,
            stop_turn: None,
            message: None,
        })
    }

    #[tokio::test]
    async fn same_step_duplicate_awaits_original_result() {
        let dedup = ToolCallDeduplicator::new();
        dedup.begin_step();

        let placeholder = dedup.check_same_step("id-1", "read", &serde_json::json!({"path": "/a"}));
        assert!(placeholder.is_none());

        let placeholder = dedup.check_same_step("id-2", "read", &serde_json::json!({"path": "/a"}));
        assert!(placeholder.is_some());

        let original = dedup
            .finalize_result(
                "id-1",
                "read",
                &serde_json::json!({"path": "/a"}),
                text_result("ok"),
            )
            .await;
        assert_eq!(original, text_result("ok"));

        let dup = dedup
            .finalize_result(
                "id-2",
                "read",
                &serde_json::json!({"path": "/a"}),
                text_result("ignored"),
            )
            .await;
        assert_eq!(dup, text_result("ok"));
    }

    #[tokio::test]
    async fn different_args_are_not_deduplicated() {
        let dedup = ToolCallDeduplicator::new();
        dedup.begin_step();

        assert!(dedup
            .check_same_step("id-1", "read", &serde_json::json!({"path": "/a"}))
            .is_none());
        assert!(dedup
            .check_same_step("id-2", "read", &serde_json::json!({"path": "/b"}))
            .is_none());

        let r1 = dedup
            .finalize_result(
                "id-1",
                "read",
                &serde_json::json!({"path": "/a"}),
                text_result("a"),
            )
            .await;
        let r2 = dedup
            .finalize_result(
                "id-2",
                "read",
                &serde_json::json!({"path": "/b"}),
                text_result("b"),
            )
            .await;
        assert_eq!(r1, text_result("a"));
        assert_eq!(r2, text_result("b"));
    }

    #[tokio::test]
    async fn cross_step_streak_appends_reminder_at_thresholds() {
        let dedup = ToolCallDeduplicator::new();
        let args = serde_json::json!({"path": "/a"});

        for _ in 0..2 {
            dedup.begin_step();
            assert!(dedup.check_same_step("id", "read", &args).is_none());
            dedup
                .finalize_result("id", "read", &args, text_result("ok"))
                .await;
            dedup.end_step();
        }

        // Third consecutive occurrence -> streak 3 -> REMINDER_TEXT_1
        dedup.begin_step();
        assert!(dedup.check_same_step("id", "read", &args).is_none());
        let r = dedup
            .finalize_result("id", "read", &args, text_result("ok"))
            .await;
        match r {
            ExecutableToolResult::Success(s) => {
                let ExecutableToolOutput::Text(text) = s.output else {
                    panic!("expected text output");
                };
                assert!(text.contains("You are repeating"));
            }
            _ => panic!("expected success"),
        }
    }

    #[tokio::test]
    async fn begin_step_clears_tracking_for_unfinished_deferreds() {
        let dedup = ToolCallDeduplicator::new();
        dedup.begin_step();
        assert!(dedup
            .check_same_step("id-1", "read", &serde_json::json!({"path": "/a"}))
            .is_none());
        let placeholder = dedup
            .check_same_step("id-2", "read", &serde_json::json!({"path": "/a"}))
            .unwrap();

        // New step begins before the original finalized. Tracking is cleared so
        // the dup's finalize returns the placeholder it was passed, matching TS
        // behavior that defensively resolves leaked deferreds but no longer
        // routes them.
        dedup.begin_step();
        let dup = dedup
            .finalize_result(
                "id-2",
                "read",
                &serde_json::json!({"path": "/a"}),
                placeholder.clone(),
            )
            .await;
        assert_eq!(dup, placeholder);
    }
}
