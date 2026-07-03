use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use lazy_static::lazy_static;

#[allow(unused_imports)]
use futures_util::StreamExt;

use crate::errors::ChatProviderError;
use crate::http_client::{HttpClient, MultipartPart, ReqwestClient};
use crate::message::ContentPart;
use crate::message::UrlPayload;

// ── MIME table ────────────────────────────────────────────────────────────

lazy_static! {
    static ref VALID_VIDEO_MIMES: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("mp4", "video/mp4");
        m.insert("mpeg", "video/mpeg");
        m.insert("mpg", "video/mpeg");
        m.insert("mov", "video/quicktime");
        m.insert("webm", "video/webm");
        m.insert("mkv", "video/x-matroska");
        m.insert("avi", "video/x-msvideo");
        m.insert("flv", "video/x-flv");
        m.insert("3gp", "video/3gpp");
        m
    };
}

fn guess_mime_from_extension(path: &Path) -> Option<&'static str> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
        .and_then(|ext| VALID_VIDEO_MIMES.get(ext.as_str()).copied())
}

fn guess_mime_from_path_or_bytes(
    path: Option<&Path>,
    bytes: Option<&[u8]>,
) -> Option<&'static str> {
    if let Some(p) = path {
        if let Some(mime) = guess_mime_from_extension(p) {
            return Some(mime);
        }
    }
    if let Some(data) = bytes {
        // MIME-from-magic: check first bytes
        if data.len() >= 12 {
            // Check for WebM (1A 45 DF A3)
            if &data[0..4] == b"\x1a\x45\xdf\xa3" {
                return Some("video/webm");
            }
            // Check for Matroska/MKV (same as WebM header)
            // Check for MP4/MOV (ftyp at offset 4)
            if data.len() >= 12 && &data[4..8] == b"ftyp" {
                let brand = &data[8..12];
                if brand == b"qt  " {
                    return Some("video/quicktime");
                }
                return Some("video/mp4");
            }
            // Check for FLV (46 4C 56 01)
            if &data[0..4] == b"FLV\x01" {
                return Some("video/x-flv");
            }
            // Check for AVI (52 49 46 46 ... 41 56 49 20)
            if &data[0..4] == b"RIFF" && data.len() >= 12 && &data[8..12] == b"AVI " {
                return Some("video/x-msvideo");
            }
        }
        // Check for MPEG (00 00 01 BA or 00 00 01 B3)
        if data.len() >= 4 {
            if &data[0..3] == b"\x00\x00\x01" && (data[3] == 0xBA || data[3] == 0xB3) {
                return Some("video/mpeg");
            }
        }
        // Check for 3GP
        if data.len() >= 12 && &data[4..8] == b"ftyp" {
            let brand = &data[8..12];
            if brand == b"3gp5" || brand == b"3gp4" {
                return Some("video/3gpp");
            }
        }
    }
    None
}

fn check_video_mime(mime: &str) -> Result<(), ChatProviderError> {
    if !VALID_VIDEO_MIMES.values().any(|v| *v == mime) {
        return Err(ChatProviderError::Other(format!(
            "Unsupported video MIME type: {}. Supported types: {:?}",
            mime,
            VALID_VIDEO_MIMES.values().collect::<Vec<_>>()
        )));
    }
    Ok(())
}

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct VideoUploadInput {
    pub file_path: Option<String>,
    pub file_bytes: Option<Vec<u8>>,
    pub purpose: Option<String>,
    pub mime_type: Option<String>,
    pub filename: Option<String>,
}

#[derive(Debug, Clone)]
pub enum KimiVideoUpload {
    Path(String),
    Bytes(Vec<u8>),
}

pub struct KimiFilesOptions {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub default_headers: Option<HashMap<String, String>>,
    pub http_client: Option<Arc<dyn HttpClient>>,
}

pub struct KimiUploadOptions {
    pub purpose: Option<String>,
    pub mime_type: Option<String>,
    pub filename: Option<String>,
}

pub struct KimiFiles {
    api_key: Option<String>,
    base_url: String,
    default_headers: Option<HashMap<String, String>>,
    http_client: Arc<dyn HttpClient>,
}

impl KimiFiles {
    pub fn new(options: KimiFilesOptions) -> Self {
        let api_key = options
            .api_key
            .or_else(|| std::env::var("KIMI_API_KEY").ok())
            .filter(|k| !k.is_empty());
        let base_url = options
            .base_url
            .or_else(|| std::env::var("KIMI_BASE_URL").ok())
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| "https://api.moonshot.ai/v1".into());
        let http_client = options
            .http_client
            .unwrap_or_else(|| Arc::new(ReqwestClient::new(reqwest::Client::new())));

        Self {
            api_key,
            base_url,
            default_headers: options.default_headers,
            http_client,
        }
    }

    fn build_headers(&self) -> Result<HashMap<String, String>, ChatProviderError> {
        // Note: multipart doesn't use content-type: application/json;
        // it will be set automatically by the multipart form.
        let api_key = self
            .api_key
            .as_ref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| {
                ChatProviderError::MissingApiKey(crate::errors::APIMissingApiKeyError {
                    provider: "kimi".into(),
                })
            })?;

        let mut headers = HashMap::new();
        headers.insert("authorization".into(), format!("Bearer {}", api_key));

        if let Some(default) = &self.default_headers {
            for (k, v) in default {
                if k.to_lowercase() != "content-type" {
                    headers.insert(k.clone(), v.clone());
                }
            }
        }
        Ok(headers)
    }

    pub async fn upload_video(
        &self,
        input: VideoUploadInput,
        options: Option<KimiUploadOptions>,
    ) -> Result<ContentPart, ChatProviderError> {
        let file_name = options
            .as_ref()
            .and_then(|o| o.filename.clone())
            .or_else(|| input.filename.clone())
            .or_else(|| {
                input
                    .file_path
                    .as_ref()
                    .and_then(|p| Path::new(p).file_name())
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "video.mp4".into());

        let path = input.file_path.as_deref().map(Path::new);

        // Determine MIME type
        let mime_type = input.mime_type.clone()
            .or_else(|| options.as_ref().and_then(|o| o.mime_type.clone()))
            .or_else(|| {
                guess_mime_from_path_or_bytes(
                    path,
                    input.file_bytes.as_deref(),
                )
                .map(|s| s.to_string())
            })
            .ok_or_else(|| {
                ChatProviderError::Other(
                    "Cannot determine video MIME type. Provide mime_type or a recognized file extension.".into(),
                )
            })?;

        check_video_mime(&mime_type)?;

        // Read file data
        let data = if let Some(ref file_path) = input.file_path {
            let path = Path::new(file_path);
            if let Some(mime) = guess_mime_from_extension(path) {
                check_video_mime(mime)?;
            }
            std::fs::read(path).map_err(|e| {
                ChatProviderError::Other(format!(
                    "Failed to read video file at {}: {}",
                    file_path, e
                ))
            })?
        } else if let Some(ref bytes) = input.file_bytes {
            bytes.clone()
        } else {
            return Err(ChatProviderError::Other(
                "Either file_path or file_bytes must be provided".into(),
            ));
        };

        let purpose = options
            .as_ref()
            .and_then(|o| o.purpose.clone())
            .or_else(|| input.purpose.clone())
            .unwrap_or_else(|| "file-extract".into());

        let headers = self.build_headers()?;

        let base = self.base_url.trim_end_matches('/');
        let url = format!("{}/files", base);

        let part = MultipartPart {
            name: "file".into(),
            file_name: Some(file_name),
            mime_type: Some(mime_type),
            data,
        };

        let mut fields = HashMap::new();
        fields.insert("purpose".into(), purpose);

        let response = self
            .http_client
            .post_multipart(&url, headers, vec![part], fields)
            .await?;

        let status = response.status();
        if !(200..300).contains(&status) {
            let body_bytes = futures_util::StreamExt::collect::<Vec<Result<bytes::Bytes, _>>>(
                response.bytes_stream(),
            )
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .fold(Vec::new(), |mut acc, b| {
                acc.extend_from_slice(&b);
                acc
            });
            let body_str = String::from_utf8_lossy(&body_bytes);
            return Err(ChatProviderError::Other(format!(
                "Kimi file upload failed with status {}: {}",
                status, body_str
            )));
        }

        let body_bytes = futures_util::StreamExt::collect::<Vec<Result<bytes::Bytes, _>>>(
            response.bytes_stream(),
        )
        .await
        .into_iter()
        .filter_map(|r| r.ok())
        .fold(Vec::new(), |mut acc, b| {
            acc.extend_from_slice(&b);
            acc
        });

        let response_body: serde_json::Value =
            serde_json::from_slice(&body_bytes).map_err(|e| {
                ChatProviderError::Other(format!(
                    "Failed to parse Kimi file upload response: {}",
                    e
                ))
            })?;

        let file_id = response_body
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ChatProviderError::Other("Kimi file upload response missing 'id' field".into())
            })?;

        Ok(ContentPart::VideoUrl {
            video_url: UrlPayload {
                url: format!("ms://{}", file_id),
                id: Some(file_id.to_string()),
            },
        })
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_client::HttpResponse;

    struct CaptureMultipartClient {
        response_status: u16,
        response_body: Vec<u8>,
        captured_parts: std::sync::Mutex<Option<Vec<MultipartPart>>>,
    }

    impl CaptureMultipartClient {
        fn new(status: u16, body: Vec<u8>) -> Self {
            Self {
                response_status: status,
                response_body: body,
                captured_parts: std::sync::Mutex::new(None),
            }
        }

        fn captured_parts(&self) -> Option<Vec<MultipartPart>> {
            self.captured_parts.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl HttpClient for CaptureMultipartClient {
        async fn post_json(
            &self,
            _url: &str,
            _headers: HashMap<String, String>,
            _body: serde_json::Value,
        ) -> Result<HttpResponse, ChatProviderError> {
            unreachable!()
        }

        async fn post_multipart(
            &self,
            _url: &str,
            _headers: HashMap<String, String>,
            parts: Vec<MultipartPart>,
            _fields: HashMap<String, String>,
        ) -> Result<HttpResponse, ChatProviderError> {
            *self.captured_parts.lock().unwrap() = Some(parts.clone());
            let chunks = vec![self.response_body.clone()];
            let stream =
                futures_util::stream::iter(chunks.into_iter().map(|c| Ok(bytes::Bytes::from(c))))
                    .boxed();
            Ok(HttpResponse::new(self.response_status, stream))
        }
    }

    fn make_kimi_files(api_key: &str) -> KimiFiles {
        KimiFiles::new(KimiFilesOptions {
            api_key: Some(api_key.into()),
            base_url: None,
            default_headers: None,
            http_client: None,
        })
    }

    #[tokio::test]
    async fn upload_video_from_bytes() {
        let response = serde_json::json!({
            "id": "file-abc123",
            "object": "file",
            "bytes": 1024,
            "created_at": 1234567890,
            "filename": "video.mp4"
        });
        let client = Arc::new(CaptureMultipartClient::new(
            200,
            serde_json::to_vec(&response).unwrap(),
        ));

        let files = KimiFiles {
            http_client: client.clone(),
            ..make_kimi_files("sk-test")
        };

        // Create minimal MP4 file bytes (ftyp box)
        let mp4_bytes = vec![
            0x00, 0x00, 0x00, 0x14, // box size
            b'f', b't', b'y', b'p', // ftyp
            b'm', b'p', b'4', b'2', // major brand
            0x00, 0x00, 0x00, 0x00, // minor version
            0x00, 0x00, 0x00, 0x00,
        ];

        let result = files
            .upload_video(
                VideoUploadInput {
                    file_path: None,
                    file_bytes: Some(mp4_bytes.clone()),
                    purpose: None,
                    mime_type: Some("video/mp4".into()),
                    filename: Some("test.mp4".into()),
                },
                None,
            )
            .await
            .unwrap();

        match result {
            ContentPart::VideoUrl { video_url } => {
                assert_eq!(video_url.url, "ms://file-abc123");
                assert_eq!(video_url.id, Some("file-abc123".into()));
            }
            _ => panic!("expected VideoUrl content part"),
        }

        let parts = client.captured_parts().unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].name, "file");
        assert_eq!(parts[0].file_name.as_deref(), Some("test.mp4"));
        assert_eq!(parts[0].data, mp4_bytes);
    }

    #[tokio::test]
    async fn upload_video_rejects_non_video_mime() {
        let files = make_kimi_files("sk-test");

        let err = files
            .upload_video(
                VideoUploadInput {
                    file_path: None,
                    file_bytes: Some(b"not-a-video".to_vec()),
                    purpose: None,
                    mime_type: Some("application/pdf".into()),
                    filename: Some("test.pdf".into()),
                },
                None,
            )
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("Unsupported video MIME type"),
            "Unexpected error: {}",
            err
        );
    }

    #[tokio::test]
    async fn upload_video_from_path_rejects_bad_extension() {
        let files = make_kimi_files("sk-test");

        let err = files
            .upload_video(
                VideoUploadInput {
                    file_path: Some("/nonexistent/bad_extension.txt".into()),
                    file_bytes: None,
                    purpose: None,
                    mime_type: None,
                    filename: None,
                },
                None,
            )
            .await
            .unwrap_err();

        // Should either fail with "Cannot determine video MIME type" or file read error
        let msg = err.to_string();
        assert!(
            msg.contains("Cannot determine video MIME type")
                || msg.contains("Failed to read video file"),
            "Unexpected error: {}",
            msg
        );
    }
}
