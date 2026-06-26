# Design Mode 切回时自动提示继续未完成的设计

> **审计级别**: Deep [C:USER]
> **日期**: 2026-06-12

---

## Scope In/Out

### In

| # | 功能 | 范围 | 来源 |
|---|------|------|------|
| 1 | 检测可继续的拆分设计 | 基于 `SessionCheckpoint.designModeContext.sessions[].approvedPath` 读取 split index，解析 Parts manifest 的 `pending` 行 | [C:USER] |
| 2 | 分两次弹框选择 | 先选 split index，再选该 index 下的 pending part；通过 `agent.rpc.requestQuestion` 实现 | [C:USER] |
| 3 | 进入 design mode 并定位到选中 index | `Agent.enterPlan({ kind: 'design' })` 内部完成选择后，把 index 路径设为当前 design-mode file | [C:USER] |
| 4 | Entry reminder 提示当前应写哪个 part | `DesignModeInjector` 读取 `SessionMode.targetPendingPart`，在 entry/reentry reminder 中追加提示 | [C:USER] |
| 5 | 测试覆盖 | 新增/扩展测试，验证扫描、选择、进入、reminder | [C:USER] |

### Out

| # | 功能 | 延后理由 |
|---|------|---------|
| 1 | 跨项目/目录扫描 `.ody-code/designs/` | 用户选择只基于 checkpoint 记录的 `approvedPath` [C:USER] |
| 2 | 判断设计是否被 plan/normal 执行过 | 完成标准只认 Parts manifest 的 `done` 状态 [C:USER] |
| 3 | feature flag | 用户选择直接作为默认行为 [C:USER] |
| 4 | 暴露 ResumeDesignModeTool 给 LLM | 用户选择只作为内部封装 [C:USER] |
| 5 | 修改 plan→normal 流程 | 本设计只解决 design mode 重入问题 [C:USER] |

---

## Architecture

```
User types /design in TUI
  │
  ▼
node-sdk: Session.setSessionMode('design')
  │
  ▼
RPC: enterPlan({ kind: 'design' })
  │
  ▼
Agent.enterPlan(payload)
  ├─ if kind === 'design':
  │    ├─ resumeDesignMode.scanPendingDesigns()
  │    │      ├─ collect approvedPaths from sessionMode.designSessions
  │    │      ├─ read each index file
  │    │      └─ parsePartsManifest(content) → pending parts
  │    ├─ if any pending:
  │    │    ├─ requestQuestion(index selector) → selectedIndexPath
  │    │    ├─ requestQuestion(part selector) → selectedPartName
  │    │    └─ sessionMode.setTargetPendingPart(selectedIndexPath, selectedPartName)
  │    └─ sessionMode.enter(..., 'design')
  │         └─ sessionMode.sessionModeFilePath = selectedIndexPath (if selected)
  │
  ▼
Next turn: DesignModeInjector.getInjection()
  ├─ detects design now active
  ├─ reads targetPendingPart
  └─ appends "continue writing <part>" directive to entry/reentry reminder
```

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|-----------|------------|-----------------|---------------|

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|-----------|--------|-----------|


---

## Data Models & State

### `SessionMode` 新增临时状态

```ts
private _targetPendingPart: {
  indexPath: string;
  partName: string;
} | null = null;
```

**Contract**: 保存用户通过选择器选中的目标 part，供 `DesignModeInjector` 在 entry/reminder 中提示。不持久化到 checkpoint，随 session mode 退出/取消清空 [C:USER]。

### `SessionMode.setTargetPendingPart`

```ts
setTargetPendingPart(indexPath: string, partName: string): void
```

**Contract**: 在 `Agent.enterPlan` 选择完成后写入目标 part；空字符串或无效值应被拒绝 [C:INFERRED]。

### `SessionMode.getTargetPendingPart`

```ts
getTargetPendingPart(): { indexPath: string; partName: string } | null
```

**Contract**: 只读访问 `_targetPendingPart`；返回 null 表示没有待继续的 part [C:INFERRED]。

### `PendingDesignIndex`

```ts
interface PendingDesignIndex {
  readonly indexPath: string;
  readonly pendingParts: readonly PendingPart[];
}

interface PendingPart {
  readonly file: string;
  readonly scope: string;
}
```

**Contract**: 一个 split design index 及其未完成的 parts [C:INFERRED]。

### `ResumeDesignModeContext`

```ts
interface ResumeDesignModeContext {
  readonly agent: Agent;
  readonly requestQuestion: (request: QuestionRequest) => Promise<QuestionResult>;
}
```

**Contract**: `ResumeDesignMode` 内部逻辑所需的最小上下文；隔离 RPC 与 agent 依赖，便于测试 [C:INFERRED]。

---

## Interfaces & Types

### `ResumeDesignMode`

```ts
class ResumeDesignMode {
  constructor(ctx: ResumeDesignModeContext);

  /** 扫描并返回所有有待完成 part 的 split design index。 */
  async scanPendingDesigns(): Promise<PendingDesignIndex[]>;

  /** 让用户先选 index，再选 part。返回选中的 indexPath 与 partName；若用户 dismiss 则返回 null。 */
  async promptForPendingPart(pending: PendingDesignIndex[]): Promise<{ indexPath: string; partName: string } | null>;
}
```

**Contract**: 封装“检测未完成设计 + 弹框选择”的内部能力；不暴露给 LLM tool 列表 [C:USER]。

### `SessionMode.enter` 签名调整

```ts
async enter(
  id = this.createSessionModeId(),
  _createFile = false,
  emitStatus = true,
  kind: SessionModeKind = 'plan',
  initialFilePath?: string,
): Promise<void>
```

**Contract**: 新增可选 `initialFilePath`，进入 design/plan mode 时直接把文件路径锁定为已有 index；不传则保持现有懒解析行为 [C:INFERRED]。

### `DesignModeInjector` 扩展

```ts
// 新增/复用 directive 构建函数
function resumePartDirective(
  partName: string,
  indexStem: string,
): string
```

**Contract**: 当存在 `_targetPendingPart` 时，在 entry/reentry reminder 中追加“本 turn 应继续写该 part”的指令 [C:USER]。


---

## Algorithms

### `Agent.enterPlan` 进入 design mode 分支

```
function enterPlan(payload):
  if payload.kind === 'design':
    pendingIndexes = await resumeDesignMode.scanPendingDesigns()
    selected = null
    if pendingIndexes.length > 0 and agent.rpc?.requestQuestion is available:
      selected = await resumeDesignMode.promptForPendingPart(pendingIndexes)
    if selected !== null:
      sessionMode.setTargetPendingPart(selected.indexPath, selected.partName)
    await sessionMode.enter(
      createSessionModeId(),
      false,
      true,
      'design',
      selected?.indexPath,
    )
    track('design_enter_resolved', {
      outcome: 'auto_approved',
      resumed: selected !== null,
      resumedPart: selected?.partName ?? undefined,
    })
  else:
    // 现有 plan mode 逻辑不变
    ...
```

**要点** [C:USER]：
- 只在 `kind === 'design'` 时触发扫描与选择。
- `requestQuestion` 不可用时降级为新建空白设计。
- 选择结果通过 `setTargetPendingPart` 传递给 `DesignModeInjector`。

### `ResumeDesignMode.scanPendingDesigns`

```
function scanPendingDesigns():
  sessions = agent.sessionMode.designSessions
  approvedPaths = unique(sessions.approvedPath where exists and length > 0)
  results = []
  for path in approvedPaths:
    content = await safeReadText(path)
    if content === null: continue
    manifest = parsePartsManifest(content)
    if manifest === null or manifest.next === null: continue
    pendingParts = rows where status === 'pending'
    if pendingParts.length > 0:
      results.push({
        indexPath: path,
        pendingParts: pendingParts.map({ file, scope })
      })
  return results
```

**要点** [C:USER]：
- 只读取 checkpoint 中记录的 `approvedPath`。
- 去重：同一 index 在多次 design session 中可能被记录多次。
- 只保留至少有一个 pending part 的 index。

### `ResumeDesignMode.promptForPendingPart`

```
function promptForPendingPart(pendingIndexes):
  // Step 1: 选择 index
  if pendingIndexes.length === 1:
    selectedIndex = pendingIndexes[0]
  else:
    answer = await requestQuestion({
      question: 'Which design do you want to continue?',
      options: pendingIndexes.map(i => {
        label: basename(i.indexPath),
        description: `${i.pendingParts.length} pending part(s)`
      })
    })
    if answer dismissed: return null
    selectedIndex = pendingIndexes[answer.selectedIndex]

  // Step 2: 选择 part
  if selectedIndex.pendingParts.length === 1:
    selectedPart = selectedIndex.pendingParts[0]
  else:
    answer = await requestQuestion({
      question: `Which part of ${basename(selectedIndex.indexPath)} do you want to design?`,
      options: selectedIndex.pendingParts.map(p => {
        label: p.file,
        description: p.scope
      })
    })
    if answer dismissed: return null
    selectedPart = selectedIndex.pendingParts[answer.selectedPart]

  return { indexPath: selectedIndex.indexPath, partName: selectedPart.file }
```

**要点** [C:USER]：
- 只有多个 index 时才弹第一次选择框。
- 只有多个 pending part 时才弹第二次选择框。
- 用户 dismiss 任意一次都返回 null，降级为新建空白设计。

### `SessionMode.enter` 支持初始路径

```
function enter(id, _createFile, emitStatus, kind, initialFilePath):
  ... existing guard and setup ...
  if initialFilePath !== undefined and initialFilePath.length > 0:
    _sessionModeFilePath = initialFilePath
  ... rest unchanged ...
```

**要点** [C:INFERRED]：
- 仅在传入时覆盖文件路径；不传则走现有懒解析。
- 仍需执行 model 切换、directory 解析、WAL 记录等现有逻辑。

### `DesignModeInjector.getInjection` resume 提示

```
function getInjection():
  isDesignActive = sessionMode.isActive && sessionMode.kind === 'design'
  target = sessionMode.getTargetPendingPart()

  if not isDesignActive:
    ... existing handoff/exit logic ...

  if not wasActive:
    ... existing reentry/full logic ...
    if target !== null:
      directive = resumePartDirective(target.partName, indexStemFor(target.indexPath))
      return appendDirective(reminder, directive)

  ... existing periodic injection ...
```

**要点** [C:USER]：
- 只在首次进入 design mode 的 reminder 中追加 resume 提示。
- 提示内容引导 LLM 读取 index 并继续写指定 part。


---

## Call-Site Integration

### 1. `packages/agent-core/src/agent/index.ts` — `Agent.enterPlan`

**行范围**: 448-473

**变更点**:
- 在 `sessionMode.enter(...)` 之前插入 resume 逻辑：
  ```ts
  if (payload.kind === 'design') {
    const resume = new ResumeDesignMode({
      agent: this,
      requestQuestion: (req) => this.rpc!.requestQuestion!(req),
    });
    const pending = await resume.scanPendingDesigns();
    const selected =
      pending.length > 0 && this.rpc?.requestQuestion !== undefined
        ? await resume.promptForPendingPart(pending)
        : null;
    if (selected !== null) {
      this.sessionMode.setTargetPendingPart(selected.indexPath, selected.partName);
    }
    await this.sessionMode.enter(
      this.sessionMode.createSessionModeId(),
      false,
      true,
      'design',
      selected?.indexPath,
    );
    this.telemetry.track('design_enter_resolved', {
      outcome: 'auto_approved',
      resumed: selected !== null,
      resumedPart: selected?.partName,
    });
    return;
  }
  ```

**前后上下文**: 原 `enterPlan` 先处理 `sourceFilePath`，再调用 `sessionMode.enter(..., payload.kind ?? 'plan')`。新逻辑在 `kind === 'design'` 时提前 return，避免与 plan 的 `sourceFilePath` 路径混淆 [C:USER]。

### 2. `packages/agent-core/src/agent/session-mode/index.ts` — `SessionMode`

**行范围**: 38-43、57-127、161-183、198-223

**变更点**:
- 新增私有状态 `_targetPendingPart` 和 getter/setter（见 Data Models）。
- `enter()` 签名增加可选 `initialFilePath?: string`，进入时若传入则赋值给 `_sessionModeFilePath`。
- `cancel()` 与 `exit()` 中清空 `_targetPendingPart`，防止残留 [C:INFERRED]。

### 3. `packages/agent-core/src/agent/injection/design-mode.ts` — `DesignModeInjector`

**行范围**: 28-67、97-117

**变更点**:
- `getInjection()` 首次检测到 design active 时，读取 `sessionMode.getTargetPendingPart()`。
- 若 target 存在，把 `resumePartDirective(partName, indexStemFor(indexPath))` 拼接到 reentry/full reminder 的 `splitDirective` 位置或作为额外 directive。
- 追加提示后清空 target（消费一次），避免后续 periodic reminder 重复提示 [C:INFERRED]。

### 4. `packages/agent-core/src/agent/injection/design-mode-contract.ts` — resume directive

**行范围**: 176-193 附近追加

**变更点**:
- 新增导出函数 `designResumePartDirective(partName: string, indexStem: string): string`：
  ```
  ## Continue split design — resume pending part
  The user selected to continue designing \`${partName}\`. Read the index \`${indexStem}.md\` and write ONLY the part file \`${indexStem}/${partName}\` this turn. Do NOT rewrite already-done parts.
  ```
  [C:USER]

### 5. `packages/agent-core/src/tools/builtin/planning/resume-design-mode.ts`（新建内部模块）

**变更点**:
- 实现 `ResumeDesignMode` 类（见 Interfaces）。
- 不注册到 `ToolManager` 的 LLM 可见列表；仅由 `Agent.enterPlan` 内部实例化调用 [C:USER]。

---

## Error & Degradation

| Error class | Immediate handling | Degradation path | Recovery condition |
|------------|-------------------|------------------|-------------------|
| `requestQuestion` 不可用 | 跳过选择，selected = null | 进入空白新设计文件 | 用户在支持 TUI 的环境重新进入 |
| 用户 dismiss 选择框 | `promptForPendingPart` 返回 null | 进入空白新设计文件 | 用户重新进入 design mode 或手动指定 |
| 某 `approvedPath` 文件不存在 | `safeReadText` 返回 null，跳过该 index | 该 index 不进入选项列表 | 用户修复/重新生成设计文件 |
| 某 index 无 Parts manifest | `parsePartsManifest` 返回 null，跳过 | 该 index 不进入选项列表 | 用户把设计拆分为 split index |
| 某 index 所有 part 都 done | `pendingParts` 为空，跳过 | 该 index 不进入选项列表 | 用户新增 part 行并 mark pending |
| `enter()` 传入 `initialFilePath` 后写入失败 | 抛出异常，不进入 design mode | 保留在 normal 模式 | 用户重试 `/design` |
| `_targetPendingPart` 残留 | `cancel()`/`exit()` 清空 | 不影响后续模式切换 | 正常退出/取消即恢复 |

---

## Test Plan

### 必须通过的命令

```bash
cd packages/agent-core
pnpm test -- test/agent/index.test.ts test/agent/injection/design-mode.test.ts test/tools/resume-design-mode.test.ts
pnpm typecheck
```

### 断言映射

| 测试文件 | 测试名 | 断言 |
|---------|-------|------|
| `test/tools/resume-design-mode.test.ts` | `scanPendingDesigns returns indexes with pending parts` | mock 两个 approvedPath，一个全 done、一个有 pending；断言返回长度为 1，且 pendingParts 包含目标 part |
| `test/tools/resume-design-mode.test.ts` | `promptForPendingPart skips questions when only one choice` | 只有一个 index 一个 part 时，断言不调用 requestQuestion，直接返回该 part |
| `test/tools/resume-design-mode.test.ts` | `promptForPendingPart returns null on dismiss` | mock requestQuestion 返回 dismissed；断言结果为 null |
| `test/agent/index.test.ts` | `enterPlan design calls resume and sets target part` | mock `designSessions` 与 `requestQuestion`；断言 `sessionMode.setTargetPendingPart` 被调用，且 `enter` 接收到正确的 `initialFilePath` |
| `test/agent/index.test.ts` | `enterPlan design falls back to blank design when no pending` | mock 无 pending；断言 `enter` 被调用且 `initialFilePath` 未传入 |
| `test/agent/injection/design-mode.test.ts` | `injects resume directive when targetPendingPart is set` | mock `getTargetPendingPart` 返回值；断言 reminder 文本包含 `Continue split design` 和 part 文件名 |

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|-----------|--------|-----------|
| 1 | 用户完成一个 part 后忘记 mark `done`，导致每次重入都提示同一个 part | 中 | 中 | reminder 明确 instruct LLM 读取 index 并确认当前 pending 行；LLM 可在 mark done 后调用 ExitDesignMode |
| 2 | `approvedPath` 记录了大量历史 index，选择框过长 | 低 | 中 | 只保留唯一路径；选项描述显示 pending 数量；若未来仍过长可再限制最近 N 个 [C:DEFERRED] |
| 3 | `initialFilePath` 进入后，模型仍按懒解析逻辑覆盖路径 | 低 | 高 | `enter()` 中先赋值 `_sessionModeFilePath`，后续 `resolveFilePathFromContent/ModelRequest` 检查 `_sessionModeFilePath !== null` 时直接返回 [C:INFERRED] |
| 4 | `_targetPendingPart` 在异常退出时残留，影响下一次进入 | 低 | 中 | `cancel()`/`exit()` 中清空；`enter()` 开头也清空旧值 [C:INFERRED] |
| 5 | `requestQuestion` 在无 UI 测试环境不可用，导致设计分支无法测试 | 中 | 低 | 注入 mock `requestQuestion`；`requestQuestion` 不可用时降级为 null，测试覆盖该分支 [C:INFERRED] |

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|-----------|------------|-----------------|---------------|
| 1 | `Agent.enterPlan` 是进入 design mode 的唯一入口（TUI → node-sdk → RPC） | High | 高；若还有其他入口，resume 逻辑会漏掉 | 检查 `packages/agent-core/src/rpc/core-impl.ts` 与 `packages/node-sdk/src/rpc.ts` 中进入 design mode 的路径 |
| 2 | `parsePartsManifest` 返回的 `next` 与所有 pending rows 语义一致 | High | 中；若解析漏行，会遗漏待选 part | 已有测试覆盖，新增 adversarial 用例验证 |
| 3 | `SessionMode.enter` 新增 `initialFilePath` 后，不会破坏 plan mode 的 `sourceFilePath` 逻辑 | Medium | 高；plan 模式可能意外使用新参数 | 保持 plan 分支调用签名不变，仅 design 分支传入新参数 |
| 4 | `requestQuestion` 返回的答案格式与 AskUserQuestionTool 一致（`answers: Record<string, string>`） | High | 中；格式错误会导致选择失效 | 复用 AskUserQuestionTool 的 `normalizeQuestionResult` 逻辑 |
| 5 | 用户选择 index 后，希望 LLM 继续写 index 中列出的 part 文件，而不是重写 index 本身 | High | 中；若用户预期不同，体验偏差 | 已在 Scope 确认 |
| 6 | `SessionMode.cancel()`/`exit()` 调用时清空 `_targetPendingPart` 不会引入副作用 | High | 低 | 单元测试验证 |


---

## Self-Review

### 1–3 个最贵的决定及具体输入

**决定 1：完成标准只认 Parts manifest 的 `done` 状态，不认文件内容或后续 plan/normal 执行。**

| 输入 | 期望输出 |
|---|---|
| Index 中 phase1.md 行 `done`，phase2.md 行 `pending` | phase1 完成，phase2 待选 |
| Index 中 phase1.md 行 `done`，但对应 plan 尚未执行 | 仍视为设计已完成，phase2 待选 |
| Index 中没有 Parts manifest 表格 | 该 index 不出现在选择列表 |

**决定 2：扫描范围只基于 `SessionCheckpoint.designModeContext.sessions[].approvedPath`，不遍历 `.ody-code/designs/` 目录。**

| 输入 | 期望输出 |
|---|---|
| sessions = [{approvedPath: '/p/a.md'}, {approvedPath: '/p/a.md'}, {approvedPath: '/p/b.md'}] | 扫描 `/p/a.md` 和 `/p/b.md`，去重 |
| sessions = [{approvedPath: ''}, {}] | 忽略空/缺失路径，不扫描 |
| `.ody-code/designs/` 下存在未进入过 design mode 的文件 | 不扫描，不出现在列表 |

**决定 3：选择 index 后通过 `initialFilePath` 进入 design mode，且 `resolveFilePathFromContent/ModelRequest` 不会覆盖已有路径。**

已验证：当前 `resolveFilePathFromContent` 第一行即 `if (this._sessionModeFilePath !== null) return this._sessionModeFilePath;`（`session-mode/index.ts:431-433` 与 `:499-501`），因此只要 `enter()` 正确赋值，后续写入路径锁定 [C:INFERRED]。

| 输入 | 期望输出 |
|---|---|
| `enter(..., initialFilePath: '/p/index.md')` | `_sessionModeFilePath = '/p/index.md'`；后续写入锁定该路径 |
| `enter(...)` 不传 `initialFilePath` | 保持现有懒解析行为 |
| 模型请求写入 `/other/path.md` | `resolveFilePathFromModelRequest` 返回已有 `/p/index.md`，不跟随模型请求 |

---

### 四透镜检查

**Security**
- 路径展示策略：选择框中展示原始绝对路径，与现有 design mode 行为一致；未新增 PII 泄漏面。
- 没有新增 filter/regex，不存在 false positive/negative 风险。
- 未把用户选择结果写入日志或 telemetry 中的敏感字段。

**Test**
- 每个关键行为都有 must-pass / must-reject 断言：scan 必须返回有 pending 的 index；prompt 在仅一个选择时必须不弹框；dismiss 时必须返回 null；injector 必须包含 resume directive。
- 已验证 `parsePartsManifest` 等价逻辑可正确识别 `done`/`pending` 行并过滤 header/separator。
- 已验证 `basename` 与去重逻辑对常见输入返回预期结果。

**Ops**
- 新增扫描成本：每次进入 design mode 时读取 N 个 approvedPath 文件（N 为历史 design session 数，通常很小）。
- `requestQuestion` 是同步阻塞等待用户；已设计降级路径避免无 UI 环境卡住。
- 无 identifier 冲突；`_targetPendingPart` 是私有状态，不持久化。

**Integration**
- 已验证依赖项存在：
  - `Agent.enterPlan` 是 design mode 入口（`packages/agent-core/src/agent/index.ts:448-473`）。
  - `SessionMode.designSessions` getter 存在（`session-mode/index.ts:353-354`）。
  - `DesignSessionCheckpoint.approvedPath` 存在（`session/checkpoint/checkpoint.ts:17-22`）。
  - `parsePartsManifest` 存在（`agent/injection/parts-manifest.ts:69-77`）。
  - `Agent.rpc?.requestQuestion` 存在（`rpc/sdk-api.ts:84`，`agent/index.ts:108`）。
- 设计落在用户指定的代码路径（`packages/agent-core/src/agent/index.ts` 与 `packages/agent-core/src/agent/session-mode/index.ts`），没有静默重定向。

**Scope**
- 仍是单一、连贯的改动：只解决 design mode 重入时的选择器问题。
- 不涉及 plan→normal、不修改通用 plan-mode 合约、不新增 LLM 可见 tool、不新增 feature flag。

---

### 修复记录

- 无修复。自查未发现内部矛盾或占位符。


---

## User Final Approval

- **审计级别**: Deep — 已确认所有 section 关键论断与全部 [C:INFERRED] 假设。
- **设计文件路径**: `/Users/ranwei/workspace/ody-code/.ody-code/designs/2026-06-12-design-mode-resume-pending-parts.md`
- **状态**: 等待 ExitDesignMode 最终批准。

