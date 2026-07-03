use async_trait::async_trait;

/// Mirrors TS `DynamicInjector`.
/// Tracks `injected_at` position in context history for dedup/position tracking.
#[async_trait]
pub trait DynamicInjector: Send + Sync {
    /// The variant string used in `system-reminder` records.
    fn variant(&self) -> &str;

    /// Main injection call. Called before each step.
    /// Returns `None` if nothing to inject; `Some(text)` if an injection should be appended.
    async fn get_injection(
        &self,
        ctx: &dyn super::types::InjectionManagerContext,
    ) -> Option<String>;

    /// Reset injection state (e.g. after context clear).
    fn on_context_clear(&mut self);

    /// Adjust injected position after compaction removes messages.
    fn on_context_compacted(&mut self, compacted_count: usize);

    /// Adjust injected position after undo removes a message at `index`.
    fn on_context_message_removed(&mut self, index: usize);

    /// Whether this injector has been used at least once (for one-shot injectors).
    fn has_injected(&self) -> bool;
}

/// Default position-tracking implementation shared by all DynamicInjectors.
#[derive(Debug, Clone)]
pub struct InjectionPosition {
    /// Index in `context.history` where this injector last inserted.
    pub injected_at: Option<usize>,
}

impl Default for InjectionPosition {
    fn default() -> Self {
        Self { injected_at: None }
    }
}

impl InjectionPosition {
    /// Mark that an injection happened at the current history length.
    pub fn mark_injected(&mut self, history_len: usize) {
        self.injected_at = Some(history_len);
    }

    /// Reset position (context was cleared).
    pub fn on_context_clear(&mut self) {
        self.injected_at = None;
    }

    /// Shift position after compaction removed `compacted_count` messages.
    pub fn on_context_compacted(&mut self, compacted_count: usize) {
        if let Some(ref mut pos) = self.injected_at {
            if *pos >= compacted_count {
                *pos -= compacted_count;
            } else {
                *pos = 0;
            }
        }
    }

    /// Adjust position after message at `index` was removed.
    pub fn on_context_message_removed(&mut self, index: usize) {
        if let Some(ref mut pos) = self.injected_at {
            if *pos >= index {
                if *pos > 0 {
                    *pos -= 1;
                } else {
                    self.injected_at = None;
                }
            }
        }
    }
}
