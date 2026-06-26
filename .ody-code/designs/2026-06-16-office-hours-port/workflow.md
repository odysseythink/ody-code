# Part 3 — Office-Hours 核心工作流

## Scope

将上游 `office-hours` 技能的 Phase 1-6 诊断流程移植为 office-hours mode 的 prompt contract。流程完全由注入的 system prompt 驱动：模型根据用户回答自主推进阶段、选择问题、记录信号，最终生成设计文档。

## Interfaces

```typescript
// packages/agent-core/src/agent/injection/office-hours-contract.ts
export function officeHoursEntryMessage(designFilePath: SessionModeFilePath): string;
export function officeHoursFullReminder(designFilePath: SessionModeFilePath): string;
export function officeHoursSparseReminder(designFilePath: SessionModeFilePath): string;
export function officeHoursReentryReminder(designFilePath: SessionModeFilePath): string;
export function officeHoursExitReminder(designFilePath: SessionModeFilePath | null): string;

// 设计文档模板类型
export type OfficeHoursMode = 'startup' | 'builder';

export interface DesignDocTemplate {
  readonly mode: OfficeHoursMode;
  readonly title: string;
  readonly branch: string;
  readonly repo: string;
  readonly sections: readonly string[];
}
```

## Data Flow

```
OfficeHoursInjector.getInjection()
  │
  ▼
officeHoursEntryMessage() / officeHoursFullReminder() / officeHoursSparseReminder()
  │
  ▼
LLM receives full workflow instructions
  │
  ▼
Phase 1: Context Gathering — read CLAUDE.md/TODOS.md, git log, codebase mapping
  │
  ▼
Phase 2A/2B: Startup or Builder diagnostic — ONE AskUserQuestion at a time
  │
  ▼
Phase 2.5/2.75: Related design discovery + landscape awareness (optional WebSearch)
  │
  ▼
Phase 3: Premise Challenge — list premises, AskUserQuestion confirm
  │
  ▼
Phase 4: Alternatives Generation — 2-3 approaches, AskUserQuestion pick
  │
  ▼
Phase 4.5: Founder Signal Synthesis + builder profile append
  │
  ▼
Phase 5: Write design doc to assigned path via Write/Edit
  │
  ▼
Phase 6: Handoff — resources, next-skill recommendations, then ExitOfficeHoursModeTool
```

## Algorithms

### 模式选择（Phase 1 Beat 5）

```
function determineMode(userGoal: string): OfficeHoursMode
  if userGoal matches startup/intrapreneurship signals then return 'startup'
  else return 'builder'

Signals for startup:
  - 'building a startup', 'building a company', 'customers pay', 'fundraising',
    'revenue', 'b2b saas', 'go to market', 'product-market fit'
Signals for builder:
  - default; includes hackathon, open source, learning, side project, having fun
```

### Smart Routing for Startup Six Questions

```
function selectStartupQuestions(productStage: 'pre-product' | 'has-users' | 'has-paying' | 'engineering')
  mapping = {
    'pre-product':      [Q1, Q2, Q3],
    'has-users':        [Q2, Q4, Q5],
    'has-paying':       [Q4, Q5, Q6],
    'engineering':      [Q2, Q4]
  }
  return mapping[productStage]
```

### Builder Question Sequence

```
function selectBuilderQuestion(index: number, alreadyAnswered: string[]): string | null
  questions = [
    "What's the coolest version of this? What would make it genuinely delightful?",
    "Who would you show this to? What would make them say 'whoa'?",
    "What's the fastest path to something you can actually use or share?",
    "What existing thing is closest to this, and how is yours different?",
    "What would you add if you had unlimited time? What's the 10x version?"
  ]
  for q in questions do
    if not alreadyAnswered.includes(q) then return q
  return null
```

### Signal Counting（Phase 4.5）

```
function countFounderSignals(transcript: TranscriptEntry[]): { count: number; signals: string[] }
  observed = []
  if mentionsSpecificUsers(transcript)      then observed.push('named_users')
  if mentionsRevenueOrDemand(transcript)    then observed.push('demand_evidence')
  if pushedBackOnPremises(transcript)       then observed.push('pushback')
  if solvesOthersProblem(transcript)        then observed.push('others_need')
  if showsDomainExpertise(transcript)       then observed.push('domain_expertise')
  if caresAboutDetails(transcript)          then observed.push('taste')
  if alreadyBuilding(transcript)            then observed.push('agency')
  if defendedPremiseWithReasoning(transcript) then observed.push('reasoned_defense')
  return { count: observed.length, signals: observed }
```

### Tier Selection（Phase 6）

```
function selectTier(sessionCount: number): 'introduction' | 'welcome_back' | 'regular' | 'inner_circle'
  if sessionCount === 0 then return 'introduction'
  if sessionCount <= 3 then return 'welcome_back'
  if sessionCount <= 7 then return 'regular'
  return 'inner_circle'
```

## Call-Site Integration

### 1. packages/agent-core/src/agent/injection/office-hours-contract.ts [C:UPSTREAM]

新建文件，结构与 `design-mode-contract.ts` 对齐。核心 prompt body 从上游 `office-hours/SKILL.md` 的 `## Phase 1` 至 `## Phase 6` 移植，并做以下适配：
- 移除 gstack 特定的 `$(gpowers-path home)` 调用。
- 将 `~/.gstack/projects/$SLUG/` 替换为当前设计文件路径。
- 将 `$(gpowers-path analytics)` 替换为 `~/.ody-code/analytics/`。
- 将 `/plan-ceo-review`、`/plan-eng-review` 等下游 skill 引用替换为 ody-code 的 `/plan`、设计评审机制或文字说明 [C:USER]。

### 2. packages/agent-core/src/agent/injection/office-hours.ts [C:INFERRED]

新建 injector，行为参考 `DesignModeInjector`：

```typescript
export class OfficeHoursInjector extends DynamicInjector {
  protected override readonly injectionVariant = 'office_hours';
  private wasActive = false;

  override async getInjection(): Promise<string | undefined> {
    const isActive = this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'office-hours';
    const path = this.agent.sessionMode.sessionModeFilePath;
    if (!isActive) {
      if (!this.wasActive) return undefined;
      this.wasActive = false;
      this.injectedAt = null;
      return officeHoursExitReminder(path);
    }
    if (!this.wasActive) {
      this.wasActive = true;
      this.injectedAt = null;
      return officeHoursEntryReminder(path);
    }
    const variant = this.getVariant(); // same cadence as DesignModeInjector
    if (variant === null) return undefined;
    if (variant === 'reentry') return officeHoursReentryReminder(path);
    return variant === 'full'
      ? officeHoursFullReminder(path)
      : officeHoursSparseReminder(path);
  }
}
```

### 3. OfficeHoursInjector 注册 [C:USER]

见 `session-mode.md` Part 2 的 `InjectionManager` 集成。

## Prompt Contract 要点（非完整 prompt）

### Entry Message

```markdown
Office hours is now active. Your job is to act as a YC office hours partner.
Do NOT write code. Produce only a design document.

Design file: <path>
Write the design doc to EXACTLY this path.

Follow the workflow below. Ask ONE question at a time via AskUserQuestion.
```

### Full Reminder 结构

1. **HARD GATE**: no implementation until design approved.
2. **Voice**: builder-to-builder, concrete, no AI buzzwords.
3. **Phase 1**: Context Gathering — read CLAUDE.md, TODOS.md, git log, map relevant code.
4. **Phase 2A/2B**: Diagnostic questions, one at a time, with pushback patterns.
5. **Phase 2.5**: Related design discovery via Grep across `.ody-code/designs/`.
6. **Phase 2.75**: Landscape awareness via WebSearch with privacy gate.
7. **Phase 3**: Premise challenge — list premises, ask agree/disagree.
8. **Phase 4**: Alternatives generation — 2-3 approaches, AskUserQuestion pick.
9. **Phase 4.5**: Founder signal synthesis + builder profile append.
10. **Phase 5**: Design doc templates (startup / builder).
11. **Phase 6**: Tiered handoff + resources + next steps.
12. **Turn discipline**: end every turn with AskUserQuestion or ExitOfficeHoursModeTool.

### Design Doc Templates [C:UPSTREAM]

从上游 `## Phase 5: Design Doc` 移植，字段使用 ody-code 路径：
- Startup template: Problem Statement, Demand Evidence, Status Quo, Target User & Wedge, Constraints, Premises, Approaches, Recommended, Open Questions, Success Criteria, Distribution Plan, Dependencies, The Assignment, What I noticed.
- Builder template: Problem Statement, What Makes This Cool, Constraints, Premises, Approaches, Recommended, Open Questions, Success Criteria, Distribution Plan, Next Steps, What I noticed.

## Error & Degradation

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| LLM 跳过 AskUserQuestion 直接写设计文档 | sparse reminder 重申 "ONE question at a time" | 继续追问 | 模型遵守 prompt |
| 用户拒绝 WebSearch | prompt 中隐私门允许 skip，进入 Phase 3 | 仅使用内部知识 | 用户同意搜索 |
| 设计文件写入失败 | model receives tool error, can retry | 重试或换路径 | 磁盘可写 |
| 用户连续表示不耐烦 | escape hatch：ask 2 more critical questions then move on | 缩短流程 | 用户配合回答 |

## Test Plan

1. **Contract 内容测试**（`packages/agent-core/test/agent/injection/office-hours-contract.test.ts` 新增）：
   - `expect(officeHoursEntryMessage('/x/design.md')).toContain('/x/design.md')`
   - `expect(officeHoursFullReminder(null)).toContain('Phase 1')`
   - `expect(officeHoursFullReminder(null)).toContain('Phase 6')`

2. **Injector 变体逻辑**：同 Part 2 OfficeHoursInjector 测试。

3. **Prompt 约束测试**：
   - 验证 full reminder 包含 "Ask ONE question at a time"。
   - 验证包含 "Do NOT write code"。

## Done Criteria

- `pnpm -F @odysseythink/agent-core test` 中新增 office-hours contract 测试通过。
- 手动验证：启动 `ODY_CODE_EXPERIMENTAL_OFFICE_HOURS=1 ody --office-hours`（或最终直接 `ody --office-hours`）后，注入的 prompt 包含完整 YC 流程。
