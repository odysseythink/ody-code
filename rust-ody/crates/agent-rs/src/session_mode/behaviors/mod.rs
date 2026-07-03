use crate::records::nested::SessionModeKind;
use crate::session_mode::directory::get_mode_output_subdirectory;
use crate::session_mode::model_auth::mode_model_key_for_kind;
use crate::session_mode::types::*;

/// Shared enter logic. Called by all `SessionModeKindBehavior::on_enter` implementations.
/// Mirrors TS `BaseSessionModeBehavior.doEnter()`.
pub async fn do_enter(
    kind: SessionModeKind,
    _ctx: &ModeEnterContext,
    sm_ctx: &dyn SessionModeContext,
) -> anyhow::Result<()> {
    // 1. Resolve output directory
    let subdir = get_mode_output_subdirectory(kind);
    let project = sm_ctx.project_root().unwrap_or_else(|| sm_ctx.cwd());
    let dir = format!("{}/.ody-code/{}", project, subdir);
    sm_ctx.mkdir_p(&dir)?;

    // 2. Ensure .gitignore in .ody-code/
    let gitignore_path = format!("{}/.ody-code/.gitignore", project);
    if !sm_ctx.file_exists(&gitignore_path) {
        sm_ctx.write_file(&gitignore_path, "*\n")?;
    }

    // 3. Look up mode-specific model alias
    let model_key = mode_model_key_for_kind(kind);
    if let Some(alias) = sm_ctx.resolve_mode_model_alias(model_key) {
        sm_ctx.update_model_alias(Some(alias));
        sm_ctx.refresh_llm();
    }

    Ok(())
}

/// Shared exit logic. Mirrors TS `BaseSessionModeBehavior.doExit()`.
pub async fn do_exit(
    kind: SessionModeKind,
    _ctx: &ModeExitContext,
    sm_ctx: &dyn SessionModeContext,
    restore_target_alias: Option<String>,
) -> anyhow::Result<()> {
    // Restore pre-mode model alias
    let fallback = sm_ctx.default_model_alias();
    sm_ctx.update_model_alias(restore_target_alias.or(fallback));
    sm_ctx.refresh_llm();

    // Push replay record
    sm_ctx.push_replay_record(crate::replay::AgentReplayRecord::SessionModeUpdated {
        enabled: false,
        kind: Some(kind),
    });

    Ok(())
}

/// Shared cancel logic. Mirrors TS `BaseSessionModeBehavior.doCancel()`.
pub async fn do_cancel(
    kind: SessionModeKind,
    _ctx: &ModeExitContext,
    sm_ctx: &dyn SessionModeContext,
    restore_target_alias: Option<String>,
) -> anyhow::Result<()> {
    // Restore pre-mode model alias (WAL record is logged by SessionModeManager)
    let fallback = sm_ctx.default_model_alias();
    sm_ctx.update_model_alias(restore_target_alias.or(fallback));
    sm_ctx.refresh_llm();

    sm_ctx.push_replay_record(crate::replay::AgentReplayRecord::SessionModeUpdated {
        enabled: false,
        kind: Some(kind),
    });

    Ok(())
}

pub mod design;
pub mod plan;

pub use design::DesignModeBehavior;
pub use plan::PlanModeBehavior;

pub mod game_design;
pub mod office_hours;

pub use game_design::GameDesignModeBehavior;
pub use office_hours::OfficeHoursModeBehavior;

use std::collections::HashMap;

/// Create the default mode behavior registry — mirrors TS `createDefaultModeBehaviorRegistry()`.
pub fn create_default_mode_behavior_registry() -> ModeBehaviorRegistry {
    let mut registry: ModeBehaviorRegistry = HashMap::new();
    registry.insert(SessionModeKind::Plan, Box::new(PlanModeBehavior));
    registry.insert(SessionModeKind::Design, Box::new(DesignModeBehavior));
    registry.insert(
        SessionModeKind::OfficeHours,
        Box::new(OfficeHoursModeBehavior),
    );
    registry.insert(
        SessionModeKind::GameDesign,
        Box::new(GameDesignModeBehavior),
    );
    registry
}
