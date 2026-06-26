# Phase 2-E `ody-crypto` Native 模块实现计划

**Goal:** 用 `napi-rs` 实现首个 Native Rust 模块 `ody-crypto`，为 MCP OAuth 提供 PKCE / 随机字节 / SHA-256 / JWT 校验能力，默认启用并在加载失败时静默降级到 TS 实现。

**Architecture:** 在 `rust-ody` 新增 `crates/ody-crypto` crate，暴露四个原子函数；`packages/ody-crypto` 作为 TS 宿主包负责平台探测、`.node` 加载与 TS fallback；MCP OAuth service 改为手动构造授权 URL（PKCE）并复用 SDK `exchangeAuthorization()` 换 token，id_token 由新增模块校验；平台子包与 native asset 收集器按现有 clipboard/koffi 模式接入 SEA。

**Tech Stack:** Rust (napi-rs v3, rand, sha2, base64, jsonwebtoken), TypeScript (Node 24, pnpm workspace, vitest, tsdown, jsonwebtoken), SEA native asset pipeline。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

```text
rust-ody/
├── Cargo.toml                          # workspace 定义（已存在，需新增 ody-crypto member）
├── crates/
│   ├── ody-rust/                       # 现有 Wasm PoC
│   └── ody-crypto/                     # 新增 crate
│       ├── Cargo.toml
│       ├── build.rs                    # napi build 注册
│       └── src/
│           ├── lib.rs                  # napi 导出 + 算法实现
│           ├── crypto.rs               # randomBytes / sha256
│           ├── pkce.rs                 # PKCE verifier/challenge
│           ├── jwt.rs                  # verifyIdToken RS256/ES256
│           └── tests/                  # Rust 单元测试
├── build.sh                            # 现有 workspace 构建
└── build-crypto.sh                     # 当前平台 .node 构建（新增）
packages/ody-crypto/
├── package.json
├── tsconfig.json
├── vitest.config.ts
├── src/
│   ├── index.ts                        # 统一导出
│   ├── types.ts                        # 公共类型
│   ├── loader.ts                       # 平台探测与 .node require
│   └── fallback.ts                     # TS fallback 实现
└── test/
    └── loader.test.ts
packages/ody-crypto-<target>/           # 5 个平台子包
├── package.json                        # main=ody-crypto.node, os/cpu 限制
└── ody-crypto.node                     # 构建产物
packages/mcp-host/src/oauth/
├── service.ts                          # 手动 PKCE URL + exchangeAuthorization
├── provider.ts                         # state() 改用 ody-crypto.randomBytes
├── id-token.ts                         # JWKS 拉取 + verifyIdToken 调用
└── test/oauth/service.test.ts          # 改造/新增测试
apps/ody-code/scripts/native/native-deps.mjs          # 注册 ody-crypto
apps/ody-code/src/native/smoke.ts                     # SEA smoke 逻辑
apps/ody-code/test/native/smoke.test.ts               # smoke 单元测试
.github/workflows/native-crypto.yml                   # CI 矩阵构建
.ody-code/reports/2026-06-25-phase-2e-native-sea-cost.md  # 成本报告
```

## Dependency Overview

任务按四个 Part 组织；Part 1 与 Part 2 可在接口类型确定后并行开始，但 Part 3 依赖 `packages/ody-crypto` 可导入，Part 4 依赖平台子包与宿主包注册完成。

```text
Part 1: Rust crate (rust.md)
  Task 1 -> Task 2 -> Task 3 -> Task 4 -> Task 5

Part 2: TS 宿主包与平台子包 (ts-loader.md)
  Task 6 -> Task 8
  Task 5 -> Task 7 -> Task 8

Part 3: MCP OAuth 集成 (oauth.md)
  Task 6 -> Task 9
  Task 6 -> Task 10
  Task 6 -> Task 11

Part 4: Native asset / SEA / CI / 报告 (native-sea-ci.md)
  Task 7 + Task 8 -> Task 12
  Task 12 -> Task 13
  Task 12 -> Task 14
  Task 12 + Task 14 -> Task 15
  Task 12..Task 15 -> Task 16
```

## Risks & Open Questions

- `jsonwebtoken` crate 是否直接接受 JWK JSON 字符串验证 RS256/ES256，需在 Task 4 的 spike 测试中确认；若不行，fallback 方案改为 TS 侧 PEM 转换。
- TS fallback 使用 `jsonwebtoken` + `node:crypto.createPublicKey`，在 Node 24 上验证 RS256/ES256；若后续支持更多算法，需扩展 `verifyIdToken` 的 `alg` 校验。
- 平台子包新增 5 个 workspace 路径，`flake.nix` 的 `workspacePaths`/`workspaceNames` 必须同步更新，否则 Nix 构建静默丢文件。
- SEA 中 `.node` 解压路径与 `require` 行为依赖 `apps/ody-code/src/native/native-require.ts`，已在现有 clipboard/koffi 路径中验证，但 `ody-crypto` 首次接入需在 Task 13 smoke 测试。

## Spec-Coverage Table

| 设计需求 | 覆盖任务 | 状态 |
|---|---|---|
| Rust crate `ody-crypto`：randomBytes / sha256 / pkceChallenge / verifyIdToken | Task 1–Task 5 | covered |
| TS 加载器/封装包 `packages/ody-crypto`，自动平台探测、静默降级 | Task 6 | covered |
| 5 个平台子包（darwin-arm64/x64、linux-arm64/x64、win32-x64） | Task 7, Task 8 | covered |
| `packages/mcp-host/src/oauth/service.ts` 改为手动 PKCE URL + `exchangeAuthorization` | Task 9 | covered |
| `packages/mcp-host/src/oauth/provider.ts` `state()` 调用 `ody-crypto.randomBytes` | Task 10 | covered |
| 新增 `packages/mcp-host/src/oauth/id-token.ts` 校验 id_token | Task 11 | covered |
| `apps/ody-code/scripts/native/native-deps.mjs` 注册 `ody-crypto` | Task 12 | covered |
| SEA native asset smoke 覆盖 `ody-crypto` | Task 13 | covered |
| 新增 `build:native:crypto` 脚本 | Task 14 | covered |
| CI 矩阵构建 ody-crypto 并跑 SEA smoke | Task 15 | covered |
| Phase 2-E Native SEA 成本报告 | Task 16 | covered |
| 不替换 device code flow、不迁移 kosong/kaos、不做 JWE/refresh 签名 | — | no-op |

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-25-backend-architecture-evolution-roadmap-1/rust.md` | Rust crate `ody-crypto` | done |
| 2 | `2026-06-25-backend-architecture-evolution-roadmap-1/ts-loader.md` | TS 宿主包与平台子包 | done |
| 3 | `2026-06-25-backend-architecture-evolution-roadmap-1/oauth.md` | MCP OAuth 集成 | done |
| 4 | `2026-06-25-backend-architecture-evolution-roadmap-1/native-sea-ci.md` | Native asset / SEA / CI / 报告 | done |


---

## Self-Review

- [x] 1. **Spec-coverage table**：见上文「Spec-Coverage Table」。Phase 2-E 设计文档中的 11 项需求均映射到 Task 1–Task 16，剩余 3 项明确 out-of-scope 标注为 no-op；无 GAP。✅ 已验证。
- [x] 2. **Placeholder scan**：全文无 `TODO`/`TBD`/`implement later`；无依赖未决的占位代码；测试代码、实现代码、CI 配置、报告均给出完整内容。✅ rg 扫描 ody-crypto crate、packages/ody-crypto、packages/mcp-host/src/oauth 均为 0 matches。
- [x] 3. **No phantom tasks**：Task 1–Task 16 每个都产生可验证变更（文件创建/修改、测试通过、构建产物、workflow、报告），无 `--allow-empty` 或“已在前序任务完成”的托辞。
- [x] 4. **Dependency soundness**：
  - Part 1 内 Task 1→2→3→4→5 顺序依赖。
  - Part 2 Task 6、Task 7 并行；Task 8 依赖 6+7。
  - Part 3 Task 9、Task 10 依赖 Part 2 Task 6；Task 11 依赖 9+10。
  - Part 4 Task 12 依赖 Part 2 Task 7+8；Task 13、Task 14 依赖 Task 12；Task 15 依赖 Task 12+14；Task 16 依赖 Task 12–15。
  - 跨 Part 依赖均通过 `<id>/<part>.md: Task N` 形式显式声明。
- [x] 5. **Caller & build soundness**：
  - Task 1 修改 `rust-ody/build.sh`，限制为 `-p ody-rust`，需运行 `cargo test -p ody-rust` 验证原 Wasm PoC 未坏。
  - Task 8 修改 `flake.nix` 的 `workspacePaths`/`workspaceNames`，必须以全 workspace `pnpm run typecheck` 验证。
  - Task 9 修改 `packages/mcp-host/package.json` 新增依赖，需 `pnpm install`。
  - Task 11 以全 workspace `pnpm run typecheck` 收尾。
  - Task 13 给 `runNativeAssetSmokeIfRequested` 增加可选参数，已验证 `apps/ody-code/src/main.ts` 调用不变；任务内包含 `pnpm run typecheck`。
- [x] 6. **Test-the-risk**：
  - Rust 侧：PKCE 长度边界（43/128）、JWT 过期/篡改、随机字节长度/唯一性、sha256 已知向量均有 must-reject/must-pass 断言。
  - TS 侧：loader 的 native/fallback 分支、fallback 的 PKCE 边界与 sha256、OAuth 的 state 长度/唯一性、id_token 有效/篡改后 token 未保存均有断言。
  - Native asset：resolveTargetDeps 解析 ody-crypto 包名、smoke 在 ody-crypto 缺失时失败/存在时通过均有断言。
- [x] 7. **Type consistency**：
  - Rust napi 导出对象字段（`code_verifier`、`code_challenge`、`issuer`、`audience`、`max_age_seconds`、`sub`、`iss`、`aud`、`exp`、`iat`、`extra`）与 TS `OdyCrypto`/`IdTokenClaims` 一致。
  - `IdTokenClaims.aud` 的 TS 类型 `string | string[]` 对应 Rust `Either<String, Vec<String>>`。
  - `verifyIdToken` 的 TS fallback 与 Rust 实现均使用 JWK JSON 字符串作为公钥输入，算法白名单均为 RS256/ES256。
