use kaos_rs::kaos::Kaos;
use kaos_rs::text::ErrorMode;

/// Dispatch an internal `env.*` method to `kaos-rs`.
///
/// These methods are NOT part of the public CoreAPI contract; they exist
/// only for parity testing and internal tooling.
pub async fn dispatch(
    kaos: &Kaos,
    method: &str,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        "env.getcwd" => Ok(serde_json::json!({ "cwd": kaos.getcwd() })),
        "env.stat" => env_stat(kaos, payload).await,
        "env.glob" => env_glob(kaos, payload).await,
        "env.readText" => env_read_text(kaos, payload).await,
        "env.writeText" => env_write_text(kaos, payload).await,
        "env.exec" => env_exec(kaos, payload).await,
        _ => Err(format!("unknown env method: {method}")),
    }
}

async fn env_stat(kaos: &Kaos, payload: serde_json::Value) -> Result<serde_json::Value, String> {
    let path = payload
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("missing path")?;
    let follow_symlinks = payload
        .get("followSymlinks")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let stat = kaos
        .stat(path, follow_symlinks)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "stMode": stat.st_mode,
        "stIno": stat.st_ino,
        "stDev": stat.st_dev,
        "stNlink": stat.st_nlink,
        "stUid": stat.st_uid,
        "stGid": stat.st_gid,
        "stSize": stat.st_size,
        "stAtime": stat.st_atime,
        "stMtime": stat.st_mtime,
        "stCtime": stat.st_ctime,
        "isDir": stat.is_dir(),
    }))
}

async fn env_glob(kaos: &Kaos, payload: serde_json::Value) -> Result<serde_json::Value, String> {
    let path = payload
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("missing path")?;
    let pattern = payload
        .get("pattern")
        .and_then(|v| v.as_str())
        .ok_or("missing pattern")?;
    let case_sensitive = payload
        .get("caseSensitive")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let matches = kaos
        .glob(path, pattern, case_sensitive)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "matches": matches }))
}

async fn env_read_text(
    kaos: &Kaos,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let path = payload
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("missing path")?;
    let encoding = payload.get("encoding").and_then(|v| v.as_str());
    let errors = payload
        .get("errors")
        .and_then(|v| v.as_str())
        .map(parse_error_mode)
        .transpose()?;
    let text = kaos
        .read_text(path, encoding, errors)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "text": text }))
}

async fn env_write_text(
    kaos: &Kaos,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let path = payload
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("missing path")?;
    let text = payload
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or("missing text")?;
    let mode = payload.get("mode").and_then(|v| v.as_str());
    let encoding = payload.get("encoding").and_then(|v| v.as_str());
    let written = kaos
        .write_text(path, text, mode, encoding)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "written": written }))
}

async fn env_exec(kaos: &Kaos, payload: serde_json::Value) -> Result<serde_json::Value, String> {
    let command = payload
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or("missing command")?;
    let args: Vec<String> = payload
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let env: Option<Vec<(String, String)>> =
        payload.get("env").and_then(|v| v.as_object()).map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        });

    let all_args: Vec<&str> = std::iter::once(command)
        .chain(args.iter().map(|s| s.as_str()))
        .collect();

    let proc = if let Some(vars) = env {
        let pairs: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        kaos.exec_with_env(&all_args, &pairs).await
    } else {
        kaos.exec(&all_args).await
    }
    .map_err(|e| e.to_string())?;

    let exit_code = proc.wait().await;
    let stdout = proc.stdout().await;
    let stderr = proc.stderr().await;
    Ok(serde_json::json!({
        "exitCode": exit_code,
        "stdout": stdout,
        "stderr": stderr,
    }))
}

fn parse_error_mode(s: &str) -> Result<ErrorMode, String> {
    match s {
        "strict" => Ok(ErrorMode::Strict),
        "replace" => Ok(ErrorMode::Replace),
        "ignore" => Ok(ErrorMode::Ignore),
        _ => Err(format!("invalid errors mode: {s}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaos_rs::environment::Environment;
    use tempfile::TempDir;

    fn dummy_env() -> Environment {
        Environment {
            os_kind: std::env::consts::OS.to_string(),
            os_arch: std::env::consts::ARCH.to_string(),
            os_version: "0.0.0".to_string(),
            shell_name: "bash".to_string(),
            shell_path: "/bin/bash".to_string(),
        }
    }

    async fn test_kaos() -> (TempDir, Kaos) {
        let dir = TempDir::new().unwrap();
        let kaos = Kaos::new(dummy_env(), dir.path());
        (dir, kaos)
    }

    fn p(dir: &TempDir, name: &str) -> String {
        dir.path().join(name).to_string_lossy().to_string()
    }

    // ── T6: env.stat ────────────────────────────────────────────────────

    #[tokio::test]
    async fn stat_returns_file_size_and_dir_flag() {
        let (dir, kaos) = test_kaos().await;
        tokio::fs::write(p(&dir, "test.txt"), "hello")
            .await
            .unwrap();

        let result = dispatch(&kaos, "env.stat", serde_json::json!({"path": "test.txt"}))
            .await
            .unwrap();

        assert_eq!(result["stSize"], 5);
        assert_eq!(result["isDir"], false);
        assert!(result["stMode"].as_u64().unwrap() > 0 || cfg!(windows));
    }

    #[tokio::test]
    async fn stat_directory_has_is_dir_true() {
        let (dir, kaos) = test_kaos().await;
        tokio::fs::create_dir(p(&dir, "sub")).await.unwrap();

        let result = dispatch(&kaos, "env.stat", serde_json::json!({"path": "sub"}))
            .await
            .unwrap();

        assert_eq!(result["isDir"], true);
    }

    #[tokio::test]
    async fn stat_missing_path_returns_error() {
        let (_dir, kaos) = test_kaos().await;

        let err = dispatch(&kaos, "env.stat", serde_json::json!({"path": "missing"}))
            .await
            .unwrap_err();

        assert!(
            err.contains("No such file")
                || err.contains("not found")
                || err.contains("ENOENT")
                || err.contains("entity not found")
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn stat_symlink_follow_switch() {
        let (dir, kaos) = test_kaos().await;
        tokio::fs::write(p(&dir, "target.txt"), "x").await.unwrap();
        std::os::unix::fs::symlink(p(&dir, "target.txt"), p(&dir, "link.txt")).unwrap();

        let follow = dispatch(
            &kaos,
            "env.stat",
            serde_json::json!({"path": "link.txt", "followSymlinks": true}),
        )
        .await
        .unwrap();
        assert_eq!(follow["stSize"], 1);

        let no_follow = dispatch(
            &kaos,
            "env.stat",
            serde_json::json!({"path": "link.txt", "followSymlinks": false}),
        )
        .await
        .unwrap();
        assert!(no_follow["stSize"].as_u64().unwrap() < 100);
    }

    // ── T7: env.glob ────────────────────────────────────────────────────

    #[tokio::test]
    async fn glob_star_matches_basenames_only() {
        let (dir, kaos) = test_kaos().await;
        tokio::fs::write(p(&dir, "a.txt"), "").await.unwrap();
        tokio::fs::write(p(&dir, "b.log"), "").await.unwrap();
        tokio::fs::create_dir(p(&dir, "sub")).await.unwrap();
        tokio::fs::write(p(&dir, "sub/c.txt"), "").await.unwrap();

        let result = dispatch(
            &kaos,
            "env.glob",
            serde_json::json!({"path": ".", "pattern": "*.txt"}),
        )
        .await
        .unwrap();

        let matches: Vec<String> = serde_json::from_value(result["matches"].clone()).unwrap();
        assert_eq!(matches.len(), 1);
        assert!(matches[0].ends_with("/a.txt"));
    }

    #[tokio::test]
    async fn glob_double_star_recurses() {
        let (dir, kaos) = test_kaos().await;
        tokio::fs::write(p(&dir, "a.txt"), "").await.unwrap();
        tokio::fs::create_dir(p(&dir, "sub")).await.unwrap();
        tokio::fs::write(p(&dir, "sub/c.txt"), "").await.unwrap();

        let result = dispatch(
            &kaos,
            "env.glob",
            serde_json::json!({"path": ".", "pattern": "**/*.txt"}),
        )
        .await
        .unwrap();

        let mut matches: Vec<String> = serde_json::from_value(result["matches"].clone()).unwrap();
        matches.sort();
        assert_eq!(matches.len(), 2);
        assert!(matches[0].ends_with("/a.txt"));
        assert!(matches[1].ends_with("/sub/c.txt"));
    }

    #[tokio::test]
    async fn glob_case_insensitive_flag() {
        let (dir, kaos) = test_kaos().await;
        tokio::fs::write(p(&dir, "A.TXT"), "").await.unwrap();

        let sensitive = dispatch(
            &kaos,
            "env.glob",
            serde_json::json!({"path": ".", "pattern": "*.txt", "caseSensitive": true}),
        )
        .await
        .unwrap();
        let insensitive = dispatch(
            &kaos,
            "env.glob",
            serde_json::json!({"path": ".", "pattern": "*.txt", "caseSensitive": false}),
        )
        .await
        .unwrap();

        assert_eq!(sensitive["matches"].as_array().unwrap().len(), 0);
        assert_eq!(insensitive["matches"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn glob_missing_directory_returns_error() {
        let (_dir, kaos) = test_kaos().await;

        let err = dispatch(
            &kaos,
            "env.glob",
            serde_json::json!({"path": "missing", "pattern": "*"}),
        )
        .await
        .unwrap_err();

        assert!(
            err.contains("No such file")
                || err.contains("not found")
                || err.contains("ENOENT")
                || err.contains("entity not found")
        );
    }

    // ── T8: env.readText ────────────────────────────────────────────────

    #[tokio::test]
    async fn read_text_defaults_to_utf8_strict() {
        let (dir, kaos) = test_kaos().await;
        tokio::fs::write(p(&dir, "hello.txt"), "hello")
            .await
            .unwrap();

        let result = dispatch(
            &kaos,
            "env.readText",
            serde_json::json!({"path": "hello.txt"}),
        )
        .await
        .unwrap();

        assert_eq!(result["text"], "hello");
    }

    #[tokio::test]
    async fn read_text_strict_rejects_invalid_utf8() {
        let (dir, kaos) = test_kaos().await;
        tokio::fs::write(p(&dir, "bad.txt"), b"hello \xff world")
            .await
            .unwrap();

        let err = dispatch(
            &kaos,
            "env.readText",
            serde_json::json!({"path": "bad.txt", "errors": "strict"}),
        )
        .await
        .unwrap_err();

        assert!(err.contains("decode error") || err.contains("invalid"));
    }

    #[tokio::test]
    async fn read_text_replace_substitutes_invalid_bytes() {
        let (dir, kaos) = test_kaos().await;
        tokio::fs::write(p(&dir, "bad.txt"), b"hello \xff world")
            .await
            .unwrap();

        let result = dispatch(
            &kaos,
            "env.readText",
            serde_json::json!({"path": "bad.txt", "errors": "replace"}),
        )
        .await
        .unwrap();

        assert_eq!(result["text"], "hello \u{fffd} world");
    }

    #[tokio::test]
    async fn read_text_ignore_drops_invalid_bytes() {
        let (dir, kaos) = test_kaos().await;
        tokio::fs::write(p(&dir, "bad.txt"), b"\xff\xef\xbf\xbd hello")
            .await
            .unwrap();

        let result = dispatch(
            &kaos,
            "env.readText",
            serde_json::json!({"path": "bad.txt", "errors": "ignore"}),
        )
        .await
        .unwrap();

        assert_eq!(result["text"], "\u{fffd} hello");
    }

    #[tokio::test]
    async fn read_text_utf16le_replace() {
        let (dir, kaos) = test_kaos().await;
        // U+D800 lone surrogate + 'A' in UTF-16LE
        tokio::fs::write(p(&dir, "utf16.txt"), &[0x00u8, 0xd8, 0x41, 0x00])
            .await
            .unwrap();

        let result = dispatch(
            &kaos,
            "env.readText",
            serde_json::json!({"path": "utf16.txt", "encoding": "utf-16le", "errors": "replace"}),
        )
        .await
        .unwrap();

        assert_eq!(result["text"], "\u{fffd}A");
    }

    // ── T9: env.writeText ───────────────────────────────────────────────

    #[tokio::test]
    async fn write_text_creates_file_and_returns_char_count() {
        let (dir, kaos) = test_kaos().await;

        let result = dispatch(
            &kaos,
            "env.writeText",
            serde_json::json!({"path": "out.txt", "text": "hello"}),
        )
        .await
        .unwrap();

        assert_eq!(result["written"], 5);
        let content = tokio::fs::read_to_string(p(&dir, "out.txt")).await.unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn write_text_append_mode() {
        let (dir, kaos) = test_kaos().await;
        tokio::fs::write(p(&dir, "out.txt"), "hello").await.unwrap();

        let result = dispatch(
            &kaos,
            "env.writeText",
            serde_json::json!({"path": "out.txt", "text": " world", "mode": "a"}),
        )
        .await
        .unwrap();

        assert_eq!(result["written"], 6);
        let content = tokio::fs::read_to_string(p(&dir, "out.txt")).await.unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn write_text_overwrite_mode_by_default() {
        let (dir, kaos) = test_kaos().await;
        tokio::fs::write(p(&dir, "out.txt"), "old").await.unwrap();

        dispatch(
            &kaos,
            "env.writeText",
            serde_json::json!({"path": "out.txt", "text": "new"}),
        )
        .await
        .unwrap();

        let content = tokio::fs::read_to_string(p(&dir, "out.txt")).await.unwrap();
        assert_eq!(content, "new");
    }

    #[tokio::test]
    async fn write_text_non_utf8_encoding_maps_bytes() {
        let (dir, kaos) = test_kaos().await;

        dispatch(
            &kaos,
            "env.writeText",
            serde_json::json!({"path": "out.bin", "text": "ABC", "encoding": "latin1"}),
        )
        .await
        .unwrap();

        let content = tokio::fs::read(p(&dir, "out.bin")).await.unwrap();
        assert_eq!(content, vec![0x41, 0x42, 0x43]);
    }

    // ── T10: env.exec ───────────────────────────────────────────────────

    #[tokio::test]
    async fn exec_echo_command_and_args() {
        let (_dir, kaos) = test_kaos().await;

        let result = dispatch(
            &kaos,
            "env.exec",
            serde_json::json!({"command": "/bin/echo", "args": ["-n", "hello"]}),
        )
        .await
        .unwrap();

        assert_eq!(result["exitCode"], 0);
        assert_eq!(
            result["stdout"],
            serde_json::json!(vec![b'h', b'e', b'l', b'l', b'o'])
        );
        assert_eq!(result["stderr"], serde_json::json!(Vec::<u8>::new()));
    }

    #[tokio::test]
    async fn exec_custom_exit_code() {
        let (_dir, kaos) = test_kaos().await;

        let result = dispatch(
            &kaos,
            "env.exec",
            serde_json::json!({"command": "/bin/sh", "args": ["-c", "exit 42"]}),
        )
        .await
        .unwrap();

        assert_eq!(result["exitCode"], 42);
        assert_eq!(result["stdout"], serde_json::json!(Vec::<u8>::new()));
        assert_eq!(result["stderr"], serde_json::json!(Vec::<u8>::new()));
    }

    #[tokio::test]
    async fn exec_with_env_variables() {
        let (_dir, kaos) = test_kaos().await;

        let result = dispatch(
            &kaos,
            "env.exec",
            serde_json::json!({
                "command": "/bin/sh",
                "args": ["-c", "printf '%s' \"$MYVAR\""],
                "env": {"MYVAR": "bar"}
            }),
        )
        .await
        .unwrap();

        assert_eq!(result["exitCode"], 0);
        assert_eq!(result["stdout"], serde_json::json!(vec![b'b', b'a', b'r']));
        assert_eq!(result["stderr"], serde_json::json!(Vec::<u8>::new()));
    }

    #[tokio::test]
    async fn exec_captures_stderr() {
        let (_dir, kaos) = test_kaos().await;

        let result = dispatch(
            &kaos,
            "env.exec",
            serde_json::json!({"command": "/bin/sh", "args": ["-c", "printf err >&2"]}),
        )
        .await
        .unwrap();

        assert_eq!(result["exitCode"], 0);
        assert_eq!(result["stdout"], serde_json::json!(Vec::<u8>::new()));
        assert_eq!(result["stderr"], serde_json::json!(vec![b'e', b'r', b'r']));
    }

    #[tokio::test]
    async fn exec_missing_command_returns_error() {
        let (_dir, kaos) = test_kaos().await;

        let err = dispatch(
            &kaos,
            "env.exec",
            serde_json::json!({"command": "__missing_command_12345"}),
        )
        .await
        .unwrap_err();

        assert!(
            err.contains("not found")
                || err.contains("ENOENT")
                || err.contains("No such file")
                || err.contains("The system cannot find")
                || err.contains("entity not found")
        );
    }
}
