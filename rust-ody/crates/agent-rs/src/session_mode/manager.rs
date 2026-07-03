use crate::injection::types::{PendingDesignHandoff, PendingPlanHandoff};
use crate::records::nested::SessionModeKind;
use crate::records::AgentRecord;
use crate::session_mode::types::*;
use uuid::Uuid;

/// Design session checkpoint — mirrors TS `DesignSessionCheckpoint`.
#[derive(Debug, Clone)]
pub struct DesignSessionCheckpoint {
    pub id: String,
    pub started_at: i64,
    pub closed_at: Option<i64>,
    pub approved_path: Option<String>,
}

/// Main session-mode state machine.
/// Mirrors TS `SessionMode` class.
pub struct SessionModeManager<C: SessionModeContext> {
    pub context: C,
    registry: ModeBehaviorRegistry,

    // Active state
    is_active: bool,
    kind: Option<SessionModeKind>,
    session_mode_id: Option<String>,
    session_mode_file_path: Option<String>,
    pre_mode_model_alias: Option<String>,

    // Design sessions
    design_sessions: Vec<DesignSessionCheckpoint>,
    last_completed_design_file_path: Option<String>,

    // Handoff
    pending_handoff_for_plan: Option<PendingDesignHandoff>,
    pending_handoff_for_normal: Option<PendingPlanHandoff>,
}

impl<C: SessionModeContext> SessionModeManager<C> {
    pub fn new(context: C, registry: ModeBehaviorRegistry) -> Self {
        Self {
            context,
            registry,
            is_active: false,
            kind: None,
            session_mode_id: None,
            session_mode_file_path: None,
            pre_mode_model_alias: None,
            design_sessions: Vec::new(),
            last_completed_design_file_path: None,
            pending_handoff_for_plan: None,
            pending_handoff_for_normal: None,
        }
    }

    // ── Public getters ──

    pub fn is_active(&self) -> bool {
        self.is_active
    }
    pub fn kind(&self) -> Option<SessionModeKind> {
        self.kind
    }
    pub fn session_mode_file_path(&self) -> Option<String> {
        self.session_mode_file_path.clone()
    }
    pub fn design_sessions(&self) -> &[DesignSessionCheckpoint] {
        &self.design_sessions
    }
    /// Access to context for tests.
    pub fn context(&self) -> &C {
        &self.context
    }

    // ── Enter ──

    pub async fn enter(
        &mut self,
        kind: SessionModeKind,
        id: Option<String>,
        kind_override: Option<SessionModeKind>,
    ) -> anyhow::Result<()> {
        if self.is_active {
            anyhow::bail!("A session mode is already active: {:?}", self.kind);
        }

        let behavior = self
            .registry
            .get(&kind)
            .ok_or_else(|| anyhow::anyhow!("No behavior registered for mode: {:?}", kind))?;

        let id = id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let effective_kind = kind_override.unwrap_or(kind);

        // Save pre-mode model alias
        self.pre_mode_model_alias = self.context.default_model_alias();

        // Resolve file path
        let subdir = behavior.output_subdirectory();
        let project = self
            .context
            .project_root()
            .unwrap_or_else(|| self.context.cwd());
        let dir = format!("{}/.ody-code/{}", project, subdir);
        self.context.mkdir_p(&dir)?;
        let file_path = format!("{}/{}.md", dir, id);
        self.session_mode_file_path = Some(file_path);

        // Log WAL record BEFORE partition switch (TS ordering)
        self.context.log_record(AgentRecord::SessionModeEnter {
            time: None,
            id: id.clone(),
            kind: Some(effective_kind),
            path: self.session_mode_file_path.clone(),
        });

        // Switch context partition
        self.context.set_context_mode(Some(effective_kind));
        self.context.set_replay_mode(Some(effective_kind));
        self.context
            .push_replay_record(crate::replay::AgentReplayRecord::SessionModeUpdated {
                enabled: true,
                kind: Some(effective_kind),
            });

        // Run behavior on_enter
        let enter_ctx = ModeEnterContext {
            id: id.clone(),
            restore_target_alias: self.pre_mode_model_alias.clone(),
        };
        behavior.on_enter(&enter_ctx, &self.context).await?;

        // Design-specific: start design session
        if behavior.supports_design_sessions() {
            self.start_design_session(id.clone());
        }

        self.is_active = true;
        self.kind = Some(effective_kind);
        self.session_mode_id = Some(id);

        self.context.emit_status_updated();

        Ok(())
    }

    // ── Exit ──

    pub async fn exit(&mut self, id: Option<String>) -> anyhow::Result<()> {
        if !self.is_active {
            return Ok(()); // nothing to exit
        }

        let kind = self.kind.unwrap();
        let behavior = self
            .registry
            .get(&kind)
            .ok_or_else(|| anyhow::anyhow!("No behavior for mode: {:?}", kind))?;

        // Log WAL record BEFORE partition switch
        let exit_id = id.or_else(|| self.session_mode_id.clone());
        self.context.log_record(AgentRecord::SessionModeExit {
            time: None,
            id: exit_id.clone(),
        });

        // Run behavior on_exit
        let exit_ctx = ModeExitContext {
            id: exit_id,
            session_mode_file_path: self.session_mode_file_path.clone(),
        };
        behavior.on_exit(&exit_ctx, &self.context).await?;

        // Design-specific: close design session
        if behavior.supports_design_sessions() {
            self.close_current_design_session(self.session_mode_file_path.clone());
        }

        // Restore model
        let restore_alias = self.pre_mode_model_alias.clone();
        self.context.update_model_alias(restore_alias);
        self.context.refresh_llm();

        // Switch back to normal partition
        self.context.set_context_mode(None);
        self.context.set_replay_mode(None);

        // Push replay
        self.context
            .push_replay_record(crate::replay::AgentReplayRecord::SessionModeUpdated {
                enabled: false,
                kind: Some(kind),
            });

        self.reset_state();
        self.context.emit_status_updated();

        Ok(())
    }

    // ── Cancel ──

    pub async fn cancel(&mut self, id: Option<String>) -> anyhow::Result<()> {
        if !self.is_active {
            return Ok(());
        }

        let kind = self.kind.unwrap();
        let behavior = self
            .registry
            .get(&kind)
            .ok_or_else(|| anyhow::anyhow!("No behavior for mode: {:?}", kind))?;

        let cancel_id = id.or_else(|| self.session_mode_id.clone());
        self.context.log_record(AgentRecord::SessionModeCancel {
            time: None,
            id: cancel_id.clone(),
        });

        let exit_ctx = ModeExitContext {
            id: cancel_id,
            session_mode_file_path: self.session_mode_file_path.clone(),
        };
        behavior.on_cancel(&exit_ctx, &self.context).await?;

        if behavior.supports_design_sessions() {
            self.close_current_design_session(None);
        }

        let restore_alias = self.pre_mode_model_alias.clone();
        self.context.update_model_alias(restore_alias);
        self.context.refresh_llm();

        self.context.set_context_mode(None);
        self.context.set_replay_mode(None);

        self.context
            .push_replay_record(crate::replay::AgentReplayRecord::SessionModeUpdated {
                enabled: false,
                kind: Some(kind),
            });

        self.reset_state();
        self.context.emit_status_updated();

        Ok(())
    }

    // ── Clear ──

    pub async fn clear(&mut self) -> anyhow::Result<()> {
        if let Some(ref path) = self.session_mode_file_path {
            self.context.write_file(path, "")?;
        }
        Ok(())
    }

    // ── Restore Enter (used during resume/replay) ──

    pub fn restore_enter(
        &mut self,
        id: String,
        kind: Option<SessionModeKind>,
        path: Option<String>,
    ) {
        let effective_kind = kind.unwrap_or(SessionModeKind::Plan);
        self.pre_mode_model_alias = self.context.default_model_alias();
        self.is_active = true;
        self.kind = Some(effective_kind);
        self.session_mode_id = Some(id);
        self.session_mode_file_path = path;
        self.context.set_replay_mode(Some(effective_kind));
    }

    // ── Handoff ──

    pub async fn handoff_to(
        &mut self,
        target: &str,
        options: HandoffOptions,
    ) -> anyhow::Result<()> {
        match target {
            "plan" => {
                self.exit(None).await?;
                if let Some(path) = self.last_completed_design_file_path.clone() {
                    let filename = std::path::Path::new(&path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.pending_handoff_for_plan = Some(PendingDesignHandoff {
                        path,
                        filename,
                        selected_label: options.selected_label,
                    });
                }
            }
            "normal" => {
                // Read plan file content for handoff
                let content = self
                    .session_mode_file_path
                    .as_ref()
                    .and_then(|p| self.context.read_file(p).ok())
                    .unwrap_or_default();
                let path = self.session_mode_file_path.clone().unwrap_or_default();
                self.exit(None).await?;
                self.pending_handoff_for_normal = Some(PendingPlanHandoff {
                    content,
                    path,
                    selected_label: options.selected_label,
                });
            }
            _ => anyhow::bail!("Unknown handoff target: {}", target),
        }
        Ok(())
    }

    pub fn consume_pending_handoff_for_plan(&mut self) -> Option<PendingDesignHandoff> {
        self.pending_handoff_for_plan.take()
    }

    pub fn consume_pending_handoff_for_normal(&mut self) -> Option<PendingPlanHandoff> {
        self.pending_handoff_for_normal.take()
    }

    // ── Design Sessions ──

    fn start_design_session(&mut self, id: String) {
        self.design_sessions.push(DesignSessionCheckpoint {
            id,
            started_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
            closed_at: None,
            approved_path: None,
        });
    }

    fn close_current_design_session(&mut self, approved_path: Option<String>) {
        if let Some(session) = self.design_sessions.iter_mut().last() {
            session.closed_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64,
            );
            session.approved_path = approved_path.clone();
        }
        self.last_completed_design_file_path = approved_path;
    }

    // ── File Resolution ──

    /// Check if a path is writable in the current session mode context.
    pub fn is_writable_session_mode_path(&self, path: &str) -> bool {
        if !self.is_active {
            return false;
        }
        // Allow writes to the assigned file
        if let Some(ref file_path) = self.session_mode_file_path {
            if path == file_path {
                return true;
            }
        }
        // Allow writes to .md files inside the `<id>/` subdirectory (split parts)
        if let Some(ref file_path) = self.session_mode_file_path {
            if let Some(parent) = std::path::Path::new(file_path).parent() {
                let parent_dir = parent.to_string_lossy();
                if let Some(stem) = std::path::Path::new(file_path).file_stem() {
                    let parts_dir = format!("{}/{}/", parent_dir, stem.to_string_lossy());
                    if path.starts_with(&parts_dir) && path.ends_with(".md") {
                        return true;
                    }
                }
            }
        }
        false
    }

    // ── Helpers ──

    fn reset_state(&mut self) {
        self.is_active = false;
        self.kind = None;
        self.session_mode_id = None;
        self.session_mode_file_path = None;
        self.pre_mode_model_alias = None;
        // Note: handoff state intentionally preserved across exit for consume_* methods
    }
}
