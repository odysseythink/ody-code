# Part 5 — 辅助集成

## Scope

将上游 office-hours 的 telemetry、CLAUDE.md routing、gbrain artifacts sync 移植到 ody-code 的等价机制。

## Interfaces

```typescript
// packages/agent-core/src/office-hours/integrations.ts
export interface OfficeHoursIntegrations {
  track(event: string, properties?: Record<string, unknown>): void;
  appendRoutingRules(claudeMdPath: string): Promise<void>;
  syncArtifacts(designFilePath: string): Promise<void>;
}

export interface TelemetryOfficeHoursEvent {
  readonly event: 'office_hours_started' | 'office_hours_completed' | 'office_hours_resources_shown';
  readonly project_slug: string;
  readonly mode: 'startup' | 'builder';
  readonly signal_count?: number;
  readonly duration_s?: number;
  readonly outcome?: 'success' | 'abort' | 'unknown';
  readonly count?: number;
  readonly categories?: string;
}
```

## Data Flow

```
OfficeHours lifecycle
  │
  ├─► TelemetryClient.track('office_hours_started', ...)
  │     └─► packages/telemetry queue → sink → remote/disk
  │
  ├─► CLAUDE.md routing check (Phase 1 / preamble)
  │     └─► Read CLAUDE.md → if no routing → AskUserQuestion → Edit append
  │
  └─► gbrain artifacts sync (end of session)
        └─► detect .gbrain-source pin → run gbrain index/update if configured
```

## Algorithms

### Telemetry 事件映射

```
function mapUpstreamToTelemetry(event: OfficeHoursAnalyticsEvent): TelemetryOfficeHoursEvent
  const base = { project_slug: event.branch, mode: inferMode(event) }
  switch event.event
    case 'started':
      return { ...base, event: 'office_hours_started' }
    case 'completed':
      return { ...base, event: 'office_hours_completed', duration_s: event.duration_s, outcome: event.outcome }
    case 'resources_shown':
      return { ...base, event: 'office_hours_resources_shown', count: event.count, categories: event.categories }
    default:
      return { ...base, event: 'office_hours_started' }
```

### CLAUDE.md Routing 注入

```
async function ensureRoutingRules(cwd: string, kaos: Kaos): Promise<void>
  const path = join(cwd, 'CLAUDE.md')
  let content = ''
  try
    content = await kaos.readText(path)
  catch (error)
    if not isMissingFileError(error) throw error
    // file does not exist; upstream creates it. In ody-code we keep same behavior.
    content = ''

  if content.includes('## Skill routing') then return

  const shouldAdd = await askUserQuestionOnce(
    'Add office-hours routing rules to CLAUDE.md?',
    [ { label: 'Yes (recommended)', value: 'yes' }, { label: 'No thanks', value: 'no' } ]
  )

  if shouldAdd === 'yes' then
    const rules = `\n\n## Skill routing\n\nWhen the user's request matches an available skill, invoke it via the Skill tool.\n\nKey routing rules:\n- Product ideas/brainstorming → invoke /office-hours\n- Strategy/scope → invoke /plan\n- Architecture → invoke /design\n- Bugs/errors → invoke /investigate\n- QA/testing → invoke /qa\n- Code review → invoke /review\n`
    await kaos.writeText(path, content + rules)
    await gitAddAndCommit(path, 'chore: add office-hours routing rules to CLAUDE.md')
```

### gbrain Artifacts Sync

```
async function syncArtifactsIfConfigured(cwd: string, designFilePath: string): Promise<void>
  const repoTop = await gitTopLevel(cwd)
  if repoTop === null then return

  const gbrainSourcePath = join(repoTop, '.gbrain-source')
  let gbrainConfigExists = false
  try
    await access(join(homedir(), '.gbrain', 'config.json'))
    gbrainConfigExists = true
  catch
    gbrainConfigExists = false

  if not gbrainConfigExists then return

  const pinExists = await fileExists(gbrainSourcePath)
  if not pinExists then
    // Prompt is allowed to suggest running /sync-gbrain; actual sync is out of scope for a single session.
    return

  // If gbrain MCP tool is available, call it; otherwise best-effort shell gbrain command.
  if gbrainToolAvailable() then
    await callGbrainIndex(designFilePath)
  else
    try
      await exec('gbrain', ['index', designFilePath])
    catch
      // best-effort; do not block exit
```

## Call-Site Integration

### 1. Telemetry [C:INFERRED]

复用 `packages/telemetry`：

```typescript
// In run-office-hours.ts
import { track, withTelemetryContext } from '@odysseythink/ody-telemetry';

// On start
track('office_hours_started', { project_slug: slug });

// On completion
withTelemetryContext({ sessionId: tui.getCurrentSessionId() })
  .track('office_hours_completed', { project_slug: slug, mode, signal_count, duration_s, outcome });
```

### 2. CLAUDE.md Routing [C:UPSTREAM]

从上游 `office-hours/SKILL.md` 的 "If `HAS_ROUTING` is `no`..." 段移植，但改为：
- 不检查 gstack 配置 `routing_declined`；改用本地 marker `~/.ody-code/office-hours/.routing-declined`。
- 路由规则中 `/office-hours` 保留（即使当前入口是 `--office-hours`，未来可能支持命令），其他规则映射到 ody-code 命令。

实现为一个新的 builtin tool：`EnsureClaudeMdRoutingTool`，仅在 office-hours mode 注册：

```typescript
// packages/agent-core/src/tools/builtin/office-hours/ensure-routing.ts
new b.EnsureClaudeMdRoutingTool(this.agent),
```

添加到 `ToolManager.initializeBuiltinTools` 的条件列表中：

```typescript
this.agent.sessionMode?.kind === 'office-hours' && new b.EnsureClaudeMdRoutingTool(this.agent),
```

### 3. gbrain Artifacts Sync [C:INFERRED]

实现为 `SyncOfficeHoursArtifactTool`：

```typescript
new b.SyncOfficeHoursArtifactTool(this.agent),
```

仅在 office-hours mode 注册。prompt 在 Phase 5/6 调用它。

### 4. ToolManager 条件注册 [C:USER]

在 `packages/agent-core/src/agent/tool/index.ts:407-465` 的工具列表中新增：

```typescript
this.agent.sessionMode?.kind === 'office-hours' && new b.EnterOfficeHoursModeTool(this.agent),
this.agent.sessionMode?.kind === 'office-hours' && new b.ExitOfficeHoursModeTool(this.agent),
this.agent.sessionMode?.kind === 'office-hours' && new b.EnsureClaudeMdRoutingTool(this.agent),
this.agent.sessionMode?.kind === 'office-hours' && new b.SyncOfficeHoursArtifactTool(this.agent),
this.agent.sessionMode?.kind === 'office-hours' && new b.AppendBuilderProfileTool(this.agent),
this.agent.sessionMode?.kind === 'office-hours' && new b.AppendLearningTool(this.agent),
this.agent.sessionMode?.kind === 'office-hours' && new b.SearchLearningsTool(this.agent),
```

> 注意：`ToolManager` 在构造时 `sessionMode` 尚未激活，因此不能静态判断。应改为根据 `agent.sessionMode.kind` 动态注册，或在 `setActiveTools` / profile 中控制。更可靠的做法是：始终注册这些工具，但在 tool 内部检查 `sessionMode.kind !== 'office-hours'` 时返回 `isError: true`。

## Error & Degradation

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| Telemetry sink 未 attach | `track()` 事件进入内存 queue | 正常启动后 sink attach 时批量发送 | sink 初始化 |
| CLAUDE.md 写入失败 | tool 返回 isError | 不阻塞流程，prompt 继续 | 修复工作目录权限 |
| 用户拒绝 routing | 写入 `.routing-declined` marker | 未来 session 不再询问 | 删除 marker 可重新提示 |
| gbrain 未安装或未配置 | tool 返回信息性输出 | 设计文档仍保存在本地 | 安装/配置 gbrain |
| git commit routing 失败 | tool 返回 isError | 文件已写入，但未提交 | 用户手动提交 |

## Test Plan

1. **Telemetry 事件**（`packages/telemetry/test/client.test.ts` 新增或 `apps/ody-code/test/cli/office-hours-telemetry.test.ts`）：
   - mock telemetry client
   - 调用 `track('office_hours_started', { project_slug: 'x' })`
   - 验证 `track` 收到正确 event 和 properties

2. **Routing tool**（`packages/agent-core/test/agent/tool/ensure-routing.test.ts` 新增）：
   - 无 CLAUDE.md 时：AskUserQuestion 返回 yes → 文件创建并包含 `## Skill routing`。
   - 已有 routing 时：no-op。
   - 用户拒绝时：写入 marker，不再询问。

3. **Artifact sync tool**（`packages/agent-core/test/agent/tool/sync-artifact.test.ts` 新增）：
   - 无 gbrain 配置 → 返回 "gbrain not configured"。
   - 有配置但无 `.gbrain-source` pin → 提示用户 run `/sync-gbrain`。
   - 有配置且有 pin → 调用 gbrain index（mock）。

## Done Criteria

- `pnpm -F @odysseythink/agent-core test` 中新增 office-hours tool 测试通过。
- `pnpm -F @odysseythink/ody-code test` 中 CLI/telemetry 测试通过。
- 手动验证：office-hours 会话完成后，`office_hours_completed` 事件出现在 telemetry queue（或 sink）。
