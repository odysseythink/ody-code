# Design→Plan Handoff 不再携带完整设计正文

> **审计级别**: Deep [C:USER]
> **日期**: 2026-06-12

---

## Scope In/Out

### In

| # | 功能 | 范围 | 来源 |
|---|------|------|------|
| 1 | `ExitDesignModeTool` tool result 去正文 | 输出中移除 `## Approved Design:\n${design}`，只保留状态说明与路径 | [C:USER] |
| 2 | design→plan handoff artifact 瘦身 | `SessionMode._pendingHandoffForPlan` 从 `{ content, path }` 改为 `{ path, filename, selectedLabel? }` | [C:USER] |
| 3 | plan 模式首条 reminder 去正文 | `DesignModeInjector.designToPlanHandoffReminder` 只引用设计文件路径/文件名 | [C:USER] |
| 4 | 测试同步更新 | 更新 `exit-design-mode.test.ts` 与 `design-mode.test.ts` 中断言 | [C:USER] |

### Out

| # | 功能 | 延后理由 |
|---|------|---------|
| 1 | 新增 `summary` 字段 | 用户明确选择不要摘要 [C:USER] |
| 2 | 修改 plan-mode 通用 workflow/合约 | 只在 handoff reminder 中引用路径，不改动通用 workflow [C:USER] |
| 3 | feature flag 保护 | 用户选择直接修改默认行为 [C:USER] |
| 4 | 修改 plan→normal handoff | 该设计只解决 design→plan 的上下文膨胀问题 [C:USER] |

---

## Architecture

```
User approves design via ExitDesignMode
  │
  ▼
ExitDesignModeTool.execution(args, metadata)
  ├─ resolveDesign() ──→ SessionMode.data() ──→ { content, path }
  ├─ handoffToPlan() ──→ SessionMode.handoffTo('plan', { selectedLabel? })
  │                         ├─ SessionMode.exit()  (records session_mode.exit, sets _lastCompletedDesignFilePath)
  │                         ├─ SessionMode.enter('plan')
  │                         └─ _pendingHandoffForPlan = { path, filename, selectedLabel? }
  └─ return tool result (no design content)
         │
         ▼
Next turn: DesignModeInjector.getInjection()
  ├─ detects design no longer active
  ├─ consumePendingHandoffForPlan() ──→ { path, filename, selectedLabel? }
  └─ designToPlanHandoffReminder(path, filename, selectedLabel?)
         │
         ▼
Inject reminder into plan partition (no design content)
```

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|-----------|------------|-----------------|---------------|
| 1 | `pathe.basename` 对非空路径始终返回合法文件名，不会抛异常 | High | 极低；若抛异常会导致 handoff 失败 | 已确认 `pathe.basename` 行为稳健，且 handoffTo 会判断 `data.path.length > 0` |
| 2 | plan 模式 LLM 看到文件名/路径后会主动 `Read` 设计文件 | Medium | 中；若 LLM 不读文件，计划可能偏离 | 运行后观察 plan 模式首条消息是否包含 `Read(...)` 调用 |
| 3 | 现有 `exit-design-mode.test.ts` 与 `design-mode.test.ts` 是依赖完整设计正文的唯一测试 | Medium | 低；遗漏的测试会在 CI 中失败 | 在相关包运行 `pnpm test` 并修复失败断言 |
| 4 | 用户接受 `path` 保持为 `SessionMode` 返回的原始绝对路径，不做脱敏 | High | 低；当前 tool result 已展示原始路径 | 已确认用户选择“保持原样” [C:USER] |

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|-----------|--------|-----------|
| 1 | LLM 进入 plan 模式后未主动读取设计文件，导致计划偏离 | 中 | 高 | reminder 明确引用文件名；plan-mode workflow 本身要求“先理解再规划” |
| 2 | 现有测试/快照依赖旧输出字符串 | 高 | 低 | 同步更新断言，CI 跑相关测试包 |
| 3 | `basename` 对空路径或特殊路径返回异常值 | 低 | 中 | handoffTo 在存 artifact 前判断 `data.path.length > 0` |


---

## Interfaces & Types

### `SessionMode` private state

```ts
private _pendingHandoffForPlan: {
  path: string;
  filename: string;
  selectedLabel?: string;
} | null = null;
```

**Contract**: 缓存 design→plan 切换时需要注入下一条 plan 模式 reminder 的最小元数据，不再保存设计全文 [C:USER]。

### `SessionMode.consumePendingHandoffForPlan`

```ts
consumePendingHandoffForPlan(): {
  path: string;
  filename: string;
  selectedLabel?: string;
} | null
```

**Contract**: 取出并清空 pending handoff；返回 null 表示没有待注入的 handoff [C:INFERRED]。

### `SessionMode.handoffTo`

```ts
async handoffTo(
  target: 'plan' | 'normal',
  opts?: { selectedLabel?: string },
): Promise<void>
```

**Contract**: 退出当前模式并链式进入目标模式；`target === 'plan'` 时只保存设计文件路径/文件名/已选方案标签，不保存内容 [C:USER]。

### `DesignModeInjector.designToPlanHandoffReminder`

```ts
function designToPlanHandoffReminder(
  path: string,
  filename: string,
  selectedLabel?: string,
): string
```

**Contract**: 生成 design 退出、plan 接管后的首条系统 reminder，只引用设计文件，不嵌入正文 [C:USER]。

### `ExitDesignModeTool.formatDesignHandoffOutput`

```ts
function formatDesignHandoffOutput(
  path: string,
  selectedLabel?: string,
): string
```

**Contract**: 渲染 `ExitDesignMode` 的 tool result，不再接收也不输出完整设计正文 [C:USER]。

---

## Algorithms

### `SessionMode.handoffTo('plan', opts)` 伪代码

```
function handoffTo(target, opts):
  data = await this.data()

  if target === 'plan':
    artifact = null
    if data !== null and data.path.length > 0:
      artifact = {
        path: data.path,
        filename: basename(data.path),
        selectedLabel: opts?.selectedLabel,
      }
    this._pendingHandoffForPlan = artifact
    this.exit()
    try:
      await this.enter(this.createSessionModeId(), false, true, 'plan')
    catch error:
      this._pendingHandoffForPlan = null
      throw error
  else:
    // plan → normal 保持现有逻辑不变
    ...
```

**要点** [C:USER]：
- 用 `data.path.length > 0` 替代 `data.content.trim().length > 0` 作为是否产生 artifact 的条件，允许空 content 但有效 path 时仍 handoff。
- `filename = basename(data.path)` 在存 artifact 时一次性算好，避免 injector 重复导入 `pathe`。

### `DesignModeInjector.getInjection()` design→plan 分支 伪代码

```
function getInjection():
  isDesignActive = sessionMode.isActive && sessionMode.kind === 'design'

  if not isDesignActive:
    if not wasActive:
      return undefined
    wasActive = false
    injectedAt = null
    handoff = sessionMode.consumePendingHandoffForPlan()
    if handoff !== null:
      return designToPlanHandoffReminder(
        handoff.path,
        handoff.filename,
        handoff.selectedLabel,
      )
    return exitReminder()
  ...
```

### `designToPlanHandoffReminder` 输出模板

```
Design mode completed. The approved design has been handed off — you are now in plan mode.

Design saved to: {path}

{selectedLabelPrefix}Create a concrete, step-by-step implementation plan based on the approved design in `{filename}`. Do not implement anything yet.
```

其中 `selectedLabelPrefix`：

```
if selectedLabel !== undefined and selectedLabel.length > 0:
  return `Selected approach: ${selectedLabel}. Execute ONLY the selected approach; do not execute any unselected alternatives.\n\n`
return ''
```

### `ExitDesignModeTool.execution` 伪代码

```
function execution(args, metadata):
  if not sessionMode.isActive:
    return error('ExitDesignMode can only be called while design mode is active.')

  resolved = await resolveDesign()
  if not resolved.ok:
    return resolved.error

  selectedLabel = declaredOptionLabel(args.options, selectedLabelOf(metadata))

  failed = await handoffToPlan(selectedLabel)
  if failed !== undefined:
    return failed

  return ok(formatDesignHandoffOutput(resolved.path, selectedLabel))
```

### `formatDesignHandoffOutput` 输出模板

```
{selectedApproachPrefix}
Design mode deactivated. Now in plan mode.
{savedToLine}
Create a concrete, step-by-step implementation plan based on the approved design saved above.
```

其中：
- `selectedApproachPrefix` 沿用 `exit-mode-output.ts` 的 `selectedApproachPrefix(selectedLabel)` [C:INFERRED]。
- `savedToLine` = `path !== undefined ? 'Design saved to: ' + path + '\n\n' : ''`。

---

## Call-Site Integration

### 1. `packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts`

**行范围**: 67-207（整个工具实现）

**变更点**:
- `ResolveDesignResult` 接口移除 `design?: string` 字段，只保留 `path?: string`。
- `resolveDesign()` 在 `data.content.trim().length > 0` 时返回 `{ ok: true, path: data.path }`（不再返回 content）。
- `execution()` 中：
  ```ts
  const failed = await this.handoffToPlan(optionLabel);   // 传入 selectedLabel
  // ...
  return {
    isError: false,
    output: formatDesignHandoffOutput(resolved.path, optionLabel),
  };
  ```
- `handoffToPlan()` 签名改为 `private async handoffToPlan(selectedLabel?: string): Promise<ExecutableToolResult | undefined>`，内部调用 `this.agent.sessionMode.handoffTo('plan', { selectedLabel })`。
- `formatDesignHandoffOutput(design, path, selectedLabel)` 改为 `formatDesignHandoffOutput(path, selectedLabel)`。

### 2. `packages/agent-core/src/agent/session-mode/index.ts`

**行范围**: 38-43、252-256、278-309

**变更点**:
- `_pendingHandoffForPlan` 类型改为 `{ path: string; filename: string; selectedLabel?: string } | null`。
- `consumePendingHandoffForPlan` 返回类型同步。
- `handoffTo` 中 target === 'plan' 分支的 artifact 构造逻辑改为基于 path 和 `basename`。

### 3. `packages/agent-core/src/agent/injection/design-mode.ts`

**行范围**: 42-44、123-126

**变更点**:
- 调用 `consumePendingHandoffForPlan()` 后使用新字段：
  ```ts
  return designToPlanHandoffReminder(handoff.path, handoff.filename, handoff.selectedLabel);
  ```
- `designToPlanHandoffReminder(content, path)` 改为 `designToPlanHandoffReminder(path, filename, selectedLabel?)`。

---

## Error & Degradation

| Error class | Immediate handling | Degradation path | Recovery condition |
|------------|-------------------|------------------|-------------------|
| 设计文件 content 为空但 path 存在 | `resolveDesign()` 仍视为有效；`handoffTo('plan')` 产生只含 path 的 artifact | plan 模式首条 reminder 引用空设计文件，LLM 读取后看到空文档 | 用户补写设计文件或重新进入 design 模式 |
| 设计文件 path 为 null/空 | `handoffTo('plan')` 产生 null artifact | `DesignModeInjector` 注入 "no approved design file" 降级提示 | 用户提供新的设计请求 |
| `handoffTo` 进入 plan 失败 | catch 中清空 `_pendingHandoffForPlan`，抛出 error | `ExitDesignModeTool` 返回 `isError: true` tool result | 用户重新调用 ExitDesignMode |

---

## Test Plan

### 必须通过的命令

```bash
cd packages/agent-core
pnpm test -- test/tools/exit-design-mode.test.ts test/agent/injection/design-mode.test.ts
pnpm typecheck
```

### 断言映射

| 测试文件 | 测试名 | 变更后断言 |
|---------|-------|-----------|
| `exit-design-mode.test.ts` | `exits with the current design without consulting permission approval` | `expect(result.isError).toBe(false)`；`expect(result.output).toContain('Design saved to: /tmp/kimi-design.md')`；`expect(result.output).toContain('Design mode deactivated')`；`expect(result.output).not.toContain('# File Design')` |
| `exit-design-mode.test.ts` | `returns an error when no design content is available` | 若仍要求 content 非空，保持现有断言；若按本设计允许空 content，则该测试改为验证空 content 仍成功 handoff |
| `design-mode.test.ts` | `injects the handoff reminder (with design artifact) when a pending handoff for plan is set` | mock `pendingHandoff` 改为 `{ path: '/tmp/design.md', filename: 'design.md' }`；断言 `text` 包含 `Design mode completed`、`Design saved to: /tmp/design.md`、`approved design in 'design.md'`；断言 `text` 不包含 `# My Design` |
| `design-mode.test.ts` | `injects the exit reminder when design mode turns off after being active` | 保持现有断言（无 handoff 时返回 cancelled 提示） |

---


---

## Self-Review

### 1–3 个最贵的决定及具体输入

**决定 1：handoff artifact 以 `path` 存在性为核心，不再依赖 `content` 长度。**

| 输入 (content, path) | 期望输出 |
|---|---|
| `('', '/tmp/design.md')` | 产生 artifact `{ path: '/tmp/design.md', filename: 'design.md' }`，进入 plan 模式 |
| `('# Design', '')` | 无 artifact，进入 plan 模式并提示无设计文件 |
| `('# Design', '/tmp/design.md')` | 产生 artifact，进入 plan 模式 |

**决定 2：使用 `pathe.basename(path)` 计算 filename。**

已用 `node:path.basename` 做 ephemeral 验证（pathe 是兼容实现）：

```
'/tmp/design.md'                          -> 'design.md'
'/tmp/.ody-code/designs/2026-06-12-x.md'  -> '2026-06-12-x.md'
''                                        -> ''
```

因为 `handoffTo` 先判断 `data.path.length > 0`，空路径不会调用 `basename`，所以不存在异常风险。

**决定 3：tool result 与 handoff reminder 均不再嵌入 `data.content`。**

| 输入 | 期望输出 |
|---|---|
| 设计文件包含 10 万字 | tool result + handoff reminder 仍只有若干行，context 不膨胀 |
| 设计文件只有标题 | tool result + handoff reminder 仍只引用路径 |
| 无设计文件 | 进入 plan 模式并提示无设计文件 |

---

### 四透镜检查

**Security**
- 检查了路径展示策略：用户选择保持原样，tool result 与 reminder 中仍使用 `SessionMode` 返回的原始路径，这与当前 `ExitDesignMode` 行为一致，没有新增 PII 泄漏面。
- 没有新增 filter/regex，不存在 false positive/negative 风险。

**Test**
- 每个关键行为都映射了 must-pass / must-reject 断言：tool result 必须包含 `Design saved to:`、必须不包含 `# File Design`；handoff reminder 必须包含文件名、必须不包含 `# My Design`。
- 检查了自我矛盾：测试不能同时期望 `not.toContain('# File Design')` 和旧的 `toContain('# File Design')`；已用新断言替换旧断言。

**Ops**
- 没有新增 I/O 调用，只是减少上下文传输量。
- `basename` 在 handoff 时只计算一次，无重复成本。
- 无 identifier 冲突；filename 来自已存在的 path，不引入新唯一性约束。

**Integration**
- 已验证依赖项存在：
  - `SessionMode.data()` 返回 `{ content, path }`（`packages/agent-core/src/agent/session-mode/index.ts:506-520`）。
  - `SessionMode.handoffTo()` 已实现 design→plan 分支（`packages/agent-core/src/agent/session-mode/index.ts:278-309`）。
  - `DesignModeInjector` 已消费 `consumePendingHandoffForPlan()`（`packages/agent-core/src/agent/injection/design-mode.ts:42-44`）。
  - `pathe.basename` 已在 `design-mode.ts` 与 `plan-mode.ts` 中导入使用。
- 设计落在用户问题涉及的代码路径（`packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts`、`packages/agent-core/src/agent/injection/design-mode.ts`、`packages/agent-core/src/agent/session-mode/index.ts`），没有静默重定向。

**Scope**
- 仍是单一、连贯的改动：只修改 design→plan 交接时的消息内容/artifact 结构，不涉及 plan→normal、不新增 feature flag、不修改通用 plan-mode 合约。没有膨胀成多个子项目。

---

### 修复记录

- 无修复。自查未发现内部矛盾或占位符。

