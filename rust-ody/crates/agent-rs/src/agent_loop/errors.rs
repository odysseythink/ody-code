use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum LoopError {
    #[error("Turn exceeded maxSteps={max_steps}")]
    MaxStepsExceeded { max_steps: u32 },
    #[error("Aborted")]
    Aborted,
    #[error("{0}")]
    Other(String),
}

impl LoopError {
    pub fn is_abort(&self) -> bool {
        matches!(self, LoopError::Aborted)
    }

    pub fn is_max_steps(&self) -> bool {
        matches!(self, LoopError::MaxStepsExceeded { .. })
    }
}

pub fn create_max_steps_exceeded_error(max_steps: u32) -> LoopError {
    LoopError::MaxStepsExceeded { max_steps }
}

pub fn is_max_steps_exceeded_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<LoopError>()
        .map(|e| e.is_max_steps())
        .unwrap_or(false)
}

pub fn is_abort_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<LoopError>()
        .map(|e| e.is_abort())
        .unwrap_or(false)
}

pub fn error_message(err: &anyhow::Error) -> String {
    err.to_string()
}
