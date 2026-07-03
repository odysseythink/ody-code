use std::sync::Mutex;

use async_trait::async_trait;

use crate::injection::dynamic_injector::{DynamicInjector, InjectionPosition};
use crate::injection::types::*;

/// Injects permission-mode transition notices (auto-mode enter/exit).
/// Mirrors TS `PermissionModeInjector`.
pub struct PermissionModeInjector {
    pub pos: Mutex<InjectionPosition>,
    pub previous_mode: Mutex<Option<String>>,
}

impl PermissionModeInjector {
    pub fn new() -> Self {
        Self {
            pos: Mutex::new(InjectionPosition::default()),
            previous_mode: Mutex::new(None),
        }
    }
}

#[async_trait]
impl DynamicInjector for PermissionModeInjector {
    fn variant(&self) -> &str {
        VARIANT_PERMISSION_MODE
    }

    async fn get_injection(&self, ctx: &dyn InjectionManagerContext) -> Option<String> {
        let current = ctx.permission_mode();
        let prev = self.previous_mode.lock().unwrap().clone();
        if current == prev {
            return None;
        }
        *self.previous_mode.lock().unwrap() = current.clone();
        match current.as_deref() {
            Some("auto") => Some(
                "Permission mode is now Auto. Tools will be approved automatically when possible."
                    .to_string(),
            ),
            Some("yolo") => Some(
                "Permission mode is now YOLO. All tools are approved without asking.".to_string(),
            ),
            _ => None,
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
