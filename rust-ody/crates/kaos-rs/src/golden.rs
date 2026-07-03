use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::environment::{detect_environment, Environment, EnvironmentDeps};
use crate::file;
use crate::glob::glob_pattern_to_regex;
use crate::path::normpath;
use crate::text::{decode_text_with_errors, ErrorMode};

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct FixtureFile {
    pub version: u32,
    pub cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
pub struct Case {
    pub name: String,
    pub op: Op,
    pub expected: Value,
}

/// Map from relative path to raw bytes, used to set up files in a tempdir.
pub type FileSet = HashMap<String, Vec<u8>>;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Op {
    // ── L1 path ops ────────────────────────────────────────────────────
    Normpath {
        input: String,
    },
    DetectEnvironment {
        platform: String,
        arch: String,
        release: String,
        #[serde(default)]
        env: HashMap<String, String>,
        #[serde(default)]
        files: Vec<String>,
        #[serde(default)]
        executables: HashMap<String, String>,
    },
    // ── L1 text decode ─────────────────────────────────────────────────
    Decode {
        encoding: String,
        mode: Mode,
        bytes: Vec<u8>,
    },
    // ── L1 glob ────────────────────────────────────────────────────────
    PatternToRegex {
        pattern: String,
        #[serde(rename = "caseSensitive")]
        case_sensitive: bool,
        inputs: Vec<String>,
    },
    // ── L1 file I/O ────────────────────────────────────────────────────
    ReadBytes {
        path: String,
        #[serde(default)]
        n: Option<u64>,
        #[serde(default)]
        files: FileSet,
    },
    ReadText {
        path: String,
        #[serde(default)]
        encoding: Option<String>,
        #[serde(default)]
        mode: Option<Mode>,
        #[serde(default)]
        files: FileSet,
    },
    ReadLines {
        path: String,
        #[serde(default)]
        encoding: Option<String>,
        #[serde(default)]
        mode: Option<Mode>,
        #[serde(default)]
        files: FileSet,
    },
    WriteBytes {
        path: String,
        data: Vec<u8>,
    },
    WriteText {
        path: String,
        data: String,
        #[serde(rename = "writeMode", default)]
        write_mode: Option<String>,
        #[serde(default)]
        encoding: Option<String>,
    },
    // ── L1 directory ops ─────────────────────────────────────────────────
    Stat {
        path: String,
        #[serde(default = "default_true")]
        follow_symlinks: bool,
        #[serde(default)]
        files: FileSet,
    },
    Iterdir {
        path: String,
        #[serde(default)]
        files: FileSet,
    },
    Glob {
        path: String,
        pattern: String,
        #[serde(rename = "caseSensitive", default = "default_true")]
        case_sensitive: bool,
        #[serde(default)]
        files: FileSet,
    },
    Mkdir {
        path: String,
        #[serde(default)]
        parents: bool,
        #[serde(rename = "existOk", default)]
        exist_ok: bool,
        #[serde(default)]
        files: FileSet,
    },
    Chdir {
        path: String,
        #[serde(default)]
        files: FileSet,
    },
    // ── L1 process ops ─────────────────────────────────────────────────
    Exec {
        command: String,
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        #[serde(default)]
        stdin: Option<Vec<u8>>,
        #[serde(default)]
        files: FileSet,
    },
    KillTree {
        command: String,
        args: Vec<String>,
        #[serde(default)]
        files: FileSet,
        #[serde(rename = "sleepMs")]
        sleep_ms: u64,
    },
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Strict,
    Replace,
    Ignore,
}

impl From<Mode> for ErrorMode {
    fn from(m: Mode) -> Self {
        match m {
            Mode::Strict => ErrorMode::Strict,
            Mode::Replace => ErrorMode::Replace,
            Mode::Ignore => ErrorMode::Ignore,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl CaseResult {
    pub fn ok(value: Value) -> Self {
        Self {
            result: Some(value),
            error: None,
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            result: None,
            error: Some(msg.into()),
        }
    }
}

/// Run a single golden case.  Returns `None` for cases that don't need a
/// tempdir (pure functions).  Returns `Some(result)` for file I/O cases
/// that need a tempdir — the caller must set up the tempdir first.
pub async fn run_case_async(case: &Case, temp_dir: Option<&PathBuf>) -> CaseResult {
    match &case.op {
        Op::Normpath { input } => CaseResult::ok(Value::String(normpath(input))),
        Op::DetectEnvironment {
            platform,
            arch,
            release,
            env,
            files,
            executables,
        } => {
            let file_set: std::collections::HashSet<String> = files.iter().cloned().collect();
            let deps = EnvironmentDeps {
                platform: platform.clone(),
                arch: arch.clone(),
                release: release.clone(),
                env: env.clone(),
                is_file: Box::new(move |p| file_set.contains(p)),
                find_executable: Box::new({
                    let executables = executables.clone();
                    move |name| executables.get(name).cloned()
                }),
            };
            let e = detect_environment(&deps);
            CaseResult::ok(serde_json::to_value(EnvOutput::from(e)).unwrap())
        }
        Op::Decode {
            encoding,
            mode,
            bytes,
        } => match decode_text_with_errors(bytes, encoding, (*mode).into()) {
            Ok(s) => CaseResult::ok(Value::String(s)),
            Err(_) => CaseResult::err("decode error"),
        },
        Op::PatternToRegex {
            pattern,
            case_sensitive,
            inputs,
        } => {
            let re = glob_pattern_to_regex(pattern, *case_sensitive);
            let matches: Vec<bool> = inputs.iter().map(|i| re.is_match(i)).collect();
            CaseResult::ok(serde_json::json!({
                "regex": re.as_str(),
                "matches": matches,
            }))
        }
        Op::ReadBytes { path, n, files: _ } => {
            let p = resolve(temp_dir, path);
            match file::read_bytes(&p, *n).await {
                Ok(data) => CaseResult::ok(serde_json::json!(data)),
                Err(e) => CaseResult::err(format!("{}", e)),
            }
        }
        Op::ReadText {
            path,
            encoding,
            mode,
            files: _,
        } => {
            let p = resolve(temp_dir, path);
            match file::read_text(&p, encoding.as_deref(), mode.map(|m| m.into())).await {
                Ok(s) => CaseResult::ok(Value::String(s)),
                Err(e) => CaseResult::err(format!("{}", e)),
            }
        }
        Op::ReadLines {
            path,
            encoding,
            mode,
            files: _,
        } => {
            let p = resolve(temp_dir, path);
            match file::read_lines(&p, encoding.as_deref(), mode.map(|m| m.into())).await {
                Ok(lines) => CaseResult::ok(serde_json::json!(lines)),
                Err(e) => CaseResult::err(format!("{}", e)),
            }
        }
        Op::WriteBytes { path, data } => {
            let p = resolve(temp_dir, path);
            match file::write_bytes(&p, data).await {
                Ok(n) => {
                    // Read back to verify what was written
                    match tokio::fs::read(&p).await {
                        Ok(content) => CaseResult::ok(serde_json::json!({
                            "written": n,
                            "content": content,
                        })),
                        Err(e) => CaseResult::err(format!("read-back error: {}", e)),
                    }
                }
                Err(e) => CaseResult::err(format!("{}", e)),
            }
        }
        Op::WriteText {
            path,
            data,
            write_mode,
            encoding,
        } => {
            let p = resolve(temp_dir, path);
            match file::write_text(&p, data, write_mode.as_deref(), encoding.as_deref()).await {
                Ok(n) => match tokio::fs::read(&p).await {
                    Ok(content) => CaseResult::ok(serde_json::json!({
                        "written": n,
                        "content": content,
                    })),
                    Err(e) => CaseResult::err(format!("read-back error: {}", e)),
                },
                Err(e) => CaseResult::err(format!("{}", e)),
            }
        }
        Op::Stat {
            path,
            follow_symlinks,
            files: _,
        } => {
            let p = resolve(temp_dir, path);
            match crate::dir::stat(&p, *follow_symlinks).await {
                Ok(s) => CaseResult::ok(serde_json::json!({
                    "isDir": s.is_dir(),
                    "size": if s.is_dir() { 0 } else { s.st_size },
                })),
                Err(e) => CaseResult::err(canonical_io_error(&e)),
            }
        }
        Op::Iterdir { path, files: _ } => {
            let p = resolve(temp_dir, path);
            match crate::dir::iterdir(&p).await {
                Ok(mut entries) => {
                    entries = entries
                        .into_iter()
                        .map(|e| relativize(&e, temp_dir))
                        .collect();
                    entries.sort();
                    CaseResult::ok(serde_json::to_value(entries).unwrap())
                }
                Err(e) => CaseResult::err(canonical_io_error(&e)),
            }
        }
        Op::Glob {
            path,
            pattern,
            case_sensitive,
            files: _,
        } => {
            let p = resolve(temp_dir, path);
            match crate::dir::glob(&p, pattern, *case_sensitive).await {
                Ok(mut matches) => {
                    matches = matches
                        .into_iter()
                        .map(|m| relativize(&m, temp_dir))
                        .collect();
                    matches.sort();
                    CaseResult::ok(serde_json::to_value(matches).unwrap())
                }
                Err(e) => CaseResult::err(canonical_io_error(&e)),
            }
        }
        Op::Mkdir {
            path,
            parents,
            exist_ok,
            files: _,
        } => {
            let p = resolve(temp_dir, path);
            match crate::dir::mkdir(&p, *parents, *exist_ok).await {
                Ok(()) => match tokio::fs::metadata(&p).await {
                    Ok(m) if m.is_dir() => CaseResult::ok(serde_json::json!({ "created": true })),
                    Ok(_) => CaseResult::err("created path is not a directory"),
                    Err(e) => CaseResult::err(canonical_io_error(&e)),
                },
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("already exists but is not a directory") {
                        // Strip tempdir prefix from the error message
                        let rel_msg = if let Some(td) = temp_dir {
                            let base = normpath(td);
                            msg.replace(&format!("{}/", base), "")
                        } else {
                            msg
                        };
                        CaseResult::err(rel_msg)
                    } else if e.kind() == std::io::ErrorKind::AlreadyExists
                        || msg.contains("already exists")
                    {
                        CaseResult::err("already exists")
                    } else {
                        CaseResult::err(canonical_io_error(&e))
                    }
                }
            }
        }
        Op::Chdir { path, files: _ } => {
            let cwd = match temp_dir {
                Some(td) => td.clone(),
                None => match std::env::current_dir() {
                    Ok(d) => d,
                    Err(e) => return CaseResult::err(format!("cwd error: {}", e)),
                },
            };
            let mut kaos =
                crate::kaos::Kaos::new(crate::environment::detect_environment_from_node(), cwd);
            match kaos.chdir(path).await {
                Ok(()) => CaseResult::ok(serde_json::json!({ "changed": true })),
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("Not a directory") {
                        CaseResult::err("not a directory".to_string())
                    } else {
                        CaseResult::err(canonical_io_error(&e))
                    }
                }
            }
        }
        Op::Exec {
            command,
            args,
            env,
            stdin,
            files: _,
        } => {
            let cwd = match temp_dir {
                Some(td) => td.clone(),
                None => match std::env::current_dir() {
                    Ok(d) => d,
                    Err(e) => return CaseResult::err(format!("cwd error: {}", e)),
                },
            };
            let kaos =
                crate::kaos::Kaos::new(crate::environment::detect_environment_from_node(), cwd);
            let mut all_args = vec![command.as_str()];
            all_args.extend(args.iter().map(|s| s.as_str()));
            let env_pairs: Vec<(&str, &str)> =
                env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            match kaos.exec_with_env(&all_args, &env_pairs).await {
                Ok(proc) => {
                    if let Some(input) = stdin {
                        if let Err(e) = proc.write_stdin(input).await {
                            return CaseResult::err(format!("stdin write error: {}", e));
                        }
                        if let Err(e) = proc.close_stdin().await {
                            return CaseResult::err(format!("stdin close error: {}", e));
                        }
                    }
                    let code = proc.wait().await;
                    let stdout = proc.stdout().await;
                    let stderr = proc.stderr().await;
                    CaseResult::ok(serde_json::json!({
                        "stdout": stdout,
                        "stderr": stderr,
                        "exitCode": code,
                    }))
                }
                Err(e) => CaseResult::err(format!("{}", e)),
            }
        }
        Op::KillTree {
            command,
            args,
            files: _,
            sleep_ms,
        } => {
            #[cfg(unix)]
            {
                let cwd = match temp_dir {
                    Some(td) => td.clone(),
                    None => match std::env::current_dir() {
                        Ok(d) => d,
                        Err(e) => return CaseResult::err(format!("cwd error: {}", e)),
                    },
                };
                let kaos =
                    crate::kaos::Kaos::new(crate::environment::detect_environment_from_node(), cwd);
                let mut all_args = vec![command.as_str()];
                all_args.extend(args.iter().map(|s| s.as_str()));
                let proc = match kaos.exec(&all_args).await {
                    Ok(p) => p,
                    Err(e) => return CaseResult::err(format!("{}", e)),
                };
                tokio::time::sleep(std::time::Duration::from_millis(*sleep_ms)).await;
                if let Err(e) = proc.kill(None).await {
                    return CaseResult::err(format!("{}", e));
                }
                let _ = proc.wait().await;

                let marker = match temp_dir {
                    Some(td) => td.join("pids.txt"),
                    None => {
                        return CaseResult::err(
                            "kill_tree requires a tempdir for the pid marker file".to_string(),
                        )
                    }
                };
                let content = tokio::fs::read_to_string(&marker).await.unwrap_or_default();
                for pid_str in content.split_whitespace() {
                    let pid: i32 = match pid_str.parse() {
                        Ok(p) => p,
                        Err(_) => {
                            return CaseResult::err(format!("bad pid in marker: {}", pid_str))
                        }
                    };
                    let alive = std::process::Command::new("kill")
                        .args(["-0", &pid.to_string()])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    if alive {
                        return CaseResult::err(format!("pid {} still alive", pid));
                    }
                }
                CaseResult::ok(serde_json::json!({ "killed": true }))
            }
            #[cfg(not(unix))]
            {
                let _ = (command, args, sleep_ms);
                CaseResult::err("kill_tree is POSIX-only".to_string())
            }
        }
    }
}

fn canonical_io_error(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => "not found".to_string(),
        std::io::ErrorKind::AlreadyExists => "already exists".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => e.to_string(),
    }
}

fn relativize(path: &str, temp_dir: Option<&PathBuf>) -> String {
    let p = normpath(path);
    if let Some(td) = temp_dir {
        let base = normpath(td);
        if let Some(stripped) = p.strip_prefix(&format!("{}/", base)) {
            return stripped.to_string();
        }
    }
    p
}

fn resolve(temp_dir: Option<&PathBuf>, path: &str) -> String {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        path.to_string()
    } else if let Some(dir) = temp_dir {
        dir.join(p).to_string_lossy().to_string()
    } else {
        path.to_string()
    }
}

/// Set up files in the given tempdir from a FileSet.
pub async fn setup_files(
    temp_dir: &Path,
    files: &FileSet,
) -> Result<(), Box<dyn std::error::Error>> {
    for (rel_path, data) in files {
        let full = temp_dir.join(rel_path);
        if rel_path.ends_with('/') {
            tokio::fs::create_dir_all(&full).await?;
        } else {
            if let Some(parent) = full.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&full, data).await?;
        }
    }
    Ok(())
}

/// Return the list of file-based ops that need a tempdir.
pub fn needs_tempdir(op: &Op) -> bool {
    matches!(
        op,
        Op::ReadBytes { .. }
            | Op::ReadText { .. }
            | Op::ReadLines { .. }
            | Op::WriteBytes { .. }
            | Op::WriteText { .. }
            | Op::Stat { .. }
            | Op::Iterdir { .. }
            | Op::Glob { .. }
            | Op::Mkdir { .. }
            | Op::Chdir { .. }
            | Op::Exec { .. }
            | Op::KillTree { .. }
    )
}

/// Extract the FileSet from a file-based op (empty for non-file ops).
pub fn files_for_op(op: &Op) -> FileSet {
    match op {
        Op::ReadBytes { files, .. } => files.clone(),
        Op::ReadText { files, .. } => files.clone(),
        Op::ReadLines { files, .. } => files.clone(),
        Op::Stat { files, .. } => files.clone(),
        Op::Iterdir { files, .. } => files.clone(),
        Op::Glob { files, .. } => files.clone(),
        Op::Mkdir { files, .. } => files.clone(),
        Op::Chdir { files, .. } => files.clone(),
        Op::Exec { files, .. } => files.clone(),
        Op::KillTree { files, .. } => files.clone(),
        _ => HashMap::new(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvOutput {
    os_kind: String,
    os_arch: String,
    os_version: String,
    shell_name: String,
    shell_path: String,
}

impl From<Environment> for EnvOutput {
    fn from(e: Environment) -> Self {
        Self {
            os_kind: e.os_kind,
            os_arch: e.os_arch,
            os_version: e.os_version,
            shell_name: e.shell_name,
            shell_path: e.shell_path,
        }
    }
}

/// Create a temporary directory unique to this process run.
fn make_temp_dir() -> Result<PathBuf, std::io::Error> {
    let dir = std::env::temp_dir().join(format!("kaos-golden-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Run all cases in a fixture file, handling tempdir creation for file I/O ops.
pub async fn run_fixture_file_async(
    path: &str,
) -> Result<HashMap<String, CaseResult>, Box<dyn std::error::Error>> {
    let content = tokio::fs::read_to_string(path).await?;
    let fixture: FixtureFile = serde_json::from_str(&content)?;

    // Collect all unique files across all cases to create a single tempdir
    let mut all_files: FileSet = HashMap::new();
    for case in &fixture.cases {
        for (k, v) in files_for_op(&case.op) {
            all_files.insert(k, v);
        }
    }

    let temp_dir = if all_files.is_empty() {
        None
    } else {
        let dir = make_temp_dir()?;
        setup_files(&dir, &all_files).await?;
        Some(dir)
    };

    let mut out = HashMap::new();
    for case in &fixture.cases {
        let td = needs_tempdir(&case.op)
            .then_some(temp_dir.as_ref())
            .flatten();
        out.insert(case.name.clone(), run_case_async(case, td).await);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn exec_runs_echo() {
        let case = Case {
            name: "exec echo".to_string(),
            op: Op::Exec {
                command: "/bin/echo".to_string(),
                args: vec!["-n".to_string(), "hello".to_string()],
                env: HashMap::new(),
                stdin: None,
                files: HashMap::new(),
            },
            expected: serde_json::json!({
                "result": {
                    "stdout": [104, 101, 108, 108, 111],
                    "stderr": [],
                    "exitCode": 0,
                }
            }),
        };
        let actual = run_case_async(&case, None).await;
        let expected_inner = case.expected.get("result").unwrap_or(&case.expected);
        assert_eq!(actual.result.unwrap(), *expected_inner);
        assert!(actual.error.is_none());
    }
}
