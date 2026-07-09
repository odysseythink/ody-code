use std::sync::Mutex;

use async_trait::async_trait;

use crate::injection::dynamic_injector::{DynamicInjector, InjectionPosition};
use crate::injection::types::*;

const TODO_REMINDER_INTERVAL: usize = 10;

/// Reminds the model to update the TODO list every N turns.
/// Mirrors TS `TodoListReminderInjector`.
pub struct TodoListReminderInjector {
    pub pos: Mutex<InjectionPosition>,
}

impl TodoListReminderInjector {
    pub fn new() -> Self {
        Self {
            pos: Mutex::new(InjectionPosition::default()),
        }
    }
}

#[async_trait]
impl DynamicInjector for TodoListReminderInjector {
    fn variant(&self) -> &str {
        VARIANT_TODO_LIST_REMINDER
    }

    async fn get_injection(&self, ctx: &dyn InjectionManagerContext) -> Option<String> {
        if !ctx.is_tool_active("TodoList") {
            return None;
        }
        let turns = ctx.assistant_turn_count();
        if turns > 0 && turns % TODO_REMINDER_INTERVAL == 0 {
            Some("Reminder: Update your TODO list to reflect the current progress.".to_string())
        } else {
            None
        }
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
