use std::sync::Mutex;

use async_trait::async_trait;

use crate::injection::dynamic_injector::{DynamicInjector, InjectionPosition};
use crate::injection::types::*;

/// One-shot injector for plugin session-start messages.
/// Mirrors TS `PluginSessionStartInjector`.
pub struct PluginSessionStartInjector {
    pub pos: Mutex<InjectionPosition>,
}

impl PluginSessionStartInjector {
    pub fn new() -> Self {
        Self {
            pos: Mutex::new(InjectionPosition::default()),
        }
    }
}

#[async_trait]
impl DynamicInjector for PluginSessionStartInjector {
    fn variant(&self) -> &str {
        VARIANT_PLUGIN_SESSION_START
    }

    async fn get_injection(&self, _ctx: &dyn InjectionManagerContext) -> Option<String> {
        if self.pos.lock().unwrap().injected_at.is_some() {
            return None;
        }
        // In the real impl, queries plugin registry for session-start messages.
        // For now, returns None (no plugins configured).
        None
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
