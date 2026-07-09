use std::fmt;

/// Structured error type carried inside `anyhow::Error` so the turn layer can
/// classify provider and runtime failures without parsing strings.
#[derive(Debug, Clone)]
pub struct OdyError {
    pub code: String,
    pub name: String,
    pub message: String,
    pub retryable: bool,
    pub details: Option<serde_json::Value>,
}

impl OdyError {
    pub fn new(
        code: impl Into<String>,
        name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            name: name.into(),
            message: message.into(),
            retryable: false,
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

impl fmt::Display for OdyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.message)
    }
}

impl std::error::Error for OdyError {}

pub fn to_ody_error(error: &anyhow::Error) -> Option<OdyError> {
    error.downcast_ref::<OdyError>().cloned()
}
