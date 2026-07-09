use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_other: i64,
    pub output: i64,
    pub input_cache_read: i64,
    pub input_cache_creation: i64,
}

impl TokenUsage {
    pub fn input_total(&self) -> i64 {
        self.input_other + self.input_cache_read + self.input_cache_creation
    }

    pub fn grand_total(&self) -> i64 {
        self.input_total() + self.output
    }
}
