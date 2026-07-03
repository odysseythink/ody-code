use serde_json::json;
use std::sync::Arc;
use tools_rs::builtin::cron::cron_create::CronCreateTool;
use tools_rs::builtin::cron::cron_delete::CronDeleteTool;
use tools_rs::builtin::cron::cron_list::CronListTool;
use tools_rs::builtin::cron::{CronManager, MockCronManager, SessionCronTaskInit};
use tools_rs::builtin::{BuiltinTool, ExecutableToolContext};

fn make_manager(now_ms: u64) -> Arc<MockCronManager> {
    Arc::new(MockCronManager::new(Some(now_ms)))
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().unwrap()
}

fn default_ctx() -> ExecutableToolContext {
    ExecutableToolContext {
        turn_id: "".into(),
        tool_call_id: "".into(),
        signal: tools_rs::builtin::AbortSignal::new(),
        metadata: None,
    }
}

// ── CronCreateTool tests ──

#[test]
fn test_cron_create_valid_recurring() {
    let mgr = make_manager(1700000000000);
    let tool = CronCreateTool::new(mgr.clone());
    let exec = tool
        .resolve_execution(json!({
            "cron": "0 9 * * *",
            "prompt": "daily check",
            "recurring": true
        }))
        .unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(
        !result.is_error,
        "expected success, got: {}",
        result.to_text()
    );
    let text = result.to_text();
    assert!(text.contains("daily check"));
    assert!(text.contains("recurring"));

    // Verify the task was added
    let tasks = mgr.list_tasks();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].cron, "0 9 * * *");
    assert_eq!(tasks[0].prompt, "daily check");
}

#[test]
fn test_cron_create_invalid_expression() {
    let mgr = make_manager(1700000000000);
    let tool = CronCreateTool::new(mgr);
    // Minute 60 is invalid
    let result = tool.resolve_execution(json!({
        "cron": "60 * * * *",
        "prompt": "bad cron",
        "recurring": true
    }));
    assert!(result.is_err());
}

#[test]
fn test_cron_create_empty_prompt() {
    let mgr = make_manager(1700000000000);
    let tool = CronCreateTool::new(mgr);
    let result = tool.resolve_execution(json!({
        "cron": "0 9 * * *",
        "prompt": "",
        "recurring": true
    }));
    assert!(result.is_err());
}

#[test]
fn test_cron_create_prompt_too_long() {
    let mgr = make_manager(1700000000000);
    let tool = CronCreateTool::new(mgr);
    let long_prompt = "x".repeat(8200); // 8192 byte limit
    let result = tool.resolve_execution(json!({
        "cron": "0 9 * * *",
        "prompt": long_prompt,
        "recurring": true
    }));
    assert!(result.is_err());
}

#[test]
fn test_cron_create_session_cap() {
    let mgr = make_manager(1700000000000);
    // Add 50 tasks to fill the cap
    for i in 0..50 {
        mgr.add_task(SessionCronTaskInit {
            cron: format!("{} * * * *", i % 60),
            prompt: format!("task {}", i),
            recurring: true,
        });
    }
    // Verify the tasks were actually added
    assert_eq!(
        mgr.list_tasks().len(),
        50,
        "should have 50 tasks before cap check"
    );
    let tool = CronCreateTool::new(mgr);
    let result = tool.resolve_execution(json!({
        "cron": "0 9 * * *",
        "prompt": "overflow",
        "recurring": true
    }));
    assert!(result.is_err());
    // The error message should indicate a limit was hit
    let ok = result
        .err()
        .map(|e| format!("{:?}", e))
        .map(|s| {
            s.contains("cap")
                || s.contains("limit")
                || s.contains("50")
                || s.contains("limit reached")
        })
        .unwrap_or(false);
    assert!(ok, "expected limit message");
}

// ── CronListTool tests ──

#[test]
fn test_cron_list_empty() {
    let mgr = make_manager(1700000000000);
    let tool = CronListTool::new(mgr);
    let exec = tool.resolve_execution(json!({})).unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("cron_jobs: 0"));
    assert!(text.contains("No cron jobs scheduled"));
}

#[test]
fn test_cron_list_with_tasks() {
    let mgr = make_manager(1700000000000);
    let task1 = mgr.add_task(SessionCronTaskInit {
        cron: "0 9 * * *".into(),
        prompt: "daily check".into(),
        recurring: true,
    });
    let task2 = mgr.add_task(SessionCronTaskInit {
        cron: "*/5 * * * *".into(),
        prompt: "poll status".into(),
        recurring: true,
    });

    let tool = CronListTool::new(mgr);
    let exec = tool.resolve_execution(json!({})).unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("cron_jobs: 2"));
    assert!(text.contains(&task1.id));
    assert!(text.contains(&task2.id));
    // prompt should be JSON-stringified
    assert!(text.contains("\"daily check\""));
    assert!(text.contains("\"poll status\""));
}

#[test]
fn test_cron_list_stale_detection() {
    // Create a task that's 8 days old (past 7-day threshold)
    let now = 1700000000000u64;
    let stale_created = now - 8 * 24 * 3600 * 1000;
    let mgr = make_manager(now);

    // Use the store directly to insert a task with custom created_at
    mgr.store.add(
        SessionCronTaskInit {
            cron: "0 9 * * *".into(),
            prompt: "old task".into(),
            recurring: true,
        },
        stale_created,
    );

    let tool = CronListTool::new(mgr);
    let exec = tool.resolve_execution(json!({})).unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(
        text.contains("stale: true"),
        "expected stale: true in output: {}",
        text
    );
}

// ── CronDeleteTool tests ──

#[test]
fn test_cron_delete_existing() {
    let mgr = make_manager(1700000000000);
    let task = mgr.add_task(SessionCronTaskInit {
        cron: "0 9 * * *".into(),
        prompt: "daily check".into(),
        recurring: true,
    });
    let id = task.id.clone();

    let tool = CronDeleteTool::new(mgr.clone());
    let exec = tool.resolve_execution(json!({"id": id})).unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("deleted"));

    // Verify removed
    assert!(mgr.get_task(&id).is_none());
}

#[test]
fn test_cron_delete_not_found() {
    let mgr = make_manager(1700000000000);
    let tool = CronDeleteTool::new(mgr);
    let exec = tool.resolve_execution(json!({"id": "deadbeef"})).unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(result.is_error);
    let text = result.to_text();
    assert!(
        text.contains("not found") || text.contains("No cron job"),
        "expected not-found error, got: {}",
        text
    );
}

#[test]
fn test_cron_delete_invalid_id() {
    let mgr = make_manager(1700000000000);
    let tool = CronDeleteTool::new(mgr);
    // Non-hex ID
    let result = tool.resolve_execution(json!({"id": "not-hex!"}));
    assert!(result.is_err());
}

#[test]
fn test_cron_delete_multiple_removal() {
    let mgr = make_manager(1700000000000);
    let t1 = mgr.add_task(SessionCronTaskInit {
        cron: "0 9 * * *".into(),
        prompt: "task 1".into(),
        recurring: true,
    });
    mgr.add_task(SessionCronTaskInit {
        cron: "0 10 * * *".into(),
        prompt: "task 2".into(),
        recurring: true,
    });

    let tool = CronDeleteTool::new(mgr.clone());
    let exec = tool.resolve_execution(json!({"id": t1.id})).unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(!result.is_error);

    // Only task 1 should be removed
    assert!(mgr.get_task(&t1.id).is_none());
    assert_eq!(mgr.list_tasks().len(), 1);
}
