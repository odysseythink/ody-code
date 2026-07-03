use std::collections::HashMap;
use std::sync::Mutex;

pub mod cron_create;
pub mod cron_delete;
pub mod cron_list;

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
    counter: std::sync::atomic::AtomicU64,
}

impl SessionCronStore {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
            counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Generate a unique 8-hex ID and add the task.
    pub fn add(&self, init: SessionCronTaskInit, now_ms: u64) -> CronTask {
        let id = Self::generate_id(&self);
        let task = CronTask {
            id,
            cron: init.cron,
            prompt: init.prompt,
            created_at: now_ms,
            recurring: init.recurring,
            last_fired_at: None,
        };
        self.tasks
            .lock()
            .unwrap()
            .insert(task.id.clone(), task.clone());
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

    fn generate_id(store: &SessionCronStore) -> String {
        let counter = store
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        // Produce a unique 8-hex ID from time + counter, using all bits
        let mut v = now.wrapping_add(counter.wrapping_mul(0x9e3779b97f4a7c15)); // golden ratio multiplier
        let mut id = String::with_capacity(8);
        for _ in 0..8 {
            id.push(char::from_digit((v & 0xF) as u32, 16).unwrap_or('0'));
            v >>= 4; // shift to use all bits, not just LCG low bits
        }
        id
    }
}

impl Default for SessionCronStore {
    fn default() -> Self {
        Self::new()
    }
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
    pub store: SessionCronStore,
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
