use std::collections::HashMap;
use std::sync::Mutex;

pub mod task_list;
pub mod task_output;
pub mod task_stop;

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
        matches!(
            self,
            Self::Completed | Self::Failed | Self::TimedOut | Self::Killed | Self::Lost
        )
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
    fn get_output_snapshot(
        &self,
        task_id: &str,
        max_preview_bytes: usize,
    ) -> Option<BackgroundTaskOutputSnapshot>;
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
        self.tasks
            .lock()
            .unwrap()
            .insert(task.task_id.clone(), task);
    }

    pub fn set_output_snapshot(&self, task_id: &str, snapshot: BackgroundTaskOutputSnapshot) {
        self.output_snapshots
            .lock()
            .unwrap()
            .insert(task_id.to_string(), snapshot);
    }

    /// Set a task's status (for testing stop scenarios)
    pub fn set_task_status(
        &self,
        task_id: &str,
        status: BackgroundTaskStatus,
        stop_reason: Option<String>,
    ) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(t) = tasks.get_mut(task_id) {
            t.status = status;
            t.stop_reason = stop_reason;
        }
    }
}

impl Default for MockBackgroundManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BackgroundManager for MockBackgroundManager {
    fn list(&self, active_only: bool, limit: Option<usize>) -> Vec<BackgroundTaskInfoData> {
        let tasks = self.tasks.lock().unwrap();
        let mut result: Vec<BackgroundTaskInfoData> = tasks
            .values()
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

    fn get_output_snapshot(
        &self,
        task_id: &str,
        _max_preview_bytes: usize,
    ) -> Option<BackgroundTaskOutputSnapshot> {
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
