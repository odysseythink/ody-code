use super::providers::{HttpFetchError, UrlFetchKind, UrlFetchResult, UrlFetcher};
use async_trait::async_trait;
use reqwest::Client;

const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36";

const DEFAULT_MAX_BYTES: usize = 10 * 1024 * 1024;

pub struct LocalFetchURLProvider {
    client: Client,
    user_agent: String,
    max_bytes: usize,
    allow_private_addresses: bool,
}

impl LocalFetchURLProvider {
    pub fn new(allow_private_addresses: bool) -> Self {
        Self {
            client: Client::new(),
            user_agent: DEFAULT_USER_AGENT.to_string(),
            max_bytes: DEFAULT_MAX_BYTES,
            allow_private_addresses,
        }
    }

    /// SSRF guard — reject non-http(s) schemes and private/loopback/link-local IPs.
    /// Mirrors TS `assertSafeFetchTarget()` in `local-fetch-url.ts:69-129`.
    fn assert_safe_target(&self, url_str: &str) -> Result<(), HttpFetchError> {
        let parsed = url::Url::parse(url_str)
            .map_err(|_| HttpFetchError::new(0, format!("Invalid URL: \"{url_str}\"")))?;

        let scheme = parsed.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(HttpFetchError::new(
                0,
                format!("Unsupported URL scheme \"{scheme}\" — only http(s) allowed."),
            ));
        }

        if self.allow_private_addresses {
            return Ok(());
        }

        let host = parsed.host_str().unwrap_or("").to_lowercase();
        let host = host
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .unwrap_or(&host);

        // Literal "localhost" / loopback aliases
        if host == "localhost" || host.ends_with(".localhost") {
            return Err(HttpFetchError::new(
                0,
                format!("Refusing to fetch private host: \"{host}\""),
            ));
        }

        // IPv6 loopback / ULA / link-local
        if host == "::1"
            || host == "::"
            || host.starts_with("fe80:")
            || host.starts_with("fc")
            || host.starts_with("fd")
        {
            return Err(HttpFetchError::new(
                0,
                format!("Refusing to fetch private host: \"{host}\""),
            ));
        }

        // IPv4 literal check
        if let Some(octets) = parse_ipv4(&host) {
            let [a, b, _c, _d] = octets;
            let is_loopback = a == 127;
            let is_private10 = a == 10;
            let is_private192 = a == 192 && b == 168;
            let is_private172 = a == 172 && (16..=31).contains(&b);
            let is_link_local = a == 169 && b == 254;
            let is_zero = a == 0;
            let is_cgnat = a == 100 && (64..=127).contains(&b);
            if is_loopback
                || is_private10
                || is_private192
                || is_private172
                || is_link_local
                || is_zero
                || is_cgnat
            {
                return Err(HttpFetchError::new(
                    0,
                    format!("Refusing to fetch private address: \"{host}\""),
                ));
            }
        }

        Ok(())
    }

    /// Extract main text content from HTML. Simplified Readability fallback:
    /// 1. Try `<article>` content
    /// 2. Try `<main>` content
    /// 3. Fall back to `<body>` content
    /// Mirrors TS `extractMainContent()` in `local-fetch-url.ts:193-228`.
    fn extract_main_content(&self, html: &str) -> String {
        let document = scraper::Html::parse_document(html);

        // Try to find title
        let title = document
            .select(&scraper::Selector::parse("title").unwrap())
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        // Try content containers in priority order
        for selector_str in &["article", "main", "body"] {
            let selector = scraper::Selector::parse(selector_str).unwrap();
            if let Some(el) = document.select(&selector).next() {
                let text: String = el.text().collect();
                let text = text.trim().to_string();
                // Collapse whitespace
                let text = text
                    .lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    if title.is_empty() {
                        return text;
                    }
                    return format!("# {title}\n\n{text}");
                }
            }
        }

        String::new()
    }
}

fn parse_ipv4(host: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut octets = [0u8; 4];
    for (i, part) in parts.iter().enumerate() {
        let n: u8 = part.parse().ok()?;
        octets[i] = n;
    }
    Some(octets)
}

#[async_trait]
impl UrlFetcher for LocalFetchURLProvider {
    async fn fetch(
        &self,
        url: &str,
        _tool_call_id: Option<&str>,
    ) -> Result<UrlFetchResult, HttpFetchError> {
        self.assert_safe_target(url)?;

        let response = self
            .client
            .get(url)
            .header("User-Agent", &self.user_agent)
            .send()
            .await
            .map_err(|e| HttpFetchError::new(0, format!("Request failed: {e}")))?;

        let status = response.status().as_u16();
        if status >= 400 {
            let msg = format!(
                "HTTP {status} {}",
                response.status().canonical_reason().unwrap_or("")
            );
            return Err(HttpFetchError::new(status, msg));
        }

        // Check Content-Length before buffering
        if let Some(cl) = response.content_length() {
            if cl as usize > self.max_bytes {
                return Err(HttpFetchError::new(
                    0,
                    format!(
                        "Response body too large: {cl} bytes exceeds maxBytes ({}).",
                        self.max_bytes
                    ),
                ));
            }
        }

        let body = response
            .text()
            .await
            .map_err(|e| HttpFetchError::new(0, format!("Failed to read response body: {e}")))?;

        // Defensive size check
        if body.len() > self.max_bytes {
            return Err(HttpFetchError::new(
                0,
                format!(
                    "Response body too large: {} bytes exceeds maxBytes ({}).",
                    body.len(),
                    self.max_bytes
                ),
            ));
        }

        // Check content type for passthrough
        // (We can't easily check content-type headers here since we already read the body;
        //  in a full implementation we'd check before reading. For now, try to detect HTML.)
        if body.trim_start().starts_with('<') || body.trim_start().starts_with("<!") {
            let extracted = self.extract_main_content(&body);
            if extracted.is_empty() {
                return Err(HttpFetchError::new(
                    0,
                    "Failed to extract meaningful content from the page.".to_string(),
                ));
            }
            return Ok(UrlFetchResult {
                content: extracted,
                kind: UrlFetchKind::Extracted,
            });
        }

        Ok(UrlFetchResult {
            content: body,
            kind: UrlFetchKind::Passthrough,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_safe_target_rejects_loopback() {
        let p = LocalFetchURLProvider::new(false);
        assert!(p.assert_safe_target("http://127.0.0.1/test").is_err());
        assert!(p.assert_safe_target("http://localhost/test").is_err());
    }

    #[test]
    fn assert_safe_target_allows_loopback_when_opted_in() {
        let p = LocalFetchURLProvider::new(true);
        assert!(p.assert_safe_target("http://127.0.0.1/test").is_ok());
    }

    #[test]
    fn assert_safe_target_rejects_private_10() {
        let p = LocalFetchURLProvider::new(false);
        assert!(p.assert_safe_target("http://10.0.0.1/test").is_err());
    }

    #[test]
    fn assert_safe_target_rejects_private_192_168() {
        let p = LocalFetchURLProvider::new(false);
        assert!(p.assert_safe_target("http://192.168.1.1/test").is_err());
    }

    #[test]
    fn assert_safe_target_rejects_cgnat() {
        let p = LocalFetchURLProvider::new(false);
        assert!(p.assert_safe_target("http://100.64.0.1/test").is_err());
    }

    #[test]
    fn assert_safe_target_allows_public_ip() {
        let p = LocalFetchURLProvider::new(false);
        assert!(p.assert_safe_target("https://93.184.216.34/test").is_ok()); // example.com IP
    }

    #[test]
    fn assert_safe_target_rejects_invalid_url() {
        let p = LocalFetchURLProvider::new(false);
        assert!(p.assert_safe_target("not a url").is_err());
    }

    #[test]
    fn assert_safe_target_rejects_non_http_scheme() {
        let p = LocalFetchURLProvider::new(false);
        assert!(p.assert_safe_target("ftp://example.com/file").is_err());
    }

    #[test]
    fn extract_main_content_finds_title() {
        let p = LocalFetchURLProvider::new(false);
        let html = "<html><head><title>My Page</title></head><body><article><p>Hello world</p></article></body></html>";
        let result = p.extract_main_content(html);
        assert!(result.starts_with("# My Page"));
        assert!(result.contains("Hello world"));
    }

    #[test]
    fn extract_main_content_falls_back_to_body() {
        let p = LocalFetchURLProvider::new(false);
        let html = "<html><body><p>Just text</p></body></html>";
        let result = p.extract_main_content(html);
        assert!(result.contains("Just text"));
    }

    #[test]
    fn extract_main_content_empty_html_returns_empty() {
        let p = LocalFetchURLProvider::new(false);
        let html = "<html><head></head><body></body></html>";
        let result = p.extract_main_content(html);
        assert_eq!(result, "");
    }
}
