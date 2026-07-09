use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub output: String,
    pub is_error: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub struct ToolResultBuilder {
    max_line_length: Option<usize>,
    chunks: Vec<String>,
    n_chars: usize,
}

impl ToolResultBuilder {
    pub fn new(max_line_length: Option<usize>) -> Self {
        Self {
            max_line_length: max_line_length.or(Some(500)),
            chunks: Vec::new(),
            n_chars: 0,
        }
    }

    pub fn write(&mut self, text: &str) {
        if let Some(limit) = self.max_line_length {
            self.chunks.push(
                text.lines()
                    .map(|line| {
                        if line.len() > limit {
                            format!("{}…", &line[..limit])
                        } else {
                            line.to_owned()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        } else {
            self.chunks.push(text.to_owned());
        }
        self.n_chars += text.len();
    }

    pub fn n_chars(&self) -> usize {
        self.n_chars
    }

    pub fn ok(self, message: Option<String>) -> ToolResult {
        ToolResult {
            output: self.build_output(),
            is_error: false,
            message,
        }
    }

    pub fn error(self, message: String) -> ToolResult {
        ToolResult {
            output: self.build_output(),
            is_error: true,
            message: Some(message),
        }
    }

    fn build_output(&self) -> String {
        self.chunks.join("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_long_lines_at_default_500() {
        let mut b = ToolResultBuilder::new(None);
        let long = "a".repeat(510);
        b.write(&long);
        let r = b.ok(None);
        assert_eq!(r.output.len(), 503);
        assert!(r.output.ends_with('…'));
    }

    #[test]
    fn tracks_character_count_before_truncation() {
        let mut b = ToolResultBuilder::new(None);
        b.write("hello");
        b.write("world");
        assert_eq!(b.n_chars(), 10);
        let r = b.ok(Some("done".into()));
        assert_eq!(r.message, Some("done".into()));
        assert!(!r.is_error);
    }

    #[test]
    fn error_marks_is_error() {
        let b = ToolResultBuilder::new(None);
        let r = b.error("it broke".into());
        assert!(r.is_error);
        assert_eq!(r.message, Some("it broke".into()));
    }
}
