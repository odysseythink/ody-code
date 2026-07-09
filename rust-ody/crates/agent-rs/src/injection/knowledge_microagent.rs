use std::sync::Mutex;

use async_trait::async_trait;

use crate::injection::dynamic_injector::{DynamicInjector, InjectionPosition};
use crate::injection::types::*;

/// Injects repo-knowledge microagent results. Only active in normal (non-session-mode) context
/// when the `repo-knowledge` experimental flag is enabled.
/// Mirrors TS `KnowledgeMicroagentInjector`.
pub struct KnowledgeMicroagentInjector {
    pub pos: Mutex<InjectionPosition>,
}

impl KnowledgeMicroagentInjector {
    pub fn new() -> Self {
        Self {
            pos: Mutex::new(InjectionPosition::default()),
        }
    }
}

#[async_trait]
impl DynamicInjector for KnowledgeMicroagentInjector {
    fn variant(&self) -> &str {
        VARIANT_KNOWLEDGE_MICROAGENT
    }

    async fn get_injection(&self, ctx: &dyn InjectionManagerContext) -> Option<String> {
        // Only in normal mode (no session mode active) and with flag enabled
        if ctx.is_session_mode_active() {
            return None;
        }
        if !ctx.is_flag_enabled("repo-knowledge") {
            return None;
        }
        if self.pos.lock().unwrap().injected_at.is_some() {
            return None;
        }
        // In real impl, queries the knowledge microagent. Stub for now.
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
