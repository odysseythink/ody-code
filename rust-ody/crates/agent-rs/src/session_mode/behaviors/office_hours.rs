use super::do_cancel;
use super::do_enter;
use super::do_exit;
use crate::records::nested::SessionModeKind;
use crate::session_mode::types::*;
use async_trait::async_trait;

pub struct OfficeHoursModeBehavior;

#[async_trait]
impl SessionModeKindBehavior for OfficeHoursModeBehavior {
    fn kind(&self) -> SessionModeKind {
        SessionModeKind::OfficeHours
    }
    fn output_subdirectory(&self) -> &str {
        "products"
    }
    fn mode_model_key(&self) -> &str {
        "officeHours"
    }
    fn handoff_target(&self) -> Option<&str> {
        None
    }
    fn supports_design_sessions(&self) -> bool {
        false
    }

    async fn on_enter(
        &self,
        ctx: &ModeEnterContext,
        sm_ctx: &dyn SessionModeContext,
    ) -> anyhow::Result<()> {
        do_enter(SessionModeKind::OfficeHours, ctx, sm_ctx).await
    }

    async fn on_exit(
        &self,
        ctx: &ModeExitContext,
        sm_ctx: &dyn SessionModeContext,
    ) -> anyhow::Result<()> {
        do_exit(SessionModeKind::OfficeHours, ctx, sm_ctx, None).await
    }

    async fn on_cancel(
        &self,
        ctx: &ModeExitContext,
        sm_ctx: &dyn SessionModeContext,
    ) -> anyhow::Result<()> {
        do_cancel(SessionModeKind::OfficeHours, ctx, sm_ctx, None).await
    }
}
