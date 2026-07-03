use std::sync::Mutex;

use async_trait::async_trait;

use crate::injection::base_session_mode::{
    BaseSessionModeInjector, DEDUP_MIN_TURNS, FULL_REFRESH_TURNS,
};
use crate::injection::contracts::*;
use crate::injection::dynamic_injector::{DynamicInjector, InjectionPosition};
use crate::injection::types::*;
use crate::records::nested::SessionModeKind;

/// Default implementation of `get_injection` for session-mode injectors that use `Mutex<bool>` for
/// `was_active` (interior mutability). Called from within `DynamicInjector::get_injection(&self, ctx)`.
fn session_mode_get_injection_mutex(
    _pos: &InjectionPosition,
    was_active: &Mutex<bool>,
    ctx: &dyn InjectionManagerContext,
    injector: &dyn BaseSessionModeInjector,
) -> Option<String> {
    let is_active = injector.is_mode_active(ctx);
    let prev_was_active = *was_active.lock().unwrap();

    let injection = if !prev_was_active && is_active {
        // Mode just became active
        let path = ctx.session_mode_file_path();
        let content = path
            .and_then(|p| std::fs::read_to_string(&p).ok())
            .unwrap_or_default();
        if content.trim().is_empty() {
            // Check for normal→plan handoff (only for plan mode)
            if injector.mode_kind() == SessionModeKind::Plan {
                if let Some(handoff) = ctx.consume_pending_handoff_for_normal() {
                    let handoff_text =
                        plan_to_normal_handoff_reminder(&handoff.content, &handoff.path);
                    Some(format!(
                        "{}\n\n{}",
                        handoff_text,
                        injector.get_entry_reminder()
                    ))
                } else {
                    Some(injector.get_entry_reminder())
                }
            } else {
                Some(injector.get_entry_reminder())
            }
        } else {
            Some(injector.get_reentry_reminder())
        }
    } else if prev_was_active && !is_active {
        // Mode just became inactive
        Some(injector.get_exit_reminder())
    } else if is_active {
        // Staying active: compute full/sparse variant
        let turns = ctx.assistant_turn_count();
        if turns % FULL_REFRESH_TURNS == 0 {
            Some(injector.get_full_reminder())
        } else if turns % DEDUP_MIN_TURNS == 0 {
            Some(injector.get_sparse_reminder())
        } else {
            return None; // skip this step
        }
    } else {
        None
    };

    *was_active.lock().unwrap() = is_active;

    injection.map(|base| injector.decorate_reminder(ctx, base))
}

/// Plan mode injector — mirrors TS `PlanModeInjector`.
pub struct PlanModeInjector {
    pub pos: InjectionPosition,
    was_active: Mutex<bool>,
}

impl PlanModeInjector {
    pub fn new() -> Self {
        Self {
            pos: InjectionPosition::default(),
            was_active: Mutex::new(false),
        }
    }

    pub fn set_was_active(&self, val: bool) {
        *self.was_active.lock().unwrap() = val;
    }
}

#[async_trait]
impl DynamicInjector for PlanModeInjector {
    fn variant(&self) -> &str {
        VARIANT_PLAN_MODE
    }

    async fn get_injection(&self, ctx: &dyn InjectionManagerContext) -> Option<String> {
        session_mode_get_injection_mutex(&self.pos, &self.was_active, ctx, self)
    }

    fn on_context_clear(&mut self) {
        self.pos.on_context_clear();
    }

    fn on_context_compacted(&mut self, count: usize) {
        self.pos.on_context_compacted(count);
    }

    fn on_context_message_removed(&mut self, index: usize) {
        self.pos.on_context_message_removed(index);
    }

    fn has_injected(&self) -> bool {
        self.pos.injected_at.is_some()
    }
}

impl BaseSessionModeInjector for PlanModeInjector {
    fn mode_kind(&self) -> SessionModeKind {
        SessionModeKind::Plan
    }

    fn is_mode_active(&self, ctx: &dyn InjectionManagerContext) -> bool {
        ctx.is_session_mode_active() && ctx.session_mode_kind() == Some(SessionModeKind::Plan)
    }

    fn get_entry_reminder(&self) -> String {
        PLAN_ENTRY_REMINDER.to_string()
    }

    fn get_reentry_reminder(&self) -> String {
        PLAN_REENTRY_REMINDER.to_string()
    }

    fn get_full_reminder(&self) -> String {
        PLAN_FULL_REMINDER.to_string()
    }

    fn get_sparse_reminder(&self) -> String {
        PLAN_SPARSE_REMINDER.to_string()
    }

    fn get_exit_reminder(&self) -> String {
        PLAN_EXIT_REMINDER.to_string()
    }

    fn decorate_reminder(&self, ctx: &dyn InjectionManagerContext, base: String) -> String {
        if let Some(skills) = ctx.get_unavailable_skills_reminder(SessionModeKind::Plan) {
            format!("{}\n\n{}", base, skills)
        } else {
            base
        }
    }

    fn pos(&self) -> &InjectionPosition {
        &self.pos
    }

    fn pos_mut(&mut self) -> &mut InjectionPosition {
        &mut self.pos
    }

    fn was_active(&self) -> bool {
        *self.was_active.lock().unwrap()
    }

    fn set_was_active(&mut self, val: bool) {
        *self.was_active.lock().unwrap() = val;
    }
}

/// Design mode injector — mirrors TS `DesignModeInjector`.
pub struct DesignModeInjector {
    pub pos: InjectionPosition,
    was_active: Mutex<bool>,
}

impl DesignModeInjector {
    pub fn new() -> Self {
        Self {
            pos: InjectionPosition::default(),
            was_active: Mutex::new(false),
        }
    }

    pub fn set_was_active(&self, val: bool) {
        *self.was_active.lock().unwrap() = val;
    }
}

#[async_trait]
impl DynamicInjector for DesignModeInjector {
    fn variant(&self) -> &str {
        VARIANT_DESIGN_MODE
    }

    async fn get_injection(&self, ctx: &dyn InjectionManagerContext) -> Option<String> {
        session_mode_get_injection_mutex(&self.pos, &self.was_active, ctx, self)
    }

    fn on_context_clear(&mut self) {
        self.pos.on_context_clear();
    }

    fn on_context_compacted(&mut self, count: usize) {
        self.pos.on_context_compacted(count);
    }

    fn on_context_message_removed(&mut self, index: usize) {
        self.pos.on_context_message_removed(index);
    }

    fn has_injected(&self) -> bool {
        self.pos.injected_at.is_some()
    }
}

impl BaseSessionModeInjector for DesignModeInjector {
    fn mode_kind(&self) -> SessionModeKind {
        SessionModeKind::Design
    }

    fn is_mode_active(&self, ctx: &dyn InjectionManagerContext) -> bool {
        ctx.is_session_mode_active() && ctx.session_mode_kind() == Some(SessionModeKind::Design)
    }

    fn get_entry_reminder(&self) -> String {
        DESIGN_ENTRY_REMINDER.to_string()
    }

    fn get_reentry_reminder(&self) -> String {
        DESIGN_REENTRY_REMINDER.to_string()
    }

    fn get_full_reminder(&self) -> String {
        DESIGN_FULL_REMINDER.to_string()
    }

    fn get_sparse_reminder(&self) -> String {
        DESIGN_SPARSE_REMINDER.to_string()
    }

    fn get_exit_reminder(&self) -> String {
        DESIGN_EXIT_REMINDER.to_string()
    }

    fn pos(&self) -> &InjectionPosition {
        &self.pos
    }

    fn pos_mut(&mut self) -> &mut InjectionPosition {
        &mut self.pos
    }

    fn was_active(&self) -> bool {
        *self.was_active.lock().unwrap()
    }

    fn set_was_active(&mut self, val: bool) {
        *self.was_active.lock().unwrap() = val;
    }
}

/// Office hours mode injector.
pub struct OfficeHoursInjector {
    pub pos: InjectionPosition,
    was_active: Mutex<bool>,
}

impl OfficeHoursInjector {
    pub fn new() -> Self {
        Self {
            pos: InjectionPosition::default(),
            was_active: Mutex::new(false),
        }
    }

    pub fn set_was_active(&self, val: bool) {
        *self.was_active.lock().unwrap() = val;
    }
}

#[async_trait]
impl DynamicInjector for OfficeHoursInjector {
    fn variant(&self) -> &str {
        VARIANT_OFFICE_HOURS
    }

    async fn get_injection(&self, ctx: &dyn InjectionManagerContext) -> Option<String> {
        session_mode_get_injection_mutex(&self.pos, &self.was_active, ctx, self)
    }

    fn on_context_clear(&mut self) {
        self.pos.on_context_clear();
    }

    fn on_context_compacted(&mut self, count: usize) {
        self.pos.on_context_compacted(count);
    }

    fn on_context_message_removed(&mut self, index: usize) {
        self.pos.on_context_message_removed(index);
    }

    fn has_injected(&self) -> bool {
        self.pos.injected_at.is_some()
    }
}

impl BaseSessionModeInjector for OfficeHoursInjector {
    fn mode_kind(&self) -> SessionModeKind {
        SessionModeKind::OfficeHours
    }

    fn is_mode_active(&self, ctx: &dyn InjectionManagerContext) -> bool {
        ctx.is_session_mode_active()
            && ctx.session_mode_kind() == Some(SessionModeKind::OfficeHours)
    }

    fn get_entry_reminder(&self) -> String {
        OFFICE_HOURS_ENTRY_REMINDER.to_string()
    }

    fn get_reentry_reminder(&self) -> String {
        OFFICE_HOURS_REENTRY_REMINDER.to_string()
    }

    fn get_full_reminder(&self) -> String {
        OFFICE_HOURS_FULL_REMINDER.to_string()
    }

    fn get_sparse_reminder(&self) -> String {
        OFFICE_HOURS_SPARSE_REMINDER.to_string()
    }

    fn get_exit_reminder(&self) -> String {
        OFFICE_HOURS_EXIT_REMINDER.to_string()
    }

    fn pos(&self) -> &InjectionPosition {
        &self.pos
    }

    fn pos_mut(&mut self) -> &mut InjectionPosition {
        &mut self.pos
    }

    fn was_active(&self) -> bool {
        *self.was_active.lock().unwrap()
    }

    fn set_was_active(&mut self, val: bool) {
        *self.was_active.lock().unwrap() = val;
    }
}

/// Game design mode injector.
pub struct GameDesignInjector {
    pub pos: InjectionPosition,
    was_active: Mutex<bool>,
}

impl GameDesignInjector {
    pub fn new() -> Self {
        Self {
            pos: InjectionPosition::default(),
            was_active: Mutex::new(false),
        }
    }

    pub fn set_was_active(&self, val: bool) {
        *self.was_active.lock().unwrap() = val;
    }
}

#[async_trait]
impl DynamicInjector for GameDesignInjector {
    fn variant(&self) -> &str {
        VARIANT_GAME_DESIGN
    }

    async fn get_injection(&self, ctx: &dyn InjectionManagerContext) -> Option<String> {
        session_mode_get_injection_mutex(&self.pos, &self.was_active, ctx, self)
    }

    fn on_context_clear(&mut self) {
        self.pos.on_context_clear();
    }

    fn on_context_compacted(&mut self, count: usize) {
        self.pos.on_context_compacted(count);
    }

    fn on_context_message_removed(&mut self, index: usize) {
        self.pos.on_context_message_removed(index);
    }

    fn has_injected(&self) -> bool {
        self.pos.injected_at.is_some()
    }
}

impl BaseSessionModeInjector for GameDesignInjector {
    fn mode_kind(&self) -> SessionModeKind {
        SessionModeKind::GameDesign
    }

    fn is_mode_active(&self, ctx: &dyn InjectionManagerContext) -> bool {
        ctx.is_session_mode_active() && ctx.session_mode_kind() == Some(SessionModeKind::GameDesign)
    }

    fn get_entry_reminder(&self) -> String {
        GAME_DESIGN_ENTRY_REMINDER.to_string()
    }

    fn get_reentry_reminder(&self) -> String {
        GAME_DESIGN_REENTRY_REMINDER.to_string()
    }

    fn get_full_reminder(&self) -> String {
        GAME_DESIGN_FULL_REMINDER.to_string()
    }

    fn get_sparse_reminder(&self) -> String {
        GAME_DESIGN_SPARSE_REMINDER.to_string()
    }

    fn get_exit_reminder(&self) -> String {
        GAME_DESIGN_EXIT_REMINDER.to_string()
    }

    fn pos(&self) -> &InjectionPosition {
        &self.pos
    }

    fn pos_mut(&mut self) -> &mut InjectionPosition {
        &mut self.pos
    }

    fn was_active(&self) -> bool {
        *self.was_active.lock().unwrap()
    }

    fn set_was_active(&mut self, val: bool) {
        *self.was_active.lock().unwrap() = val;
    }
}
