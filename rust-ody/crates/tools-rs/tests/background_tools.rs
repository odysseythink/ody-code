use serde_json::json;
use std::sync::Arc;
use tools_rs::builtin::background::task_list::TaskListTool;
use tools_rs::builtin::background::task_output::TaskOutputTool;
use tools_rs::builtin::background::task_stop::TaskStopTool;
use tools_rs::builtin::background::{
    BackgroundManager, BackgroundTaskInfoData, BackgroundTaskOutputSnapshot, BackgroundTaskStatus,
    MockBackgroundManager,
};
use tools_rs::builtin::{BuiltinTool, ExecutableToolContext};

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

// ── TaskListTool tests ──

#[test]
fn test_task_list_empty() {
    let mgr = Arc::new(MockBackgroundManager::new());
    let tool = TaskListTool::new(mgr);
    let exec = tool
        .resolve_execution(json!({"active_only": false, "limit": 20}))
        .unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(!result.is_error);
    assert!(result.to_text().contains("background_tasks: 0"));
}

#[test]
fn test_task_list_with_tasks() {
    let mgr = Arc::new(MockBackgroundManager::new());
    mgr.add_task(BackgroundTaskInfoData {
        task_id: "task-001".into(),
        description: "test task".into(),
        status: BackgroundTaskStatus::Running,
        started_at: 1000,
        ended_at: None,
        stop_reason: None,
        terminal_notification_suppressed: false,
    });
    mgr.add_task(BackgroundTaskInfoData {
        task_id: "task-002".into(),
        description: "another task".into(),
        status: BackgroundTaskStatus::Completed,
        started_at: 2000,
        ended_at: Some(3000),
        stop_reason: None,
        terminal_notification_suppressed: false,
    });

    let tool = TaskListTool::new(mgr);
    let exec = tool
        .resolve_execution(json!({"active_only": true, "limit": 20}))
        .unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("active_background_tasks: 1"));
    assert!(text.contains("task-001"));
    assert!(!text.contains("task-002")); // completed tasks filtered out
}

#[test]
fn test_task_list_all() {
    let mgr = Arc::new(MockBackgroundManager::new());
    mgr.add_task(BackgroundTaskInfoData {
        task_id: "task-001".into(),
        description: "test task".into(),
        status: BackgroundTaskStatus::Running,
        started_at: 1000,
        ended_at: None,
        stop_reason: None,
        terminal_notification_suppressed: false,
    });
    mgr.add_task(BackgroundTaskInfoData {
        task_id: "task-002".into(),
        description: "another task".into(),
        status: BackgroundTaskStatus::Completed,
        started_at: 2000,
        ended_at: Some(3000),
        stop_reason: None,
        terminal_notification_suppressed: false,
    });

    let tool = TaskListTool::new(mgr);
    let exec = tool
        .resolve_execution(json!({"active_only": false, "limit": 20}))
        .unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("background_tasks: 2"));
    assert!(text.contains("task-001"));
    assert!(text.contains("task-002"));
}

#[test]
fn test_task_list_limit() {
    let mgr = Arc::new(MockBackgroundManager::new());
    for i in 0..5 {
        mgr.add_task(BackgroundTaskInfoData {
            task_id: format!("task-{:03}", i),
            description: format!("task {}", i),
            status: BackgroundTaskStatus::Running,
            started_at: (1000 + i) as u64,
            ended_at: None,
            stop_reason: None,
            terminal_notification_suppressed: false,
        });
    }

    let tool = TaskListTool::new(mgr);
    let exec = tool
        .resolve_execution(json!({"active_only": false, "limit": 3}))
        .unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("background_tasks: 5")); // total count
}

// ── TaskOutputTool tests ──

#[test]
fn test_task_output_snapshot() {
    let mgr = Arc::new(MockBackgroundManager::new());
    mgr.add_task(BackgroundTaskInfoData {
        task_id: "task-001".into(),
        description: "test task".into(),
        status: BackgroundTaskStatus::Running,
        started_at: 1000,
        ended_at: None,
        stop_reason: None,
        terminal_notification_suppressed: false,
    });
    mgr.set_output_snapshot(
        "task-001",
        BackgroundTaskOutputSnapshot {
            output_path: Some("/tmp/output.log".into()),
            output_size_bytes: 100,
            preview_bytes: 11,
            truncated: false,
            full_output_available: true,
            preview: "hello world".into(),
        },
    );

    let tool = TaskOutputTool::new(mgr);
    let exec = tool
        .resolve_execution(json!({"task_id": "task-001", "block": false}))
        .unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("hello world"));
    assert!(text.contains("outputPath"));
    assert!(text.contains("/tmp/output.log"));
}

#[test]
fn test_task_output_not_found() {
    let mgr = Arc::new(MockBackgroundManager::new());
    let tool = TaskOutputTool::new(mgr);
    let exec = tool
        .resolve_execution(json!({"task_id": "nonexistent", "block": false}))
        .unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(result.is_error);
    let text = result.to_text();
    assert!(text.contains("not found"));
}

#[test]
fn test_task_output_truncated() {
    let mgr = Arc::new(MockBackgroundManager::new());
    mgr.add_task(BackgroundTaskInfoData {
        task_id: "task-001".into(),
        description: "test task".into(),
        status: BackgroundTaskStatus::Completed,
        started_at: 1000,
        ended_at: Some(2000),
        stop_reason: None,
        terminal_notification_suppressed: false,
    });
    mgr.set_output_snapshot(
        "task-001",
        BackgroundTaskOutputSnapshot {
            output_path: None,
            output_size_bytes: 1024 * 1024,
            preview_bytes: 32768,
            truncated: true,
            full_output_available: true,
            preview: "a".repeat(100), // minimal preview for brevity
        },
    );

    let tool = TaskOutputTool::new(mgr);
    let exec = tool
        .resolve_execution(json!({"task_id": "task-001", "block": false}))
        .unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("outputTruncated: true"));
    assert!(text.contains("fullOutputAvailable: true"));
}

// ── TaskStopTool tests ──

#[test]
fn test_task_stop_running() {
    let mgr = Arc::new(MockBackgroundManager::new());
    mgr.add_task(BackgroundTaskInfoData {
        task_id: "task-001".into(),
        description: "test task".into(),
        status: BackgroundTaskStatus::Running,
        started_at: 1000,
        ended_at: None,
        stop_reason: None,
        terminal_notification_suppressed: false,
    });

    let tool = TaskStopTool::new(mgr.clone());
    let exec = tool
        .resolve_execution(json!({"task_id": "task-001", "reason": "test"}))
        .unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("stopped"));

    // Verify status changed
    let info = mgr.get_task("task-001").unwrap();
    assert_eq!(info.status, BackgroundTaskStatus::Killed);
}

#[test]
fn test_task_stop_not_found() {
    let mgr = Arc::new(MockBackgroundManager::new());
    let tool = TaskStopTool::new(mgr);
    let exec = tool
        .resolve_execution(json!({"task_id": "nonexistent"}))
        .unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(result.is_error);
    let text = result.to_text();
    assert!(text.contains("No background task found"));
}

#[test]
fn test_task_stop_already_terminal() {
    let mgr = Arc::new(MockBackgroundManager::new());
    mgr.add_task(BackgroundTaskInfoData {
        task_id: "task-001".into(),
        description: "test task".into(),
        status: BackgroundTaskStatus::Completed,
        started_at: 1000,
        ended_at: Some(2000),
        stop_reason: None,
        terminal_notification_suppressed: false,
    });

    let tool = TaskStopTool::new(mgr);
    let exec = tool
        .resolve_execution(json!({"task_id": "task-001"}))
        .unwrap();
    let result = rt().block_on((exec.execute)(default_ctx()));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("already terminal"));
}
