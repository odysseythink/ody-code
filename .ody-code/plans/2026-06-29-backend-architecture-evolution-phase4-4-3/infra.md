# Part 1: Infrastructure — Traits, Cron Expression, Jitter, Time

**Depends on:** none (standalone infrastructure, depends only on existing `tools-rs` crate)

## File Summary

| Action | Path | Purpose |
|---|---|---|
| Modify | `rust-ody/crates/tools-rs/Cargo.toml` | add `chrono` dependency |
| Modify | `rust-ody/crates/tools-rs/src/lib.rs` | expose `pub mod cron` |
| Create | `rust-ody/crates/tools-rs/src/cron/mod.rs` | submodule declarations |
| Create | `rust-ody/crates/tools-rs/src/cron/cron_expr.rs` | parse, computeNextCronRun, cronToHuman |
| Create | `rust-ody/crates/tools-rs/src/cron/jitter.rs` | deterministic jitter for fire times |
| Create | `rust-ody/crates/tools-rs/src/cron/time_format.rs` | ISO 8601 local time formatting |
| Create | `rust-ody/crates/tools-rs/src/cron/clock.rs` | ClockSources abstraction |
| Modify | `rust-ody/crates/tools-rs/src/builtin/mod.rs` | add `pub mod background`, `pub mod cron` |
| Create | `rust-ody/crates/tools-rs/src/builtin/background/mod.rs` | BackgroundManager trait + BackgroundTaskInfo types |
| Create | `rust-ody/crates/tools-rs/src/builtin/cron/mod.rs` | CronManager trait + SessionCronStore |

---

### Task 1: BackgroundManager/CronManager traits + SessionCronStore

**Depends on:** none
**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/background/mod.rs`
- Create: `rust-ody/crates/tools-rs/src/builtin/cron/mod.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/mod.rs` (+2 lines)

- [ ] Write the failing test

Create `rust-ody/crates/tools-rs/tests/background_cron_traits.rs`:

```rust
use tools_rs::builtin::background::{
    BackgroundManager, BackgroundTaskInfo, BackgroundTaskStatus,
    BackgroundTaskOutputSnapshot, MockBackgroundManager,
};
use tools_rs::builtin::cron::{
    CronManager, CronTask, SessionCronStore, MockCronManager,
};

#[test]
fn test_mock_background_manager_list_empty() {
    let mgr = MockBackgroundManager::default();
    let list = mgr.list(true, Some(20));
    assert!(list.is_empty());
}

#[test]
fn test_mock_background_manager_list_with_tasks() {
    let mut mgr = MockBackgroundManager::default();
    mgr.add_task(make_task_info("task-001", BackgroundTaskStatus::Running));
    mgr.add_task(make_task_info("task-002", BackgroundTaskStatus::Completed));

    let active = mgr.list(true, None);
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].task_id(), "task-001");

    let all = mgr.list(false, None);
    assert_eq!(all.len(), 2);
}

#[test]
fn test_mock_background_manager_get_task() {
    let mut mgr = MockBackgroundManager::default();
    mgr.add_task(make_task_info("task-001", BackgroundTaskStatus::Running));
    assert!(mgr.get_task("task-001").is_some());
    assert!(mgr.get_task("nonexistent").is_none());
}

#[test]
fn test_mock_background_manager_get_output_snapshot() {
    let mut mgr = MockBackgroundManager::default();
    mgr.set_output_snapshot("task-001", BackgroundTaskOutputSnapshot {
        output_path: None,
        output_size_bytes: 100,
        preview_bytes: 50,
        truncated: false,
        full_output_available: true,
        preview: "hello world".to_string(),
    });
    let snap = mgr.get_output_snapshot("task-001", 1024).unwrap();
    assert_eq!(snap.preview, "hello world");
    assert!(!snap.truncated);
}

#[test]
fn test_mock_background_manager_stop() {
    let mut mgr = MockBackgroundManager::default();
    mgr.add_task(make_task_info("task-001", BackgroundTaskStatus::Running));
    let info = mgr.stop("task-001", Some("test stop".to_string())).unwrap();
    assert_eq!(info.status(), BackgroundTaskStatus::Killed);
    assert_eq!(info.stop_reason(), Some("test stop"));
}

#[test]
fn test_session_cron_store_add_and_list() {
    let mut store = SessionCronStore::default();
    let task = store.add(SessionCronTaskInit {
        cron: "0 9 * * *".to_string(),
        prompt: "daily check".to_string(),
        recurring: true,
    }, 1000);
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
    let mut store = SessionCronStore::default();
    let task = store.add(SessionCronTaskInit {
        cron: "*/5 * * * *".to_string(),
        prompt: "every 5 min".to_string(),
        recurring: true,
    }, 1000);
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

fn make_task_info(id: &str, status: BackgroundTaskStatus) -> MockTaskInfo {
    MockTaskInfo {
        task_id: id.to_string(),
        description: "test task".to_string(),
        status,
        started_at: 1000,
        ended_at: None,
        stop_reason: None,
        terminal_notification_suppressed: false,
    }
}

#[derive(Clone)]
struct MockTaskInfo {
    task_id: String,
    description: String,
    status: BackgroundTaskStatus,
    started_at: u64,
    ended_at: Option<u64>,
    stop_reason: Option<String>,
    terminal_notification_suppressed: bool,
}

impl BackgroundTaskInfo for MockTaskInfo {
    fn task_id(&self) -> &str { &self.task_id }
    fn description(&self) -> &str { &self.description }
    fn status(&self) -> BackgroundTaskStatus { self.status }
    fn started_at(&self) -> u64 { self.started_at }
    fn ended_at(&self) -> Option<u64> { self.ended_at }
    fn stop_reason(&self) -> Option<&str> { self.stop_reason.as_deref() }
    fn terminal_notification_suppressed(&self) -> bool { self.terminal_notification_suppressed }
}
```

- [ ] Run it and verify it FAILS (module doesn't exist yet)

```bash
cd rust-ody && cargo test -p tools-rs --test background_cron_traits 2>&1 | tail -5
# Expected: error[E0432]: unresolved import `tools_rs::builtin::background`
```

- [ ] Write the minimal implementation

**`rust-ody/crates/tools-rs/src/builtin/background/mod.rs`:**

```rust
use std::collections::HashMap;
use std::sync::Mutex;

// ---- Types ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundTaskStatus {
    Running,
    Completed,
    Failed,
    TimedOut,
    Killed,
    Lost,
}

impl BackgroundTaskStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::TimedOut | Self::Killed | Self::Lost)
    }
}

pub trait BackgroundTaskInfo: Send + Sync {
    fn task_id(&self) -> &str;
    fn description(&self) -> &str;
    fn status(&self) -> BackgroundTaskStatus;
    fn started_at(&self) -> u64;
    fn ended_at(&self) -> Option<u64>;
    fn stop_reason(&self) -> Option<&str>;
    fn terminal_notification_suppressed(&self) -> bool;
}

#[derive(Debug, Clone)]
pub struct BackgroundTaskOutputSnapshot {
    pub output_path: Option<String>,
    pub output_size_bytes: u64,
    pub preview_bytes: usize,
    pub truncated: bool,
    pub full_output_available: bool,
    pub preview: String,
}

// ---- BackgroundManager trait ----

pub trait BackgroundManager: Send + Sync {
    type Task: BackgroundTaskInfo;

    fn list(&self, active_only: bool, limit: Option<usize>) -> Vec<&Self::Task>;
    fn get_task(&self, task_id: &str) -> Option<&Self::Task>;
    fn get_output_snapshot(&self, task_id: &str, max_preview_bytes: usize) -> Option<BackgroundTaskOutputSnapshot>;
    fn stop(&self, task_id: &str, reason: Option<String>) -> Option<BackgroundTaskStopResult>;
    fn wait(&self, task_id: &str, timeout_ms: Option<u64>) -> Option<&Self::Task>;
    fn suppress_terminal_notification(&self, task_id: &str);
}

#[derive(Debug, Clone)]
pub struct BackgroundTaskStopResult {
    pub task_id: String,
    pub status: BackgroundTaskStatus,
}

// ---- Mock implementation ----

pub struct MockBackgroundManager<T: BackgroundTaskInfo + Clone> {
    tasks: Mutex<HashMap<String, T>>,
    output_snapshots: Mutex<HashMap<String, BackgroundTaskOutputSnapshot>>,
}

impl<T: BackgroundTaskInfo + Clone> MockBackgroundManager<T> {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
            output_snapshots: Mutex::new(HashMap::new()),
        }
    }

    pub fn add_task(&self, task: T) {
        self.tasks.lock().unwrap().insert(task.task_id().to_string(), task);
    }

    pub fn set_output_snapshot(&self, task_id: &str, snapshot: BackgroundTaskOutputSnapshot) {
        self.output_snapshots.lock().unwrap().insert(task_id.to_string(), snapshot);
    }
}

impl<T: BackgroundTaskInfo + Clone> Default for MockBackgroundManager<T> {
    fn default() -> Self { Self::new() }
}

impl<T: BackgroundTaskInfo + Clone> BackgroundManager for MockBackgroundManager<T> {
    type Task = T;

    fn list(&self, active_only: bool, limit: Option<usize>) -> Vec<&Self::Task> {
        let tasks = self.tasks.lock().unwrap();
        let mut result: Vec<&T> = tasks.values()
            .filter(|t| !active_only || !t.status().is_terminal())
            .collect();
        if let Some(lim) = limit {
            result.truncate(lim);
        }
        result
    }

    fn get_task(&self, task_id: &str) -> Option<&Self::Task> {
        // Safe: we're returning a reference that lives as long as self.
        // The caller must not hold the MutexGuard across await points.
        // For mock usage this is fine since everything is sync.
        let tasks = self.tasks.lock().unwrap();
        // We need to leak the reference — this is a mock, so we use unsafe
        // or restructure. Simpler: return cloned for mock.
        None // See note below — mock returns Option<T> for testing simplicity
    }

    fn get_output_snapshot(&self, task_id: &str, _max_preview_bytes: usize) -> Option<BackgroundTaskOutputSnapshot> {
        self.output_snapshots.lock().unwrap().get(task_id).cloned()
    }

    fn stop(&self, task_id: &str, reason: Option<String>) -> Option<BackgroundTaskStopResult> {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.get_mut(task_id).map(|t| {
            // Clone, mutate, re-insert — this works for mock
            let mut cloned = t.clone();
            // We can't mutate through trait, so use a concrete type
            drop(t);
            BackgroundTaskStopResult {
                task_id: task_id.to_string(),
                status: BackgroundTaskStatus::Killed,
            }
        })
    }

    fn wait(&self, _task_id: &str, _timeout_ms: Option<u64>) -> Option<&Self::Task> {
        None
    }

    fn suppress_terminal_notification(&self, _task_id: &str) {}
}
```

**Wait** — the trait returning `&Self::Task` from methods that hold a `MutexGuard` is problematic. Let me redesign to return owned/cloned values through a concrete `BackgroundTaskInfoData` struct, since the tools don't need references — they just need data to format.

**Revised `rust-ody/crates/tools-rs/src/builtin/background/mod.rs`:**

```rust
use std::collections::HashMap;
use std::sync::Mutex;

// ---- Types ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BackgroundTaskStatus {
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "timed_out")]
    TimedOut,
    #[serde(rename = "killed")]
    Killed,
    #[serde(rename = "lost")]
    Lost,
}

impl BackgroundTaskStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::TimedOut | Self::Killed | Self::Lost)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackgroundTaskInfoData {
    #[serde(rename = "taskId")]
    pub task_id: String,
    pub description: String,
    pub status: BackgroundTaskStatus,
    #[serde(rename = "startedAt")]
    pub started_at: u64,
    #[serde(rename = "endedAt")]
    pub ended_at: Option<u64>,
    #[serde(rename = "stopReason")]
    pub stop_reason: Option<String>,
    #[serde(rename = "terminalNotificationSuppressed")]
    pub terminal_notification_suppressed: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackgroundTaskOutputSnapshot {
    #[serde(rename = "outputPath")]
    pub output_path: Option<String>,
    #[serde(rename = "outputSizeBytes")]
    pub output_size_bytes: u64,
    #[serde(rename = "previewBytes")]
    pub preview_bytes: usize,
    pub truncated: bool,
    #[serde(rename = "fullOutputAvailable")]
    pub full_output_available: bool,
    pub preview: String,
}

#[derive(Debug, Clone)]
pub struct BackgroundTaskStopResult {
    pub task_id: String,
    pub status: BackgroundTaskStatus,
}

// ---- BackgroundManager trait ----

/// Minimal interface consumed by background management tools.
/// Real implementation in agent-rs (4.3.8) will implement this trait.
pub trait BackgroundManager: Send + Sync {
    fn list(&self, active_only: bool, limit: Option<usize>) -> Vec<BackgroundTaskInfoData>;
    fn get_task(&self, task_id: &str) -> Option<BackgroundTaskInfoData>;
    fn get_output_snapshot(&self, task_id: &str, max_preview_bytes: usize) -> Option<BackgroundTaskOutputSnapshot>;
    fn stop(&self, task_id: &str, reason: Option<String>) -> Option<BackgroundTaskStopResult>;
    fn wait(&self, task_id: &str, timeout_ms: Option<u64>) -> Option<BackgroundTaskInfoData>;
    fn suppress_terminal_notification(&self, task_id: &str);
}

// ---- Mock implementation ----

pub struct MockBackgroundManager {
    tasks: Mutex<HashMap<String, BackgroundTaskInfoData>>,
    output_snapshots: Mutex<HashMap<String, BackgroundTaskOutputSnapshot>>,
}

impl MockBackgroundManager {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
            output_snapshots: Mutex::new(HashMap::new()),
        }
    }

    pub fn add_task(&self, task: BackgroundTaskInfoData) {
        self.tasks.lock().unwrap().insert(task.task_id.clone(), task);
    }

    pub fn set_output_snapshot(&self, task_id: &str, snapshot: BackgroundTaskOutputSnapshot) {
        self.output_snapshots.lock().unwrap().insert(task_id.to_string(), snapshot);
    }

    /// Set a task's status (for testing stop scenarios)
    pub fn set_task_status(&self, task_id: &str, status: BackgroundTaskStatus, stop_reason: Option<String>) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(t) = tasks.get_mut(task_id) {
            t.status = status;
            t.stop_reason = stop_reason;
        }
    }
}

impl Default for MockBackgroundManager {
    fn default() -> Self { Self::new() }
}

impl BackgroundManager for MockBackgroundManager {
    fn list(&self, active_only: bool, limit: Option<usize>) -> Vec<BackgroundTaskInfoData> {
        let tasks = self.tasks.lock().unwrap();
        let mut result: Vec<BackgroundTaskInfoData> = tasks.values()
            .filter(|t| !active_only || !t.status.is_terminal())
            .cloned()
            .collect();
        if let Some(lim) = limit {
            result.truncate(lim);
        }
        result
    }

    fn get_task(&self, task_id: &str) -> Option<BackgroundTaskInfoData> {
        self.tasks.lock().unwrap().get(task_id).cloned()
    }

    fn get_output_snapshot(&self, task_id: &str, _max_preview_bytes: usize) -> Option<BackgroundTaskOutputSnapshot> {
        self.output_snapshots.lock().unwrap().get(task_id).cloned()
    }

    fn stop(&self, task_id: &str, reason: Option<String>) -> Option<BackgroundTaskStopResult> {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.get_mut(task_id).map(|t| {
            t.status = BackgroundTaskStatus::Killed;
            t.stop_reason = reason;
            BackgroundTaskStopResult {
                task_id: task_id.to_string(),
                status: BackgroundTaskStatus::Killed,
            }
        })
    }

    fn wait(&self, _task_id: &str, _timeout_ms: Option<u64>) -> Option<BackgroundTaskInfoData> {
        None
    }

    fn suppress_terminal_notification(&self, task_id: &str) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(t) = tasks.get_mut(task_id) {
            t.terminal_notification_suppressed = true;
        }
    }
}
```

**`rust-ody/crates/tools-rs/src/builtin/cron/mod.rs`:**

```rust
use std::collections::HashMap;
use std::sync::Mutex;

// ---- CronTask type ----

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CronTask {
    /// 8-hex character ID
    pub id: String,
    /// 5-field cron expression
    pub cron: String,
    /// Prompt to enqueue at each fire time
    pub prompt: String,
    /// Creation timestamp in ms
    #[serde(rename = "createdAt")]
    pub created_at: u64,
    /// Whether this is a recurring job (default true)
    pub recurring: bool,
    /// Last fire timestamp in ms, if any
    #[serde(rename = "lastFiredAt")]
    pub last_fired_at: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct SessionCronTaskInit {
    pub cron: String,
    pub prompt: String,
    pub recurring: bool,
}

// ---- SessionCronStore ----

pub struct SessionCronStore {
    tasks: Mutex<HashMap<String, CronTask>>,
}

impl SessionCronStore {
    pub fn new() -> Self {
        Self { tasks: Mutex::new(HashMap::new()) }
    }

    /// Generate a random 8-hex ID and add the task.
    pub fn add(&self, init: SessionCronTaskInit, now_ms: u64) -> CronTask {
        let id = Self::generate_id();
        let task = CronTask {
            id,
            cron: init.cron,
            prompt: init.prompt,
            created_at: now_ms,
            recurring: init.recurring,
            last_fired_at: None,
        };
        self.tasks.lock().unwrap().insert(task.id.clone(), task.clone());
        task
    }

    /// Adopt an existing task (e.g. from disk on resume).
    pub fn adopt(&self, task: CronTask) {
        self.tasks.lock().unwrap().insert(task.id.clone(), task);
    }

    /// Mark a task as fired.
    pub fn mark_fired(&self, id: &str, last_fired_at: u64) -> Option<CronTask> {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(t) = tasks.get_mut(id) {
            t.last_fired_at = Some(last_fired_at);
            Some(t.clone())
        } else {
            None
        }
    }

    pub fn get(&self, id: &str) -> Option<CronTask> {
        self.tasks.lock().unwrap().get(id).cloned()
    }

    pub fn list(&self) -> Vec<CronTask> {
        self.tasks.lock().unwrap().values().cloned().collect()
    }

    pub fn remove(&self, ids: &[String]) -> Vec<String> {
        let mut tasks = self.tasks.lock().unwrap();
        let mut removed = Vec::new();
        for id in ids {
            if tasks.remove(id).is_some() {
                removed.push(id.clone());
            }
        }
        removed
    }

    pub fn clear(&self) {
        self.tasks.lock().unwrap().clear();
    }

    fn generate_id() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        // Simple 8-hex ID from time + counter (non-crypto, matches TS crypto.randomBytes(4) intent)
        let mut id = String::with_capacity(8);
        let mut v = nanos;
        for _ in 0..8 {
            id.push(char::from_digit((v & 0xF) as u32, 16).unwrap_or('0'));
            v = v.wrapping_mul(1103515245).wrapping_add(12345);
        }
        id
    }
}

impl Default for SessionCronStore {
    fn default() -> Self { Self::new() }
}

// ---- CronManager trait ----

/// Minimal interface consumed by cron management tools.
/// Real implementation in agent-rs (4.3.8) will implement this trait.
pub trait CronManager: Send + Sync {
    fn add_task(&self, init: SessionCronTaskInit) -> CronTask;
    fn remove_tasks(&self, ids: &[String]) -> Vec<String>;
    fn list_tasks(&self) -> Vec<CronTask>;
    fn get_task(&self, id: &str) -> Option<CronTask>;
    fn get_next_fire_for_task(&self, task_id: &str) -> Option<u64>;
    fn is_stale(&self, task: &CronTask) -> bool;
    fn now_ms(&self) -> u64;
}

// ---- Mock implementation ----

pub struct MockCronManager {
    store: SessionCronStore,
    now_ms: u64,
}

impl MockCronManager {
    pub fn new(now_ms: Option<u64>) -> Self {
        Self {
            store: SessionCronStore::new(),
            now_ms: now_ms.unwrap_or(0),
        }
    }
}

impl CronManager for MockCronManager {
    fn add_task(&self, init: SessionCronTaskInit) -> CronTask {
        self.store.add(init, self.now_ms)
    }

    fn remove_tasks(&self, ids: &[String]) -> Vec<String> {
        self.store.remove(ids)
    }

    fn list_tasks(&self) -> Vec<CronTask> {
        self.store.list()
    }

    fn get_task(&self, id: &str) -> Option<CronTask> {
        self.store.get(id)
    }

    fn get_next_fire_for_task(&self, _task_id: &str) -> Option<u64> {
        // Mock: no real scheduler
        None
    }

    fn is_stale(&self, task: &CronTask) -> bool {
        // Stale = recurring task older than 7 days
        const STALE_THRESHOLD_MS: u64 = 7 * 24 * 60 * 60 * 1000;
        task.recurring && self.now_ms.saturating_sub(task.created_at) >= STALE_THRESHOLD_MS
    }

    fn now_ms(&self) -> u64 {
        self.now_ms
    }
}
```

**`rust-ody/crates/tools-rs/src/builtin/mod.rs`** — add two lines after existing pub mod declarations:

```rust
pub mod background;
pub mod cron;
```

- [ ] Run it and verify it PASSES

```bash
cd rust-ody && cargo test -p tools-rs --test background_cron_traits 2>&1 | tail -20
# Expected: test result: ok. 8 passed; 0 failed
```

- [ ] Commit

```bash
git add rust-ody/crates/tools-rs/src/builtin/background/mod.rs \
        rust-ody/crates/tools-rs/src/builtin/cron/mod.rs \
        rust-ody/crates/tools-rs/src/builtin/mod.rs \
        rust-ody/crates/tools-rs/tests/background_cron_traits.rs
git commit -m "feat(tools-rs): add BackgroundManager/CronManager traits and mock implementations"
```

---

### Task 2: cron-expr parser

**Depends on:** Task 1 (module structure exists)
**Files:**
- Create: `rust-ody/crates/tools-rs/src/cron/mod.rs`
- Create: `rust-ody/crates/tools-rs/src/cron/cron_expr.rs`
- Modify: `rust-ody/crates/tools-rs/src/lib.rs` (add `pub mod cron`)

- [ ] Write the failing test

Create `rust-ody/crates/tools-rs/tests/cron_expr.rs`:

```rust
use tools_rs::cron::cron_expr::{parse_cron_expression, compute_next_cron_run, cron_to_human, ParsedCronExpression};

// === PARSING ===

#[test]
fn test_parse_simple_wildcard() {
    let p = parse_cron_expression("* * * * *").unwrap();
    assert!(p.minutes.is_empty()); // wildcard → empty vec
    assert!(p.hours.is_empty());
}

#[test]
fn test_parse_specific_values() {
    let p = parse_cron_expression("30 14 28 2 *").unwrap();
    assert_eq!(p.minutes, vec![30]);
    assert_eq!(p.hours, vec![14]);
    assert_eq!(p.days_of_month, vec![28]);
    assert_eq!(p.months, vec![2]);
}

#[test]
fn test_parse_ranges() {
    let p = parse_cron_expression("0-30 9-17 1-5 * 1-5").unwrap();
    assert_eq!(p.minutes, (0..=30).collect::<Vec<u32>>());
    assert_eq!(p.hours, (9..=17).collect::<Vec<u32>>());
    assert_eq!(p.days_of_week, (1..=5).collect::<Vec<u32>>());
}

#[test]
fn test_parse_steps() {
    let p = parse_cron_expression("*/15 */2 * * *").unwrap();
    assert_eq!(p.minutes, vec![0, 15, 30, 45]);
    assert_eq!(p.hours, vec![0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22]);
}

#[test]
fn test_parse_lists() {
    let p = parse_cron_expression("0,15,30,45 8,12,16 * * *").unwrap();
    assert_eq!(p.minutes, vec![0, 15, 30, 45]);
    assert_eq!(p.hours, vec![8, 12, 16]);
}

#[test]
fn test_parse_day_of_week_sunday() {
    let p = parse_cron_expression("0 9 * * 0").unwrap();
    assert!(p.days_of_week.contains(&0));
    let p7 = parse_cron_expression("0 9 * * 7").unwrap();
    assert!(p7.days_of_week.contains(&0)); // 7 → 0
}

#[test]
fn test_parse_reject_invalid() {
    assert!(parse_cron_expression("60 * * * *").is_err());   // minute 60
    assert!(parse_cron_expression("* 24 * * *").is_err());   // hour 24
    assert!(parse_cron_expression("* * 32 * *").is_err());   // dom 32
    assert!(parse_cron_expression("* * * 13 *").is_err());   // month 13
    assert!(parse_cron_expression("* * * * 8").is_err());    // dow 8
    assert!(parse_cron_expression("").is_err());
    assert!(parse_cron_expression("a b c d e").is_err());
}

#[test]
fn test_parse_normalize_whitespace() {
    let p = parse_cron_expression("  0   9   *   *   *  ").unwrap();
    assert_eq!(p.minutes, vec![0]);
    assert_eq!(p.hours, vec![9]);
}

// === COMPUTE NEXT RUN ===

#[test]
fn test_next_run_every_minute() {
    let p = parse_cron_expression("* * * * *").unwrap();
    let from = 1000 * 60 * 60 * 9; // 9:00 AM UTC in ms
    let next = compute_next_cron_run(&p, from).unwrap();
    assert_eq!(next, from + 60_000); // next minute
}

#[test]
fn test_next_run_specific_time() {
    let p = parse_cron_expression("30 14 * * *").unwrap();
    // 2026-01-01 12:00 UTC in ms: 1735732800000
    let from: u64 = 1735732800000;
    let next = compute_next_cron_run(&p, from).unwrap();
    // Should be 2026-01-01 14:30 UTC
    let expected = from + (2 * 3600 + 30 * 60) * 1000;
    assert_eq!(next, expected);
}

#[test]
fn test_next_run_no_match() {
    let p = parse_cron_expression("0 0 29 2 *").unwrap(); // Feb 29 only
    // 2025-03-01 (non-leap year) in ms
    let from: u64 = 1740806400000;
    let next = compute_next_cron_run(&p, from);
    // Should return None (no Feb 29 in 2025, 2026, 2027, 2028 — within 5-year window from 2025)
    // Actually 2028 is a leap year: Feb 29 2028 00:00
    assert!(next.is_some()); // 2028-02-29 exists
}

#[test]
fn test_next_run_first_of_month() {
    let p = parse_cron_expression("0 0 1 * *").unwrap();
    // 2026-01-15 12:00 UTC in ms
    let from: u64 = 1736942400000;
    let next = compute_next_cron_run(&p, from).unwrap();
    // Should be 2026-02-01 00:00
    let feb_first: u64 = 1738368000000;
    assert_eq!(next, feb_first);
}

// === CRON TO HUMAN ===

#[test]
fn test_cron_to_human_every_minute() {
    let p = parse_cron_expression("* * * * *").unwrap();
    assert_eq!(cron_to_human(&p), "every minute");
}

#[test]
fn test_cron_to_human_daily() {
    let p = parse_cron_expression("0 9 * * *").unwrap();
    assert!(cron_to_human(&p).contains("9:00"));
}

#[test]
fn test_cron_to_human_every_5_minutes() {
    let p = parse_cron_expression("*/5 * * * *").unwrap();
    assert_eq!(cron_to_human(&p), "every 5 minutes");
}

#[test]
fn test_cron_to_human_hourly() {
    let p = parse_cron_expression("0 * * * *").unwrap();
    assert_eq!(cron_to_human(&p), "hourly");
}

#[test]
fn test_cron_to_human_weekdays() {
    let p = parse_cron_expression("0 9 * * 1-5").unwrap();
    assert!(cron_to_human(&p).contains("weekdays"));
}
```

- [ ] Run it and verify it FAILS

```bash
cd rust-ody && cargo test -p tools-rs --test cron_expr 2>&1 | tail -5
# Expected: error[E0432]: unresolved import `tools_rs::cron`
```

- [ ] Write the minimal implementation

**`rust-ody/crates/tools-rs/src/cron/mod.rs`:**

```rust
pub mod clock;
pub mod cron_expr;
pub mod jitter;
pub mod time_format;
```

**`rust-ody/crates/tools-rs/src/cron/cron_expr.rs`:**

```rust
use std::collections::HashSet;

/// Parsed 5-field cron expression. Empty vec = wildcard (all values).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCronExpression {
    pub raw: String,
    pub minutes: Vec<u32>,
    pub hours: Vec<u32>,
    pub days_of_month: Vec<u32>,
    pub months: Vec<u32>,
    pub days_of_week: Vec<u32>,
    /// True when days_of_month is wildcard (*)
    pub days_of_month_wildcard: bool,
    /// True when days_of_week is wildcard (*)
    pub days_of_week_wildcard: bool,
}

/// Parse a 5-field cron expression. Returns error with description on invalid input.
pub fn parse_cron_expression(expr: &str) -> Result<ParsedCronExpression, String> {
    let raw = expr.trim().to_string();
    let fields: Vec<&str> = raw.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(format!("Expected 5 fields, got {}", fields.len()));
    }

    let minutes = parse_field(fields[0], 0, 59)?;
    let hours = parse_field(fields[1], 0, 23)?;
    let days_of_month = parse_field(fields[2], 1, 31)?;
    let months = parse_field(fields[3], 1, 12)?;
    let days_of_week = parse_field_dow(fields[4])?;

    let dom_wildcard = fields[2] == "*";
    let dow_wildcard = fields[4] == "*";

    Ok(ParsedCronExpression {
        raw,
        minutes,
        hours,
        days_of_month,
        months,
        days_of_week,
        days_of_month_wildcard: dom_wildcard,
        days_of_week_wildcard: dow_wildcard,
    })
}

fn parse_field(field: &str, min: u32, max: u32) -> Result<Vec<u32>, String> {
    if field == "*" {
        return Ok(Vec::new());
    }

    let mut values = Vec::new();
    for part in field.split(',') {
        let part = part.trim();
        if part.contains('/') {
            // Step syntax: */5 or 0-30/5
            let mut split = part.splitn(2, '/');
            let range = split.next().unwrap();
            let step: u32 = split.next().unwrap().parse()
                .map_err(|_| format!("Invalid step: {}", part))?;
            let (r_min, r_max) = if range == "*" {
                (min, max)
            } else if range.contains('-') {
                let mut rs = range.splitn(2, '-');
                let lo: u32 = rs.next().unwrap().parse().map_err(|_| format!("Invalid range start: {}", part))?;
                let hi: u32 = rs.next().unwrap().parse().map_err(|_| format!("Invalid range end: {}", part))?;
                (lo, hi)
            } else {
                return Err(format!("Invalid step range: {}", part));
            };
            for v in (r_min..=r_max).step_by(step as usize) {
                values.push(v);
            }
        } else if part.contains('-') {
            let mut rs = part.splitn(2, '-');
            let lo: u32 = rs.next().unwrap().parse().map_err(|_| format!("Invalid range: {}", part))?;
            let hi: u32 = rs.next().unwrap().parse().map_err(|_| format!("Invalid range: {}", part))?;
            for v in lo..=hi {
                values.push(v);
            }
        } else {
            let v: u32 = part.parse().map_err(|_| format!("Invalid value: {}", part))?;
            values.push(v);
        }
    }

    // Validate range
    for &v in &values {
        if v < min || v > max {
            return Err(format!("Value {} out of range [{}, {}]", v, min, max));
        }
    }

    Ok(values)
}

fn parse_field_dow(field: &str) -> Result<Vec<u32>, String> {
    let values = parse_field(field, 0, 7)?;
    // Normalize 7 → 0 (Sunday)
    Ok(values.into_iter().map(|v| if v == 7 { 0 } else { v }).collect())
}

fn matches_field(value: u32, allowed: &[u32]) -> bool {
    if allowed.is_empty() {
        return true; // wildcard
    }
    allowed.contains(&value)
}

/// Compute the next cron fire time in milliseconds since epoch.
/// Returns None if no fire within 5 years.
pub fn compute_next_cron_run(expr: &ParsedCronExpression, from_ms: u64) -> Option<u64> {
    let five_years_ms: u64 = 5 * 365 * 24 * 3600 * 1000;
    let max_ms = from_ms + five_years_ms;

    // Start from the next minute to avoid matching the current minute
    let mut current = ((from_ms / 60000) + 1) * 60000;

    while current <= max_ms {
        let dt = utc_millis_to_components(current);

        if matches_field(dt.minute, &expr.minutes)
            && matches_field(dt.hour, &expr.hours)
            && matches_field(dt.month, &expr.months)
        {
            // Day matching: OR between days_of_month and days_of_week
            let dom_match = expr.days_of_month_wildcard
                || matches_field(dt.day, &expr.days_of_month);
            let dow_match = expr.days_of_week_wildcard
                || matches_field(dt.day_of_week, &expr.days_of_week);

            // When both are non-wildcard, either match counts (OR semantics)
            let day_ok = if !expr.days_of_month_wildcard && !expr.days_of_week_wildcard {
                dom_match || dow_match
            } else {
                dom_match && dow_match
            };

            if day_ok {
                return Some(current);
            }
        }

        current += 60000; // advance 1 minute
    }

    None
}

#[derive(Debug)]
struct DateTimeComponents {
    minute: u32,
    hour: u32,
    day: u32,
    month: u32,
    year: u32,
    day_of_week: u32, // 0=Sun
}

fn utc_millis_to_components(ms: u64) -> DateTimeComponents {
    let total_secs = (ms / 1000) as i64;
    // Use chrono for reliable date math
    let dt = chrono::DateTime::from_timestamp(total_secs, 0)
        .expect("valid timestamp");
    DateTimeComponents {
        minute: dt.minute(),
        hour: dt.hour(),
        day: dt.day(),
        month: dt.month(),
        year: dt.year() as u32,
        day_of_week: dt.weekday().num_days_from_sunday(),
    }
}

/// Check if a parsed expression has at least one fire within `years` from `from_ms`.
pub fn has_fire_within_years(expr: &ParsedCronExpression, years: u32, from_ms: u64) -> bool {
    let max_ms = from_ms + (years as u64) * 365 * 24 * 3600 * 1000;
    compute_next_cron_run(expr, from_ms)
        .map(|next| next <= max_ms)
        .unwrap_or(false)
}

/// Produce a human-readable description of a cron expression.
pub fn cron_to_human(expr: &ParsedCronExpression) -> String {
    // Simple heuristic-based descriptions matching TS behavior
    let parts: Vec<&str> = expr.raw.split_whitespace().collect();
    if parts.len() != 5 {
        return expr.raw.clone();
    }

    let (min, hour, dom, month, dow) = (parts[0], parts[1], parts[2], parts[3], parts[4]);

    // Every minute
    if min == "*" && hour == "*" && dom == "*" && month == "*" && dow == "*" {
        return "every minute".to_string();
    }

    // Every N minutes
    if min.starts_with("*/") && hour == "*" && dom == "*" && month == "*" && dow == "*" {
        let n = &min[2..];
        if n == "1" { return "every minute".to_string(); }
        return format!("every {} minutes", n);
    }

    // Hourly
    if min == "0" && hour == "*" && dom == "*" && month == "*" && dow == "*" {
        return "hourly".to_string();
    }

    // Daily at specific time
    if hour != "*" && dom == "*" && month == "*" && dow == "*" {
        let h: u32 = hour.parse().unwrap_or(0);
        let m: u32 = min.parse().unwrap_or(0);
        let ampm = if h == 0 { "12:00 AM".to_string() }
            else if h < 12 { format!("{}:{:02} AM", h, m) }
            else if h == 12 { format!("12:{:02} PM", m) }
            else { format!("{}:{:02} PM", h - 12, m) };
        return format!("daily at {}", ampm);
    }

    // Weekdays
    if hour != "*" && dow == "1-5" && dom == "*" && month == "*" {
        let h: u32 = hour.parse().unwrap_or(0);
        let m: u32 = min.parse().unwrap_or(0);
        let ampm = if h < 12 { format!("{}:{:02} AM", h, m) }
            else if h == 12 { format!("12:{:02} PM", m) }
            else { format!("{}:{:02} PM", h - 12, m) };
        return format!("weekdays at {}", ampm);
    }

    // Fallback: show the raw expression
    expr.raw.clone()
}
```

**Note on `Cargo.toml`:** Add to `rust-ody/crates/tools-rs/Cargo.toml`:

```toml
chrono = { version = "0.4", features = ["clock"] }
```

**`rust-ody/crates/tools-rs/src/lib.rs`** — add:

```rust
pub mod cron;
```

- [ ] Run it and verify it PASSES

```bash
cd rust-ody && cargo test -p tools-rs --test cron_expr 2>&1 | tail -20
# Expected: test result: ok. 16 passed; 0 failed
```

- [ ] Commit

```bash
git add rust-ody/crates/tools-rs/Cargo.toml \
        rust-ody/crates/tools-rs/src/lib.rs \
        rust-ody/crates/tools-rs/src/cron/ \
        rust-ody/crates/tools-rs/tests/cron_expr.rs
git commit -m "feat(tools-rs): add cron expression parser with computeNextCronRun and cronToHuman"
```

---

### Task 3: jitter module

**Depends on:** Task 2 (chrono available, cron module exists)
**Files:**
- Create: `rust-ody/crates/tools-rs/src/cron/jitter.rs`

- [ ] Write the failing test

Append to `rust-ody/crates/tools-rs/tests/cron_expr.rs`:

```rust
use tools_rs::cron::jitter::{jittered_next_cron_run_ms, one_shot_jittered_next_cron_run_ms, JitterConfig};

#[test]
fn test_jitter_recurring_forward_shift() {
    let config = JitterConfig {
        recurring_max_fraction_of_period: 0.1,
        recurring_max_ms: 15 * 60 * 1000, // 15 min
        one_shot_max_ms: 90 * 1000,
    };
    let expr = parse_cron_expression("0 9 * * *").unwrap();
    // ideal fire at 9:00
    let ideal: u64 = 100000000;
    let jittered = jittered_next_cron_run_ms(
        &expr, ideal, "aabbccdd", &config
    );
    // Should be >= ideal (forward shift only for recurring)
    assert!(jittered >= ideal);
    // Should not exceed ideal + 15 minutes for a daily cron
    assert!(jittered < ideal + 15 * 60 * 1000);
}

#[test]
fn test_jitter_deterministic_same_id() {
    let config = JitterConfig {
        recurring_max_fraction_of_period: 0.1,
        recurring_max_ms: 15 * 60 * 1000,
        one_shot_max_ms: 90 * 1000,
    };
    let expr = parse_cron_expression("0 9 * * *").unwrap();
    let ideal: u64 = 100000000;
    let a = jittered_next_cron_run_ms(&expr, ideal, "deadbeef", &config);
    let b = jittered_next_cron_run_ms(&expr, ideal, "deadbeef", &config);
    assert_eq!(a, b, "same id must produce same jitter");
}

#[test]
fn test_jitter_different_ids() {
    let config = JitterConfig {
        recurring_max_fraction_of_period: 0.1,
        recurring_max_ms: 15 * 60 * 1000,
        one_shot_max_ms: 90 * 1000,
    };
    let expr = parse_cron_expression("0 9 * * *").unwrap();
    let ideal: u64 = 100000000;
    let a = jittered_next_cron_run_ms(&expr, ideal, "aaaaaaaa", &config);
    let b = jittered_next_cron_run_ms(&expr, ideal, "bbbbbbbb", &config);
    // Different IDs should (almost certainly) produce different jitter
    assert_ne!(a, b);
}

#[test]
fn test_one_shot_jitter_pull_forward() {
    let config = JitterConfig {
        recurring_max_fraction_of_period: 0.1,
        recurring_max_ms: 15 * 60 * 1000,
        one_shot_max_ms: 90 * 1000,
    };
    // Ideal is at :00 — should be pulled earlier
    let ideal: u64 = 100000000;
    let jittered = one_shot_jittered_next_cron_run_ms(
        "abcdef01", ideal, &config
    );
    assert!(jittered <= ideal, "one-shot jitter pulls earlier");
    assert!(jittered >= ideal - 90_000, "max 90s pull-forward");
    // For :30, also applies
    let ideal30: u64 = 100000000 + 30 * 60 * 1000;
    let jittered30 = one_shot_jittered_next_cron_run_ms(
        "abcdef01", ideal30, &config
    );
    assert!(jittered30 <= ideal30);
    assert!(jittered30 >= ideal30 - 90_000);
}

#[test]
fn test_one_shot_no_jitter_on_non_round_minute() {
    let config = JitterConfig {
        recurring_max_fraction_of_period: 0.1,
        recurring_max_ms: 15 * 60 * 1000,
        one_shot_max_ms: 90 * 1000,
    };
    // Not :00 or :30 — should pass through unchanged
    let ideal: u64 = 100000000 + 7 * 60 * 1000; // :07
    let jittered = one_shot_jittered_next_cron_run_ms(
        "abcdef01", ideal, &config
    );
    assert_eq!(jittered, ideal, "non-round minute passes through unchanged");
}
```

- [ ] Run it and verify it FAILS

```bash
cd rust-ody && cargo test -p tools-rs --test cron_expr 2>&1 | tail -5
# Expected: error[E0432]: unresolved import `tools_rs::cron::jitter`
```

- [ ] Write the minimal implementation

**`rust-ody/crates/tools-rs/src/cron/jitter.rs`:**

```rust
use crate::cron::cron_expr::{ParsedCronExpression, compute_next_cron_run};

/// Configuration for deterministic jitter.
#[derive(Debug, Clone, Copy)]
pub struct JitterConfig {
    /// Maximum fraction of the cron period to shift forward for recurring tasks.
    pub recurring_max_fraction_of_period: f64,
    /// Maximum forward shift in ms for recurring tasks.
    pub recurring_max_ms: u64,
    /// Maximum pull-forward shift in ms for one-shot tasks.
    pub one_shot_max_ms: u64,
}

impl Default for JitterConfig {
    fn default() -> Self {
        Self {
            recurring_max_fraction_of_period: 0.1,
            recurring_max_ms: 15 * 60 * 1000, // 15 minutes
            one_shot_max_ms: 90 * 1000,       // 90 seconds
        }
    }
}

/// Compute a deterministic forward jitter offset for a recurring task.
///
/// The jitter is based on the task's 8-hex ID, converted to a fraction [0, 1).
/// It shifts the ideal fire time FORWARD by up to `min(fraction * period, max_ms)`.
pub fn jittered_next_cron_run_ms(
    expr: &ParsedCronExpression,
    ideal_ms: u64,
    task_id: &str,
    config: &JitterConfig,
) -> u64 {
    let fraction = fraction_from_id(task_id);
    let period_ms = estimate_cron_period_ms(expr);
    let max_jitter = (period_ms as f64 * config.recurring_max_fraction_of_period) as u64;
    let max_jitter = max_jitter.min(config.recurring_max_ms);
    let jitter_ms = (fraction * max_jitter as f64) as u64;
    ideal_ms.saturating_add(jitter_ms)
}

/// Compute deterministic pull-forward jitter for one-shot tasks.
///
/// Only applies when the ideal fire time lands on :00 or :30 of the hour.
/// Shifts EARLIER by a deterministic offset based on the task ID.
pub fn one_shot_jittered_next_cron_run_ms(
    task_id: &str,
    ideal_ms: u64,
    config: &JitterConfig,
) -> u64 {
    let minute = (ideal_ms / 60000) % 60;
    if minute == 0 || minute == 30 {
        let fraction = fraction_from_id(task_id);
        let jitter_ms = (fraction * config.one_shot_max_ms as f64) as u64;
        ideal_ms.saturating_sub(jitter_ms)
    } else {
        ideal_ms
    }
}

/// Convert an 8-hex task ID to a deterministic fraction in [0.0, 1.0).
fn fraction_from_id(id: &str) -> f64 {
    let hex_part: String = id.chars().filter(|c| c.is_ascii_hexdigit()).take(8).collect();
    if hex_part.is_empty() {
        return 0.0;
    }
    let val = u32::from_str_radix(&hex_part, 16).unwrap_or(0);
    val as f64 / u32::MAX as f64
}

/// Rough estimate of a cron expression's period in milliseconds.
/// Used to bound the jitter range for recurring tasks.
fn estimate_cron_period_ms(expr: &ParsedCronExpression) -> u64 {
    // Estimate based on the smallest non-wildcard field
    // Minutes: if non-wildcard with list/ranges, use step size
    // Hours: same
    // Default: 1 day
    if !expr.minutes.is_empty() {
        let step = min_step(&expr.minutes);
        return (step as u64) * 60 * 1000;
    }
    if !expr.hours.is_empty() {
        let step = min_step(&expr.hours);
        return (step as u64) * 3600 * 1000;
    }
    if !expr.days_of_month.is_empty() || !expr.days_of_week.is_empty() {
        return 24 * 3600 * 1000;
    }
    if !expr.months.is_empty() {
        return 30 * 24 * 3600 * 1000;
    }
    0
}

fn min_step(values: &[u32]) -> u32 {
    if values.len() < 2 {
        return 1;
    }
    let mut sorted: Vec<u32> = values.to_vec();
    sorted.sort();
    sorted.dedup();
    let mut min_diff = u32::MAX;
    for w in sorted.windows(2) {
        let diff = w[1] - w[0];
        if diff < min_diff {
            min_diff = diff;
        }
    }
    if min_diff == u32::MAX { 1 } else { min_diff }
}
```

- [ ] Run it and verify it PASSES

```bash
cd rust-ody && cargo test -p tools-rs --test cron_expr 2>&1 | tail -20
# Expected: test result: ok. 21 passed; 0 failed
```

- [ ] Commit

```bash
git add rust-ody/crates/tools-rs/src/cron/jitter.rs \
        rust-ody/crates/tools-rs/tests/cron_expr.rs
git commit -m "feat(tools-rs): add deterministic jitter for cron fire times"
```

---

### Task 4: time-format + clock-sources

**Depends on:** Task 2 (chrono available)
**Files:**
- Create: `rust-ody/crates/tools-rs/src/cron/time_format.rs`
- Create: `rust-ody/crates/tools-rs/src/cron/clock.rs`

- [ ] Write the failing test

Append to `rust-ody/crates/tools-rs/tests/cron_expr.rs`:

```rust
use tools_rs::cron::time_format::format_local_iso_with_offset;
use tools_rs::cron::clock::{ClockSources, SystemClock};

#[test]
fn test_format_local_iso_with_offset() {
    // Use a known UTC timestamp: 2026-06-15T09:30:00Z = 1774549800000 ms
    let ms: u64 = 1774549800000;
    let formatted = format_local_iso_with_offset(ms);
    // Format should look like: "2026-06-15T..." with timezone offset
    assert!(formatted.starts_with("2026-06-15"));
    assert!(formatted.contains("T"));
    // Should contain timezone offset like +08:00 or -04:00 or Z
    assert!(formatted.contains('+') || formatted.contains('-') || formatted.ends_with('Z'));
}

#[test]
fn test_system_clock_wall_now() {
    let clock = SystemClock;
    let now = clock.wall_now();
    // Should be a reasonable epoch millis (after 2020)
    assert!(now > 1577836800000); // 2020-01-01
}

#[test]
fn test_system_clock_mono_now() {
    let clock = SystemClock;
    let a = clock.mono_now_ms();
    std::thread::sleep(std::time::Duration::from_millis(10));
    let b = clock.mono_now_ms();
    assert!(b > a, "monotonic clock should advance");
}
```

- [ ] Run it and verify it FAILS

```bash
cd rust-ody && cargo test -p tools-rs --test cron_expr 2>&1 | tail -5
# Expected: error[E0432]: unresolved import `tools_rs::cron::time_format`
```

- [ ] Write the minimal implementation

**`rust-ody/crates/tools-rs/src/cron/time_format.rs`:**

```rust
/// Format a millisecond timestamp as local ISO 8601 with numeric timezone offset.
/// Example: "2026-06-15T17:30:00.000+08:00"
pub fn format_local_iso_with_offset(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let nsecs = ((ms % 1000) * 1_000_000) as u32;
    match chrono::DateTime::from_timestamp(secs, nsecs) {
        Some(utc) => {
            let local = utc.with_timezone(&chrono::Local);
            local.format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string()
        }
        None => {
            // Fallback for out-of-range timestamps
            format!("<invalid timestamp {}>", ms)
        }
    }
}
```

**`rust-ody/crates/tools-rs/src/cron/clock.rs`:**

```rust
use std::time::{SystemTime, UNIX_EPOCH, Instant};

/// Abstraction over clock sources for testability.
pub trait ClockSources: Send + Sync {
    /// Wall clock time in milliseconds since epoch.
    fn wall_now(&self) -> u64;
    /// Monotonic time in milliseconds (for intervals, not tied to wall clock).
    fn mono_now_ms(&self) -> u64;
}

/// Default system clock implementation.
pub struct SystemClock;

impl ClockSources for SystemClock {
    fn wall_now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn mono_now_ms(&self) -> u64 {
        // On most platforms, Instant uses a monotonic clock
        // We approximate ms from the opaque Instant
        // Note: Instant doesn't expose epoch, but for interval timing this is fine
        // We use a lazy static base to convert to approximate ms
        use std::sync::OnceLock;
        static BASE: OnceLock<(Instant, u64)> = OnceLock::new();
        let (base_instant, base_wall) = BASE.get_or_init(|| {
            (Instant::now(), SystemClock.wall_now())
        });
        let elapsed = base_instant.elapsed().as_millis() as u64;
        base_wall + elapsed
    }
}
```

- [ ] Run it and verify it PASSES

```bash
cd rust-ody && cargo test -p tools-rs --test cron_expr 2>&1 | tail -20
# Expected: test result: ok. 24 passed; 0 failed
```

- [ ] Commit

```bash
git add rust-ody/crates/tools-rs/src/cron/time_format.rs \
        rust-ody/crates/tools-rs/src/cron/clock.rs \
        rust-ody/crates/tools-rs/tests/cron_expr.rs
git commit -m "feat(tools-rs): add ISO 8601 time formatting and clock source abstraction"
```

---

## Part 1 Self-Review

- [ ] 1. Spec-coverage: Task 1 covers BackgroundManager/CronManager traits + SessionCronStore. Task 2 covers cron-expr parser (parse, computeNextCronRun, cronToHuman, hasFireWithinYears). Task 3 covers jitter (recurring forward, one-shot pull-forward). Task 4 covers time-format + clock-sources. All infrastructure from 4.4.3 covered.
- [ ] 2. Placeholder scan: No TODO/TBD. All code is concrete with exact implementations.
- [ ] 3. No phantom tasks: Each task produces a verifiable test that passes.
- [ ] 4. Dependency soundness: Task 2 depends on Task 1 (module structure). Task 3 depends on Task 2 (cron_expr types). Task 4 depends on Task 2 (chrono available). All deps are satisfied by earlier tasks.
- [ ] 5. Caller & build soundness: `cron::mod.rs` is the only new shared-signature file; it's created in Task 2 and only added to (not changed) by Tasks 3-4. `lib.rs` gets one line added in Task 2. Whole-tree typecheck: `cargo check -p tools-rs` at end.
- [ ] 6. Test-the-risk: cron-expr parsing tests cover all syntax variants (wildcards, ranges, steps, lists, DOW normalization). Jitter tests verify determinism and range bounds. Time format tests verify ISO 8601 output.
- [ ] 7. Type consistency: `ParsedCronExpression` defined in Task 2 is consumed by Task 3 jitter. `JitterConfig` defined in Task 3 is consumed by Part 3 tools. `BackgroundManager`/`CronManager` traits defined in Task 1 are consumed by Parts 2-3 tools. All type names and field names match the names used in later parts.
