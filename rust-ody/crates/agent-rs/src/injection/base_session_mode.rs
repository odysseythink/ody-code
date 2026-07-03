use super::dynamic_injector::{DynamicInjector, InjectionPosition};
use super::types::InjectionManagerContext;
use crate::records::nested::SessionModeKind;
use async_trait::async_trait;

/// How often to emit a "full" reminder (every N assistant turns).
pub const FULL_REFRESH_TURNS: usize = 5;
/// Minimum turns between sparse reminders.
pub const DEDUP_MIN_TURNS: usize = 2;

/// Abstract base for session-mode injectors.
/// Mirrors TS `BaseSessionModeInjector`.
///
/// Concrete implementations must provide:
/// - `get_entry_reminder()` — shown on first enter
/// - `get_reentry_reminder()` — shown on re-enter (mode already active when step starts)
/// - `get_full_reminder()` — shown every `FULL_REFRESH_TURNS` turns
/// - `get_sparse_reminder()` — shown on turns that are not full refresh
/// - `get_exit_reminder()` — shown when mode just became inactive
/// - `mode_kind()` — which `SessionModeKind` this injector serves
/// - inline `injection_variant() -> &'static str`
#[async_trait]
pub trait BaseSessionModeInjector: DynamicInjector {
    /// Which `SessionModeKind` this injector watches.
    fn mode_kind(&self) -> SessionModeKind;

    /// Whether the mode is currently active (stateful — tracks `was_active` across calls).
    fn is_mode_active(&self, ctx: &dyn InjectionManagerContext) -> bool;

    fn get_entry_reminder(&self) -> String;
    fn get_reentry_reminder(&self) -> String;
    fn get_full_reminder(&self) -> String;
    fn get_sparse_reminder(&self) -> String;
    fn get_exit_reminder(&self) -> String;

    /// Optional decorator: append skills-unavailable reminder.
    fn decorate_reminder(&self, ctx: &dyn InjectionManagerContext, base: String) -> String {
        if let Some(skills_reminder) = ctx.get_unavailable_skills_reminder(self.mode_kind()) {
            format!("{}\n\n{}", base, skills_reminder)
        } else {
            base
        }
    }

    /// Position tracker.
    fn pos(&self) -> &InjectionPosition;
    fn pos_mut(&mut self) -> &mut InjectionPosition;

    /// Stateful flag: was this mode active on the previous injection call?
    fn was_active(&self) -> bool;
    fn set_was_active(&mut self, val: bool);
}

/// Default implementation of `get_injection` for session-mode injectors.
pub async fn session_mode_get_injection(
    injector: &mut (dyn BaseSessionModeInjector + Send),
    ctx: &dyn InjectionManagerContext,
) -> Option<String> {
    let is_active = injector.is_mode_active(ctx);
    let was_active = injector.was_active();

    let injection = if !was_active && is_active {
        // Mode just became active
        let path = ctx.session_mode_file_path();
        let content = path
            .and_then(|p| std::fs::read_to_string(&p).ok())
            .unwrap_or_default();
        if content.trim().is_empty() {
            Some(injector.get_entry_reminder())
        } else {
            Some(injector.get_reentry_reminder())
        }
    } else if was_active && !is_active {
        // Mode just became inactive
        Some(injector.get_exit_reminder())
    } else if is_active {
        // Staying active: compute full/sparse variant
        let turns = ctx.assistant_turn_count();
        let reminder = if turns % FULL_REFRESH_TURNS == 0 {
            injector.get_full_reminder()
        } else if turns % DEDUP_MIN_TURNS == 0 {
            injector.get_sparse_reminder()
        } else {
            return None; // skip this step
        };
        Some(reminder)
    } else {
        None
    };

    injector.set_was_active(is_active);

    injection.map(|base| injector.decorate_reminder(ctx, base))
}
