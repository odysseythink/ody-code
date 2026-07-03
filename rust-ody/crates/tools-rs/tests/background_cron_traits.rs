use tools_rs::builtin::background::{
    BackgroundManager, BackgroundTaskInfoData, BackgroundTaskOutputSnapshot, BackgroundTaskStatus,
    MockBackgroundManager,
};
use tools_rs::builtin::cron::{
    CronManager, MockCronManager, SessionCronStore, SessionCronTaskInit,
};

#[test]
fn test_mock_background_manager_list_empty() {
    let mgr = MockBackgroundManager::new();
    let list = mgr.list(true, Some(20));
    assert!(list.is_empty());
}

#[test]
fn test_mock_background_manager_list_with_tasks() {
    let mgr = MockBackgroundManager::new();
    mgr.add_task(make_task_info("task-001", BackgroundTaskStatus::Running));
    mgr.add_task(make_task_info("task-002", BackgroundTaskStatus::Completed));

    let active = mgr.list(true, None);
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].task_id, "task-001");

    let all = mgr.list(false, None);
    assert_eq!(all.len(), 2);
}

#[test]
fn test_mock_background_manager_get_task() {
    let mgr = MockBackgroundManager::new();
    mgr.add_task(make_task_info("task-001", BackgroundTaskStatus::Running));
    assert!(mgr.get_task("task-001").is_some());
    assert!(mgr.get_task("nonexistent").is_none());
}

#[test]
fn test_mock_background_manager_get_output_snapshot() {
    let mgr = MockBackgroundManager::new();
    mgr.set_output_snapshot(
        "task-001",
        BackgroundTaskOutputSnapshot {
            output_path: None,
            output_size_bytes: 100,
            preview_bytes: 50,
            truncated: false,
            full_output_available: true,
            preview: "hello world".to_string(),
        },
    );
    let snap = mgr.get_output_snapshot("task-001", 1024).unwrap();
    assert_eq!(snap.preview, "hello world");
    assert!(!snap.truncated);
}

#[test]
fn test_mock_background_manager_stop() {
    let mgr = MockBackgroundManager::new();
    mgr.add_task(make_task_info("task-001", BackgroundTaskStatus::Running));
    let result = mgr.stop("task-001", Some("test stop".to_string())).unwrap();
    assert_eq!(result.status, BackgroundTaskStatus::Killed);

    let info = mgr.get_task("task-001").unwrap();
    assert_eq!(info.status, BackgroundTaskStatus::Killed);
    assert_eq!(info.stop_reason, Some("test stop".to_string()));
}

#[test]
fn test_session_cron_store_add_and_list() {
    let store = SessionCronStore::new();
    let task = store.add(
        SessionCronTaskInit {
            cron: "0 9 * * *".to_string(),
            prompt: "daily check".to_string(),
            recurring: true,
        },
        1000,
    );
    assert_eq!(task.cron, "0 9 * * *");
    assert_eq!(task.prompt, "daily check");
    assert!(task.recurring);
    assert_eq!(task.created_at, 1000);
    assert_eq!(task.id.len(), 8);

    let list = store.list();
    assert_eq!(list.len(), 1);
}

#[test]
fn test_session_cron_store_remove() {
    let store = SessionCronStore::new();
    let task = store.add(
        SessionCronTaskInit {
            cron: "*/5 * * * *".to_string(),
            prompt: "every 5 min".to_string(),
            recurring: true,
        },
        1000,
    );
    let removed = store.remove(&[task.id.clone()]);
    assert_eq!(removed.len(), 1);
    assert!(store.get(&task.id).is_none());
}

#[test]
fn test_mock_cron_manager_add_and_list() {
    let mgr = MockCronManager::new(Some(2000));
    let task = mgr.add_task(SessionCronTaskInit {
        cron: "0 9 * * *".to_string(),
        prompt: "daily check".to_string(),
        recurring: true,
    });
    assert_eq!(task.cron, "0 9 * * *");

    let list = mgr.list_tasks();
    assert_eq!(list.len(), 1);
}

#[test]
fn test_mock_cron_manager_remove() {
    let mgr = MockCronManager::new(Some(2000));
    let task = mgr.add_task(SessionCronTaskInit {
        cron: "*/5 * * * *".to_string(),
        prompt: "every 5 min".to_string(),
        recurring: true,
    });
    let removed = mgr.remove_tasks(&[task.id.clone()]);
    assert_eq!(removed.len(), 1);
    assert!(mgr.list_tasks().is_empty());
}

// -- helpers

fn make_task_info(id: &str, status: BackgroundTaskStatus) -> BackgroundTaskInfoData {
    BackgroundTaskInfoData {
        task_id: id.to_string(),
        description: "test task".to_string(),
        status,
        started_at: 1000,
        ended_at: None,
        stop_reason: None,
        terminal_notification_suppressed: false,
    }
}
