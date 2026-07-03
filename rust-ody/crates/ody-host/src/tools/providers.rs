use async_trait::async_trait;

/// How the returned content relates to the original response body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UrlFetchKind {
    Passthrough,
    Extracted,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlFetchResult {
    pub content: String,
    pub kind: UrlFetchKind,
}

/// Host-injected URL fetcher. The host provides the real HTTP implementation.
#[async_trait]
pub trait UrlFetcher: Send + Sync {
    async fn fetch(
        &self,
        url: &str,
        tool_call_id: Option<&str>,
    ) -> Result<UrlFetchResult, HttpFetchError>;
}

/// Thrown when the upstream HTTP request completed but returned a non-success status.
#[derive(Debug)]
pub struct HttpFetchError {
    pub status: u16,
    pub message: String,
}

impl HttpFetchError {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for HttpFetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP {}: {}", self.status, self.message)
    }
}

impl std::error::Error for HttpFetchError {}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_content: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Host-injected web search provider.
#[async_trait]
pub trait WebSearchProvider: Send + Sync {
    fn name(&self) -> Option<&str> {
        None
    }
    async fn search(
        &self,
        query: &str,
        options: Option<WebSearchOptions>,
    ) -> Result<Vec<WebSearchResult>, Box<dyn std::error::Error + Send>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_fetch_kind_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&UrlFetchKind::Passthrough).unwrap(),
            "\"passthrough\""
        );
        assert_eq!(
            serde_json::to_string(&UrlFetchKind::Extracted).unwrap(),
            "\"extracted\""
        );
    }

    #[test]
    fn url_fetch_result_round_trips() {
        let r = UrlFetchResult {
            content: "hello".into(),
            kind: UrlFetchKind::Passthrough,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"content\""));
        assert!(json.contains("\"passthrough\""));
        let round: UrlFetchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(round.content, "hello");
        assert_eq!(round.kind, UrlFetchKind::Passthrough);
    }

    #[test]
    fn web_search_result_skips_optional_fields() {
        let r = WebSearchResult {
            title: "T".into(),
            url: "U".into(),
            snippet: "S".into(),
            date: None,
            content: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("date"));
        assert!(!json.contains("content"));
    }

    #[test]
    fn http_fetch_error_carries_status() {
        let e = HttpFetchError::new(404, "Not Found");
        assert_eq!(e.status, 404);
        assert!(e.to_string().contains("404"));
        assert!(e.to_string().contains("Not Found"));
    }
}
