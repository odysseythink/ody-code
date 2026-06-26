# Epic A-A4: 简化债务台账（Simplification Debt Ledger）

**Document Type**: Design Document
**Date**: 2026-06-18
**Audit Level**: Deep [C:USER]
**Status**: DRAFT — pending incremental approval

## Scope

### In Scope [C:USER]

1. 新增 `debt-ledger` builtin skill，让 agent 知道何时收割 `ody:` 简化债务标记。
2. 新增 `harvest-ody-markers` builtin tool，确定性扫描工作区中的 `ody:` 标记并输出中文优先的债务台账。
3. 更新 A1 `simplicity-first.md` skill，在 full/ultra 档中教 agent 对 deliberate simplification 主动留 `// ody: <天花板>, <升级触发条件>` 标记 [C:USER]。
4. 扩展 A3 反过度设计 review/audit prompt，命中过度设计时建议补 `ody:` 债务标记 [C:USER]。
5. 只读内存报告：不自动写文件，用户明确要求持久化才写 [C:USER]。
6. 输出格式：按文件分组，每行 `<文件>:<行> — <被简化了什么>。天花板：<上限>。升级：<触发条件>。`；缺触发条件标 `⚠️ rot`；结尾汇总 `<N> 个标记，<M> 个 rot 风险` [C:USER]。
7. 支持注释前缀：`//` 与 `#` [C:USER]。
8. rot 判定：内容不含逗号，或逗号后仅空白 [C:USER]。
9. 硬上限 200 条，超出则截断并提示 [C:USER]。
10. 复用 GrepTool 的敏感文件过滤与路径安全策略 [C:USER]。
11. telemetry 事件：`debt_ledger_harvested`（含 `marker_count`、`rot_risk_count`）、`debt_ledger_failed` [C:USER]。
12. 默认启用，无 feature flag [C:USER]。

### Out of Scope [C:USER]

1. **CLI/TUI 独立命令**：A4 只通过 skill/tool 暴露，不新增 `/debt-ledger` 斜杠命令或 `ody debt-ledger` 子命令；需要时再扩展 [C:DEFERRED]。
2. **持久化文件**：默认不写 `.ody-code/debt-ledger.md` 或 JSON records；用户明确要求时才写 [C:DEFERRED]。
3. **git blame / owner**：上游 Ponytail 可选 owner，本次不做 [C:DEFERRED]。
4. **多宿主适配器分发**：只取 Ponytail 规则内容，不做 `.cursor/` 等适配器 [C:USER]。
5. **块注释 / HTML 注释前缀**：`/* … */` 与 `<!-- … -->` 暂不支持 [C:DEFERRED]。
6. **兼容 `ponytail:` 标记**：本次只用 `ody:` [C:DEFERRED]。
7. **行为门控基准 / P2-A**：不在 A4 内 [C:DEFERRED]。

## Prior Art [C:UPSTREAM]

上游 Ponytail 4.7.0 `skills/ponytail-debt/SKILL.md` [C:UPSTREAM]：

- 约定 `ponytail: <ceiling>, <upgrade path>` 注释标记。
- 使用 `grep -rnE '(#|//) ?ponytail:'` 扫描，跳过 `node_modules`、`.git`、build output。
- 输出：`<file>:<line> — <what was simplified>. ceiling: <...>. upgrade: <...>.`
- rot 风险：无 upgrade path/trigger 的标记标 `no-trigger`。
- 默认只读，用户要求时才写入 `PONYTAIL-DEBT.md`。

本设计将前缀改为 `ody:`，输出语言改为中文优先，并接入 ody-code 的 skill/tool/telemetry 基础设施。

## Resolved Decisions [C:USER]

| # | 维度 | 决策 |
|---|---|---|
| 1 | 入口形态 | 仅 agent skill/tool，无独立 CLI/TUI 命令 |
| 2 | 标记格式 | `// ody: <天花板>, <升级触发条件>` / `# ody: <天花板>, <升级触发条件>` |
| 3 | A1 回改 | 同步更新 `simplicity-first.md`，教 agent 留 `ody:` 标记 |
| 4 | 输出持久化 | 只读内存报告 |
| 5 | 输出格式 | 中文优先表格 + rot 标记 |
| 6 | skill/tool 分工 | `debt-ledger` skill + `harvest-ody-markers` tool；A3 建议补标记 |
| 7 | 大量标记降级 | 硬上限 200，截断并提示 |
| 8 | 安全过滤 | 复用 GrepTool 敏感文件过滤 |
| 9 | telemetry | `debt_ledger_harvested`、`debt_ledger_failed` |
| 10 | 发布策略 | 直接发布，无 flag |
| 11 | 注释前缀 | `//` 与 `#` |
| 12 | rot 判定 | 无逗号或逗号后仅空白 |
| 13 | 硬上限可配置性 | 硬编码 200，暂不可配置 |
| 14 | 设计文件路径 | `.ody-code/designs/2026-06-18-epic-a-a4-simplification-debt-ledger.md` |

## Architecture

### Data Flow

```
User/Agent
   │
   ▼
debt-ledger skill (`packages/agent-core/src/skill/builtin/debt-ledger.md`) [C:USER]
   │  提供意图、触发词、格式约定
   ▼
agent 调用 harvest-ody-markers tool [C:USER]
   │
   ├─► GrepTool.resolveExecution(...) [C:INFERRED]
   │       pattern: '(#|//) ?ody:'
   │       output_mode: 'content'
   │       include_ignored: false
   │       head_limit: MAX_MARKERS (200)
   │
   ▼
parseOdyMarker(line) [NEW]
   │  从 rg 输出行提取 file/line/ceiling/upgrade/rot
   ▼
renderDebtLedger(markers) [NEW]
   │  按文件分组，中文格式化，标 rot
   ▼
telemetry.track('debt_ledger_harvested', ...) [C:INFERRED]
   │
   ▼
markdown report → agent 返回给用户
```

### Components

| Component | File | Contract |
|---|---|---|
| `debt-ledger.md` skill | `packages/agent-core/src/skill/builtin/debt-ledger.md` [C:USER] | Inline builtin skill；触发词包括 “债务台账”“ody debt”“列出 ody 标记”；教 agent 何时调用 tool 及标记格式 |
| `debt-ledger.ts` skill constant | `packages/agent-core/src/skill/builtin/debt-ledger.ts` [C:INFERRED] | `DEBT_LEDGER_SKILL: SkillDefinition`，由 `registerBuiltinSkills()` 注册 |
| `HarvestOdyMarkersTool` | `packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.ts` [C:INFERRED] | `BuiltinTool<HarvestOdyMarkersInput>`；执行扫描、解析、格式化 |
| `parseOdyMarker` | 同文件 | `(rawLine: string) => DebtLedgerMarker \| null` |
| `renderDebtLedger` | 同文件 | `(markers: DebtLedgerMarker[]) => string` |
| A1 联动改动 | `packages/agent-core/src/skill/builtin/simplicity-first.md` [C:USER] | 在 full/ultra 档输出纪律中追加 “deliberate simplification 留 `ody:` 标记” |
| A3 联动改动 | `packages/agent-core/src/code-review/simplicity.ts` [C:USER] | 在 review/audit prompt 中把 “may mention `ody:` annotation” 升级为正式建议 |

### Call Sites

1. **Skill 注册** `packages/agent-core/src/skill/builtin/index.ts` line ~17–32 [C:INFERRED]
   - 在 `registerBuiltinSkills(registry)` 中新增：
     ```pseudocode
     registry.registerBuiltinSkill(DEBT_LEDGER_SKILL);
     ```

2. **Tool 注册** `packages/agent-core/src/agent/tool/index.ts` line ~407–482 [C:INFERRED]
   - 在 `initializeBuiltinTools()` 的数组中新增：
     ```pseudocode
     new b.HarvestOdyMarkersTool(kaos, workspace),
     ```

3. **Tool barrel 导出** `packages/agent-core/src/tools/builtin/index.ts` line ~40 [C:INFERRED]
   - 追加：
     ```pseudocode
     export * from './code-quality/harvest-ody-markers';
     ```

4. **A1 skill 内容** `packages/agent-core/src/skill/builtin/simplicity-first.md` line ~67 之后 [C:USER]
   - 在 “输出纪律（所有档位）” 末尾追加一段，约：
     > 如果你 deliberate 选择了更简的方案（例如用全局锁、临时文件、JSON.parse 而非 schema 校验），必须在相关代码旁留注释：`// ody: <天花板>, <升级触发条件>`。没有升级触发条件的标记会被债务台账标为 rot。
   - 仅在 full/ultra 档显示该段（用 `<!-- FULL[ -->` / `<!-- ULTRA[ -->` 包裹）。

5. **A3 prompt** `packages/agent-core/src/code-review/simplicity.ts` line ~195 [C:USER]
   - 把原句：
     > If you find something that was deliberately kept simple and could use an `ody:` annotation, you may mention it in the detail — but do not create a finding for it.
   - 改为：
     > If you find something that was deliberately kept simple and could use an `ody:` annotation, suggest adding `// ody: <ceiling>, <upgrade trigger>` in the detail — but do not create a finding for it.

6. **Telemetry** `packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.ts` [C:INFERRED]
   - 在 tool execution 中通过依赖注入的 `telemetry: TelemetryClient` 调用：
     ```pseudocode
     telemetry.track('debt_ledger_harvested', {
       marker_count: markers.length,
       rot_risk_count: markers.filter(m => m.rot).length,
     });
     ```

## Data Models

### `DebtLedgerMarker`

```typescript
interface DebtLedgerMarker {
  readonly file: string;      // relative path from workspace root
  readonly line: number;      // 1-based line number
  readonly ceiling: string;   // 天花板（简化的能力上限）
  readonly upgrade: string;   // 升级触发条件；空字符串表示缺失
  readonly rot: boolean;      // true 当且仅当 upgrade 为空
}
```

### `HarvestOdyMarkersInput`

```typescript
interface HarvestOdyMarkersInput {
  readonly path?: string;     // optional subdirectory or file to scan
}
```

### `HarvestOdyMarkersOutput`

```typescript
interface HarvestOdyMarkersOutput {
  readonly markdown: string;  // rendered ledger report
  readonly markerCount: number;
  readonly rotRiskCount: number;
  readonly truncated: boolean;
}
```

### Tool result shape

`HarvestOdyMarkersTool.resolveExecution()` returns `ToolExecution` whose `execute()` returns `ExecutableToolResult`:

```typescript
{
  isError: boolean;
  output: string;          // JSON-stringified HarvestOdyMarkersOutput
}
```

## Algorithms

### Algorithm 1: Scan for `ody:` markers

```pseudocode
function scanOdyMarkers(toolDeps, input: HarvestOdyMarkersInput): Promise<GrepOutput>
  grepInput := {
    pattern: '(#|//) ?ody:',
    path: input.path ?? workspaceDir,
    output_mode: 'content',
    '-n': true,
    head_limit: MAX_MARKERS,        // 200 [C:USER]
    include_ignored: false,
  }
  execution := GrepTool.resolveExecution(grepInput)
  result := await execution.execute({ signal })
  if result.isError
    throw HarvestError(result.output)
  return JSON.parse(result.output) as GrepOutput
```

### Algorithm 2: Parse a single marker

```pseudocode
function parseOdyMarker(rawLine: string): DebtLedgerMarker | null
  // rawLine comes from rg content mode, format: "path/to/file.ext:12:// ody: 全局锁, 吞吐>100rps"
  match := rawLine.match(/^(.+?):(\d+):\s*(?:(?:\/\/)|#)\s*ody:\s*(.*)$$/)
  if not match
    return null

  file := match[1]
  line := parseInt(match[2], 10)
  body := match[3].trim()

  commaIdx := body.indexOf(',')
  if commaIdx === -1
    return { file, line, ceiling: body, upgrade: '', rot: true }

  ceiling := body.slice(0, commaIdx).trim()
  upgrade := body.slice(commaIdx + 1).trim()
  rot := upgrade === ''
  return { file, line, ceiling, upgrade, rot }
```

**Edge case verified** [C:INFERRED]: 中文逗号 `，` 不匹配英文逗号 `,`，因此会被判定为 rot。设计文档约定使用英文逗号；测试中覆盖此 adversarial 输入。

### Algorithm 3: Render ledger report

```pseudocode
function renderDebtLedger(markers: DebtLedgerMarker[], truncated: boolean): string
  if markers.length === 0
    return '未找到 `ody:` 债务标记。台账干净。'

  groups := groupBy(markers, m => m.file)
  lines := []
  for each file in sorted(groups.keys())
    lines.push(`### ${file}`)
    for each m in sorted(groups[file], by line ascending)
      rotTag := m.rot ? ' ⚠️ rot' : ''
      lines.push(`${m.file}:${m.line} — ${m.ceiling}。天花板：${m.ceiling}。升级：${m.upgrade || '（未指定）'}${rotTag}`)
    lines.push('')

  totalRot := markers.filter(m => m.rot).length
  lines.push(`**汇总**：${markers.length} 个标记，${totalRot} 个 rot 风险。`)
  if truncated
    lines.push('结果已截断至前 200 条；如需完整扫描，请指定更小的目录或文件。')
  return lines.join('\n')
```

### Algorithm 4: Tool execution entrypoint

```pseudocode
class HarvestOdyMarkersTool implements BuiltinTool<HarvestOdyMarkersInput>
  name := 'HarvestOdyMarkers'
  parameters := jsonSchema(HarvestOdyMarkersInputSchema)

  resolveExecution(args): ToolExecution
    return {
      accesses: ToolAccesses.searchTree(workspaceDir),
      description: 'Harvesting ody: simplification debt markers',
      display: { kind: 'file_io', operation: 'grep', path: workspaceDir },
      approvalRule: literalRulePattern(this.name, ''),
      execute: ({ signal }) => this.run(args, signal),
    }

  async run(args, signal): ExecutableToolResult
    try
      grepResult := await scanOdyMarkers(this.deps, args)
      rawLines := grepResult.content.split('\n').filter(nonEmpty)
      markers := rawLines.map(parseOdyMarker).filter(notNull)
      truncated := (grepResult.appliedLimit ?? 0) >= MAX_MARKERS
      markdown := renderDebtLedger(markers, truncated)
      telemetry.track('debt_ledger_harvested', { marker_count: markers.length, rot_risk_count: markers.filter(m=>m.rot).length })
      return { isError: false, output: JSON.stringify({ markdown, markerCount: markers.length, rotRiskCount: markers.filter(m=>m.rot).length, truncated }) }
    catch error
      telemetry.track('debt_ledger_failed', { error: error.message })
      return { isError: true, output: `债务台账扫描失败：${error.message}` }
```

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | `GrepTool` 可被其他 tool 内部调用并返回结构化结果 | High | 需要重新实现 rg 调用 | 已读取 `packages/agent-core/src/tools/builtin/file/grep.ts`；`GrepInput`/`GrepOutput` schema 已确认 [C:INFERRED] |
| 2 | `SkillRegistry.registerBuiltinSkill` 支持新增 `.md` skill | High | 需要扩展注册机制 | explore agent 已确认 `packages/agent-core/src/skill/registry.ts` line 60–62 [C:INFERRED] |
| 3 | A1 `simplicity-first.md` 当前未教 agent 留 `ody:` 标记 | High | 会重复或冲突 | 已读取 `packages/agent-core/src/skill/builtin/simplicity-first.md`，确认无 `ody:` 相关指令 [C:INFERRED] |
| 4 | A3 prompt 中已有一句可扩展为建议补 `ody:` 标记 | High | 需要新增联动段落 | 已读取 `packages/agent-core/src/code-review/simplicity.ts` line 195，确认存在可扩展句子 [C:INFERRED] |
| 5 | 硬上限 200 不会命中常见仓库 | Medium | 需要调整或支持分页 | 实现后用本仓库自测 [C:INFERRED] |
| 6 | 中文逗号 `，` 不会被识别为分隔符，会被标 rot | High | 用户可能误标 rot | 已用 `node -e` 验证 parse 算法；文档约定使用英文逗号 [C:INFERRED] |

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | 标记格式不统一导致解析失败或误标 rot | Medium | 报告质量下降 | 文档化格式；测试覆盖中英文逗号、空触发条件、多逗号等 adversarial 输入 |
| 2 | 债务标记被滥用为逃避 review 的借口 | Medium | 技术债失控 | rot 判定强制要求升级触发条件；A1 skill 明确“没有触发条件 = 未完成” |
| 3 | 大量标记导致报告/token 爆炸 | Low | 用户体验差 | 硬上限 200 + 截断提示 |
| 4 | GrepTool 扫描把非债务注释（如本文档中的 `ody:` 示例）误收 | Low | 误报 | 要求标记必须出现在代码注释前缀 `//` 或 `#` 后；GrepTool 已按行匹配 |

## Reuse Analysis

| File | Symbol | Verdict | Notes |
|---|---|---|---|
| `packages/agent-core/src/tools/builtin/file/grep.ts` | `GrepTool`, `GrepInputSchema`, `GrepOutputSchema` | **Use as-is** | 扫描 `ody:` 标记；自动处理敏感文件过滤、路径安全、超时、分页 |
| `packages/agent-core/src/skill/registry.ts` | `SkillRegistry.registerBuiltinSkill()` | **Use as-is** | 注册 `debt-ledger` builtin skill |
| `packages/agent-core/src/skill/parser.ts` | `parseSkillText()` | **Use as-is** | 解析 `.md` skill frontmatter + body |
| `packages/agent-core/src/agent/skill/index.ts` | `SkillManager.activate()` | **Use as-is** | 注入 skill 上下文 |
| `packages/agent-core/src/agent/tool/index.ts` | `ToolManager.initializeBuiltinTools()` | **Adapt** | 在 builtin tools 数组中新增 `HarvestOdyMarkersTool` |
| `packages/agent-core/src/tools/builtin/index.ts` | re-export barrel | **Adapt** | 导出新增 tool |
| `packages/agent-core/src/telemetry.ts` | `TelemetryClient` | **Use as-is** | 发送 `debt_ledger_harvested` / `debt_ledger_failed` |
| `packages/agent-core/src/code-review/simplicity.ts` | `buildSimplicityReviewPrompt()` / `buildSimplicityAuditPrompt()` | **Adapt** | 扩展 A3 prompt，建议补 `ody:` 标记 |
| `packages/agent-core/src/skill/builtin/simplicity-first.md` | skill content | **Adapt** | 追加“留 `ody:` 标记”指令 |
| — | marker harvester / ledger formatter | **Greenfield** | 现有代码库无此功能 |

## Error Handling

### Error / Degradation Table

| Error Class | Immediate Handling | Degradation Path | Recovery Condition |
|---|---|---|---|
| ripgrep 不可用 | tool 返回 `isError: true`，输出 `rg 未安装或无法访问` | 无法扫描；提示用户检查环境 | 安装 rg 后重试 |
| GrepTool 超时（>20s） | 返回错误，提示“尝试指定更小的 path” | 用户可传入 `path` 参数缩小范围 | 范围缩小后重试 |
| 0 个 `ody:` 标记 | 返回 `isError: false`，输出 `未找到 ody: 债务标记。台账干净。` | 无 | N/A |
| 标记数 > 200 | 返回前 200 条，`truncated: true`，报告末尾附加截断提示 | 用户按目录分批扫描 | 用户缩小扫描范围 |
| 某行解析失败 | 跳过该行，不中断整体扫描；telemetry 记录 `parse_failed_count` | 该行不出现在报告中 | 用户修正标记格式 |
| 路径越界 | `resolvePathAccessPath` 拒绝；tool 返回权限错误 | 无 | 用户传入工作区内路径 |

### Retry / Cooldown

- 不重试：GrepTool 内部已对 `EAGAIN` 做重试 [C:INFERRED]；A4 工具层不再额外重试。
- 冷却：无；每次用户/ agent 调用都是独立扫描。

## Test Plan

### Unit Tests

#### File: `packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.test.ts` [C:INFERRED]

1. **parseOdyMarker — valid `//` marker**
   - Input: `src/lock.ts:12:// ody: 全局锁, 吞吐 > 100 rps 时改为按账户锁`
   - Assert: `file === 'src/lock.ts'`, `line === 12`, `ceiling === '全局锁'`, `upgrade === '吞吐 > 100 rps 时改为按账户锁'`, `rot === false`

2. **parseOdyMarker — valid `#` marker**
   - Input: `scripts/parse.py:8:# ody: 用 JSON.parse, 需要 schema 校验时改为 zod`
   - Assert: `file === 'scripts/parse.py'`, `line === 8`, `ceiling === '用 JSON.parse'`, `upgrade === '需要 schema 校验时改为 zod'`, `rot === false`

3. **parseOdyMarker — missing trigger (rot)**
   - Input: `src/cache.ts:5:// ody: 全局锁`
   - Assert: `rot === true`, `upgrade === ''`

4. **parseOdyMarker — empty trigger after comma (rot)**
   - Input: `src/cache.ts:5:// ody: 全局锁,   `
   - Assert: `rot === true`, `upgrade === ''`

5. **parseOdyMarker — Chinese comma is NOT a separator (rot)**
   - Input: `src/lock.ts:12:// ody: 全局锁，吞吐 > 100 rps 时改为按账户锁`
   - Assert: `rot === true`（约定使用英文逗号）

6. **parseOdyMarker — no `ody:` prefix**
   - Input: `src/lock.ts:12:// TODO: fix lock`
   - Assert: returns `null`

7. **renderDebtLedger — empty**
   - Input: `[]`
   - Assert: output equals `未找到 \`ody:\` 债务标记。台账干净。`

8. **renderDebtLedger — grouped with rot tag**
   - Input: two markers, one rot
   - Assert: output contains `### src/lock.ts`, `src/lock.ts:12 — 全局锁`, `天花板：全局锁`, `升级：（未指定） ⚠️ rot`, `**汇总**：2 个标记，1 个 rot 风险。`

9. **renderDebtLedger — truncated hint**
   - Input: 1 marker, `truncated = true`
   - Assert: output contains `结果已截断至前 200 条`

### Integration Tests

#### File: `packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.integration.test.ts` [C:INFERRED]

1. **Tool scans fixtures correctly**
   - Fixture dir with 3 files containing valid/rot/no `ody:` markers.
   - Call `HarvestOdyMarkersTool` with `path: fixtureDir`.
   - Assert: `markerCount === 2`, `rotRiskCount === 1`, markdown contains both files.

2. **Tool respects head_limit**
   - Fixture with 250 markers.
   - Assert: `markerCount === 200`, `truncated === true`.

3. **Tool emits telemetry**
   - Mock `TelemetryClient.track`.
   - Call tool.
   - Assert: `track` called with event `'debt_ledger_harvested'`, properties `marker_count` and `rot_risk_count`.

4. **Tool handles GrepTool failure**
   - Mock GrepTool to return error.
   - Assert: tool returns `isError: true`, telemetry `'debt_ledger_failed'` fired.

### Skill / Injection Tests

#### File: `packages/agent-core/src/skill/builtin/debt-ledger.test.ts` [C:INFERRED]

1. **Skill is registered**
   - Load `SkillRegistry`.
   - Assert: `getSkill('debt-ledger')` returns non-null definition.

2. **A1 skill contains marker instruction after update**
   - Parse `simplicity-first.md`.
   - Assert: full/ultra body contains `ody:` and `升级触发条件`。

### Done Criteria

- `pnpm --filter @odysseythink/agent-core test harvest-ody-markers` passes [C:INFERRED].
- `pnpm --filter @odysseythink/agent-core test simplicity-first` still passes（A1 改动未破坏现有测试）[C:INFERRED]。
- `pnpm --filter @odysseythink/agent-core test code-review` still passes（A3 prompt 改动未破坏现有测试）[C:INFERRED]。
- `pnpm lint` passes for changed files [C:INFERRED].

## Self-Review

### Highest-stakes decisions scrutinized

#### D1 — `parseOdyMarker` regex and comma-split logic

This decides what counts as a valid debt marker vs. rot. Verified with `node -e`.

| Input | Expected | Actual |
|---|---|---|
| `src/lock.ts:12:// ody: 全局锁, 吞吐 > 100 rps 时改为按账户锁` | ceiling=`全局锁`, upgrade=`吞吐 > 100 rps 时改为按账户锁`, rot=false | ✅ |
| `src/cache.ts:5:// ody: 全局锁` | rot=true | ✅ |
| `src/cache.ts:5:// ody: 全局锁,   ` | rot=true | ✅ |
| `src/lock.ts:12:// ody: 全局锁，吞吐 > 100 rps 时改为按账户锁` | rot=true (Chinese comma) | ✅ |
| `src/lock.ts:12:/* ody: 全局锁, upgrade */` | null (block comment not supported) | ✅ |

#### D2 — GrepTool pattern `(#|//) ?ody:`

| Input | Should match? | Reason |
|---|---|---|
| `// ody: foo` | ✅ yes | primary format |
| `# ody: foo` | ✅ yes | Python/Shell/Markdown |
| `//ody: foo` | ✅ yes | optional space |
| `/* ody: foo */` | ❌ no | block comment not in Scope |
| `body: foo` | ❌ no | not `ody:` prefix |

rg pattern behavior inferred from `GrepInputSchema.pattern` being a regular expression and the documented ripgrep engine.

#### D3 — Hard cap 200 truncation

| Scenario | Expected |
|---|---|
| 150 markers | `truncated=false`, all returned |
| 250 markers | `truncated=true`, 200 returned, hint appended |
| 0 markers | clean message, no error |

### Four-lens sweep

- **Security**: The regex only matches `//`/`#` line comments, so block comments and prose are excluded. Sensitive-file filtering is inherited from `GrepTool`. No new secrets/PII surface introduced. No log or filename leaks identified.
- **Test**: Every behavior has a must-pass and must-reject case (see Test Plan). No assertion contradicts a constant it depends on. Adversarial inputs (Chinese comma, empty trigger, missing prefix, block comment) are covered.
- **Ops**: The tool is stateless; repeat calls are idempotent. Cost/latency bounded by `GrepTool` 20s timeout and 200-marker head limit. No identifier collisions (`HarvestOdyMarkers`, `debt-ledger` are new).
- **Integration**: All relied-upon hooks verified in code:
  - `GrepTool`/`GrepInput`/`GrepOutput` exist in `packages/agent-core/src/tools/builtin/file/grep.ts`.
  - `SkillRegistry.registerBuiltinSkill` exists in `packages/agent-core/src/skill/registry.ts`.
  - `ToolManager.initializeBuiltinTools` exists in `packages/agent-core/src/agent/tool/index.ts`.
  - A1 `simplicity-first.md` exists and currently has no `ody:` marker instruction.
  - A3 `simplicity.ts` line 195 has the extensible sentence.
  - `TelemetryClient` exists in `packages/agent-core/src/telemetry.ts`.
- **Scope**: This remains one coherent subsystem (debt ledger). No decomposition needed.

### Fixes applied during self-review

- None. The design stayed within the clarified boundaries.

## User Final Approval

- **Audit level**: Deep [C:USER]
- **All seven decision dimensions clarified**: ✅ [C:USER]
- **Approach selected**: 方案 A — skill + tool + A1/A3 联动 [C:USER]
- **Chapter key claims confirmed**: ✅ [C:USER]
- **[C:INFERRED] assumptions signed off**: ✅ all 6 accepted [C:USER]
- **Self-review completed**: ✅ [C:INFERRED]
- **Design file path**: `.ody-code/designs/2026-06-18-epic-a-a4-simplification-debt-ledger.md` [C:USER]

**Approval state**: APPROVED pending ExitDesignMode.
