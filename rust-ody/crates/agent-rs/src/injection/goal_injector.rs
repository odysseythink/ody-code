use std::sync::Mutex;

use async_trait::async_trait;

use crate::injection::dynamic_injector::{DynamicInjector, InjectionPosition};
use crate::injection::types::*;

/// Injects the current goal text at continuation boundaries.
/// Mirrors TS `GoalInjector`.
pub struct GoalInjector {
    pub pos: Mutex<InjectionPosition>,
}

impl GoalInjector {
    pub fn new() -> Self {
        Self {
            pos: Mutex::new(InjectionPosition::default()),
        }
    }
}

#[async_trait]
impl DynamicInjector for GoalInjector {
    fn variant(&self) -> &str {
        VARIANT_GOAL
    }

    async fn get_injection(&self, ctx: &dyn InjectionManagerContext) -> Option<String> {
        if self.pos.lock().unwrap().injected_at.is_some() {
            return None;
        }
        ctx.get_active_goal_text()
    }

    fn on_context_clear(&mut self) {
        self.pos.lock().unwrap().on_context_clear();
    }

    fn on_context_compacted(&mut self, count: usize) {
        self.pos.lock().unwrap().on_context_compacted(count);
    }

    fn on_context_message_removed(&mut self, index: usize) {
        self.pos.lock().unwrap().on_context_message_removed(index);
    }

    fn has_injected(&self) -> bool {
        self.pos.lock().unwrap().injected_at.is_some()
    }
}
