use std::fmt;

/// Equivalent to Python's `FileExistsError` and TS `KaosFileExistsError`.
#[derive(Debug)]
pub struct KaosFileExistsError {
    message: String,
}

impl KaosFileExistsError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for KaosFileExistsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for KaosFileExistsError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn file_exists_error_display_matches_message() {
        let e = KaosFileExistsError::new("/tmp/foo already exists");
        assert_eq!(format!("{}", e), "/tmp/foo already exists");
        assert!(e.source().is_none());
    }
}
