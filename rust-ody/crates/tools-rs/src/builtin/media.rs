use base64::Engine;
use kaos_rs::kaos::Kaos;
use serde_json::{json, Value};

use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolOutput, ExecutableToolResult, ToolError,
    ToolExecution,
};
use crate::policies::path_access::{assert_path_allowed, AssertPathOptions, PathAccessOperation};
use crate::schema::InputSchema;
use crate::tool_accesses::ToolAccesses;
use crate::workspace::WorkspaceConfig;

const MAX_MEDIA_BYTES: u64 = 10 * 1024 * 1024;

fn read_media_file_parameters() -> Value {
    InputSchema::object(vec![(
        "file_path",
        InputSchema::string().description("Absolute or workspace-relative path to the media file"),
    )])
    .build()
}

pub struct ReadMediaFileTool {
    kaos: Kaos,
    workspace: WorkspaceConfig,
}

impl ReadMediaFileTool {
    pub fn new(kaos: Kaos, workspace: WorkspaceConfig) -> Self {
        Self { kaos, workspace }
    }
}

impl BuiltinTool for ReadMediaFileTool {
    fn name(&self) -> &str {
        "read_media_file"
    }

    fn description(&self) -> &str {
        "Read an image or video file and return it as a base64 resource with MIME type and dimensions."
    }

    fn parameters(&self) -> Value {
        read_media_file_parameters()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let path = args
            .get("file_path")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidArgs("file_path is required".into()))?;
        let safe_path = assert_path_allowed(
            path,
            &self.kaos.getcwd(),
            &self.workspace,
            AssertPathOptions {
                mode: PathAccessOperation::Read,
                check_sensitive: Some(true),
                path_class: None,
            },
        )?;

        let kaos = self.kaos.clone();
        let path = path.to_string();
        let safe_path2 = safe_path.clone();

        Ok(ToolExecution {
            accesses: ToolAccesses::read_file(&safe_path),
            description: format!("Read media file {}", path),
            matches_rule: None,
            display: None,
            approval_rule: format!("read_file:{}", path),
            execute: Box::new(move |ctx| {
                let kaos = kaos.clone();
                let path = path.clone();
                let safe_path = safe_path2.clone();
                Box::pin(async move { media_execution(kaos, path, safe_path, ctx).await })
            }),
        })
    }
}

async fn media_execution(
    kaos: Kaos,
    display_path: String,
    safe_path: String,
    ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    if ctx.signal.aborted() {
        return ExecutableToolResult::error_text(
            "Aborted before read started".into(),
            "Aborted".into(),
        );
    }

    let stat = match kaos.stat(&safe_path, false).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ExecutableToolResult::error_text(
                format!("\"{}\" does not exist.", display_path),
                "File not found".into(),
            );
        }
        Err(e) => {
            return ExecutableToolResult::error_text(
                format!("Failed to stat \"{}\": {}", display_path, e),
                "Stat failed".into(),
            );
        }
    };

    if stat.is_dir() {
        return ExecutableToolResult::error_text(
            format!("\"{}\" is a directory, not a file.", display_path),
            "Not a file".into(),
        );
    }

    if stat.st_size > MAX_MEDIA_BYTES {
        return ExecutableToolResult::error_text(
            format!(
                "File \"{}\" is too large ({} bytes). Maximum size is {} bytes (10 MiB).",
                display_path, stat.st_size, MAX_MEDIA_BYTES
            ),
            "File too large".into(),
        );
    }

    let bytes = match kaos.read_bytes(&safe_path, Some(MAX_MEDIA_BYTES)).await {
        Ok(b) => b,
        Err(e) => {
            return ExecutableToolResult::error_text(
                format!("Failed to read \"{}\": {}", display_path, e),
                "Read failed".into(),
            );
        }
    };

    let kind = match infer::get(&bytes) {
        Some(k) => k,
        None => {
            return ExecutableToolResult::error_text(
                format!(
                    "\"{}\" is not a recognized image or video file.",
                    display_path
                ),
                "Unrecognized media type".into(),
            );
        }
    };

    let media_type = if kind.mime_type().starts_with("image/") {
        "image"
    } else if kind.mime_type().starts_with("video/") {
        "video"
    } else {
        return ExecutableToolResult::error_text(
            format!(
                "\"{}\" has an unsupported MIME type: {}.",
                display_path,
                kind.mime_type()
            ),
            "Unsupported media type".into(),
        );
    };

    let dimensions = if media_type == "image" {
        match image::load_from_memory(&bytes) {
            Ok(img) => Some(json!({
                "width": img.width(),
                "height": img.height(),
            })),
            Err(_) => None,
        }
    } else {
        None
    };

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    ExecutableToolResult {
        output: ExecutableToolOutput::Parts(vec![json!({
            "type": media_type,
            "mime_type": kind.mime_type(),
            "media_type": media_type,
            "dimensions": dimensions,
            "data": b64,
        })]),
        message: None,
        is_error: false,
        stop_turn: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::WorkspaceConfig;
    use kaos_rs::environment::Environment;

    fn dummy_env() -> Environment {
        Environment {
            os_kind: "macOS".to_string(),
            os_arch: "arm64".to_string(),
            os_version: "23.0.0".to_string(),
            shell_name: "bash".to_string(),
            shell_path: "/bin/bash".to_string(),
        }
    }

    fn workspace(tmp: &std::path::Path) -> WorkspaceConfig {
        WorkspaceConfig::new(tmp.to_string_lossy().to_string())
    }

    #[tokio::test]
    async fn reads_png_with_dimensions_and_base64() {
        let tmp = tempfile::tempdir().unwrap();
        // Create a 2x1 RGB PNG
        let img = image::RgbImage::from_pixel(2, 1, image::Rgb([255, 0, 0]));
        let mut png_bytes = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
        tokio::fs::write(tmp.path().join("test.png"), &png_bytes)
            .await
            .unwrap();

        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = ReadMediaFileTool::new(kaos, workspace(tmp.path()));
        let exec = tool
            .resolve_execution(json!({"file_path": "test.png"}))
            .unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "".into(),
            tool_call_id: "".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;

        assert!(
            !result.is_error,
            "expected success, got: {:?}",
            result.message
        );

        let parts = match result.output {
            ExecutableToolOutput::Parts(p) => p,
            _ => panic!("expected Parts output"),
        };
        assert_eq!(parts.len(), 1);
        let obj = parts[0].as_object().unwrap();
        assert_eq!(obj["type"], "image");
        assert_eq!(obj["mime_type"], "image/png");
        let dims = obj["dimensions"].as_object().unwrap();
        assert_eq!(dims["width"], 2);
        assert_eq!(dims["height"], 1);
        let data = obj["data"].as_str().unwrap();
        // PNG files start with the magic bytes that encode to "iVBOR"
        assert!(
            data.starts_with("iVBOR"),
            "PNG base64 should start with iVBOR, got: {}",
            &data[..10]
        );
    }

    #[tokio::test]
    async fn rejects_oversized_media() {
        let tmp = tempfile::tempdir().unwrap();
        // Create a file larger than 10 MiB
        let size = MAX_MEDIA_BYTES as usize + 1024;
        let big_data = vec![0u8; size];
        tokio::fs::write(tmp.path().join("big.bin"), &big_data)
            .await
            .unwrap();

        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = ReadMediaFileTool::new(kaos, workspace(tmp.path()));
        let exec = tool
            .resolve_execution(json!({"file_path": "big.bin"}))
            .unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "".into(),
            tool_call_id: "".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;

        assert!(result.is_error, "expected error for oversized file");
        let message = result.message.unwrap_or_default();
        assert!(
            message.contains("too large"),
            "message should mention 'too large', got: {}",
            message
        );
    }
}
