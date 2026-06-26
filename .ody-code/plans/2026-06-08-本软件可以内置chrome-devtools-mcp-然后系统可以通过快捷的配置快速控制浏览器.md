# 内置 Chrome DevTools MCP 浏览器自动化 — 执行计划

**Goal:** 将 `chrome-devtools-mcp` 以内置 MCP server 形式集成到 `ody-code`，用户无需额外安装即可通过浏览器自动化工具控制本地 Chrome。

**Architecture:** 引入 `BuiltInMcpRegistry` 注册表（独立于插件系统），在 `CoreImpl.createSession` 的 MCP 配置合并阶段注入内置 server 配置；`resolveBuiltInRoot` 在运行时按 native 二进制 → npm 包 → 开发 repo 的顺序定位 vendored 源码；`ChromeTraceRecorder` 在 `McpConnectionManager.callTool` 结果路径旁路录制操作轨迹。整个功能默认启用，可通过 `config.toml` 的 `[browser]` 段禁用或调整端口。

**Tech Stack:** TypeScript, Node.js 24, zod, vitest, pnpm workspace, tsdown (native SEA bundle), yazl (zip packaging).

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure (new / modified)

| Path | Responsibility |
|---|---|
| `packages/agent-core/src/config/schema.ts` | 新增 `BrowserConfigSchema`；注入 `KimiConfigSchema`/`KimiConfigPatchSchema` |
| `packages/agent-core/src/mcp/built-in/registry.ts` | `BuiltInMcpRegistry` 类：注册/过滤内置 MCP server |
| `packages/agent-core/src/mcp/built-in/resolve-root.ts` | `resolveBuiltInRoot(serverName)` 路径解析算法 |
| `packages/agent-core/src/mcp/built-in/chrome-devtools.ts` | `ChromeDevToolsServer` 定义（启动配置 + envResolver） |
| `packages/agent-core/src/mcp/built-in/index.ts` | 统一导出 |
| `packages/agent-core/src/rpc/core-impl.ts` | `CoreImpl` 新增 `builtInMcpRegistry` 字段、`mergeBuiltInMcpConfig` 方法、构造函数注册 |
| `packages/agent-core/src/agent/permission/policies.ts` *(新建)* | 浏览器工具权限策略（首次 session 确认） |
| `packages/agent-core/src/mcp/trace-recorder.ts` | `ChromeTraceRecorder` 旁路录制 |
| `packages/agent-core/src/mcp/connection-manager.ts` | `callTool` 后调用 trace recorder |
| `apps/ody-code/built-in/chrome-devtools/` | vendored `chrome-devtools-mcp` 上游源码 |
| `apps/ody-code/package.json` | `files` 数组新增 `"built-in"` |
| `apps/ody-code/scripts/native/package.mjs` | zip 打包加入 `built-in/` 目录 |
| `packages/agent-core/test/mcp/built-in/registry.test.ts` | `BuiltInMcpRegistry` 单元测试 |
| `packages/agent-core/test/mcp/built-in/resolve-root.test.ts` | `resolveBuiltInRoot` 单元测试 |
| `packages/agent-core/test/config/browser-config.test.ts` | `BrowserConfigSchema` 解析测试 |
| `packages/agent-core/test/mcp/connection-manager.test.ts` | 追加内置 server 集成测试 |
| `packages/agent-core/test/mcp/trace-recorder.test.ts` | `ChromeTraceRecorder` 测试 |

---

## Dependency Overview

```
Phase A: 配置与注册表基础
  Task 1: BrowserConfigSchema
  Task 2: BuiltInMcpRegistry + resolveBuiltInRoot
  Task 3: ChromeDevToolsServer 定义

Phase B: Core 集成与权限
  Task 4: CoreImpl mergeBuiltInMcpConfig + 注册表初始化
  Task 5: 浏览器工具权限策略

Phase C: 轨迹与分发
  Task 6: ChromeTraceRecorder
  Task 7: vendored chrome-devtools-mcp 源码
  Task 8: package.json + native package.mjs 分发配置

Phase D: 端到端测试与验证
  Task 9: 全树 typecheck + 现有测试回归
  Task 10: Session 集成测试 + smoke test
```

---

## Risks & Open Questions

| # | Risk | Mitigation in plan |
|---|---|---|
| 1 | `chrome-devtools-mcp` 上游许可证不允许 vendored 分发 | Task 7 之前必须确认上游 LICENSE 为 Apache-2.0 或兼容许可证 |
| 2 | Native SEA bundle 无法访问外部 `built-in/` 目录 | Task 8 修改 `package.mjs` 将 `built-in/` 打进 zip；运行时 `resolveBuiltInRoot` 优先检查 `dirname(process.execPath)` |
| 3 | Chrome 未启动时 MCP server 启动失败影响用户体验 | 设计已确认延迟启动（`McpConnectionManager` 的现有行为）+ 状态面板提示 |
| 4 | 截图/轨迹数据磁盘空间占用 | `traceRetentionDays` 配置项 + 按 session 隔离；不实现自动清理（deferred） |
| 5 | 跨平台路径解析差异 | `resolveBuiltInRoot` 使用 `pathe`/`node:path`；Task 10 的 smoke test 在 macOS 验证 |

---

## Spec-Coverage Table (preliminary)

| 设计章节 | 覆盖状态 | 对应 Task(s) |
|---|---|---|
| Scope In: 内置 MCP server 集成 | covered | Task 3, 4 |
| Scope In: 26+ 工具暴露 | covered | Task 3, 4, 7 |
| Scope In: 连接本地 Chrome (port 9222) | covered | Task 3, 1 |
| Scope In: 随 CLI 二进制发布 | covered | Task 7, 8 |
| Scope In: 首次 Session 权限确认 | covered | Task 5 |
| Scope In: Chrome 不可用时延迟启动 | covered | Task 4 (复用现有 McpConnectionManager) |
| Scope In: 完整轨迹录制 | covered | Task 6 |
| Scope In: 默认启用可禁用 | covered | Task 1, 2 |
| Scope Out: 自动启动 Chrome | no-op | — |
| Architecture: BuiltInMcpRegistry | covered | Task 2 |
| Architecture: ChromeDevToolsServer | covered | Task 3 |
| Architecture: resolveBuiltInRoot | covered | Task 2 |
| Components: Session Trace Recorder | covered | Task 6 |
| Config: BrowserConfigSchema | covered | Task 1 |
| Error: BUILT_IN_ROOT_NOT_FOUND | covered | Task 2, 4 |
| Error: CHROME_NOT_REACHABLE | covered | Task 4 (现有行为) |
| Error: TRACE_WRITE_ERROR | covered | Task 6 |
| Security: 权限模型 | covered | Task 5 |
| Security: 敏感数据隔离 | covered | Task 6 |
| Call-Site 1: CoreImpl mergeBuiltInMcpConfig | covered | Task 4 |
| Call-Site 2: CoreImpl 构造函数注册 | covered | Task 4 |
| Call-Site 3: KimiConfigSchema | covered | Task 1 |
| Call-Site 4: Session trace 钩子 | covered | Task 6 |
| Call-Site 5: 分发路径 | covered | Task 8 |
| Test: BuiltInMcpRegistry | covered | Task 2 |
| Test: resolveBuiltInRoot | covered | Task 2 |
| Test: BrowserConfigSchema | covered | Task 1 |
| Test: Session MCP 连接 | covered | Task 10a |
| Test: Tool 调用端到端 | no-op | 需要真实 Chrome 实例，超出自动化单元测试范围；Task 6 已覆盖 trace 录制 |
| Test: Native 二进制路径 | covered | Task 10b |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-08-本软件可以内置chrome-devtools-mcp-然后系统可以通过快捷的配置快速控制浏览器/core-config.md` | Phase A: 配置与注册表基础 | done |
| 2 | `2026-06-08-本软件可以内置chrome-devtools-mcp-然后系统可以通过快捷的配置快速控制浏览器/core-integration.md` | Phase B: Core 集成与权限 | done |
| 3 | `2026-06-08-本软件可以内置chrome-devtools-mcp-然后系统可以通过快捷的配置快速控制浏览器/distribution.md` | Phase C: 轨迹与分发 | done |
| 4 | `2026-06-08-本软件可以内置chrome-devtools-mcp-然后系统可以通过快捷的配置快速控制浏览器/testing.md` | Phase D: 端到端测试与验证 | done |
