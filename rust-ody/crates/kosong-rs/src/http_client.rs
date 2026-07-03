use crate::errors::{APIConnectionError, APITimeoutError, ChatProviderError};
use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use serde_json::Value;
use std::collections::HashMap;
use std::pin::Pin;
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<bytes::Bytes, ChatProviderError>> + Send>>;
pub struct HttpResponse {
    status: u16,
    body: ByteStream,
}
impl HttpResponse {
    pub fn new(status: u16, body: ByteStream) -> Self {
        Self { status, body }
    }
    pub fn status(&self) -> u16 {
        self.status
    }
    pub fn bytes_stream(self) -> ByteStream {
        self.body
    }
}

#[derive(Debug, Clone)]
pub struct MultipartPart {
    pub name: String,
    pub file_name: Option<String>,
    pub mime_type: Option<String>,
    pub data: Vec<u8>,
}

#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn post_json(
        &self,
        url: &str,
        headers: HashMap<String, String>,
        body: Value,
    ) -> Result<HttpResponse, ChatProviderError>;
    async fn post_multipart(
        &self,
        url: &str,
        headers: HashMap<String, String>,
        parts: Vec<MultipartPart>,
        fields: HashMap<String, String>,
    ) -> Result<HttpResponse, ChatProviderError>;
}
pub struct ReqwestClient {
    client: reqwest::Client,
}
impl ReqwestClient {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}
#[async_trait]
impl HttpClient for ReqwestClient {
    async fn post_json(
        &self,
        url: &str,
        headers: HashMap<String, String>,
        body: Value,
    ) -> Result<HttpResponse, ChatProviderError> {
        let mut req = self.client.post(url).json(&body);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let resp = req.send().await.map_err(classify_reqwest_error)?;
        let status = resp.status().as_u16();
        let body = resp
            .bytes_stream()
            .map(|r| r.map_err(classify_reqwest_error))
            .boxed();
        Ok(HttpResponse::new(status, body))
    }
    async fn post_multipart(
        &self,
        url: &str,
        headers: HashMap<String, String>,
        parts: Vec<MultipartPart>,
        fields: HashMap<String, String>,
    ) -> Result<HttpResponse, ChatProviderError> {
        let mut form = reqwest::multipart::Form::new();
        for (field_name, field_value) in &fields {
            form = form.text(field_name.clone(), field_value.clone());
        }
        for part in &parts {
            let mime = part
                .mime_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".into());
            let file_name = part.file_name.clone().unwrap_or_else(|| "file".into());
            let file_part = reqwest::multipart::Part::bytes(part.data.clone())
                .file_name(file_name)
                .mime_str(&mime)
                .map_err(|e| ChatProviderError::Other(format!("Invalid mime type: {e}")))?;
            form = form.part(part.name.clone(), file_part);
        }
        let mut req = self.client.post(url);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        req = req.multipart(form);
        let resp = req.send().await.map_err(classify_reqwest_error)?;
        let status = resp.status().as_u16();
        let body = resp
            .bytes_stream()
            .map(|r| r.map_err(classify_reqwest_error))
            .boxed();
        Ok(HttpResponse::new(status, body))
    }
}
pub struct MockHttpClient {
    status: u16,
    chunks: Vec<Vec<u8>>,
}
impl MockHttpClient {
    pub fn new(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            chunks: vec![body.into()],
        }
    }
    pub fn with_chunks(status: u16, chunks: Vec<Vec<u8>>) -> Self {
        Self { status, chunks }
    }
}
#[async_trait]
impl HttpClient for MockHttpClient {
    async fn post_json(
        &self,
        _url: &str,
        _headers: HashMap<String, String>,
        _body: Value,
    ) -> Result<HttpResponse, ChatProviderError> {
        let chunks = self.chunks.clone();
        let stream =
            futures_util::stream::iter(chunks.into_iter().map(|c| Ok(bytes::Bytes::from(c))))
                .boxed();
        Ok(HttpResponse::new(self.status, stream))
    }
    async fn post_multipart(
        &self,
        _url: &str,
        _headers: HashMap<String, String>,
        _parts: Vec<MultipartPart>,
        _fields: HashMap<String, String>,
    ) -> Result<HttpResponse, ChatProviderError> {
        let chunks = self.chunks.clone();
        let stream =
            futures_util::stream::iter(chunks.into_iter().map(|c| Ok(bytes::Bytes::from(c))))
                .boxed();
        Ok(HttpResponse::new(self.status, stream))
    }
}
fn classify_reqwest_error(err: reqwest::Error) -> ChatProviderError {
    if err.is_timeout() {
        return ChatProviderError::Timeout(APITimeoutError);
    }
    let msg = err.to_string();
    let re_network =
        regex::Regex::new(r"(?i)network|connection|connect|disconnect|terminated").unwrap();
    let re_timeout = regex::Regex::new(r"(?i)timed?\s*out|timeout|deadline").unwrap();
    if re_timeout.is_match(&msg) {
        ChatProviderError::Timeout(APITimeoutError)
    } else if re_network.is_match(&msg) || err.is_connect() {
        ChatProviderError::Connection(APIConnectionError)
    } else if let Some(status) = err.status() {
        crate::errors::normalize_api_status_error(status.as_u16(), msg, None)
    } else {
        ChatProviderError::Other(format!("Error: {msg}"))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    #[tokio::test]
    async fn mock_returns_configured_status_and_body() {
        let client = MockHttpClient::new(200, b"hello");
        let resp = client
            .post_json("http://x", HashMap::new(), Value::Null)
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let bytes = resp.bytes_stream().next().await.unwrap().unwrap();
        assert_eq!(bytes.as_ref(), b"hello");
    }
    #[tokio::test]
    async fn chunked_mock_streams_all_chunks() {
        let client = MockHttpClient::with_chunks(200, vec![b"a".to_vec(), b"b".to_vec()]);
        let resp = client
            .post_json("http://x", HashMap::new(), Value::Null)
            .await
            .unwrap();
        let chunks: Vec<_> = resp.bytes_stream().collect().await;
        assert_eq!(chunks.len(), 2);
    }
    #[tokio::test]
    async fn mock_post_multipart_returns_body() {
        let client = MockHttpClient::new(200, b"ok");
        let parts = vec![MultipartPart {
            name: "file".into(),
            file_name: Some("test.mp4".into()),
            mime_type: Some("video/mp4".into()),
            data: b"fake-video".to_vec(),
        }];
        let resp = client
            .post_multipart("http://x", HashMap::new(), parts, HashMap::new())
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let bytes = resp.bytes_stream().next().await.unwrap().unwrap();
        assert_eq!(bytes.as_ref(), b"ok");
    }
}
