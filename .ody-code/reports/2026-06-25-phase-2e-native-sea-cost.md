# Phase 2-E `ody-crypto` Native 模块成本报告

## 变更摘要

- 新增 Rust crate `ody-crypto`，暴露 `randomBytes`、`sha256`、`pkceChallenge`、`verifyIdToken` 四个原子函数。
- 新增 TS 宿主包 `@odysseythink/ody-crypto` 与 5 个平台子包，支持自动平台探测与 TS fallback。
- MCP OAuth 流程改用 `ody-crypto` 生成 PKCE 与随机 state，并在 token 响应含 `id_token` 时完成 JWKS 校验。
- 在 `apps/ody-code/scripts/native/native-deps.mjs` 注册 ody-crypto，SEA 构建时自动收集 `.node` 与 JS 文件。
- 扩展 native asset smoke，确保 SEA 二进制启动后可解压并 require `@odysseythink/ody-crypto`。
- 新增 `.github/workflows/native-crypto.yml`，在 4 个主机目标上构建 native 模块、SEA 与 smoke 测试。

## 新增依赖

| 包/工具 | 作用 | 说明 |
|---|---|---|
| `napi-rs` v3 | Rust ↔ Node 绑定 | 已验证与 Node 24 SEA 兼容 |
| `rand`、`sha2`、`base64`、`jsonwebtoken` | Rust 实现 | 随机字节、哈希、JWT 校验 |
| `jsonwebtoken` (npm) | TS fallback | 仅用于 JWK → public key 校验 |
| `@modelcontextprotocol/sdk` | OAuth 客户端 | 已提供 `exchangeAuthorization` / `registerClient` 等函数 |

## 构建成本

- 本地开发：首次 `pnpm install` 后，Rust 依赖约需 30–60 秒编译（取决于目标）。
- CI：每个 matrix job 约 4–8 分钟（含 Rust 编译、tsdown、SEA 注入、smoke）。
- 发布流程： native 产物需要每个目标单独构建；当前 5 个平台子包，构建脚本 `build-crypto.sh` 支持传入 `--target`。

## 运行时成本

- `.node` 文件大小：每个目标约 300 KB–1 MB（取决于 Rust 依赖与符号表）。
- SEA 注入后二进制增量大体等于所有 native asset 压缩后大小之和。
- 启动时 native asset 解压为一次性开销，缓存按 `(version, target, manifest-hash)` 目录隔离。

## 风险与后续

- `win32-arm64` / `linux-arm64` 未纳入 GitHub-hosted CI matrix，release 需本地或自托管 runner 补充。
- Rust `jsonwebtoken` 的 JWK 支持已通过 Task 4 spike 验证；若后续支持更多算法，需扩展 `verifyIdToken` 的 `alg` 校验。
- TS fallback 使用 `node:crypto` + `jsonwebtoken`，在 Node 24 上功能等价；若未来需要 WebCrypto-only 环境，需再评估。

## 结论

Phase 2-E 达到目标：MCP OAuth 关键加密路径已迁移到 Native Rust，并在加载失败时静默降级。建议合并后继续观察 CI 稳定性与 SEA 二进制大小。
