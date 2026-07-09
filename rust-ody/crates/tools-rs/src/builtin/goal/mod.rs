/// Mirrors the TS `SessionGoalStore` API surface consumed by the 4 goal tools.
/// The real implementation lives in `agent-rs`; tools-rs only depends on this trait.
pub trait GoalStore: Send + Sync {
    fn create_goal(&self, input: CreateGoalInput) -> Result<GoalSnapshot, GoalStoreError>;
    fn get_goal(&self) -> GoalToolResult;
    fn set_budget_limits(
        &self,
        limits: GoalBudgetLimits,
        actor: GoalActor,
    ) -> Result<GoalSnapshot, GoalStoreError>;
    fn resume_goal(&self, actor: GoalActor) -> Result<GoalSnapshot, GoalStoreError>;
    fn mark_complete(&self, actor: GoalActor) -> Result<Option<GoalSnapshot>, GoalStoreError>;
    fn mark_blocked(&self, actor: GoalActor) -> Result<Option<GoalSnapshot>, GoalStoreError>;
    fn pause_goal(&self, actor: GoalActor) -> Result<GoalSnapshot, GoalStoreError>;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateGoalInput {
    pub objective: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_criterion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<GoalActor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GoalActor {
    User,
    Model,
    Runtime,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GoalStatus {
    Active,
    Paused,
    Blocked,
    Complete,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalBudgetLimits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_budget: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_clock_budget_ms: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalBudgetReport {
    pub token_budget: Option<u64>,
    pub turn_budget: Option<u64>,
    pub wall_clock_budget_ms: Option<u64>,
    pub remaining_tokens: Option<u64>,
    pub remaining_turns: Option<u64>,
    pub remaining_wall_clock_ms: Option<u64>,
    pub token_budget_reached: bool,
    pub turn_budget_reached: bool,
    pub wall_clock_budget_reached: bool,
    pub over_budget: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalSnapshot {
    pub goal_id: String,
    pub objective: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_criterion: Option<String>,
    pub status: GoalStatus,
    pub created_at: String,
    pub updated_at: String,
    pub started_by: GoalActor,
    pub updated_by: GoalActor,
    pub turns_used: u64,
    pub tokens_used: u64,
    pub wall_clock_ms: u64,
    pub budget: GoalBudgetReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalToolResult {
    pub goal: Option<GoalSnapshot>,
}

#[derive(Debug, thiserror::Error)]
pub enum GoalStoreError {
    #[error("no current goal")]
    NotFound,
    #[error("a goal already exists; use replace to start a new one")]
    AlreadyExists,
    #[error("goal objective cannot be empty")]
    ObjectiveEmpty,
    #[error("goal objective cannot exceed {0} characters")]
    ObjectiveTooLong(usize),
    #[error("cannot {action} a goal in status \"{status}\"")]
    InvalidStatus { action: String, status: String },
    #[error("{0}")]
    Other(String),
}

/// Deterministic completion message, mirroring TS `buildGoalCompletionMessage`.
pub fn build_goal_completion_message(goal: &GoalSnapshot) -> String {
    let head = match &goal.terminal_reason {
        Some(reason) if !reason.is_empty() => format!("✓ Goal complete — {}.", reason),
        _ => "✓ Goal complete.".to_string(),
    };
    let turns = if goal.turns_used == 1 {
        "1 turn".to_string()
    } else {
        format!("{} turns", goal.turns_used)
    };
    let stats = format!(
        "Worked {} over {}, using {} tokens.",
        turns,
        format_elapsed(goal.wall_clock_ms),
        format_tokens(goal.tokens_used)
    );
    format!("{}\n{}", head, stats)
}

fn format_elapsed(ms: u64) -> String {
    let total_seconds = (ms as f64 / 1000.0).round() as u64;
    if total_seconds < 60 {
        return format!("{}s", total_seconds);
    }
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    if minutes < 60 {
        return format!("{}m{:02}s", minutes, seconds);
    }
    let hours = minutes / 60;
    format!("{}h{:02}m", hours, minutes % 60)
}

fn format_tokens(tokens: u64) -> String {
    if tokens < 1000 {
        return tokens.to_string();
    }
    if tokens < 1_000_000 {
        return format!("{:.1}k", tokens as f64 / 1000.0);
    }
    format!("{:.1}M", tokens as f64 / 1_000_000.0)
}

/// Mock GoalStore for golden testing.
pub struct MockGoalStore {
    state: std::sync::Mutex<Option<GoalSnapshot>>,
}

impl MockGoalStore {
    pub fn new(goal: Option<GoalSnapshot>) -> Self {
        Self {
            state: std::sync::Mutex::new(goal),
        }
    }
    fn now_iso(&self) -> String {
        "2026-01-01T00:00:00.000Z".to_string()
    }
}

impl GoalStore for MockGoalStore {
    fn create_goal(&self, input: CreateGoalInput) -> Result<GoalSnapshot, GoalStoreError> {
        let mut state = self.state.lock().unwrap();
        let obj = input.objective.trim().to_string();
        if obj.is_empty() {
            return Err(GoalStoreError::ObjectiveEmpty);
        }
        if obj.len() > 4000 {
            return Err(GoalStoreError::ObjectiveTooLong(4000));
        }
        if state.is_some() && input.replace != Some(true) {
            return Err(GoalStoreError::AlreadyExists);
        }
        let now = self.now_iso();
        let snapshot = GoalSnapshot {
            goal_id: "mock-goal-1".to_string(),
            objective: obj,
            completion_criterion: input.completion_criterion.filter(|s| !s.trim().is_empty()),
            status: GoalStatus::Active,
            created_at: now.clone(),
            updated_at: now,
            started_by: input.actor.unwrap_or(GoalActor::User),
            updated_by: input.actor.unwrap_or(GoalActor::User),
            turns_used: 0,
            tokens_used: 0,
            wall_clock_ms: 0,
            budget: GoalBudgetReport {
                token_budget: None,
                turn_budget: None,
                wall_clock_budget_ms: None,
                remaining_tokens: None,
                remaining_turns: None,
                remaining_wall_clock_ms: None,
                token_budget_reached: false,
                turn_budget_reached: false,
                wall_clock_budget_reached: false,
                over_budget: false,
            },
            terminal_reason: None,
        };
        *state = Some(snapshot.clone());
        Ok(snapshot)
    }

    fn get_goal(&self) -> GoalToolResult {
        GoalToolResult {
            goal: self.state.lock().unwrap().clone(),
        }
    }

    fn set_budget_limits(
        &self,
        limits: GoalBudgetLimits,
        actor: GoalActor,
    ) -> Result<GoalSnapshot, GoalStoreError> {
        let mut state = self.state.lock().unwrap();
        let mut g = state.clone().ok_or(GoalStoreError::NotFound)?;
        g.budget.token_budget = limits.token_budget.or(g.budget.token_budget);
        g.budget.turn_budget = limits.turn_budget.or(g.budget.turn_budget);
        g.budget.wall_clock_budget_ms = limits
            .wall_clock_budget_ms
            .or(g.budget.wall_clock_budget_ms);
        g.updated_by = actor;
        g.updated_at = self.now_iso();
        *state = Some(g.clone());
        Ok(g)
    }

    fn resume_goal(&self, actor: GoalActor) -> Result<GoalSnapshot, GoalStoreError> {
        let mut state = self.state.lock().unwrap();
        let mut g = state.clone().ok_or(GoalStoreError::NotFound)?;
        if g.status == GoalStatus::Active {
            return Ok(g);
        }
        if !matches!(g.status, GoalStatus::Paused | GoalStatus::Blocked) {
            return Err(GoalStoreError::InvalidStatus {
                action: "resume".into(),
                status: format!("{:?}", g.status),
            });
        }
        g.status = GoalStatus::Active;
        g.updated_by = actor;
        g.updated_at = self.now_iso();
        g.terminal_reason = None;
        *state = Some(g.clone());
        Ok(g)
    }

    fn mark_complete(&self, _actor: GoalActor) -> Result<Option<GoalSnapshot>, GoalStoreError> {
        let mut state = self.state.lock().unwrap();
        let g = state.clone().ok_or(GoalStoreError::NotFound)?;
        if g.status != GoalStatus::Active {
            return Ok(None);
        }
        let snapshot = GoalSnapshot {
            status: GoalStatus::Complete,
            ..g
        };
        *state = None; // transient — cleared on completion
        Ok(Some(snapshot))
    }

    fn mark_blocked(&self, actor: GoalActor) -> Result<Option<GoalSnapshot>, GoalStoreError> {
        let mut state = self.state.lock().unwrap();
        let mut g = state.clone().ok_or(GoalStoreError::NotFound)?;
        if g.status != GoalStatus::Active {
            return Ok(None);
        }
        g.status = GoalStatus::Blocked;
        g.updated_by = actor;
        g.updated_at = self.now_iso();
        *state = Some(g.clone());
        Ok(Some(g))
    }

    fn pause_goal(&self, actor: GoalActor) -> Result<GoalSnapshot, GoalStoreError> {
        let mut state = self.state.lock().unwrap();
        let mut g = state.clone().ok_or(GoalStoreError::NotFound)?;
        if g.status == GoalStatus::Paused {
            return Ok(g);
        }
        if g.status != GoalStatus::Active {
            return Err(GoalStoreError::InvalidStatus {
                action: "pause".into(),
                status: format!("{:?}", g.status),
            });
        }
        g.status = GoalStatus::Paused;
        g.updated_by = actor;
        g.updated_at = self.now_iso();
        *state = Some(g.clone());
        Ok(g)
    }
}

pub mod create_goal;
pub mod get_goal;
pub mod set_goal_budget;
pub mod update_goal;
