#[cfg(test)]
mod tests {
    use super::*;

    fn init() -> CronTaskInit {
        CronTaskInit {
            cron: "0 9 * * *".to_string(),
            prompt: "morning standup".to_string(),
            recurring: Some(true),
        }
    }

    #[test]
    fn store_add_and_list() {
        let mut store = SessionCronStore::new();
        let t = store.add(init(), 1000);
        assert!(t.id.len() == 8);
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.get(&t.id).unwrap().prompt, "morning standup");
    }

    #[test]
    fn store_mark_fired_updates_last_fired_at() {
        let mut store = SessionCronStore::new();
        let t = store.add(init(), 1000);
        let updated = store.mark_fired(&t.id, 5000).unwrap();
        assert_eq!(updated.last_fired_at, Some(5000));
        assert_eq!(store.get(&t.id).unwrap().last_fired_at, Some(5000));
    }

    #[test]
    fn store_remove_returns_only_present() {
        let mut store = SessionCronStore::new();
        let t = store.add(init(), 1000);
        let removed = store.remove(&[t.id.clone(), "ffffffff".to_string()]);
        assert_eq!(removed, vec![t.id]);
        assert!(store.list().is_empty());
    }

    #[test]
    fn store_adopt_preserves_id() {
        let mut store = SessionCronStore::new();
        let t = CronTask {
            id: "a1b2c3d4".to_string(),
            cron: "0 9 * * *".to_string(),
            prompt: "x".to_string(),
            created_at: 42,
            recurring: None,
            last_fired_at: None,
        };
        store.adopt(t.clone());
        assert_eq!(store.get("a1b2c3d4").unwrap().created_at, 42);
    }
}

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronTask {
    pub id: String,
    pub cron: String,
    pub prompt: String,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurring: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_fired_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct CronTaskInit {
    pub cron: String,
    pub prompt: String,
    pub recurring: Option<bool>,
}

pub struct SessionCronStore {
    tasks: HashMap<String, CronTask>,
    id_generator: Box<dyn Fn() -> String + Send + Sync>,
}

const ID_REGEX: &str = r"^[0-9a-f]{8}$";
const MAX_ID_ATTEMPTS: usize = 8;

impl Default for SessionCronStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionCronStore {
    pub fn new() -> Self {
        Self::with_id_generator(Box::new(random_hex_id))
    }

    pub fn with_id_generator(gen: Box<dyn Fn() -> String + Send + Sync>) -> Self {
        Self {
            tasks: HashMap::new(),
            id_generator: gen,
        }
    }

    pub fn add(&mut self, init: CronTaskInit, now_ms: i64) -> CronTask {
        let id = self.generate_unique_id();
        let task = CronTask {
            id,
            cron: init.cron,
            prompt: init.prompt,
            created_at: now_ms,
            recurring: init.recurring,
            last_fired_at: None,
        };
        self.tasks.insert(task.id.clone(), task.clone());
        task
    }

    pub fn adopt(&mut self, task: CronTask) {
        self.tasks.insert(task.id.clone(), task);
    }

    pub fn mark_fired(&mut self, id: &str, last_fired_at: i64) -> Option<CronTask> {
        let existing = self.tasks.get_mut(id)?;
        existing.last_fired_at = Some(last_fired_at);
        Some(existing.clone())
    }

    pub fn get(&self, id: &str) -> Option<&CronTask> {
        self.tasks.get(id)
    }

    pub fn list(&self) -> Vec<CronTask> {
        self.tasks.values().cloned().collect()
    }

    pub fn remove(&mut self, ids: &[String]) -> Vec<String> {
        let mut removed = Vec::new();
        for id in ids {
            if self.tasks.remove(id).is_some() {
                removed.push(id.clone());
            }
        }
        removed
    }

    pub fn clear(&mut self) {
        self.tasks.clear();
    }

    fn generate_unique_id(&mut self) -> String {
        for _ in 0..MAX_ID_ATTEMPTS {
            let candidate = (self.id_generator)();
            if regex::Regex::new(ID_REGEX).unwrap().is_match(&candidate)
                && !self.tasks.contains_key(&candidate)
            {
                return candidate;
            }
        }
        panic!(
            "SessionCronStore: failed to generate a unique 8-hex id after {} attempts",
            MAX_ID_ATTEMPTS
        );
    }
}

fn random_hex_id() -> String {
    let bytes: [u8; 4] = rand::thread_rng().gen();
    hex::encode(bytes)
}
