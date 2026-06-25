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

## TS fallback vs Native 基准

使用 `packages/ody-crypto/scripts/native-vs-fallback-bench.mjs` 在以下环境跑 2s 测量窗口（含 500ms warmup）：

- 平台：`darwin-arm64`
- Node：`22.22.3`
- 负载：
  - `randomBytes(32)`
  - `sha256(1 KB hex string)`
  - `pkceChallenge()`（默认 43 字符 verifier）
  - `verifyIdToken(RS256)`（2048-bit RSA JWK）
  - 混合负载：1000 次 OAuth 授权流（randomBytes + pkceChallenge + verifyIdToken）

### 结果

| 函数 | native 中位 latency | TS fallback 中位 latency | native / fallback | native throughput | TS fallback throughput |
|---|---|---|---|---|---|
| `randomBytes(32)` | 209 ns/op | 584 ns/op | **0.36×** | 3.47 M ops/s | 1.51 M ops/s |
| `sha256(1 KB)` | 3,917 ns/op | 958 ns/op | **4.09×** | 0.25 M ops/s | 0.99 M ops/s |
| `pkceChallenge()` | 1,208 ns/op | 1,125 ns/op | **1.07×** | 0.83 M ops/s | 0.76 M ops/s |
| `verifyIdToken(RS256)` | 13,959 ns/op | 20,667 ns/op | **0.68×** | 0.07 M ops/s | 0.05 M ops/s |
| **mixed 1000 flows** | **15.95 ms** | **25.47 ms** | **0.63×** | — | — |

### 关键发现

1. **native 在小字符串哈希上反而更慢**。`sha256(1 KB)` 的 native 实现比 `node:crypto` 慢约 4×，主因是 JS string → Rust string → JS hex string 的跨边界拷贝与编码开销，而 `node:crypto` 的 C++ 绑定路径更短、更针对 string/buffer 优化。
2. **随机数与 JWT 校验 native 更快**。`randomBytes` 快约 2.8×，`verifyIdToken` 快约 1.5×。
3. **PKCE 两者基本持平**。该函数 dominated by JS 字符串拼接与 base64url，Rust 优化空间不大。
4. **OAuth 端到端混合负载 native 快约 1.6×**。这与 MCP OAuth 场景直接相关，说明在真实调用组合中 native 仍有净收益。

> 注意：上述数字高度依赖输入大小与绑定实现。若将 `sha256` 改为处理大 Buffer（如 1 MB），native 的相对表现会显著改善；但当前 OAuth 场景实际输入普遍偏小。

## W（Wasm）方案估算

假设将 Rust `ody-crypto` 编译为 `wasm32-unknown-unknown` 并通过 JS host 调用：

| 维度 | 估算 | 说明 |
|---|---|---|
| 构建产物 | 单文件 ~150–400 KB `.wasm` | 无需 per-platform `.node`，但需 bundler/loader 处理 |
| 构建复杂度 | 中 | 一次编译全平台；但 Rust crypto 依赖（如 `ring`）对 wasm 支持有限，需替换或补丁 |
| 运行时启动 | 中-高 | Wasm 实例化 + 内存分配；SEA 中需把 wasm 作为 asset 注入 |
| 小输入 latency | 略高于 native | JS ↔ Wasm memory copy 替代 napi binding，仍有边界开销 |
| 大输入 throughput | 接近 native | 计算密集时 wasm 接近 native 速度 |
| 功能完整性 | 中 | `jsonwebtoken` 依赖的 RSA/ECDSA 在 wasm 中可用，但调试和算法支持弱于 Node `crypto` |
| 安全/合规 | 中 | 失去 Node `crypto` 的 FIPS/系统熵保证，需自行保证随机源质量 |

**结论**：Wasm 能解决 native 的多平台构建痛点，但在 Node 24 环境下对当前这组小输入加密函数未必比 TS fallback 更快；其主要价值是统一构建矩阵，而非性能。对于 ody-crypto 这类强依赖系统熵与标准 JWK 校验的功能，Wasm 的边际收益不足以抵消切换成本。

## N 增量 vs H 等待结论

| 方案 | 代表 | 收益 | 成本 | 适用场景 |
|---|---|---|---|---|
| **N 增量** | 继续为每个新 I/O 模块加 napi-rs native 模块 | 可立即获得 Rust 生态能力；大负载下性能优秀 | 每目标构建、SEA asset、CI matrix、JS↔Rust 边界调试 | 大负载、计算密集、Node/TS 无法提供的能力 |
| **W 中转** | 用 Wasm 统一交付 | 单构建产物；性能接近 native | wasm loader、内存拷贝、熵/算法限制 | 计算密集、不需要系统能力、想避免 native 矩阵 |
| **H 等待** | 等 Rust Host 落地，I/O 直接跑在 Host 内 | 无 JS 边界；Rust 原生性能；统一运行时 | 需要 Rust Host 本身完成并稳定 | 几乎所有高频 I/O、小数据调用 |

**决策结论**：

- **后续 I/O 模块不建议继续走 N 增量**，除非满足以下任一条件：
  1. 该模块处理的数据包足够大（>100 KB），能把 JS↔Rust 边界开销摊薄到 <10% 总耗时；
  2. Node/TS 没有等价实现（例如特定 Rust crate、系统级能力、确定性跨平台行为）。
- **推荐攒着等 H（Rust Host）**。本次基准显示，即使是 crypto 这种看起来"应该 native 更快"的场景，小输入下也可能被 `node:crypto` 反超；只有消除 JS↔Rust 边界的 Rust Host 才能稳定发挥 native 优势。
- **ody-crypto 作为 Phase 2 试点保留**：它已经在 CI/SEA 中跑通，且在 OAuth 混合负载中有 1.6× 净收益；但不再扩大 native 表面，后续加密需求优先评估 TS fallback 或 Rust-Host 路线，而非新增 napi-rs 模块。

## 结论

Phase 2-E 达到目标：MCP OAuth 关键加密路径已迁移到 Native Rust，并在加载失败时静默降级。本次补充的基准数据表明，native 收益在小输入场景下会被绑定开销显著侵蚀，因此**后续 I/O 模块应优先等待 Rust Host，而非继续 napi-rs 增量**。建议合并后继续观察 CI 稳定性、SEA 二进制大小，并在下一次 I/O 模块立项前复用 `packages/ody-crypto/scripts/native-vs-fallback-bench.mjs` 的评估方法。
