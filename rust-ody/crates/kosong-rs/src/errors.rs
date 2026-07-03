use thiserror::Error;

#[derive(Debug, Clone, Copy, Error)]
#[error("API connection error")]
pub struct APIConnectionError;

#[derive(Debug, Clone, Copy, Error)]
#[error("API timeout error")]
pub struct APITimeoutError;

#[derive(Debug, Clone, Error)]
#[error("API status error {status_code}: {message}")]
pub struct APIStatusError {
    pub status_code: u16,
    pub message: String,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Error)]
#[error("API context overflow error {status_code}: {message}")]
pub struct APIContextOverflowError {
    pub status_code: u16,
    pub message: String,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Error)]
#[error("The API returned an empty response (no content, no tool calls). Provider: {provider}, model: {model}")]
pub struct APIEmptyResponseError {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("The operation was aborted.")]
pub struct AbortError;

#[derive(Debug, Clone, thiserror::Error)]
#[error("{provider}: apiKey is required. Provide it via the constructor options, the provider's API-key environment variable, options.auth.apiKey on each request, or an OAuth login.")]
pub struct APIMissingApiKeyError {
    pub provider: String,
}

#[derive(Debug, Error)]
pub enum ChatProviderError {
    #[error("API connection error")]
    Connection(APIConnectionError),
    #[error("API timeout error")]
    Timeout(APITimeoutError),
    #[error(transparent)]
    Status(APIStatusError),
    #[error(transparent)]
    ContextOverflow(APIContextOverflowError),
    #[error(transparent)]
    Empty(APIEmptyResponseError),
    #[error("The operation was aborted.")]
    Aborted(AbortError),
    #[error(transparent)]
    MissingApiKey(APIMissingApiKeyError),
    #[error("{0}")]
    Other(String),
}

pub fn is_retryable_generate_error(error: &ChatProviderError) -> bool {
    match error {
        ChatProviderError::Connection(_)
        | ChatProviderError::Timeout(_)
        | ChatProviderError::Empty(_) => true,
        ChatProviderError::Status(APIStatusError { status_code, .. }) => {
            matches!(status_code, 429 | 500 | 502 | 503 | 504)
        }
        _ => false,
    }
}

const CONTEXT_OVERFLOW_PATTERNS: &[&str] = &[
    r"context[ _-]?length",
    r"context[ _-]?window.*exceed|exceed.*context[ _-]?window",
    r"maximum context",
    r"exceed(?:ed|s|ing)?\s+(?:the\s+)?max(?:imum)?\s+tokens?",
    r"too many tokens.*(?:prompt|input|context)|(?:prompt|input|context).*too many tokens",
    r"prompt is too long.*maximum",
    r"input token count.*exceeds?.*maximum number of tokens",
    r"request.*exceed(?:ed|s|ing)?.*model token limit",
];

pub fn is_context_overflow_error_code(code: Option<&str>) -> bool {
    code == Some("context_length_exceeded")
}

pub fn is_context_overflow_status_error(status_code: u16, message: &str) -> bool {
    if !matches!(status_code, 400 | 413 | 422) {
        return false;
    }
    let lower = message.to_lowercase();
    CONTEXT_OVERFLOW_PATTERNS.iter().any(|pat| {
        regex::Regex::new(pat)
            .ok()
            .map(|re| re.is_match(&lower))
            .unwrap_or(false)
    })
}

pub fn normalize_api_status_error(
    status_code: u16,
    message: impl Into<String>,
    request_id: Option<String>,
) -> ChatProviderError {
    let msg = message.into();
    if is_context_overflow_status_error(status_code, &msg) {
        ChatProviderError::ContextOverflow(APIContextOverflowError {
            status_code,
            message: msg,
            request_id,
        })
    } else {
        ChatProviderError::Status(APIStatusError {
            status_code,
            message: msg,
            request_id,
        })
    }
}
