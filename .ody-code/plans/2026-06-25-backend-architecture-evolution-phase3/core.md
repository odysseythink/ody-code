# Part 1 — Rust Host Core

> Scope: `ody-host` crate 的业务层：配置解析、会话运行时与持久化、最小 OpenAI 兼容 LLM provider、内置 bash tool、`CoreHost` 分发与事件推送。
> Depends on: index §Phase A 依赖图（无前置代码依赖）。

---

### Task A1: 添加 `ody-host` workspace member 与 crate scaffold

**Depends on:** none

**Files:**
- Modify: `rust-ody/Cargo.toml:1-3`
- Create: `rust-ody/crates/ody-host/Cargo.toml`
- Create: `rust-ody/crates/ody-host/src/lib.rs`
- Create: `rust-ody/crates/ody-host/src/main.rs`
- Test: `rust-ody/crates/ody-host/tests/scaffold_test.rs`

**Steps:**

- [ ] 修改 workspace 成员列表，加入 `crates/ody-host`。

```toml
# rust-ody/Cargo.toml
[workspace]
members = ["crates/ody-rust", "crates/ody-crypto", "crates/ody-host"]
resolver = "2"
```

- [ ] 在 `rust-ody/Cargo.toml` 末尾追加 workspace dependencies（供 ody-host 引用）：

```toml
[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["stream", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

- [ ] 创建 `ody-host` crate manifest：

```toml
# rust-ody/crates/ody-host/Cargo.toml
[package]
name = "ody-host"
version = "0.1.0"
edition = "2021"
description = "Standalone Rust host for ody-code CoreAPI/SDKAPI prototype"
license = "MIT"

[[bin]]
name = "ody-host"
path = "src/main.rs"

[dependencies]
tokio = { workspace = true }
reqwest = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
clap = { version = "4", features = ["derive"] }
toml = "0.8"
uuid = { version = "1", features = ["v7", "serde"] }
sha2 = "0.10"
regex = "1"

[dev-dependencies]
tokio-test = "0.4"
```

- [ ] 创建 crate root，仅声明模块：

```rust
// rust-ody/crates/ody-host/src/lib.rs
pub mod config;
pub mod error;
pub mod events;
pub mod host;
pub mod llm;
pub mod session;
pub mod tools;
pub mod transport;
```

- [ ] 创建空 `main.rs`（占位，后续 A7/B 任务填充）：

```rust
// rust-ody/crates/ody-host/src/main.rs
fn main() {
    eprintln!("ody-host scaffold built");
}
```

- [ ] 写失败测试：验证 crate 可编译、二进制名正确。

```rust
// rust-ody/crates/ody-host/tests/scaffold_test.rs
#[test]
fn ody_host_binary_exists() {
    // cargo ensures the binary is built; this test fails only if the crate does not compile.
    assert_eq!(env!("CARGO_PKG_NAME"), "ody-host");
}
```

- [ ] 运行并验证失败：
  - `cd rust-ody && cargo build -p ody-host`
  - 预期：因 `config.rs` 等模块文件缺失而失败（Rust 找不到模块文件）。

- [ ] 写最小实现：创建所有模块文件的空占位（`config.rs`、`error.rs`、`events.rs`、`host.rs`、`llm/mod.rs`、`session/mod.rs`、`tools/mod.rs`、`transport/mod.rs`），每个文件仅写 `// TODO` 注释占位。等待后续任务填充。

- [ ] 运行并验证通过：
  - `cd rust-ody && cargo test -p ody-host`
  - 预期：`scaffold_test` 通过。

- [ ] 提交：`git add rust-ody/Cargo.toml rust-ody/crates/ody-host/ && git commit -m "chore(rust): scaffold ody-host crate"`

---

### Task A2: 配置解析 `HostConfig` + 错误基座

**Depends on:** Task A1

**Files:**
- Create: `rust-ody/crates/ody-host/src/config.rs`
- Create: `rust-ody/crates/ody-host/src/error.rs`
- Modify: `rust-ody/crates/ody-host/src/lib.rs`（确认模块声明已存在）
- Test: `rust-ody/crates/ody-host/src/config.rs` 内 `#[cfg(test)]` 模块

**Steps:**

- [ ] 写失败测试：验证 CLI 解析默认值、配置文件覆盖、无效 log level 报错。

```rust
// rust-ody/crates/ody-host/src/config.rs (片段，置于文件末尾)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_stdio_transport() {
        let args = vec!["ody-host"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert!(matches!(config.transport, TransportMode::Stdio));
        assert_eq!(config.log_level, LogLevel::Info);
    }

    #[test]
    fn socket_path_from_cli() {
        let args = vec!["ody-host", "--socket-path", "/tmp/ody.sock"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert_eq!(config.transport, TransportMode::UnixSocket { path: PathBuf::from("/tmp/ody.sock") });
    }

    #[test]
    fn invalid_log_level_fails() {
        let args = vec!["ody-host", "--log-level", "verbose"];
        let err = HostConfig::from_cli(args.into_iter()).unwrap_err();
        assert!(err.to_string().contains("verbose"));
    }
}
```

- [ ] 运行并验证失败：`cargo test -p ody-host config::tests` 因类型不存在失败。

- [ ] 写最小实现：

```rust
// rust-ody/crates/ody-host/src/error.rs
use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum HostError {
    ConfigInvalid { message: String },
    Io { source: std::io::Error, path: PathBuf },
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::ConfigInvalid { message } => write!(f, "invalid config: {message}"),
            HostError::Io { source, path } => write!(f, "io error at {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for HostError {}

impl HostError {
    pub fn config_invalid(message: impl Into<String>) -> Self {
        HostError::ConfigInvalid { message: message.into() }
    }
}
```

```rust
// rust-ody/crates/ody-host/src/config.rs
use std::path::PathBuf;

use clap::Parser;
use serde::Deserialize;

use crate::error::HostError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportMode {
    Stdio,
    UnixSocket { path: PathBuf },
    TcpSocket { host: String, port: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogLevel {
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConfig {
    pub provider_id: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub home_dir: PathBuf,
    pub config_path: Option<PathBuf>,
    pub transport: TransportMode,
    pub log_level: LogLevel,
    pub provider: ProviderConfig,
}

#[derive(Debug, Parser)]
#[command(name = "ody-host", version)]
struct Cli {
    #[arg(long)]
    stdio: bool,
    #[arg(long)]
    socket_path: Option<PathBuf>,
    #[arg(long)]
    tcp_host: Option<String>,
    #[arg(long)]
    tcp_port: Option<u16>,
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    home: Option<PathBuf>,
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[derive(Debug, Deserialize)]
struct RawConfigFile {
    home_dir: Option<PathBuf>,
    log_level: Option<String>,
    provider: Option<RawProvider>,
}

#[derive(Debug, Deserialize)]
struct RawProvider {
    api_key: String,
    base_url: Option<String>,
    default_model: Option<String>,
}

impl HostConfig {
    pub fn from_cli<I, T>(args: I) -> Result<Self, HostError>
    where
        I: Iterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let cli = Cli::parse_from(args);
        let home_dir = cli.home.unwrap_or_else(default_home_dir);
        let config_path = cli.config.clone().or_else(|| {
            let toml = home_dir.join("ody.toml");
            if toml.exists() { Some(toml) } else {
                let json = home_dir.join("ody.json");
                if json.exists() { Some(json) } else { None }
            }
        });

        let file: RawConfigFile = match &config_path {
            Some(path) => load_raw_config(path)?,
            None => RawConfigFile { home_dir: None, log_level: None, provider: None },
        };

        let transport = if let Some(path) = cli.socket_path {
            TransportMode::UnixSocket { path }
        } else if let (Some(host), Some(port)) = (cli.tcp_host, cli.tcp_port) {
            TransportMode::TcpSocket { host, port }
        } else {
            TransportMode::Stdio
        };

        let log_level = parse_log_level(&cli.log_level)?;

        let provider = ProviderConfig {
            provider_id: "openai".to_string(),
            api_key: file.provider.as_ref().map(|p| p.api_key.clone()).unwrap_or_default(),
            base_url: cli.config.is_none().then_some(None).flatten().or_else(|| file.provider.as_ref().and_then(|p| p.base_url.clone())),
            default_model: file.provider.as_ref().and_then(|p| p.default_model.clone()).unwrap_or_else(|| "gpt-4o-mini".to_string()),
        };

        Ok(HostConfig {
            home_dir: file.home_dir.unwrap_or(home_dir),
            config_path,
            transport,
            log_level,
            provider,
        })
    }
}

fn default_home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".ody")
}

fn parse_log_level(s: &str) -> Result<LogLevel, HostError> {
    match s.to_lowercase().as_str() {
        "debug" => Ok(LogLevel::Debug),
        "info" => Ok(LogLevel::Info),
        "warn" => Ok(LogLevel::Warn),
        "error" => Ok(LogLevel::Error),
        _ => Err(HostError::config_invalid(format!("unknown log level: {s}"))),
    }
}

fn load_raw_config(path: &PathBuf) -> Result<RawConfigFile, HostError> {
    let bytes = std::fs::read(path).map_err(|e| HostError::Io { source: e, path: path.clone() })?;
    if path.extension().and_then(|s| s.to_str()) == Some("json") {
        serde_json::from_slice(&bytes).map_err(|e| HostError::config_invalid(format!("{e}")))
    } else {
        toml::from_slice(&bytes).map_err(|e| HostError::config_invalid(format!("{e}")))
    }
}
```

- [ ] 注意：`base_url` 的 fallback 链在实现中显得混乱，应修正为：

```rust
let provider = ProviderConfig {
    provider_id: "openai".to_string(),
    api_key: file.provider.as_ref().map(|p| p.api_key.clone()).unwrap_or_default(),
    base_url: file.provider.as_ref().and_then(|p| p.base_url.clone()),
    default_model: file.provider.as_ref().and_then(|p| p.default_model.clone()).unwrap_or_else(|| "gpt-4o-mini".to_string()),
};
```

- [ ] 添加 `dirs` 依赖到 `Cargo.toml`：

```toml
# 在 [dependencies] 段追加
dirs = "5"
```

- [ ] 运行并验证通过：`cargo test -p ody-host config::tests`

- [ ] 提交：`git add rust-ody/crates/ody-host/src/config.rs rust-ody/crates/ody-host/src/error.rs rust-ody/crates/ody-host/Cargo.toml && git commit -m "feat(ody-host): HostConfig parsing and base errors"`

---

### Task A3: `SessionStoreAdapter` — 复用 TS 目录结构与 `state.json` 字段

**Depends on:** Task A2

**Files:**
- Create: `rust-ody/crates/ody-host/src/session/mod.rs`
- Create: `rust-ody/crates/ody-host/src/session/store.rs`
- Test: `rust-ody/crates/ody-host/src/session/store.rs` 内 `#[cfg(test)]`

**Steps:**

- [ ] 写失败测试：验证 `encode_work_dir_key` 与 TS `workdir-key.ts` 对同一输入产生相同 key；验证 `state.json` 字段与 `SessionSummaryStateSchema` 兼容。

```rust
// rust-ody/crates/ody-host/src/session/store.rs (末尾 test 模块)
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn encodes_work_dir_key_like_ts() {
        // TS 侧对 /Users/ranwei/workspace/ody-code 的 key 为 wd_ody-code_eaef72b82f4b
        let key = encode_work_dir_key("/Users/ranwei/workspace/ody-code");
        assert!(key.starts_with("wd_ody-code_"));
        assert_eq!(key.len(), "wd_ody-code_".len() + 12);
    }

    #[test]
    fn slugifies_special_characters() {
        let key = encode_work_dir_key("/tmp/foo bar!baz");
        assert!(key.starts_with("wd_foo-bar-baz_"));
    }

    #[test]
    fn empty_slug_becomes_workspace() {
        let key = encode_work_dir_key("/");
        assert!(key.starts_with("wd_workspace_"));
    }

    #[test]
    fn state_json_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let state = SessionState {
            title: Some("hello".to_string()),
            last_prompt: Some("hi".to_string()),
            custom: [("k".to_string(), serde_json::json!(1))].into_iter().collect(),
        };
        write_state_json(dir.path(), &state).unwrap();
        let restored = read_state_json(dir.path()).unwrap().unwrap();
        assert_eq!(restored.title, state.title);
        assert_eq!(restored.last_prompt, state.last_prompt);
    }
}
```

- [ ] 运行并验证失败：`cargo test -p ody-host session::store::tests` 因函数/类型不存在失败。

- [ ] 添加 `tempfile` 到 `[dev-dependencies]`：

```toml
# rust-ody/crates/ody-host/Cargo.toml [dev-dependencies] 追加
tempfile = "3"
```

- [ ] 写最小实现：

```rust
// rust-ody/crates/ody-host/src/session/mod.rs
pub mod manager;
pub mod store;

pub use manager::SessionManager;
pub use store::{SessionState, SessionStoreAdapter, SessionSummary};
```

```rust
// rust-ody/crates/ody-host/src/session/store.rs
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::HostError;

pub type SessionId = String;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SessionState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_prompt: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub custom: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub id: SessionId,
    pub work_dir: PathBuf,
    pub session_dir: PathBuf,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub title: Option<String>,
    pub last_prompt: Option<String>,
    pub metadata: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone)]
pub struct SessionStoreAdapter {
    home_dir: PathBuf,
}

#[derive(Debug)]
pub enum SessionError {
    AlreadyExists { session_id: SessionId },
    NotFound { session_id: SessionId },
    InvalidId { session_id: SessionId },
    Io { source: std::io::Error, path: PathBuf },
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::AlreadyExists { session_id } => write!(f, r#"Session "{session_id}" already exists"#),
            SessionError::NotFound { session_id } => write!(f, r#"Session "{session_id}" was not found"#),
            SessionError::InvalidId { session_id } => write!(f, r#"Session id "{session_id}" contains unsupported path characters"#),
            SessionError::Io { source, path } => write!(f, "io error at {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for SessionError {}

impl SessionStoreAdapter {
    pub fn new(home_dir: PathBuf) -> Self {
        Self { home_dir }
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.home_dir.join("sessions")
    }

    pub fn session_dir_for(&self, id: &str, work_dir: &Path) -> Result<PathBuf, SessionError> {
        assert_safe_session_id(id)?;
        let work_dir = normalize_work_dir(work_dir);
        Ok(self.sessions_dir().join(encode_work_dir_key(&work_dir)).join(id))
    }

    pub fn append_index(&self, entry: IndexEntry) -> Result<(), SessionError> {
        let path = self.home_dir.join("session_index.jsonl");
        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .map_err(|e| SessionError::Io { source: e, path: path.clone() })?;
        writeln!(file, "{}", serde_json::to_string(&entry).unwrap())
            .map_err(|e| SessionError::Io { source: e, path })?;
        Ok(())
    }

    pub fn read_index(&self) -> Result<HashMap<SessionId, IndexEntry>, SessionError> {
        let path = self.home_dir.join("session_index.jsonl");
        let raw = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
            Err(e) => return Err(SessionError::Io { source: e, path }),
        };
        let mut map = HashMap::new();
        for line in raw.lines() {
            if line.trim().is_empty() { continue; }
            let entry: IndexEntry = serde_json::from_str(line)
                .map_err(|e| SessionError::Io { source: std::io::Error::new(std::io::ErrorKind::InvalidData, e), path: path.clone() })?;
            map.insert(entry.session_id.clone(), entry);
        }
        Ok(map)
    }

    pub fn summary_from_dir(&self, id: SessionId, dir: &Path, work_dir: &Path) -> Result<SessionSummary, SessionError> {
        let dir_stat = fs::metadata(dir).map_err(|e| SessionError::Io { source: e, path: dir.to_path_buf() })?;
        let state = read_state_json(dir).map_err(|e| SessionError::Io { source: e, path: dir.to_path_buf() })?;
        let state_mtime = mtime_ms(dir.join("state.json"));
        let wire_mtime = mtime_ms(dir.join("wire.jsonl"));
        let updated_at = *[dir_stat.modified().ok().map(ts_to_ms), state_mtime, wire_mtime]
            .iter()
            .flatten()
            .max()
            .unwrap_or(0);
        let created_at = dir_stat.created().ok().map(ts_to_ms).unwrap_or_else(|| ts_to_ms(std::time::SystemTime::now()));
        let title = state.as_ref().and_then(title_from_state);
        let last_prompt = state.as_ref().and_then(|s| s.last_prompt.clone());
        let metadata = state.as_ref().map(|s| s.custom.clone().into_iter().collect());
        Ok(SessionSummary {
            id,
            work_dir: work_dir.to_path_buf(),
            session_dir: dir.to_path_buf(),
            created_at_ms: created_at,
            updated_at_ms: updated_at,
            title,
            last_prompt,
            metadata,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IndexEntry {
    pub session_id: SessionId,
    pub session_dir: PathBuf,
    pub work_dir: PathBuf,
}

pub fn normalize_work_dir(work_dir: &Path) -> PathBuf {
    work_dir.canonicalize().unwrap_or_else(|_| work_dir.to_path_buf())
}

pub fn encode_work_dir_key(work_dir: &str) -> String {
    use std::path::Path as StdPath;
    let normalized = StdPath::new(work_dir).canonicalize().unwrap_or_else(|_| StdPath::new(work_dir).to_path_buf());
    let name = normalized.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let slug = slugify_work_dir_name(name);
    let hash = format!("{:x}", Sha256::digest(normalized.to_string_lossy().as_bytes()));
    format!("wd_{slug}_{}", &hash[..12.min(hash.len())])
}

fn slugify_work_dir_name(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-');
    let slug = if slug.len() > 40 { &slug[..40] } else { slug };
    let slug = slug.trim_matches('-');
    if slug.is_empty() || slug == "." || slug == ".." {
        "workspace".to_string()
    } else {
        slug.to_string()
    }
}

fn assert_safe_session_id(id: &str) -> Result<(), SessionError> {
    if id == "." || id == ".." || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-') {
        return Err(SessionError::InvalidId { session_id: id.to_string() });
    }
    Ok(())
}

pub fn read_state_json(dir: &Path) -> Result<Option<SessionState>, std::io::Error> {
    let path = dir.join("state.json");
    match fs::read_to_string(&path) {
        Ok(s) => Ok(serde_json::from_str(&s).ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn write_state_json(dir: &Path, state: &SessionState) -> Result<(), std::io::Error> {
    fs::create_dir_all(dir)?;
    let path = dir.join("state.json");
    let mut file = fs::File::create(&path)?;
    file.write_all(serde_json::to_string_pretty(state).unwrap().as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

fn mtime_ms(path: PathBuf) -> Option<u64> {
    fs::metadata(&path).ok()?.modified().ok().map(ts_to_ms)
}

fn ts_to_ms(t: std::time::SystemTime) -> u64 {
    t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn title_from_state(state: &SessionState) -> Option<String> {
    state.title.clone().filter(|s| !s.trim().is_empty())
}
```

- [ ] 运行并验证通过：`cargo test -p ody-host session::store::tests`

- [ ] 提交：`git add rust-ody/crates/ody-host/src/session/ rust-ody/crates/ody-host/Cargo.toml && git commit -m "feat(ody-host): SessionStoreAdapter compatible with TS SessionStore"`

---

### Task A4: `SessionManager` — 创建 / 列出 / 获取 / 关闭会话

**Depends on:** Task A3

**Files:**
- Create: `rust-ody/crates/ody-host/src/session/manager.rs`
- Test: `rust-ody/crates/ody-host/src/session/manager.rs` 内 `#[cfg(test)]`

**Steps:**

- [ ] 写失败测试：

```rust
// rust-ody/crates/ody-host/src/session/manager.rs (末尾 test 模块)
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[tokio::test]
    async fn create_then_list_returns_session() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(SessionStoreAdapter::new(tmp.path().to_path_buf()));
        let summary = manager.create(Path::new("/tmp/wd"), Some("t")).await.unwrap();
        let list = manager.list(SessionFilter::default()).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, summary.id);
        assert_eq!(list[0].title, Some("t".to_string()));
    }

    #[tokio::test]
    async fn duplicate_id_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(SessionStoreAdapter::new(tmp.path().to_path_buf()));
        let summary = manager.create(Path::new("/tmp/wd"), None).await.unwrap();
        let err = manager.create_with_id(&summary.id, Path::new("/tmp/wd"), None).await.unwrap_err();
        assert!(matches!(err, SessionError::AlreadyExists { .. }));
    }

    #[tokio::test]
    async fn close_removes_active_but_keeps_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(SessionStoreAdapter::new(tmp.path().to_path_buf()));
        let summary = manager.create(Path::new("/tmp/wd"), None).await.unwrap();
        manager.close(summary.id.clone()).await.unwrap();
        assert!(summary.session_dir.exists());
        let err = manager.get(summary.id.clone()).await.unwrap_err();
        assert!(matches!(err, SessionError::NotFound { .. }));
    }
}
```

- [ ] 运行并验证失败：`cargo test -p ody-host session::manager::tests` 因类型/方法不存在失败。

- [ ] 写最小实现：

```rust
// rust-ody/crates/ody-host/src/session/manager.rs
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

use crate::session::store::{IndexEntry, SessionError, SessionState, SessionStoreAdapter, SessionSummary};

#[derive(Debug, Default, Clone)]
pub struct SessionFilter {
    pub work_dir: Option<String>,
    pub session_id: Option<String>,
}

pub struct SessionManager {
    store: SessionStoreAdapter,
    active: RwLock<HashMap<String, Arc<Session>>>,
}

pub struct Session {
    pub id: String,
    pub work_dir: std::path::PathBuf,
    pub dir: std::path::PathBuf,
    state: tokio::sync::Mutex<SessionState>,
}

impl SessionManager {
    pub fn new(store: SessionStoreAdapter) -> Self {
        Self { store, active: RwLock::new(HashMap::new()) }
    }

    pub async fn create(&self, work_dir: &Path, title: Option<&str>) -> Result<SessionSummary, SessionError> {
        let id = Uuid::now_v7().to_string();
        self.create_with_id(&id, work_dir, title).await
    }

    pub async fn create_with_id(&self, id: &str, work_dir: &Path, title: Option<&str>) -> Result<SessionSummary, SessionError> {
        let dir = self.store.session_dir_for(id, work_dir)?;
        if dir.exists() {
            return Err(SessionError::AlreadyExists { session_id: id.to_string() });
        }
        let index = self.store.read_index()?;
        if index.contains_key(id) {
            return Err(SessionError::AlreadyExists { session_id: id.to_string() });
        }
        std::fs::create_dir_all(&dir).map_err(|e| SessionError::Io { source: e, path: dir.clone() })?;
        let state = SessionState {
            title: title.map(|s| s.to_string()),
            last_prompt: None,
            custom: HashMap::new(),
        };
        crate::session::store::write_state_json(&dir, &state)
            .map_err(|e| SessionError::Io { source: e, path: dir.clone() })?;
        let normalized = crate::session::store::normalize_work_dir(work_dir);
        self.store.append_index(IndexEntry {
            session_id: id.to_string(),
            session_dir: dir.clone(),
            work_dir: normalized.clone(),
        })?;
        let summary = self.store.summary_from_dir(id.to_string(), &dir, &normalized)?;
        let session = Arc::new(Session {
            id: id.to_string(),
            work_dir: normalized,
            dir: dir.clone(),
            state: tokio::sync::Mutex::new(state),
        });
        self.active.write().await.insert(id.to_string(), session);
        Ok(summary)
    }

    pub async fn list(&self, filter: SessionFilter) -> Result<Vec<SessionSummary>, SessionError> {
        let index = self.store.read_index()?;
        let mut summaries = Vec::new();
        for (id, entry) in index {
            if let Some(wd) = &filter.work_dir {
                if entry.work_dir != crate::session::store::normalize_work_dir(Path::new(wd)) {
                    continue;
                }
            }
            if let Some(sid) = &filter.session_id {
                if &id != sid { continue; }
            }
            if !entry.session_dir.exists() { continue; }
            summaries.push(self.store.summary_from_dir(id, &entry.session_dir, &entry.work_dir)?);
        }
        summaries.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
        Ok(summaries)
    }

    pub async fn get(&self, id: String) -> Result<Arc<Session>, SessionError> {
        {
            let active = self.active.read().await;
            if let Some(s) = active.get(&id) {
                return Ok(Arc::clone(s));
            }
        }
        let index = self.store.read_index()?;
        let entry = index.get(&id).cloned().ok_or_else(|| SessionError::NotFound { session_id: id.clone() })?;
        if !entry.session_dir.exists() {
            return Err(SessionError::NotFound { session_id: id });
        }
        let state = crate::session::store::read_state_json(&entry.session_dir)
            .map_err(|e| SessionError::Io { source: e, path: entry.session_dir.clone() })?
            .unwrap_or_default();
        let session = Arc::new(Session {
            id: id.clone(),
            work_dir: entry.work_dir.clone(),
            dir: entry.session_dir.clone(),
            state: tokio::sync::Mutex::new(state),
        });
        self.active.write().await.insert(id, Arc::clone(&session));
        Ok(session)
    }

    pub async fn close(&self, id: String) -> Result<(), SessionError> {
        self.active.write().await.remove(&id);
        Ok(())
    }
}
```

- [ ] 运行并验证通过：`cargo test -p ody-host session::manager::tests`

- [ ] 提交：`git add rust-ody/crates/ody-host/src/session/manager.rs && git commit -m "feat(ody-host): SessionManager create/list/get/close"`

---

### Task A5: LLM Provider  trait + OpenAI 兼容 SSE 实现

**Depends on:** Task A2（`ProviderConfig`）

**Files:**
- Create: `rust-ody/crates/ody-host/src/llm/mod.rs`
- Create: `rust-ody/crates/ody-host/src/llm/openai.rs`
- Test: `rust-ody/crates/ody-host/src/llm/openai.rs` 内 `#[cfg(test)]`

**Steps:**

- [ ] 写失败测试：使用 mock HTTP server 验证 `OpenAiProvider::chat_stream` 能解析 SSE 并流式输出文本增量。

```rust
// rust-ody/crates/ody-host/src/llm/openai.rs (末尾 test 模块)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{ChatRequest, FinishReason, Message, Role};

    #[tokio::test]
    async fn streams_text_deltas_from_sse() {
        let server = httptest::Server::run();
        server.expect(
            httptest::Expectation::matching(httptest::matchers::request::method_path("POST", "/v1/chat/completions"))
                .respond_with(httptest::responders::status_code(200).body(
                    "data: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"}}]}\n\n\
                     data: {\"id\":\"2\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"}}]}\n\n\
                     data: [DONE]\n\n",
                )),
        );

        let provider = OpenAiProvider::new(ProviderConfig {
            provider_id: "openai".to_string(),
            api_key: "sk-test".to_string(),
            base_url: Some(server.url("/v1").to_string()),
            default_model: "gpt-4o-mini".to_string(),
        });

        let request = ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message { role: Role::User, content: "hi".to_string() }],
            tools: vec![],
            stream: true,
        };

        let mut deltas = Vec::new();
        let reason = provider.chat_stream(request, &mut |d| {
            if let Some(c) = d.content { deltas.push(c); }
        }).await.unwrap();

        assert_eq!(deltas, vec!["Hello", " world"]);
        assert_eq!(reason, FinishReason::Stop);
    }
}
```

- [ ] 添加 `httptest` 到 `[dev-dependencies]`：

```toml
# rust-ody/crates/ody-host/Cargo.toml [dev-dependencies] 追加
httptest = "0.16"
```

- [ ] 运行并验证失败：`cargo test -p ody-host llm::openai::tests` 因类型/方法不存在失败。

- [ ] 写最小实现：

```rust
// rust-ody/crates/ody-host/src/llm/mod.rs
use serde::{Deserialize, Serialize};

use crate::config::ProviderConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tools: Vec<ToolDefinition>,
    pub stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct ChatDelta {
    pub index: usize,
    pub content: Option<String>,
    pub tool_call: Option<ToolCallDelta>,
}

#[derive(Debug, Clone, Default)]
pub struct ToolCallDelta {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    Stop,
    ToolCalls,
    Length,
    ContentFilter,
    Other,
}

#[derive(Debug)]
pub enum LlmError {
    ApiError { status: u16, body: String },
    StreamParse { message: String },
    RequestFailed { source: reqwest::Error },
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::ApiError { status, body } => write!(f, "LLM API error {status}: {body}"),
            LlmError::StreamParse { message } => write!(f, "LLM stream parse error: {message}"),
            LlmError::RequestFailed { source } => write!(f, "LLM request failed: {source}"),
        }
    }
}

impl std::error::Error for LlmError {}

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat_stream(
        &self,
        request: ChatRequest,
        on_delta: &mut dyn FnMut(ChatDelta),
    ) -> Result<FinishReason, LlmError>;
}

pub mod openai;
```

```rust
// rust-ody/crates/ody-host/src/llm/openai.rs
use std::time::Duration;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap};
use reqwest_eventsource::{Event, EventSource};

use crate::config::ProviderConfig;
use crate::llm::{ChatDelta, ChatRequest, FinishReason, LlmError, LlmProvider, Message, Role, ToolCallDelta, ToolDefinition};

pub struct OpenAiProvider {
    client: reqwest::Client,
    config: ProviderConfig,
}

impl OpenAiProvider {
    pub fn new(config: ProviderConfig) -> Self {
        Self { client: reqwest::Client::new(), config }
    }

    fn base_url(&self) -> String {
        self.config.base_url.clone().unwrap_or_else(|| "https://api.openai.com/v1".to_string())
    }

    fn headers(&self) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(CONTENT_TYPE, "application/json".parse().unwrap());
        h.insert(AUTHORIZATION, format!("Bearer {}", self.config.api_key).parse().unwrap());
        h
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiProvider {
    async fn chat_stream(
        &self,
        request: ChatRequest,
        on_delta: &mut dyn FnMut(ChatDelta),
    ) -> Result<FinishReason, LlmError> {
        let url = format!("{}/chat/completions", self.base_url());
        let body = serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(|m| serde_json::json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
            "tools": request.tools.iter().map(|t| serde_json::json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.parameters}})).collect::<Vec<_>>(),
            "stream": true,
        });

        // For prototype, use reqwest plain POST + read bytes_stream to avoid extra deps.
        let response = self.client
            .post(&url)
            .headers(self.headers())
            .json(&body)
            .timeout(Duration::from_secs(120))
            .send()
            .await
            .map_err(|e| LlmError::RequestFailed { source: e })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::ApiError { status: status.as_u16(), body });
        }

        let mut stream = response.bytes_stream();
        let mut finish_reason = FinishReason::Stop;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| LlmError::RequestFailed { source: e })?;
            let text = String::from_utf8_lossy(&chunk);
            for line in text.lines() {
                let line = line.trim();
                if !line.starts_with("data: ") { continue; }
                let data = &line["data: ".len()..];
                if data == "[DONE]" { break; }
                let event: SseEvent = serde_json::from_str(data)
                    .map_err(|e| LlmError::StreamParse { message: e.to_string() })?;
                for choice in event.choices {
                    let delta = ChatDelta {
                        index: choice.index,
                        content: choice.delta.content.clone(),
                        tool_call: choice.delta.tool_calls.as_ref().and_then(|tcs| tcs.first()).map(|tc| ToolCallDelta {
                            id: tc.id.clone().unwrap_or_default(),
                            name: tc.function.name.clone().unwrap_or_default(),
                            arguments: serde_json::from_str(&tc.function.arguments.clone().unwrap_or_default()).unwrap_or(serde_json::Value::Null),
                        }),
                    };
                    on_delta(delta);
                    if let Some(fr) = choice.finish_reason {
                        finish_reason = parse_finish_reason(&fr);
                    }
                }
            }
        }
        Ok(finish_reason)
    }
}

fn parse_finish_reason(s: &str) -> FinishReason {
    match s {
        "stop" => FinishReason::Stop,
        "tool_calls" => FinishReason::ToolCalls,
        "length" => FinishReason::Length,
        "content_filter" => FinishReason::ContentFilter,
        _ => FinishReason::Other,
    }
}

#[derive(Debug, serde::Deserialize)]
struct SseEvent {
    choices: Vec<SseChoice>,
}

#[derive(Debug, serde::Deserialize)]
struct SseChoice {
    index: usize,
    delta: SseDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct SseDelta {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<SseToolCall>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct SseToolCall {
    id: Option<String>,
    function: SseToolCallFunction,
}

#[derive(Debug, Default, serde::Deserialize)]
struct SseToolCallFunction {
    name: Option<String>,
    arguments: Option<String>,
}

// Re-export for tests
pub use crate::config::ProviderConfig;
```

- [ ] 添加依赖到 `Cargo.toml`：

```toml
# [dependencies] 追加
async-trait = "0.1"
futures = "0.3"
```

- [ ] 运行并验证通过：`cargo test -p ody-host llm::openai::tests`

- [ ] 提交：`git add rust-ody/crates/ody-host/src/llm/ rust-ody/crates/ody-host/Cargo.toml && git commit -m "feat(ody-host): OpenAI-compatible SSE LLM provider"`

---

### Task A6: Tool Registry + BashTool + Approval 反向 RPC

**Depends on:** Task A4（`Session`）, Task A2（`ProviderConfig` 不需要，但 `Tool` 需要 session）

**Files:**
- Create: `rust-ody/crates/ody-host/src/tools/mod.rs`
- Create: `rust-ody/crates/ody-host/src/tools/bash.rs`
- Test: `rust-ody/crates/ody-host/src/tools/bash.rs` 内 `#[cfg(test)]`

**Steps:**

- [ ] 写失败测试：

```rust
// rust-ody/crates/ody-host/src/tools/bash.rs (末尾 test 模块)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::store::{SessionState, write_state_json};
    use crate::tools::{ApprovalClient, ApprovalDecision, ApprovalRequest, Tool};
    use std::path::Path;
    use std::sync::Arc;

    struct AlwaysApprove;

    #[async_trait::async_trait]
    impl ApprovalClient for AlwaysApprove {
        async fn request(&self, _req: ApprovalRequest) -> Result<crate::tools::ApprovalResponse, crate::tools::ToolError> {
            Ok(crate::tools::ApprovalResponse { decision: ApprovalDecision::Approved })
        }
    }

    #[tokio::test]
    async fn bash_executes_after_approval() {
        let tmp = tempfile::tempdir().unwrap();
        write_state_json(tmp.path(), &SessionState::default()).unwrap();
        let session = Arc::new(crate::session::manager::Session {
            id: "s1".to_string(),
            work_dir: tmp.path().to_path_buf(),
            dir: tmp.path().to_path_buf(),
            state: tokio::sync::Mutex::new(SessionState::default()),
        });
        let tool = BashTool;
        let result = tool.execute(session, "tc1", serde_json::json!({"command": "echo hi"}), &AlwaysApprove).await.unwrap();
        assert_eq!(result.output.trim(), "hi");
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn bash_declined_returns_cancelled() {
        struct AlwaysDecline;
        #[async_trait::async_trait]
        impl ApprovalClient for AlwaysDecline {
            async fn request(&self, _req: ApprovalRequest) -> Result<crate::tools::ApprovalResponse, crate::tools::ToolError> {
                Ok(crate::tools::ApprovalResponse { decision: ApprovalDecision::Rejected })
            }
        }
        let tmp = tempfile::tempdir().unwrap();
        write_state_json(tmp.path(), &SessionState::default()).unwrap();
        let session = Arc::new(crate::session::manager::Session {
            id: "s1".to_string(),
            work_dir: tmp.path().to_path_buf(),
            dir: tmp.path().to_path_buf(),
            state: tokio::sync::Mutex::new(SessionState::default()),
        });
        let tool = BashTool;
        let result = tool.execute(session, "tc1", serde_json::json!({"command": "echo hi"}), &AlwaysDecline).await.unwrap();
        assert!(result.output.contains("declined"));
        assert!(!result.is_error);
    }
}
```

- [ ] 运行并验证失败：`cargo test -p ody-host tools::bash::tests` 因类型不存在失败。

- [ ] 写最小实现：

```rust
// rust-ody/crates/ody-host/src/tools/mod.rs
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use crate::session::manager::Session;

pub mod bash;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approved,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub tool_call_id: String,
    pub tool_name: String,
    pub action: String,
    pub display: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ApprovalResponse {
    pub decision: ApprovalDecision,
}

#[derive(Debug)]
pub enum ToolError {
    InvalidArgs,
    ExecutionFailed { message: String },
    ApprovalFailed { source: Box<dyn std::error::Error + Send + Sync> },
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::InvalidArgs => write!(f, "invalid tool arguments"),
            ToolError::ExecutionFailed { message } => write!(f, "tool execution failed: {message}"),
            ToolError::ApprovalFailed { source } => write!(f, "approval failed: {source}"),
        }
    }
}

impl std::error::Error for ToolError {}

#[async_trait]
pub trait ApprovalClient: Send + Sync {
    async fn request(&self, request: ApprovalRequest) -> Result<ApprovalResponse, ToolError>;
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub output: String,
    pub is_error: bool,
}

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn definition(&self) -> ToolDefinition;
    async fn execute(
        &self,
        session: Arc<Session>,
        tool_call_id: &str,
        args: serde_json::Value,
        approval: &dyn ApprovalClient,
    ) -> Result<ToolResult, ToolError>;
}

pub struct ToolRegistry {
    tools: std::collections::HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn with_builtin() -> Self {
        let mut tools = std::collections::HashMap::new();
        let bash: Box<dyn Tool> = Box::new(bash::BashTool);
        tools.insert(bash.name().to_string(), bash);
        Self { tools }
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|b| b.as_ref())
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }
}
```

```rust
// rust-ody/crates/ody-host/src/tools/bash.rs
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use tokio::process::Command;
use tokio::time::timeout;

use crate::session::manager::Session;
use crate::tools::{ApprovalClient, ApprovalDecision, ApprovalRequest, ApprovalResponse, Tool, ToolDefinition, ToolError, ToolResult};

pub struct BashTool;

const DEFAULT_TIMEOUT_MS: u64 = 30000;

impl BashTool {
    fn display(command: &str) -> serde_json::Value {
        json!({ "command": command })
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str { "bash" }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "bash".to_string(),
            description: "Execute a shell command after user approval.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "timeout_ms": { "type": "integer", "default": DEFAULT_TIMEOUT_MS }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(
        &self,
        session: Arc<Session>,
        tool_call_id: &str,
        args: serde_json::Value,
        approval: &dyn ApprovalClient,
    ) -> Result<ToolResult, ToolError> {
        let command = args["command"].as_str().ok_or(ToolError::InvalidArgs)?;
        let timeout_ms = args["timeout_ms"].as_u64().unwrap_or(DEFAULT_TIMEOUT_MS);

        let req = ApprovalRequest {
            tool_call_id: tool_call_id.to_string(),
            tool_name: "bash".to_string(),
            action: format!("Execute: {}", command),
            display: Self::display(command),
        };
        let resp = approval.request(req).await?;
        if resp.decision != ApprovalDecision::Approved {
            return Ok(ToolResult { output: "User declined.".to_string(), is_error: false });
        }

        let output = Command::new("bash")
            .arg("-c")
            .arg(command)
            .current_dir(&session.work_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();

        let result = timeout(Duration::from_millis(timeout_ms), output)
            .await
            .map_err(|_| ToolError::ExecutionFailed { message: "timed out".to_string() })?
            .map_err(|e| ToolError::ExecutionFailed { message: e.to_string() })?;

        let text = String::from_utf8_lossy(&result.stdout).to_string();
        let err = String::from_utf8_lossy(&result.stderr).to_string();
        let combined = if err.is_empty() { text } else { format!("{}\n{}", text, err) };
        Ok(ToolResult { output: combined, is_error: !result.status.success() })
    }
}
```

- [ ] 运行并验证通过：`cargo test -p ody-host tools::bash::tests`

- [ ] 提交：`git add rust-ody/crates/ody-host/src/tools/ && git commit -m "feat(ody-host): bash tool with approval gate"`

---

### Task A7: `CoreHost` 聚合根 + `EventSink` + `dispatch` 路由

**Depends on:** Task A4, Task A5, Task A6

**Files:**
- Create: `rust-ody/crates/ody-host/src/events.rs`
- Create: `rust-ody/crates/ody-host/src/host.rs`
- Modify: `rust-ody/crates/ody-host/src/main.rs`（替换占位）
- Test: `rust-ody/crates/ody-host/src/host.rs` 内 `#[cfg(test)]`

**Steps:**

- [ ] 写失败测试：验证 `CoreHost::dispatch` 对 `getCoreInfo`、`createSession`、`closeSession`、`prompt` 的路由与事件发射。

```rust
// rust-ody/crates/ody-host/src/host.rs (末尾 test 模块)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HostConfig, ProviderConfig, TransportMode};
    use crate::events::{AgentEvent, EventSink};
    use crate::llm::{ChatDelta, ChatRequest, FinishReason, LlmProvider, Message, Role};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    struct MockProvider;

    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        async fn chat_stream(
            &self,
            _request: ChatRequest,
            on_delta: &mut dyn FnMut(ChatDelta),
        ) -> Result<FinishReason, crate::llm::LlmError> {
            on_delta(ChatDelta { index: 0, content: Some("ok".to_string()), tool_call: None });
            Ok(FinishReason::Stop)
        }
    }

    struct MockSink(Arc<Mutex<Vec<AgentEvent>>>);

    impl EventSink for MockSink {
        fn emit(&self, event: AgentEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[tokio::test]
    async fn dispatch_get_core_info() {
        let host = make_host(MockProvider, MockSink::default()).await;
        let value = host.dispatch("getCoreInfo", serde_json::json!({})).await.unwrap();
        assert_eq!(value["version"].as_str().unwrap(), env!("CARGO_PKG_VERSION"));
        assert!(value["capabilities"].as_array().unwrap().contains(&serde_json::json!("bash")));
    }

    #[tokio::test]
    async fn dispatch_create_session_then_prompt_emits_events() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let host = make_host(MockProvider, MockSink(Arc::clone(&sink))).await;
        let value = host.dispatch("createSession", serde_json::json!({"workDir": "/tmp/wd"})).await.unwrap();
        let session_id = value["id"].as_str().unwrap().to_string();

        host.dispatch("prompt", serde_json::json!({"sessionId": session_id, "agentId": "main", "input": [{"type":"text","text":"hi"}]})).await.unwrap();

        // wait briefly for spawned prompt
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let events: Vec<_> = sink.lock().unwrap().clone();
        let types: Vec<_> = events.iter().map(|e| match e {
            AgentEvent::UserMessage { .. } => "user",
            AgentEvent::AssistantDelta { .. } => "delta",
            AgentEvent::AssistantFinish { .. } => "finish",
            _ => "other",
        }).collect();
        assert!(types.contains(&"user"));
        assert!(types.contains(&"delta"));
        assert!(types.contains(&"finish"));
    }

    async fn make_host<P, S>(provider: P, sink: S) -> CoreHost
    where
        P: LlmProvider + 'static,
        S: EventSink + 'static,
    {
        let config = HostConfig {
            home_dir: tempfile::tempdir().unwrap().into_path(),
            config_path: None,
            transport: TransportMode::Stdio,
            log_level: crate::config::LogLevel::Info,
            provider: ProviderConfig { provider_id: "mock".to_string(), api_key: "".to_string(), base_url: None, default_model: "mock".to_string() },
        };
        CoreHost::new(config, Box::new(sink), Box::new(provider)).unwrap()
    }
}
```

- [ ] 运行并验证失败：`cargo test -p ody-host host::tests` 因类型不存在失败。

- [ ] 写最小实现：

```rust
// rust-ody/crates/ody-host/src/events.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentEvent {
    UserMessage {
        session_id: String,
        agent_id: String,
        content: String,
    },
    AssistantDelta {
        session_id: String,
        agent_id: String,
        delta: String,
    },
    AssistantFinish {
        session_id: String,
        agent_id: String,
        finish_reason: String,
    },
    ToolResult {
        session_id: String,
        agent_id: String,
        tool_call_id: String,
        output: String,
        is_error: bool,
    },
    Error {
        session_id: String,
        agent_id: String,
        message: String,
        code: Option<String>,
    },
}

pub trait EventSink: Send + Sync {
    fn emit(&self, event: AgentEvent);
}
```

```rust
// rust-ody/crates/ody-host/src/host.rs
use std::sync::Arc;

use serde_json::json;

use crate::config::{HostConfig, ProviderConfig};
use crate::events::{AgentEvent, EventSink};
use crate::llm::{ChatDelta, ChatRequest, FinishReason, LlmProvider, Message, Role};
use crate::session::manager::{SessionFilter, SessionManager};
use crate::session::store::{SessionState, SessionStoreAdapter, write_state_json};
use crate::tools::{ApprovalClient, ApprovalDecision, ApprovalRequest, ApprovalResponse, ToolError, ToolRegistry};

#[derive(Debug)]
pub enum HostError {
    ConfigInvalid(String),
    Session(crate::session::store::SessionError),
    Llm(crate::llm::LlmError),
    Tool(crate::tools::ToolError),
    MethodNotImplemented(String),
    InvalidPayload(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::ConfigInvalid(m) => write!(f, "config invalid: {m}"),
            HostError::Session(e) => write!(f, "{e}"),
            HostError::Llm(e) => write!(f, "{e}"),
            HostError::Tool(e) => write!(f, "{e}"),
            HostError::MethodNotImplemented(m) => write!(f, "method not implemented: {m}"),
            HostError::InvalidPayload(m) => write!(f, "invalid payload: {m}"),
        }
    }
}

impl std::error::Error for HostError {}

impl From<crate::session::store::SessionError> for HostError {
    fn from(e: crate::session::store::SessionError) -> Self { HostError::Session(e) }
}

impl From<crate::llm::LlmError> for HostError {
    fn from(e: crate::llm::LlmError) -> Self { HostError::Llm(e) }
}

impl From<crate::tools::ToolError> for HostError {
    fn from(e: crate::tools::ToolError) -> Self { HostError::Tool(e) }
}

pub struct CoreHost {
    config: HostConfig,
    sessions: SessionManager,
    tools: ToolRegistry,
    provider: Box<dyn LlmProvider>,
    event_sink: Box<dyn EventSink>,
}

impl CoreHost {
    pub fn new(
        config: HostConfig,
        event_sink: Box<dyn EventSink>,
        provider: Box<dyn LlmProvider>,
    ) -> Result<Self, HostError> {
        if config.provider.api_key.is_empty() {
            // Prototype allows empty key for mock server tests; warn only.
            tracing::warn!("LLM API key is empty");
        }
        let store = SessionStoreAdapter::new(config.home_dir.clone());
        let sessions = SessionManager::new(store);
        let tools = ToolRegistry::with_builtin();
        Ok(Self { config, sessions, tools, provider, event_sink })
    }

    pub async fn dispatch(&self, method: &str, payload: serde_json::Value) -> Result<serde_json::Value, HostError> {
        match method {
            "getCoreInfo" => Ok(json!({
                "version": env!("CARGO_PKG_VERSION"),
                "capabilities": ["chat", "bash"],
            })),
            "createSession" => {
                let work_dir = payload["workDir"].as_str().ok_or_else(|| HostError::InvalidPayload("missing workDir".to_string()))?;
                let title = payload["title"].as_str();
                let summary = self.sessions.create(std::path::Path::new(work_dir), title).await?;
                Ok(session_summary_to_json(&summary))
            }
            "listSessions" => {
                let filter = SessionFilter {
                    work_dir: payload["workDir"].as_str().map(|s| s.to_string()),
                    session_id: payload["sessionId"].as_str().map(|s| s.to_string()),
                };
                let list = self.sessions.list(filter).await?;
                Ok(json!(list.iter().map(session_summary_to_json).collect::<Vec<_>>()))
            }
            "closeSession" => {
                let id = payload["sessionId"].as_str().ok_or_else(|| HostError::InvalidPayload("missing sessionId".to_string()))?;
                self.sessions.close(id.to_string()).await?;
                Ok(json!(null))
            }
            "prompt" => {
                let session_id = payload["sessionId"].as_str().ok_or_else(|| HostError::InvalidPayload("missing sessionId".to_string()))?;
                let agent_id = payload["agentId"].as_str().unwrap_or("main");
                let input = payload["input"].clone();
                let session = self.sessions.get(session_id.to_string()).await?;
                let agent_id = agent_id.to_string();
                let host = Arc::new(self);
                tokio::spawn(async move {
                    if let Err(e) = host.handle_prompt(session, agent_id, input).await {
                        host.event_sink.emit(AgentEvent::Error {
                            session_id: session_id.to_string(),
                            agent_id: agent_id.clone(),
                            message: e.to_string(),
                            code: None,
                        });
                    }
                });
                Ok(json!(null))
            }
            _ => Err(HostError::MethodNotImplemented(method.to_string())),
        }
    }

    async fn handle_prompt(&self, session: Arc<crate::session::manager::Session>, agent_id: String, input: serde_json::Value) -> Result<(), HostError> {
        let text = input.as_array()
            .and_then(|arr| arr.iter().find_map(|p| p["text"].as_str()))
            .unwrap_or("")
            .to_string();

        {
            let mut state = session.state.lock().await;
            state.last_prompt = Some(text.clone());
            write_state_json(&session.dir, &*state).map_err(|e| HostError::Session(crate::session::store::SessionError::Io { source: e, path: session.dir.clone() }))?;
        }

        self.event_sink.emit(AgentEvent::UserMessage {
            session_id: session.id.clone(),
            agent_id: agent_id.clone(),
            content: text.clone(),
        });

        let request = ChatRequest {
            model: self.config.provider.default_model.clone().unwrap_or_else(|| "gpt-4o-mini".to_string()),
            messages: vec![
                Message { role: Role::System, content: "You are a helpful coding assistant.".to_string() },
                Message { role: Role::User, content: text },
            ],
            tools: self.tools.definitions(),
            stream: true,
        };

        let mut tool_call: Option<crate::llm::ToolCallDelta> = None;
        let finish_reason = self.provider.chat_stream(request, &mut |delta: ChatDelta| {
            if let Some(content) = &delta.content {
                self.event_sink.emit(AgentEvent::AssistantDelta {
                    session_id: session.id.clone(),
                    agent_id: agent_id.clone(),
                    delta: content.clone(),
                });
            }
            if delta.tool_call.is_some() {
                tool_call = delta.tool_call.clone();
            }
        }).await?;

        self.event_sink.emit(AgentEvent::AssistantFinish {
            session_id: session.id.clone(),
            agent_id: agent_id.clone(),
            finish_reason: format!("{:?}", finish_reason).to_lowercase(),
        });

        if let Some(tc) = tool_call {
            let tool = self.tools.get(&tc.name).ok_or_else(|| HostError::Tool(ToolError::ExecutionFailed { message: format!("unknown tool {}", tc.name) }))?;
            let approval_client = CoreHostApprovalClient { sink: &*self.event_sink, session_id: session.id.clone(), agent_id: agent_id.clone() };
            let result = tool.execute(Arc::clone(&session), &tc.id, tc.arguments, &approval_client).await?;
            self.event_sink.emit(AgentEvent::ToolResult {
                session_id: session.id.clone(),
                agent_id: agent_id.clone(),
                tool_call_id: tc.id,
                output: result.output,
                is_error: result.is_error,
            });
        }

        Ok(())
    }
}

struct CoreHostApprovalClient<'a> {
    sink: &'a dyn EventSink,
    session_id: String,
    agent_id: String,
}

#[async_trait::async_trait]
impl ApprovalClient for CoreHostApprovalClient<'_> {
    async fn request(&self, request: ApprovalRequest) -> Result<ApprovalResponse, ToolError> {
        // Prototype: emit event and immediately cancel (TUI must register approval handler for real flow).
        // In full implementation this would perform a reverse RPC and await user decision.
        self.sink.emit(AgentEvent::Error {
            session_id: self.session_id.clone(),
            agent_id: self.agent_id.clone(),
            message: format!("Approval required for {}: {}", request.tool_name, request.action),
            code: Some("APPROVAL_REQUIRED".to_string()),
        });
        Ok(ApprovalResponse { decision: ApprovalDecision::Cancelled })
    }
}

fn session_summary_to_json(s: &crate::session::store::SessionSummary) -> serde_json::Value {
    json!({
        "id": s.id,
        "workDir": s.work_dir,
        "sessionDir": s.session_dir,
        "createdAt": s.created_at_ms,
        "updatedAt": s.updated_at_ms,
        "title": s.title,
        "lastPrompt": s.last_prompt,
        "metadata": s.metadata,
    })
}
```

- [ ] 注意：上面 `tokio::spawn` 闭包中 `Arc::new(self)` 需要 `CoreHost` 实现 `Clone`。应为 `CoreHost` 派生 `Clone` 或改用 `Arc<CoreHost>`。修正方案：让 `dispatch` 接收 `self: &Arc<Self>`，或在 `CoreHost` 内部存储 `Arc` 包装。更简单：将 `handle_prompt` 改为接收 `Arc<CoreHost>`。这里给出修正后的签名：

```rust
// 在 CoreHost impl 中
pub async fn dispatch(self: &Arc<Self>, method: &str, payload: serde_json::Value) -> Result<serde_json::Value, HostError> {
    // ...
    "prompt" => {
        // ...
        let host = Arc::clone(self);
        tokio::spawn(async move {
            if let Err(e) = host.handle_prompt(session, agent_id, input).await {
                host.event_sink.emit(/* ... */);
            }
        });
    }
}
```

- [ ] 写最小 `main.rs` 启动 `CoreHost`（stdio transport 占位，transport 层将在 Part 2 替换）：

```rust
// rust-ody/crates/ody-host/src/main.rs
use std::sync::Arc;

use ody_host::config::{HostConfig, LogLevel};
use ody_host::events::{AgentEvent, EventSink};
use ody_host::host::CoreHost;
use ody_host::llm::openai::OpenAiProvider;

struct StderrSink;

impl EventSink for StderrSink {
    fn emit(&self, event: AgentEvent) {
        eprintln!("{}", serde_json::to_string(&event).unwrap_or_default());
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = HostConfig::from_cli(std::env::args()).map_err(|e| e.to_string())?;
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(match config.log_level {
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        })
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let provider = Box::new(OpenAiProvider::new(config.provider.clone()));
    let host = Arc::new(CoreHost::new(config, Box::new(StderrSink), provider)?);

    // Transport layer will be wired in Part 2. For now, keep process alive.
    tracing::info!("ody-host core ready (transport placeholder)");
    tokio::signal::ctrl_c().await?;
    Ok(())
}
```

- [ ] 运行并验证通过：`cargo test -p ody-host host::tests` 与 `cargo build -p ody-host`

- [ ] 提交：`git add rust-ody/crates/ody-host/src/events.rs rust-ody/crates/ody-host/src/host.rs rust-ody/crates/ody-host/src/main.rs && git commit -m "feat(ody-host): CoreHost dispatch and event sink"`

---

## Local Self-Review

- [ ] 1. Spec-coverage table: 本 Part 覆盖 design `core.md` §2.1-2.6 与 §3.1-3.5（HostConfig、CoreHost、SessionManager、OpenAiProvider、BashTool、EventSink、dispatch 算法）。
- [ ] 2. Placeholder scan: 无 `TODO`；`main.rs` 中 transport 占位在 Part 2 任务 B6 被实际替换，此处是明确的阶段边界，非未完成依赖。
- [ ] 3. No phantom tasks: 每个任务产生可编译、可测试的代码变更。
- [ ] 4. Dependency soundness: A1→A2→A3→A4，A5/A6 依赖 A2/A4，A7 依赖 A4/A5/A6；无反向依赖。
- [ ] 5. Caller & build soundness: 本 Part 无共享 TS 签名变更；每次任务以 `cargo test -p ody-host` 结束。
- [ ] 6. Test-the-risk: A3 测试 workDirKey 与 TS 一致性；A4 测试会话创建/关闭状态；A5 测试 SSE 流式解析；A6 测试 approval 门；A7 测试 dispatch 路由与事件流。
- [ ] 7. Type consistency: `SessionSummary` 字段（`createdAt`/`updatedAt` ms 时间戳、`workDir`/`sessionDir` 路径）与 TS `SessionSummary` 对齐；`AgentEvent` 的 type tag 与 TS `events.ts` 的 `type` 字段对齐。
