use crate::injection::dynamic_injector::DynamicInjector;
use crate::injection::goal_injector::GoalInjector;
use crate::injection::knowledge_microagent::KnowledgeMicroagentInjector;
use crate::injection::permission_mode_injector::PermissionModeInjector;
use crate::injection::plugin_session_start::PluginSessionStartInjector;
use crate::injection::session_mode_injectors::*;
use crate::injection::todo_list_injector::TodoListReminderInjector;
use crate::injection::types::*;

/// Mirrors TS `InjectionManager`.
/// Owns all `DynamicInjector` instances and runs them in order before each step.
pub struct InjectionManager {
    injectors: Vec<Box<dyn DynamicInjector>>,
}

impl InjectionManager {
    /// Create a new `InjectionManager` with the standard set of injectors.
    /// Injector order follows TS `InjectionManager` constructor:
    /// PluginSessionStart → TodoList → Plan → Design → OfficeHours → GameDesign →
    /// PermissionMode → [KnowledgeMicroagent] → [Goal]
    pub fn new(_ctx: &dyn InjectionManagerContext) -> Self {
        Self {
            injectors: vec![
                Box::new(PluginSessionStartInjector::new()),
                Box::new(TodoListReminderInjector::new()),
                Box::new(PlanModeInjector::new()),
                Box::new(DesignModeInjector::new()),
                Box::new(OfficeHoursInjector::new()),
                Box::new(GameDesignInjector::new()),
                Box::new(PermissionModeInjector::new()),
                Box::new(KnowledgeMicroagentInjector::new()),
                Box::new(GoalInjector::new()),
            ],
        }
    }

    /// Run all injectors. Called before each step.
    /// Each injector's `get_injection` result is appended as a system reminder
    /// via `ctx.append_system_reminder`.
    pub async fn inject(&self, ctx: &dyn InjectionManagerContext) {
        for inj in &self.injectors {
            if let Some(text) = inj.get_injection(ctx).await {
                ctx.append_system_reminder(&text, "system-reminder", inj.variant());
            }
        }
    }

    /// Run goal injection at continuation boundaries.
    /// Only runs the GoalInjector.
    pub async fn inject_goal(&self, ctx: &dyn InjectionManagerContext) {
        // The GoalInjector is the last injector (index 8).
        // We could find it by variant, but for simplicity just run the last one.
        if let Some(goal_inj) = self.injectors.last() {
            if goal_inj.variant() == VARIANT_GOAL {
                if let Some(text) = goal_inj.get_injection(ctx).await {
                    ctx.append_system_reminder(&text, "system-reminder", VARIANT_GOAL);
                }
            }
        }
    }

    /// Notify all injectors that context was cleared.
    pub fn on_context_clear(&mut self) {
        for inj in &mut self.injectors {
            inj.on_context_clear();
        }
    }

    /// Notify all injectors that compaction removed messages.
    pub fn on_context_compacted(&mut self, compacted_count: usize) {
        for inj in &mut self.injectors {
            inj.on_context_compacted(compacted_count);
        }
    }

    /// Notify all injectors that a message was removed at `index`.
    pub fn on_context_message_removed(&mut self, index: usize) {
        for inj in &mut self.injectors {
            inj.on_context_message_removed(index);
        }
    }
}
