# Phase A1 — Rust CLI 详细设计

> **所属设计**: `.ody-code/designs/2026-06-26-index.md`  
> **Part**: 1 / 2  
> **Scope**: `rust-ody/crates/ody-host/src/config.rs` 及单元测试

---

## 1. Local Scope

### In Scope

1. 在 `config.rs` 中新增 `ServeCommand` 子命令结构，字段与全局 `Cli` 完全一致 [C:USER]
2. `HostConfig::from_cli` 同时处理以下两种 argv：
   - `ody-host --stdio`
   - `ody-host serve --stdio` [C:USER]
3. 保留现有全局 flags 行为不变 [C:USER]
4. 新增 Rust 单元测试覆盖 `serve` 子命令 [C:INFERRED]

### Out of Scope

| # | 项目 | 原因 |
|---|---|---|
| L1 | 修改 `main.rs` 或 transport 层 | CLI 解析结果仍为 `HostConfig`，下游无感知 |
| L2 | 修改 provider / LLM / tool 逻辑 | A1 只涉及启动入口 |
| L3 | 引入新 flag 或重命名 flag | 复用现有全局 flags [C:USER] |

---

## 2. Interfaces & Types

```rust
// 现有类型保持不变
pub enum TransportMode { Stdio, UnixSocket { path: PathBuf }, TcpSocket { host: String, port: u16 } }
pub enum LogLevel { Debug, Info, Warn, Error }
pub struct ProviderConfig { provider_id: String, api_key: String, base_url: Option<String>, default_model: Option<String> }
pub struct HostConfig { home_dir: PathBuf, config_path: Option<PathBuf>, transport: TransportMode, log_level: LogLevel, provider: ProviderConfig }

// 改造后的 Cli
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

    #[command(subcommand)]
    command: Option<Command>,  // 新增 [C:USER]
}

// 新增
#[derive(Debug, Parser)]
enum Command {
    /// Run the Ody host server.
    Serve(ServeArgs),  // [C:USER]
}

#[derive(Debug, Parser)]
struct ServeArgs {
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
```

**contract**: `HostConfig::from_cli` 无论收到全局 flags 还是 `serve` subcommand flags，都产出等价的 `HostConfig`；缺少 transport flag 时默认 `TransportMode::Stdio`。

---

## 3. Algorithms

### Algorithm: Parse CLI with optional `serve` subcommand

```
function HostConfig::from_cli(args):
    cli = Cli::parse_from(args)                         // clap 解析
    active = choose_active_args(cli)                    // 选择全局或 ServeArgs
    home_dir = active.home.unwrap_or(default_home_dir())
    config_path = active.config.or_else(lookup_default_config(home_dir))
    file = load_raw_config(config_path) if exists else empty
    transport = build_transport(active)                 // 与现有逻辑一致
    log_level = parse_log_level(active.log_level)
    provider = build_provider(file)
    return HostConfig { home_dir, config_path, transport, log_level, provider }

function choose_active_args(cli: Cli) -> TransportArgs:
    match cli.command:
        Some(Command::Serve(s)) -> return s as TransportArgs
        None -> return cli as TransportArgs

// 类型适配：将 Cli 或 ServeArgs 统一视为具有相同字段的 TransportArgs
// 实际实现可通过提取公共字段到结构体 + `impl From<Cli> for TransportArgs` / `impl From<ServeArgs> for TransportArgs` 完成
```

### Algorithm: Build transport from active args

```
function build_transport(active: TransportArgs) -> TransportMode:
    if active.socket_path is Some(path):
        return UnixSocket { path }
    else if active.tcp_host is Some(host) and active.tcp_port is Some(port):
        return TcpSocket { host, port }
    else:
        return Stdio
```

---

## 4. Call-Site Integration

### 4.1 `rust-ody/crates/ody-host/src/config.rs:41-58`

**Before/After**: 在 `Cli` 结构体上添加 `#[command(subcommand)] command: Option<Command>`。`ServeArgs` 定义紧随其后。

**Pseudocode sketch**:
```rust
// 现有字段下方插入：
#[command(subcommand)]
command: Option<Command>,
```

### 4.2 `rust-ody/crates/ody-host/src/config.rs:74-120`

**Before/After**: `HostConfig::from_cli` 在 `Cli::parse_from(args)` 之后，先 `choose_active_args(cli)`，再用返回的 `TransportArgs` 替代原 `cli` 访问 flags。

**Pseudocode sketch**:
```rust
let cli = Cli::parse_from(args);
let active = TransportArgs::from(cli);  // 内部处理 Command::Serve 或全局
let home_dir = active.home.unwrap_or_else(default_home_dir);
// ... 后续用 active.socket_path / active.tcp_host / active.tcp_port 等
```

### 4.3 `rust-ody/crates/ody-host/src/main.rs:10`

**Before/After**: 无需修改。`HostConfig::from_cli(std::env::args())` 已经能处理 `serve`。

---

## 5. Error Handling

| Error Class | Immediate Handling | Degradation Path | Recovery Condition |
|---|---|---|---|
| clap 解析失败（如未知 flag、缺少 value） | clap 自动打印 usage 并 exit code ≠ 0 | 用户修正命令行 | 使用合法 argv 重新启动 |
| 全局 flags 与 `serve` flags 同时提供 | clap 拒绝（`error: unexpected argument` 或子命令解析错误） | 用户去掉重复/冲突 flags | 只使用一种风格 |
| `serve` subcommand 使用旧 TS flag 名（如 `--socket`） | clap 报 `error: unexpected argument '--socket'` | 用户改用 `--socket-path` | 使用 Rust 全局 flag 名 |
| 配置文件解析失败 | 返回 `HostError::config_invalid(...)` | 进程退出 | 修正 `ody.toml` / `ody.json` |

---

## 6. Test Plan

### Rust 单元测试（`rust-ody/crates/ody-host/src/config.rs` 底部 `mod tests`）

| # | 测试名 | 输入 argv | 断言 |
|---|---|---|---|
| T1 | `serve_subcommand_defaults_to_stdio` | `["ody-host", "serve"]` | `transport == Stdio`, `log_level == Info` |
| T2 | `serve_subcommand_with_stdio_flag` | `["ody-host", "serve", "--stdio"]` | `transport == Stdio` |
| T3 | `serve_subcommand_with_socket_path` | `["ody-host", "serve", "--socket-path", "/tmp/x.sock"]` | `transport == UnixSocket { path: "/tmp/x.sock" }` |
| T4 | `serve_subcommand_with_tcp` | `["ody-host", "serve", "--tcp-host", "127.0.0.1", "--tcp-port", "9000"]` | `transport == TcpSocket { host: "127.0.0.1", port: 9000 }` |
| T5 | `global_flags_still_work` | `["ody-host", "--stdio"]` | `transport == Stdio`（回归测试） |
| T6 | `serve_subcommand_rejects_unknown_flag` | `["ody-host", "serve", "--socket", "/tmp/x.sock"]` | `unwrap_err().to_string().contains("--socket")` |

**Done criteria**:
```bash
cargo test -p ody-host
```
所有新增及既有测试通过。

---

## 7. Local Risk Notes

| # | Risk | 说明 |
|---|---|---|
| LR1 | 公共字段重复 | `Cli` 与 `ServeArgs` 字段完全一致，未来新增 flag 需同时改两处；建议用公共结构体或 macro 抽离 |
| LR2 | clap 子命令优先级 | 若用户输入 `ody-host --stdio serve`，clap 可能将 `--stdio` 视为全局、`serve` 视为子命令但无子命令 flags，导致歧义；测试需覆盖 |
| LR3 | `--help` 输出变化 | `ody-host --help` 会新增 `Commands:` 段落，ADR 和 CI 中若对 help 文本做字符串匹配需更新 |
