use async_trait::async_trait;
use regex::Regex;

mod show_design_mockup;
pub use show_design_mockup::ShowDesignMockupTool;

/// Result returned by the host when asked to open a URL in the user's browser.
#[derive(Debug, Clone)]
pub struct OpenExternalResult {
    pub opened: bool,
    pub error: Option<String>,
}

/// Host-side capability consumed by `ShowDesignMockupTool`.
#[async_trait]
pub trait DesignMockupHost: Send + Sync {
    /// Whether the host can open external URLs at all.
    fn is_available(&self) -> bool;

    /// Absolute path to the current design file, if any. The mockup is written
    /// into a `.mockups/` sibling directory when this is present.
    fn design_file_path(&self) -> Option<String>;

    /// Ask the host to open `url` in the user's browser with the given title.
    async fn open_external(&self, url: &str, title: &str) -> Result<OpenExternalResult, String>;
}

pub fn slugify_design_title(title: &str) -> String {
    let re = Regex::new(r"[^a-z0-9]+").unwrap();
    let mut slug = re
        .replace_all(&title.to_lowercase(), "-")
        .trim_matches('-')
        .to_string();
    slug = Regex::new(r"-+")
        .unwrap()
        .replace_all(&slug, "-")
        .to_string();
    if slug.len() > 40 {
        slug = slug[..40].trim_end_matches('-').to_string();
    }
    if slug.is_empty() {
        "mockup".to_string()
    } else {
        slug
    }
}

/// Deterministic mock host for golden tests.
pub struct MockDesignMockupHost {
    available: bool,
    design_file_path: Option<String>,
    pub opened_url: std::sync::Mutex<Option<String>>,
    pub opened_title: std::sync::Mutex<Option<String>>,
    pub result: Result<OpenExternalResult, String>,
}

impl MockDesignMockupHost {
    pub fn new(
        available: bool,
        design_file_path: Option<String>,
        result: Result<OpenExternalResult, String>,
    ) -> Self {
        Self {
            available,
            design_file_path,
            opened_url: std::sync::Mutex::new(None),
            opened_title: std::sync::Mutex::new(None),
            result,
        }
    }
}

#[async_trait]
impl DesignMockupHost for MockDesignMockupHost {
    fn is_available(&self) -> bool {
        self.available
    }

    fn design_file_path(&self) -> Option<String> {
        self.design_file_path.clone()
    }

    async fn open_external(&self, url: &str, title: &str) -> Result<OpenExternalResult, String> {
        *self.opened_url.lock().unwrap() = Some(url.to_string());
        *self.opened_title.lock().unwrap() = Some(title.to_string());
        self.result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_lowercases_and_truncates() {
        assert_eq!(slugify_design_title("Hello World!"), "hello-world");
        assert_eq!(
            slugify_design_title("a very long title that exceeds forty characters"),
            "a-very-long-title-that-exceeds-forty-cha"
        );
    }

    #[test]
    fn slugify_falls_back_to_mockup() {
        assert_eq!(slugify_design_title("!!!"), "mockup");
    }
}
