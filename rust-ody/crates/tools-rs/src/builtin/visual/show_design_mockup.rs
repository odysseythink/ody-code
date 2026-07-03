use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use kaos_rs::kaos::Kaos;
use serde_json::Value;
use url::Url;

use crate::builtin::visual::{slugify_design_title, DesignMockupHost, OpenExternalResult};
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};
use crate::schema::InputSchema;
use crate::tool_accesses::ToolAccesses;

pub struct ShowDesignMockupTool {
    kaos: Kaos,
    host: Arc<dyn DesignMockupHost>,
}

impl ShowDesignMockupTool {
    pub fn new(kaos: Kaos, host: Arc<dyn DesignMockupHost>) -> Self {
        Self { kaos, host }
    }
}

impl BuiltinTool for ShowDesignMockupTool {
    fn name(&self) -> &str {
        "ShowDesignMockup"
    }

    fn description(&self) -> &str {
        include_str!("show-design-mockup.md")
    }

    fn parameters(&self) -> Value {
        parameters()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let html = args
            .get("html")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidArgs("html is required".into()))?;
        if html.is_empty() {
            return Err(ToolError::InvalidArgs("html must be non-empty".into()));
        }
        let title = args
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Design mockup")
            .to_string();

        let host = Arc::clone(&self.host);
        let kaos = self.kaos.clone();
        let html = html.to_string();

        Ok(ToolExecution {
            accesses: ToolAccesses::none(),
            description: format!("Showing design mockup: {}", title),
            approval_rule: self.name().into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx| {
                let host = Arc::clone(&host);
                let kaos = kaos.clone();
                let html = html.clone();
                let title = title.clone();
                Box::pin(async move { execution(kaos, host, html, title).await })
            }),
        })
    }
}

async fn execution(
    kaos: Kaos,
    host: Arc<dyn DesignMockupHost>,
    html: String,
    title: String,
) -> ExecutableToolResult {
    if !host.is_available() {
        return ExecutableToolResult::error_text(
            "Visual companion is not available in this host (no openExternal). Describe the mockup in text instead.".into(),
            "Host cannot open external URLs".into(),
        );
    }

    let file_path = match write_mockup(&kaos, host.as_ref(), &html, &title).await {
        Ok(p) => p,
        Err(e) => {
            return ExecutableToolResult::error_text(
                format!("Failed to write mockup file: {}", e),
                "Write failed".into(),
            )
        }
    };

    let url = match Url::from_file_path(&file_path) {
        Ok(u) => u.to_string(),
        Err(_) => {
            return ExecutableToolResult::error_text(
                format!("Failed to build file URL for {}", file_path),
                "Invalid file path".into(),
            )
        }
    };

    match host.open_external(&url, &title).await {
        Ok(OpenExternalResult { opened: true, .. }) => ExecutableToolResult::ok_text(format!(
            "Opened mockup in the user's browser: {}",
            file_path
        )),
        Ok(OpenExternalResult {
            opened: false,
            error,
        }) => {
            let suffix = error.map(|e| format!(": {}", e)).unwrap_or_default();
            ExecutableToolResult::ok_text(format!(
                "Wrote mockup to {} but the host did not open it{}. Share the path with the user or describe the mockup in text.",
                file_path, suffix
            ))
        }
        Err(e) => ExecutableToolResult::error_text(
            format!("Host openExternal failed: {}", e),
            "Open failed".into(),
        ),
    }
}

async fn write_mockup(
    kaos: &Kaos,
    host: &dyn DesignMockupHost,
    html: &str,
    title: &str,
) -> Result<String, String> {
    let base_dir = match host.design_file_path() {
        Some(design_path) => {
            let parent = std::path::Path::new(&design_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string());
            format!("{}/.mockups", parent)
        }
        None => {
            format!(
                "{}/ody-design-mockups",
                std::env::temp_dir().to_string_lossy()
            )
        }
    };

    kaos.mkdir(&base_dir, true, true)
        .await
        .map_err(|e| format!("mkdir failed: {}", e))?;

    let slug = slugify_design_title(title);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let file_path = format!("{}/{}-{}.html", base_dir, timestamp, slug);

    kaos.write_text(&file_path, html, Some("w"), None)
        .await
        .map_err(|e| format!("write failed: {}", e))?;

    Ok(file_path)
}

fn parameters() -> Value {
    InputSchema::object(vec![
        (
            "html",
            InputSchema::string()
                .min_length(1)
                .description("A complete, self-contained HTML document (inline CSS only — no external assets) to render as the mockup."),
        ),
        (
            "title",
            InputSchema::string()
                .default(Value::String("Design mockup".into()))
                .description("Short title for this mockup; used for the generated file name."),
        ),
    ])
    .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::visual::MockDesignMockupHost;
    use kaos_rs::environment::Environment;
    use serde_json::json;

    fn dummy_env() -> Environment {
        Environment {
            os_kind: "macOS".into(),
            os_arch: "arm64".into(),
            os_version: "23.0.0".into(),
            shell_name: "bash".into(),
            shell_path: "/bin/bash".into(),
        }
    }

    async fn run_show(
        tmp: &tempfile::TempDir,
        args: Value,
        host: Arc<MockDesignMockupHost>,
    ) -> ExecutableToolResult {
        let design_path = tmp.path().join("design.md");
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let _ = design_path;
        let tool = ShowDesignMockupTool::new(kaos, host);
        let exec = tool.resolve_execution(args).unwrap();
        (exec.execute)(crate::builtin::ExecutableToolContext {
            turn_id: "".into(),
            tool_call_id: "".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await
    }

    #[tokio::test]
    async fn writes_and_opens_mockup_next_to_design_file() {
        let tmp = tempfile::tempdir().unwrap();
        let design_path = tmp.path().join("design.md");
        let host = Arc::new(MockDesignMockupHost::new(
            true,
            Some(design_path.to_string_lossy().to_string()),
            Ok(OpenExternalResult {
                opened: true,
                error: None,
            }),
        ));

        let result = run_show(
            &tmp,
            json!({
                "html": "<html><body>Hello</body></html>",
                "title": "Login Form"
            }),
            host.clone(),
        )
        .await;

        assert!(!result.is_error, "expected success, got {:?}", result);
        assert!(result.to_text().contains("Opened mockup"));

        let mockups_dir = tmp.path().join(".mockups");
        assert!(mockups_dir.exists());
        let entries: Vec<_> = std::fs::read_dir(&mockups_dir).unwrap().collect();
        assert_eq!(entries.len(), 1);
        let written = entries[0].as_ref().unwrap().path();
        let name = written.file_name().unwrap().to_string_lossy();
        assert!(name.ends_with("-login-form.html"));

        let opened = host.opened_url.lock().unwrap().clone().unwrap();
        assert!(opened.starts_with("file://"));
        assert!(opened.contains("login-form.html"));
    }

    #[tokio::test]
    async fn reports_error_when_host_unavailable() {
        let tmp = tempfile::tempdir().unwrap();
        let host = Arc::new(MockDesignMockupHost::new(
            false,
            None,
            Ok(OpenExternalResult {
                opened: false,
                error: None,
            }),
        ));
        let result = run_show(&tmp, json!({"html": "<html></html>"}), host).await;
        assert!(result.is_error);
        assert!(result
            .to_text()
            .contains("Visual companion is not available"));
    }

    #[tokio::test]
    async fn reports_not_opened_when_host_refuses() {
        let tmp = tempfile::tempdir().unwrap();
        let host = Arc::new(MockDesignMockupHost::new(
            true,
            None,
            Ok(OpenExternalResult {
                opened: false,
                error: Some("no browser".into()),
            }),
        ));
        let result = run_show(&tmp, json!({"html": "<html></html>"}), host).await;
        assert!(!result.is_error);
        let text = result.to_text();
        assert!(text.contains("did not open it"));
        assert!(text.contains("no browser"));
    }

    #[tokio::test]
    async fn rejects_empty_html() {
        let tmp = tempfile::tempdir().unwrap();
        let host = Arc::new(MockDesignMockupHost::new(
            true,
            None,
            Ok(OpenExternalResult {
                opened: true,
                error: None,
            }),
        ));
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = ShowDesignMockupTool::new(kaos, host);
        let result = tool.resolve_execution(json!({"html": ""}));
        assert!(result.is_err());
    }
}
