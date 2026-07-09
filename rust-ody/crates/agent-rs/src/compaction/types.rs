pub use crate::records::nested::{CompactionBeginData, CompactionResult, CompactionSource};

#[derive(Debug, Clone)]
pub struct CompactionConfig {
    pub trigger_ratio: f64,
    pub block_ratio: f64,
    pub reserved_context_size: i64,
    pub max_compaction_per_turn: i64,
    pub max_recent_messages: usize,
    pub max_recent_user_messages: usize,
    pub max_recent_size_ratio: f64,
    pub min_overflow_reduction_ratio: f64,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            trigger_ratio: 0.85,
            block_ratio: 0.85,
            reserved_context_size: 50_000,
            max_compaction_per_turn: 3,
            max_recent_messages: 4,
            max_recent_user_messages: usize::MAX,
            max_recent_size_ratio: 0.2,
            min_overflow_reduction_ratio: 0.05,
        }
    }
}
