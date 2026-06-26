# Phase 2-E `ody-crypto` Native 模块实现计划

**Goal:** 用 `napi-rs` 实现首个 Native Rust 模块 `ody-crypto`，为 MCP OAuth 提供 PKCE / 随机字节 / SHA-256 / JWT 校验能力，默认启用并在加载失败时静默降级到 TS 实现。

**Architecture:** 在 `rust-ody` 新增 `crates/ody-crypto` crate，暴露四个原子函数；`packages/ody-crypto` 作为 TS 宿主包负责平台探测、`.node` 加载与 TS fallback；MCP OAuth service 改为手动构造授权 URL（PKCE）并复用 SDK `exchangeAuthorization()` 换 token，id_token 由新增模块校验；平台子包与 native asset 收集器按现有 clipboard/koffi 模式接入 SEA。

**Tech Stack:** Rust (napi-rs v3, rand, sha2, base64, jsonwebtoken), TypeScript (Node 24, pnpm workspace, vitest, tsdown, jose, pkce-challenge), SEA native asset pipeline。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

```text
rust-ody/
├── Cargo.toml                          # workspace 定义
├── crates/
│   ├── ody-rust/                       # 现有 Wasm PoC 迁移至此
│   └── ody-crypto/
│       ├── Cargo.toml
│       ├── build.rs                    # napi build 注册
│       └── src/
│           ├── lib.rs                  # napi 导出 + 算法实现
│           └── tests/                  # Rust 单元测试
├── build.sh                            # 更新为 workspace 构建
└── build-crypto.sh                     # 当前平台 .node 构建
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
packages/ody-crypto/package.json                      # build:native 脚本
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
| 不追求 win32-arm64 / linux-musl CI 覆盖 | Task 15 | covered（本地/release 处理） |

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | 2026-06-25-backend-architecture-evolution-roadmap/rust.md | Rust crate 与 napi 绑定 | done |
| 2 | 2026-06-25-backend-architecture-evolution-roadmap/ts-loader.md | TS 宿主包与平台子包 | done |
| 3 | 2026-06-25-backend-architecture-evolution-roadmap/oauth.md | MCP OAuth 集成 | done |
| 4 | 2026-06-25-backend-architecture-evolution-roadmap/native-sea-ci.md | Native asset / SEA / CI / 报告 | done |

## Global Self-Review

- [ ] 1. **Spec-coverage table**: 所有设计需求已映射到 Task 1–Task 16；无 GAP。
- [ ] 2. **Placeholder scan**: 索引与四个 Part 文件均无 TODO/TBD；所有“后续处理”均明确为 no-op 或本地/release 构建，未写入占位任务。
- [ ] 3. **No phantom tasks**: 每个 Task 都产生可验证的文件变更或测试，无 `--allow-empty`。
- [ ] 4. **Dependency soundness**: 所有 `Depends on:` 均指向更早的 Task 或已完成 Part；Part 4 内部 Task 13/14/15/16 的依赖均已在 Task 12/14 中满足。
- [ ] 5. **Caller & build soundness**: Task 13 改变 `runNativeAssetSmokeIfRequested` 签名（增加可选参数），已确认 `apps/ody-code/src/main.ts:129` 调用不变，且任务内包含 `pnpm run typecheck` 全树检查；无其他共享签名变更跨多个任务。
- [ ] 6. **Test-the-risk**: Task 4/5（Rust JWT/PKCE）、Task 6（loader fallback）、Task 9/11（OAuth 流程）、Task 12/13（native asset 注册与 smoke）均包含行为断言，覆盖状态突变、边界输入与失败路径。
- [ ] 7. **Type consistency**: `NativeAssetOptions`、`PkceChallenge`、`IdTokenExpected` 等类型与函数名在 Part 之间保持一致；Part 4 引用的包名与 Part 2 创建的 `package.json` 一致。
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following areas:
- /Users/ranwei/workspace/ody-code/apps/ody-code/scripts/native (priority: important)
- /Users/ranwei/workspace/ody-code/apps/ody-code/src (priority: important)
- /Users/ranwei/workspace/ody-code/apps/ody-code/src/native (priority: important)
- /Users/ranwei/workspace/ody-code/packages/mcp-host/src/oauth (priority: important)

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. If the interface
requires authentication, supply a valid credential so the authorized path is
exercised and also assert the unauthorized case (401/403). You may also use the
RunE2ETests tool to scaffold and run E2E tests.

