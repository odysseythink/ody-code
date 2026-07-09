use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

pub trait ToolStore: Send + Sync {
    fn get(&self, key: &str) -> Option<Value>;
    fn set(&self, key: &str, value: Value);
}

#[derive(Debug, Default)]
pub struct InMemoryToolStore {
    data: Mutex<HashMap<String, Value>>,
}

impl InMemoryToolStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ToolStore for InMemoryToolStore {
    fn get(&self, key: &str) -> Option<Value> {
        self.data.lock().unwrap().get(key).cloned()
    }

    fn set(&self, key: &str, value: Value) {
        self.data.lock().unwrap().insert(key.to_owned(), value);
    }
}

/// Mock ToolStore for golden/testing that uses interior mutability.
pub struct MockToolStore {
    data: Mutex<HashMap<String, Value>>,
}

impl MockToolStore {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
        }
    }
}

impl ToolStore for MockToolStore {
    fn get(&self, key: &str) -> Option<Value> {
        self.data.lock().unwrap().get(key).cloned()
    }

    fn set(&self, key: &str, value: Value) {
        self.data.lock().unwrap().insert(key.to_string(), value);
    }
}
