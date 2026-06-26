# Phase A1 Part 1: Rust `ody-host` CLI

> Scope: 在 `ody-host` 中新增 `serve` 子命令，复用全局 flags，并通过单元测试覆盖 `serve --stdio`、`serve --socket-path`、全局 flags 兼容及冲突拒绝场景。

---

### Task 1: 新增 `serve` 子命令并覆盖 `serve --stdio`

**Depends on:** none

**Files:**
- Modify: `rust-ody/crates/ody-host/src/config.rs:1-8`（`use clap` 行）
- Modify: `rust-ody/crates/ody-host/src/config.rs:41-58`（`Cli` 结构体）
- Modify: `rust-ody/crates/ody-host/src/config.rs:74-120`（`HostConfig::from_cli`）
- Test: `rust-ody/crates/ody-host/src/config.rs:147-172`（底部 `mod tests`）

- [ ] Write the failing test

在 `config.rs` 底部 `mod tests` 内新增：

```rust
    #[test]
    fn serve_subcommand_stdio() {
        let args = vec!["ody-host", "serve", "--stdio"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert!(matches!(config.transport, TransportMode::Stdio));
        assert_eq!(config.log_level, LogLevel::Info);
    }
```

- [ ] Run it and verify it FAILS

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p ody-host serve_subcommand_stdio
```

Expected failure（节选）：

```
error: unexpected argument 'serve' found
...
test config::tests::serve_subcommand_stdio ... FAILED
```

- [ ] Write the minimal implementation

1. 将 `use clap::Parser;` 改为：

```rust
use clap::{Args, Parser, Subcommand};
```

2. 将 `Cli` 结构体改造为 flatten 共享 args，并增加子命令：

```rust
#[derive(Debug, Args)]
struct SharedArgs {
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

#[derive(Debug, Parser)]
#[command(name = "ody-host", version)]
struct Cli {
    #[command(flatten)]
    shared: SharedArgs,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Serve(ServeArgs),
}

#[derive(Debug, Args)]
struct ServeArgs {
    #[command(flatten)]
    shared: SharedArgs,
}
```

3. 在 `HostConfig::from_cli` 内新增 `active_args` 选择逻辑，并替换原 `cli` 字段访问为 `active`：

```rust
impl HostConfig {
    pub fn from_cli<I, T>(args: I) -> Result<Self, HostError>
    where
        I: Iterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let cli = Cli::try_parse_from(args)
            .map_err(|e| HostError::config_invalid(e.to_string()))?;
        let active = active_args(&cli)?;

        let home_dir = active.home.unwrap_or_else(default_home_dir);
        let config_path = active.config.clone().or_else(|| {
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

        let transport = if let Some(path) = active.socket_path.clone() {
            TransportMode::UnixSocket { path }
        } else if let (Some(host), Some(port)) = (active.tcp_host.clone(), active.tcp_port) {
            TransportMode::TcpSocket { host, port }
        } else {
            TransportMode::Stdio
        };

        let log_level = parse_log_level(&active.log_level)?;

        let provider = ProviderConfig {
            provider_id: "openai".to_string(),
            api_key: file.provider.as_ref().map(|p| p.api_key.clone()).unwrap_or_default(),
            base_url: file.provider.as_ref().and_then(|p| p.base_url.clone()),
            default_model: Some(file.provider.as_ref().and_then(|p| p.default_model.clone()).unwrap_or_else(|| "gpt-4o-mini".to_string())),
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

fn active_args(cli: &Cli) -> Result<&SharedArgs, HostError> {
    match &cli.command {
        Some(Command::Serve(serve)) => {
            let has_global_flags = cli.shared.stdio
                || cli.shared.socket_path.is_some()
                || cli.shared.tcp_host.is_some()
                || cli.shared.tcp_port.is_some()
                || cli.shared.config.is_some()
                || cli.shared.home.is_some()
                || cli.shared.log_level != "info";
            if has_global_flags {
                return Err(HostError::config_invalid(
                    "cannot use global flags together with the `serve` subcommand".to_string(),
                ));
            }
            Ok(&serve.shared)
        }
        None => Ok(&cli.shared),
    }
}
```

- [ ] Run it and verify it PASSES

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p ody-host serve_subcommand_stdio
```

Expected output：

```
running 1 test
test config::tests::serve_subcommand_stdio ... ok
```

- [ ] Commit

```bash
cd /Users/ranwei/workspace/ody-code
git add rust-ody/crates/ody-host/src/config.rs
git commit -m "feat(ody-host): add serve subcommand with shared flags"
```

---

### Task 2: 覆盖 `serve --socket-path`

**Depends on:** Task 1

**Files:**
- Test: `rust-ody/crates/ody-host/src/config.rs:147-172`（底部 `mod tests`）

- [ ] Write the failing test

在 `mod tests` 内新增：

```rust
    #[test]
    fn serve_subcommand_socket_path() {
        let args = vec!["ody-host", "serve", "--socket-path", "/tmp/ody-serve.sock"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert_eq!(
            config.transport,
            TransportMode::UnixSocket { path: std::path::PathBuf::from("/tmp/ody-serve.sock") }
        );
    }
```

- [ ] Run it and verify it FAILS

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p ody-host serve_subcommand_socket_path
```

Expected failure：测试不编译或 clap 报错 `unexpected argument 'serve'`，因为 Task 1 尚未实现。

> 注：若 Task 1 已完整实现，此测试会直接通过；但 TDD 流程要求先写测试并看到失败。

- [ ] Write the minimal implementation

Task 1 的实现已覆盖此场景，无需新增代码。确认 `active_args` 返回 `&serve.shared` 且 `socket_path` 字段被正确读取即可。

- [ ] Run it and verify it PASSES

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p ody-host serve_subcommand_socket_path
```

Expected output：

```
running 1 test
test config::tests::serve_subcommand_socket_path ... ok
```

- [ ] Commit

```bash
cd /Users/ranwei/workspace/ody-code
git add rust-ody/crates/ody-host/src/config.rs
git commit -m "test(ody-host): cover serve --socket-path"
```

---

### Task 3: 覆盖全局 flags 兼容与冲突拒绝

**Depends on:** Task 1

**Files:**
- Test: `rust-ody/crates/ody-host/src/config.rs:147-172`（底部 `mod tests`）

- [ ] Write the failing tests

在 `mod tests` 内新增：

```rust
    #[test]
    fn global_flags_stdio_still_works() {
        let args = vec!["ody-host", "--stdio"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert!(matches!(config.transport, TransportMode::Stdio));
        assert_eq!(config.log_level, LogLevel::Info);
    }

    #[test]
    fn global_flags_conflict_with_serve_rejected() {
        let args = vec!["ody-host", "--stdio", "serve", "--socket-path", "/tmp/ody.sock"];
        let err = HostConfig::from_cli(args.into_iter()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("global flags") || msg.contains("serve"),
            "expected conflict error, got: {msg}"
        );
    }
```

- [ ] Run it and verify it FAILS

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p ody-host global_flags
```

Expected failure（节选）：

```
test config::tests::global_flags_stdio_still_works ... ok
test config::tests::global_flags_conflict_with_serve_rejected ... FAILED
```

或两个均失败，取决于 Task 1 实现进度。

- [ ] Write the minimal implementation

Task 1 中的 `active_args` 已包含冲突检测逻辑，无需新增代码。确认 `has_global_flags` 检查覆盖 `stdio`、`socket_path`、`tcp_host`、`tcp_port`、`config`、`home`、`log_level` 即可。

- [ ] Run it and verify it PASSES

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p ody-host
```

Expected output（节选）：

```
running 7 tests
...
test config::tests::global_flags_stdio_still_works ... ok
test config::tests::global_flags_conflict_with_serve_rejected ... ok
test config::tests::serve_subcommand_socket_path ... ok
test config::tests::serve_subcommand_stdio ... ok
...
test result: ok
```

- [ ] Commit

```bash
cd /Users/ranwei/workspace/ody-code
git add rust-ody/crates/ody-host/src/config.rs
git commit -m "test(ody-host): cover global flags compatibility and serve conflict"
```

---

## Local Self-Review

- [ ] 1. Spec-coverage table（本 Part）：
  - Rust `serve` 子命令复用全局 flags → Task 1 covered
  - `ody-host serve --stdio` 测试 → Task 1 covered
  - `ody-host serve --socket-path` 测试 → Task 2 covered
  - `ody-host --stdio` 兼容测试 → Task 3 covered
  - 全局 flags 与 `serve` 冲突拒绝 → Task 3 covered
- [ ] 2. Placeholder scan：本 Part 无 TODO/TBD/"implement later"。
- [ ] 3. No phantom tasks：每个 Task 都产生可验证的代码/测试变更。
- [ ] 4. Dependency soundness：Task 2/3 仅依赖 Task 1 创建的 `Command`/`ServeArgs`/`SharedArgs`/`active_args`。
- [ ] 5. Caller & build soundness：
  - 公开签名 `HostConfig::from_cli<I, T>(args: I) -> Result<Self, HostError>` 未变；私有 `Cli` 增加 `command` 字段。
  - 搜索所有 `HostConfig::from_cli` 调用：
    ```bash
    rg -n "HostConfig::from_cli" rust-ody/crates/ody-host/src/
    ```
    仅 `main.rs:10` 与 `config.rs` 测试使用，无需更新签名。
  - 整体验证：
    ```bash
    cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p ody-host
    ```
- [ ] 6. Test-the-risk：
  - 状态变化：CLI 解析结果决定 `HostConfig.transport` 与 `log_level`；测试断言具体枚举值，而非仅编译通过。
  - 冲突拒绝测试断言错误消息包含 `"global flags"` 或 `"serve"`，对应 `active_args` 中的自定义错误文本。
- [ ] 7. Type consistency：Task 1 定义 `SharedArgs` 后，Task 2/3 的测试使用相同字段名与枚举值；无后续 Part 依赖本 Part 类型。
