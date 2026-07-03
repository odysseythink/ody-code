use super::do_cancel;
use super::do_enter;
use super::do_exit;
use crate::records::nested::SessionModeKind;
use crate::session_mode::types::*;
use async_trait::async_trait;

pub struct DesignModeBehavior;

#[async_trait]
impl SessionModeKindBehavior for DesignModeBehavior {
    fn kind(&self) -> SessionModeKind {
        SessionModeKind::Design
    }
    fn output_subdirectory(&self) -> &str {
        "designs"
    }
    fn mode_model_key(&self) -> &str {
        "design"
    }
    fn handoff_target(&self) -> Option<&str> {
        Some("plan")
    }
    fn supports_design_sessions(&self) -> bool {
        true
    }

    async fn on_enter(
        &self,
        ctx: &ModeEnterContext,
        sm_ctx: &dyn SessionModeContext,
    ) -> anyhow::Result<()> {
        do_enter(SessionModeKind::Design, ctx, sm_ctx).await
    }

    async fn on_exit(
        &self,
        ctx: &ModeExitContext,
        sm_ctx: &dyn SessionModeContext,
    ) -> anyhow::Result<()> {
        do_exit(SessionModeKind::Design, ctx, sm_ctx, None).await
    }

    async fn on_cancel(
        &self,
        ctx: &ModeExitContext,
        sm_ctx: &dyn SessionModeContext,
    ) -> anyhow::Result<()> {
        do_cancel(SessionModeKind::Design, ctx, sm_ctx, None).await
    }
}
