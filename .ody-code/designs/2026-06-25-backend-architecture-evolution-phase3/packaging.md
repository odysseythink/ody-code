# Part 4 — Build, Packaging & Done Criteria

> Scope: Rust host 构建矩阵、Node SEA 单文件分发评估、CI 集成、原型完成标准。  
> Corresponds to index: [Architecture & Data Flow](../2026-06-25-backend-architecture-evolution-phase3.md)

---

## 1. Component Overview

Packaging 部分解决两个问题：
1. **开发构建**：`ody-host` 如何与 TS monorepo 一起构建、测试、跑通原型。
2. **发布形态评估**：能否把 `ody-host` 二进制嵌入 Node SEA，实现用户视角的"单文件"分发。

原型阶段**优先保证开发构建跑通**；SEA 嵌入作为 optional evaluation，不阻塞核心交付 [C:USER]。

---

## 2. Typed Interfaces / Build Artifacts

### 2.1 Build outputs

```
rust-ody/
├── target/
│   └── release/
│       └── ody-host                    // macOS/Linux executable [C:INFERRED]
│       └── ody-host.exe                // Windows executable [C:INFERRED]
│
apps/ody-code/
└── dist/
    └── ody                             // Node SEA blob (TS TUI) [C:INFERRED]
```

### 2.2 SEA asset manifest（评估项）

```typescript
interface NativeAssetManifest {
  readonly odyHost: {
    readonly darwin_arm64: string;   // path to ody-host-darwin-arm64 inside SEA
    readonly darwin_x64: string;
    readonly linux_x64: string;
    readonly win32_x64: string;
  };
}
```

### 2.3 Host binary resolver

```typescript
async function resolveHostBinary(options: {
  readonly explicitPath?: string;
  readonly preferSeaAsset?: boolean;
}): Promise<string>;
```

---

## 3. Algorithms

### 3.1 Development build flow

```
1. cd rust-ody && cargo build -p ody-host --release
2. pnpm install                  // if not already
3. pnpm -C apps/ody-code build   // build TUI bundle
4. Run prototype:
   - Terminal A: ./rust-ody/target/release/ody-host serve --stdio
   - Terminal B: pnpm -C apps/ody-code start --host-stdio --host-binary ./rust-ody/target/release/ody-host
   OR combined:
   - pnpm -C apps/ody-code start --host=rust
```

### 3.2 `ody-host` CLI argument parsing

```
INPUT: argv
OUTPUT: HostConfig

1. --stdio              -> TransportMode::Stdio
2. --socket-path PATH   -> TransportMode::UnixSocket { path: PATH }
3. --tcp-host HOST --tcp-port PORT -> TransportMode::TcpSocket { host, port }
4. --config PATH        -> HostConfig.config_path = Some(PATH)
5. --home DIR           -> HostConfig.home_dir = DIR
6. --log-level LEVEL    -> HostConfig.log_level
7. IF no transport flag -> default to Stdio [C:USER]
8. IF config_path provided -> load_config_file(config_path)
   ELSE IF ~/.ody/ody.toml exists -> load_config_file(home_dir/ody.toml)
   ELSE IF ~/.ody/ody.json exists -> load_config_file(home_dir/ody.json)
   ELSE use default config
```

### 3.3 SEA embed & extract flow（评估项）

```
BUILD TIME:
1. Build ody-host for target platform.
2. Run `node --experimental-sea-config sea-config.json` to include:
   - apps/ody-code dist bundle (TUI)
   - ody-host-<platform> binary as asset
   - existing native assets (ody-crypto.node, koffi, etc.)
3. Output single `ody` executable.

RUN TIME:
1. User runs `ody` (single file).
2. SEA bootstrap extracts assets to temp dir on first run.
3. TUI starts and spawns temp_dir/ody-host with stdio transport.
4. On exit, optionally clean up temp files.
```

### 3.4 CI integration

```
1. New GitHub Actions job: `rust-host-smoke`:
   - Checkout
   - Install Rust toolchain (stable)
   - cargo test -p ody-host
   - cargo build -p ody-host --release
   - Run cross-language smoke test (TS test spawns Rust host)
2. Existing `ody-code` CI job extended with:
   - Download/built ody-host artifact
   - Run TUI connector tests against ody-host
3. Optional: platform matrix build for ody-host (linux/mac/windows)
```

---

## 4. Call-Site Integration

### 4.1 `rust-ody/Cargo.toml`

```toml
[workspace]
members = [
    "crates/ody-rust",
    "crates/ody-crypto",
    "crates/ody-host",
]

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["stream", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
```

### 4.2 `rust-ody/crates/ody-host/Cargo.toml`

```toml
[package]
name = "ody-host"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { workspace = true }
reqwest = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
clap = { version = "4", features = ["derive"] }
toml = "0.8"
ody-rust = { path = "../ody-rust" }
ody-crypto = { path = "../ody-crypto" }
```

### 4.3 `package.json` root scripts

```json
{
  "scripts": {
    "build:host": "cd rust-ody && cargo build -p ody-host --release",
    "test:host": "cd rust-ody && cargo test -p ody-host",
    "proto:rust-host": "pnpm run build:host && pnpm -C apps/ody-code start --host=rust"
  }
}
```

### 4.4 `apps/ody-code/scripts/native/native-assets.ts`

原型阶段评估项：在 native asset manifest 中增加 `odyHost` entry，并在 `loadNativePackage`/`smoke.ts` 中处理其提取与校验。

---

## 5. Error Handling（局部）

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| `cargo build` fails | CI job fails | No artifact produced | Fix Rust code / dependencies |
| `ody-host` binary missing at TUI launch | Print "ody-host not found" + exit(1) | TUI does not start | Build host or specify --host-binary |
| SEA asset extraction fails | TUI falls back to searching PATH for `ody-host` | May still work if host installed separately | Fix SEA config or install host separately |
| Platform mismatch (e.g. arm64 binary on x64) | Print clear error + exit(1) | TUI does not start | Build for correct target |

---

## 6. Done Criteria

### 6.1 Must-pass commands

1. `cargo test -p ody-host` — all unit tests green.
2. `cargo build -p ody-host --release` — produces executable.
3. `pnpm vitest run packages/node-sdk/src/__tests__/rust-host-connect.test.ts` — TS client能连接 Rust host 并调用 `getCoreInfo`/`createSession`/`listSessions`/`closeSession`。
4. End-to-end manual:
   ```bash
   pnpm run build:host
   pnpm -C apps/ody-code start --host=rust --host-stdio
   ```
   在 TUI 中创建会话、发送 prompt、看到 assistant 回复（真实 LLM 或 mock）。
5. ADR 文档完成并通过 review（见 index §1）。

### 6.2 Optional evaluation（不阻塞 Done）

1. Node SEA 单文件打包成功，且能提取 `ody-host` 并 spawn。
2. Cross-platform build matrix（linux/mac/windows） artifacts produced.
3. Performance baseline：Rust host 常驻内存 vs TS Core worker 常驻内存对比。

---

## 7. Local Test Notes

### Must-pass assertions

1. `rust-host-builds` — `cargo build -p ody-host --release` exits 0.
2. `rust-host-cli-help` — `ody-host --help` prints transport/config options.
3. `rust-host-stdio-ready` — spawn `ody-host serve --stdio`，stderr 输出 ready message 格式与 TS `ReadyMessage` 匹配。
4. `ts-connects-stdio` — `SDKRpcClient.connect({ transport: 'stdio' })` with binary path succeeds.
5. `ts-connects-socket` — `SDKRpcClient.connect({ transport: { socketPath } })` succeeds.

### Must-reject assertions

1. `ody-host` with unknown flag exits non-zero.
2. `ody-host --socket-path /occupied` fails with clear "address already in use" error.
3. TS client with wrong binary path fails fast with `ENOENT` message.
