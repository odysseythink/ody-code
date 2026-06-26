# Epic A-A3: 反过度设计 Review / Audit

**Document Type**: Design Specification (implementation-ready)
**Last Updated**: 2026-06-18
**Audit Level**: Deep [C:USER]
**Status**: DRAFT (awaiting approval)

## Scope

### In Scope

- 扩展 `request-code-review` 命令支持 `--focus simplicity` [C:USER]
- 新增 `--scope repo` 以支持全仓库 audit [C:USER]
- 两种输出形态都使用 Ponytail 结构化标签：`delete:` / `stdlib:` / `native:` / `yagni:` / `shrink:` [C:UPSTREAM]
- 输出格式严格遵循 Ponytail 文本约定：每行 `L<line>: <tag> <现状>. <替代>.`，结尾 `net: -N lines possible.` 或 `Lean already. Ship.` [C:USER]
- 复用现有 `packages/agent-core/src/code-review/` 的 diff 获取、模型解析、报告渲染 [C:INFERRED]
- 复用 `GrepTool` 做全仓库扫描（audit） [C:INFERRED]
- 仅报告、不自动修改代码 [C:UPSTREAM]
- 新增 telemetry 事件：`simplicity_review_started/completed/failed` 和 `simplicity_audit_started/completed/failed` [C:USER]
- 直接发布，无需 experimental flag [C:USER]

### Out of Scope

- **不实现 A4 债务台账**：A3 只建议在输出文本中补 `ody:` 标记，不收割、不持久化 [C:USER]
- **不改写 A1**：`simplicity-first` skill 已落地，A3 不修改其内容 [C:INFERRED]
- **不做多 focus 并行**：本次只新增 `correctness`（默认）和 `simplicity`；`security`、`tests` 等 focus 留给后续扩展 [C:INFERRED]
- **不做自动修复**：所有建议由用户决定是否采纳 [C:USER]
- **不保存报告文件**：结果仅当次输出，不写入 `.ody-code/reviews/` [C:USER]

## Prior Art

### Ponytail P1-B

上游 `ponytail-4.7.0/skills/ponytail-review/SKILL.md` 与 `ponytail-audit/SKILL.md` 定义了：

- 五种结构化标签：`delete:`、`stdlib:`、`native:`、`yagni:`、`shrink:` [C:UPSTREAM]
- review 为 diff 范围，audit 为全仓库范围 [C:UPSTREAM]
- 输出格式：`<file>:L<line>: <tag> <what>. <replacement>.` [C:UPSTREAM]
- 结尾净计：`net: -<N> lines possible.` [C:UPSTREAM]
- 无发现时：`Lean already. Ship.` [C:UPSTREAM]
- 只报告、不自动改 [C:UPSTREAM]
- 边界：正确性 bug、安全漏洞、性能问题交给普通 review [C:UPSTREAM]

### ody-code 现有基础设施

- `packages/agent-core/src/code-review/*`：diff 获取、prompt、parse、report 渲染、executor、模型解析 [C:INFERRED]
- `packages/agent-core/src/tools/builtin/file/grep.ts`：ripgrep 工具，带敏感文件过滤、输出限制 [C:INFERRED]
- `packages/agent-core/src/skill/builtin/simplicity-first.*`：A1 已落地，提供 lite/full/ultra 档位 [C:INFERRED]
- `packages/agent-core/src/rpc/core-api.ts` / `core-impl.ts`：`requestCodeReview` RPC [C:INFERRED]
- `apps/ody-code/src/cli/sub/request-code-review.ts` / `tui/commands/request-code-review.ts`：CLI/TUI 入口 [C:INFERRED]

## Architecture

```
CLI/TUI slash command
  │  add --focus simplicity, --scope repo
  ▼
RPC: requestCodeReview(payload)
  │  payload.focus ∈ {'correctness','simplicity'}, default 'correctness'
  │  payload.scope ∈ {'diff','repo'}, default 'diff'
  ▼
code-review/executor.ts
  │  if scope == 'repo' -> call SimplicityAuditScanner
  │  else -> fetchDiff() as today
  ▼
SimplicityAuditScanner (repo-wide)
  │  use GrepTool to collect file list + package.json deps
  │  build compact repo digest
  ▼
SimplicityPromptBuilder
  │  focus + scope -> pick prompt template
  ▼
LLM (independent model via resolveCodeReviewModel)
  │
  ▼
SimplicityReportParser
  │  parse Ponytail-style lines
  ▼
CodeReviewReport (reuse existing type, finding.detail holds raw line)
  │
  ▼
renderCodeReviewReportToMarkdown
```

## Reuse Analysis

| 组件 | 文件路径 | 复用方式 | 说明 |
|------|---------|---------|------|
| diff 获取 | `packages/agent-core/src/code-review/diff.ts` | 直接使用 | `fetchDiff()` 对 `scope=diff` 保持不变 |
| prompt/parser | `packages/agent-core/src/code-review/prompt.ts` | 扩展新增 | 新增 `buildSimplicityReviewPrompt` / `parseSimplicityReport` |
| report 渲染 | `packages/agent-core/src/code-review/report.ts` | 直接使用 | 纯文本 finding 可原样渲染 |
| executor | `packages/agent-core/src/code-review/executor.ts` | 扩展 | 注入 `auditScanner` 依赖，分支处理 `scope=repo` |
| 模型解析 | `packages/agent-core/src/code-review/model-resolver.ts` | 直接使用 | `resolveCodeReviewModel('request', ...)` 不变 |
| ripgrep | `packages/agent-core/src/tools/builtin/file/grep.ts` | 通过类型接口注入 | executor 接收 `GrepTool` 实例用于 audit |
| telemetry | `packages/agent-core/src/agent/telemetry` | 新增事件 | 通过 `agent.telemetry.track` 调用 |

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|-----------|------------|-----------------|---------------|
| 1 | `GrepTool` 可以通过其公开 `resolveExecution` + `execute` 接口被 executor 调用 | Medium | 若不能，则需要把 audit 逻辑上提到 RPC 层，改动更大 | 阅读 `GrepTool` 的 `resolveExecution` / `execute` 签名 |
| 2 | `CodeReviewFindingData` 可以容纳 `detail` 中的 Ponytail 原始行，无需新增 `tag` 字段 | Medium | 若后续需要机器消费 tag，则需扩展类型并同步 SDK | 确认 SDK 导出类型是否允许扩展 |
| 3 | 用户在 `--scope repo` 时接受扫描耗时可能较长（数十秒） | Medium | 若超时频繁，需要分页/异步化 | 在实现后实测大仓库 |
| 4 | 现有 `request-code-review` 的模型 token 上限足够容纳 audit 摘要 | Low | 摘要过大时模型会截断，需要更激进的摘要策略 | 实现后压测 |
| 5 | `--focus simplicity` 的 prompt 不会与 `--deep` 子代理模式冲突 | Medium | 若冲突需禁用 focus+deep 组合或调整子代理 prompt | 实现后组合测试 |
| 6 | `apps/ody-code/src/tui/commands/request-code-review.ts` 的手写参数解析可以安全添加 `--focus` / `--scope` | High | 解析错误会导致命令不可用 | 阅读现有 parseArgs |

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|-----------|--------|------------|
| 1 | 模型误报，建议删掉必要代码 | 中 | 高 | 明确 prompt 边界：只猎杀过度设计，不碰正确性/安全/性能；`delete:` 标签必须附带替代方案 |
| 2 | Audit 扫描大量文件导致 token 超支 | 中 | 中 | 扫描前按文件类型/大小过滤，生成紧凑摘要，超限时拒绝并提示缩小范围 |
| 3 | `--focus/--scope` 参数解析与现有位置参数冲突 | 低 | 中 | 扩展 parseArgs 时保持向后兼容：`--focus` 必须显式带值，位置参数仍解析为 base/head |
| 4 | 输出格式依赖模型自律，不遵循 Ponytail 格式 | 中 | 低 | prompt 中给出严格示例和边界；parser 宽容回退，无法解析时原样展示 |
| 5 | Simplicity review 与 correctness review 关注点冲突 | 低 | 中 | `focus` 字段二选一；未来若要合并可同时传两个 focus，本次不实现 |

## Test Plan (Done Criteria)

TBD — will be detailed in subsequent section.

## Self-Review

TBD — will be filled after design is complete.

## User Final Approval

TBD — awaiting ExitDesignMode.

## Data Models

### Extended `CodeReviewRequestInput`

```typescript
// packages/agent-core/src/code-review/types.ts
export interface CodeReviewRequestInput {
  readonly source: CodeReviewDiffSource;
  readonly modelAlias: string;
  readonly description?: string | undefined;
  readonly requirements?: string | undefined;
  readonly deep?: boolean | undefined;
  readonly timeoutMs?: number | undefined;
  readonly focus?: 'correctness' | 'simplicity' | undefined;  // [C:USER]
  readonly scope?: 'diff' | 'repo' | undefined;               // [C:USER]
}
```

- `focus` 默认 `'correctness'`，保持现有行为 [C:INFERRED]
- `scope` 默认 `'diff'`；当 `scope='repo'` 时，`source` 字段被忽略 [C:USER]

### Extended `RequestCodeReviewPayload`

```typescript
// packages/agent-core/src/rpc/core-api.ts
export interface RequestCodeReviewPayload {
  readonly modelAlias?: string | undefined;
  readonly source: CodeReviewDiffSource;
  readonly description?: string | undefined;
  readonly requirements?: string | undefined;
  readonly deep?: boolean | undefined;
  readonly timeoutMs?: number | undefined;
  readonly workDir: string;
  readonly focus?: 'correctness' | 'simplicity' | undefined;  // [C:USER]
  readonly scope?: 'diff' | 'repo' | undefined;               // [C:USER]
}
```

### New `SimplicityTag` union

```typescript
// packages/agent-core/src/code-review/simplicity.ts
export type SimplicityTag = 'delete' | 'stdlib' | 'native' | 'yagni' | 'shrink';  // [C:UPSTREAM]
```

### New `RepoAuditDigest`

```typescript
// packages/agent-core/src/code-review/simplicity.ts
export interface RepoAuditDigest {
  readonly workspaceDir: string;
  readonly fileCount: number;
  readonly files: readonly string[];        // relative paths, capped
  readonly dependencies: readonly string[]; // from package.json / pyproject.toml / etc.
  readonly snippets: readonly FileSnippet[]; // selected small excerpts for pattern hints
}

export interface FileSnippet {
  readonly path: string;
  readonly lines: string;
}
```

### New `SimplicityReviewDeps`

```typescript
// packages/agent-core/src/code-review/executor.ts
export interface CodeReviewExecutorDeps {
  readonly cwd: string;
  readonly fetchDiff: (source: CodeReviewDiffSource, cwd: string) => Promise<string>;
  readonly generate: (...) => Promise<...>;
  readonly resolveProviderConfig: (alias: string) => unknown;
  readonly estimateTokens: (text: string) => number;
  readonly deepRunner?: ...;
  readonly auditScanner?: (workspaceDir: string, signal?: AbortSignal) => Promise<RepoAuditDigest>; // [C:INFERRED]
}
```

## Algorithms

### Algorithm 1: Build Audit Digest

输入：workspaceDir, signal
输出：RepoAuditDigest

```
function buildAuditDigest(workspaceDir, signal):
  files = listSourceFiles(workspaceDir, signal)   // exclude .git, node_modules, dist, sensitive
  files = sortByMtimeDesc(files)
  files = take(files, MAX_AUDIT_FILES)            // cap, e.g. 200

  deps = readDependencyNames(workspaceDir)        // package.json dependencies/devDependencies

  snippets = []
  for file in files:
    if snippetBudgetExhausted(): break
    content = readFirstNBytes(file, 2048)
    snippets.push({ path: relative(file), lines: first 30 lines })

  return { workspaceDir, fileCount: len(files), files, dependencies: deps, snippets }
```

### Algorithm 2: Select Prompt Template

输入：input
输出：systemPrompt, userPrompt

```
function selectPrompt(input):
  if input.focus == 'simplicity':
    if input.scope == 'repo':
      return buildSimplicityAuditPrompt(digest)
    else:
      return buildSimplicityReviewPrompt(diff, input.description, input.requirements)
  else:
    return buildReviewPrompt(diff, input.description, input.requirements)  // existing
```

### Algorithm 3: Parse Simplicity Report

输入：raw LLM output
输出：CodeReviewReport

```
function parseSimplicityReport(raw, reviewerAlias):
  if raw contains "Lean already. Ship.":
    return { ok: true, reviewerAlias, findings: [], summary: "Lean already. Ship." }

  findings = []
  for each line in raw:
    parsed = parseSimplicityLine(line)
    if parsed is null: continue
    location = buildLocation(parsed.file, parsed.lineno)
    findings.push({
      severity: mapTagToSeverity(parsed.tag),  // 'important' for yagni/delete, 'minor' for shrink
      title: `[${parsed.tag.toUpperCase()}] ${parsed.what}`,
      detail: `${parsed.tag}: ${parsed.what}. ${parsed.replacement}.`,
      location,
      suggestedFix: parsed.replacement,
    })

  summary = extractNetLine(raw)  // "net: -N lines possible."
  return { ok: true, reviewerAlias, findings, summary }

function parseSimplicityLine(line):
  line = trim(line)
  TAGS = ['delete', 'stdlib', 'native', 'yagni', 'shrink']

  // Step 1: try to strip optional location prefix, but only if what follows starts with a tag
  prefixMatch = regex /^\s*(?:(.+?)(?::L?|:))?L?(\d+)(?:-\d+)?:\s*/.exec(line)
  if prefixMatch exists:
    afterPrefix = line.slice(prefixMatch[0].length)
    if any tag in TAGS is prefix of afterPrefix followed by ':':
      rest = afterPrefix
      file = prefixMatch[1]
      lineno = prefixMatch[2]
    else:
      rest = line
  else:
    rest = line

  // Step 2: parse tag
  tagMatch = regex /^((?:delete|stdlib|native|yagni|shrink)):\s*/.exec(rest)
  if tagMatch is null: return null
  tag = tagMatch[1]
  body = rest.slice(tagMatch[0].length)

  // Step 3: split on first '. ' into what / replacement
  dotIdx = indexOf('. ', body)
  if dotIdx < 0: return null
  what = trim(body.slice(0, dotIdx))
  replacement = trim(trailingDot(body.slice(dotIdx + 2)))

  return { file, lineno, tag, what, replacement }
```

## Error Handling

| Error class | Immediate handling | Degradation path | Recovery condition |
|------------|--------------------|------------------|-------------------|
| `scope=repo` + `source` conflict | TUI/CLI validation报错，拒绝执行 | 提示用户省略 base/head/pr | 用户重新输入 `--scope repo` 不带 diff 参数 |
| diff too large | executor 返回 `ok:false, note: 超过 MAX_DIFF_TOKENS` | 提示缩小 base/head 范围 | 用户缩小范围 |
| audit scan timeout | GrepTool 返回部分结果或 timeout message | 返回已收集的 digest，并提示部分扫描 | 用户缩小路径或增加 timeout |
| LLM output unparseable | parser 返回 `ok:true`，findings 为空，summary 为原始文本 | 原样展示模型输出 | 用户可重试或手动阅读 |
| GrepTool unavailable (rg missing) | 返回 `ok:false` | 提示安装 ripgrep | 安装 rg 后重试 |
| Sensitive files in scan range | GrepTool 自动过滤，报告中追加说明 | 无 | 无需恢复 |

## Call-Site Integration

### 1. CLI 参数解析

文件：`apps/ody-code/src/cli/sub/request-code-review.ts`
行范围：10–19, 105–116

```typescript
interface RequestCodeReviewCLIOptions {
  // ... existing fields ...
  focus?: 'correctness' | 'simplicity';   // [C:USER]
  scope?: 'diff' | 'repo';                // [C:USER]
}

// in registerRequestCodeReviewCommand:
.option('--focus <focus>', 'Review focus: correctness (default) or simplicity.')
.option('--scope <scope>', 'Review scope: diff (default) or repo (whole workspace audit).')

// validateRequestCodeReviewOptions:
if (opts.scope === 'repo' && (opts.base || opts.head || opts.pr)) {
  throw new OptionConflictError('Cannot combine --scope repo with --base/--head/--pr.');
}
```

### 2. TUI 参数解析

文件：`apps/ody-code/src/tui/commands/request-code-review.ts`
行范围：7–36, 90–98

```typescript
interface SlashArgs {
  // ... existing fields ...
  focus?: 'correctness' | 'simplicity';   // [C:USER]
  scope?: 'diff' | 'repo';                // [C:USER]
}

// in parseArgs:
if (token === '--focus' || token === '--scope') {
  result[camelFromFlag(token)] = tokens[i + 1];
  i += 1;
}

// in handleRequestCodeReviewCommand:
const report = await host.harness.requestCodeReview({
  source,
  modelAlias: resolvedModel,
  description: parsed.description,
  requirements: parsed.requirements,
  deep: parsed.deep,
  focus: parsed.focus,
  scope: parsed.scope,
});
```

### 3. RPC payload 透传

文件：`packages/agent-core/src/rpc/core-impl.ts`
行范围：522–529

```typescript
const report = await executor.review({
  source: payload.source,
  modelAlias: resolvedModel,
  description: payload.description,
  requirements: payload.requirements,
  deep: payload.deep,
  timeoutMs: payload.timeoutMs,
  focus: payload.focus,   // [C:USER]
  scope: payload.scope,   // [C:USER]
});
```

### 4. Executor 分支

文件：`packages/agent-core/src/code-review/executor.ts`
行范围：21–82

```typescript
async review(input):
  if (input.scope === 'repo'):
    if (deps.auditScanner === undefined):
      return fail('Repo audit is not available in this context.')
    digest = await deps.auditScanner(deps.cwd, signal)
    userPrompt = buildSimplicityAuditPrompt(digest)
  else:
    diff = await deps.fetchDiff(input.source, deps.cwd)
    if (estimatedTokens > MAX_DIFF_TOKENS) return fail(...)
    if (input.focus === 'simplicity'):
      userPrompt = buildSimplicityReviewPrompt(diff, input.description, input.requirements)
    else:
      userPrompt = buildReviewPrompt(diff, input.description, input.requirements)

  response = await deps.generate({ modelAlias, systemPrompt, userPrompt, signal })
  text = extractText(response)

  if (input.focus === 'simplicity' || input.scope === 'repo'):
    return parseSimplicityReport(text, input.modelAlias)
  else:
    return parseReviewReport(text, input.modelAlias)
```

### 5. Audit Scanner 注入

文件：`packages/agent-core/src/rpc/core-impl.ts`
行范围：490–520

```typescript
const executor = createCodeReviewExecutor({
  cwd: payload.workDir,
  fetchDiff: async (source) => codeReviewFetchDiff(source, payload.workDir),
  auditScanner: async (workspaceDir, signal) => {
    const grep = ... // resolve GrepTool instance
    return buildAuditDigest(grep, workspaceDir, signal);
  },
  // ... generate, resolveProviderConfig, estimateTokens ...
});
```

## Telemetry

新增以下事件，通过 `agent.telemetry.track` 发送 [C:USER]：

| Event | Properties |
|-------|-----------|
| `simplicity_review_started` | `scope: 'diff'`, `focus: 'simplicity'`, `has_description: bool`, `has_requirements: bool` |
| `simplicity_review_completed` | `scope: 'diff'`, `finding_count: number`, `ok: bool` |
| `simplicity_review_failed` | `scope: 'diff'`, `reason: string` |
| `simplicity_audit_started` | `scope: 'repo'`, `file_count: number` |
| `simplicity_audit_completed` | `scope: 'repo'`, `finding_count: number`, `ok: bool` |
| `simplicity_audit_failed` | `scope: 'repo'`, `reason: string` |

在 `packages/agent-core/src/rpc/core-impl.ts:requestCodeReview` 中，在调用 executor 前后埋点 [C:INFERRED]。

## Prompt Outlines

### `buildSimplicityReviewPrompt(diff, description, requirements)`

- 角色：你是反过度设计审查员，只猎杀不必要的复杂性 [C:UPSTREAM]
- 输入：`description`, `requirements`, diff [C:INFERRED]
- 任务：逐行检查 diff，只报告可删除/替换/缩短的代码 [C:UPSTREAM]
- 输出格式：每行 `L<line>: <tag> <现状>. <替代>.` [C:UPSTREAM]
- 标签定义：delete / stdlib / native / yagni / shrink [C:UPSTREAM]
- 结尾：`net: -<N> lines possible.`；无发现则 `Lean already. Ship.` [C:UPSTREAM]
- 边界：不报告正确性 bug、安全漏洞、性能问题 [C:UPSTREAM]
- 与 A1 联动：若发现适合用 `ody:` 标记的刻意简化，可在 detail 末尾建议补标记 [C:USER]

### `buildSimplicityAuditPrompt(digest)`

- 角色同上 [C:UPSTREAM]
- 输入：文件列表、依赖列表、代码片段摘要 [C:INFERRED]
- 任务：按"可削减代码量"从大到小排名 [C:UPSTREAM]
- 输出格式：每行 `<tag> <what to cut>. <replacement>. [path]` [C:UPSTREAM]
- 结尾：`net: -<N> lines, -<M> deps possible.` [C:UPSTREAM]
- 边界同上 [C:UPSTREAM]

## Test Plan

### Unit Tests

文件：`packages/agent-core/src/code-review/__tests__/simplicity.test.ts` [C:INFERRED]

1. **parseSimplicityReport parses well-formed lines**
   - Input: `L12: stdlib: 27-line validator class. Use String.prototype.includes, 1 line.`
   - Assert: `findings[0].severity === 'important'`, `findings[0].location === ':12'`, `findings[0].detail` contains `stdlib:`

2. **parseSimplicityReport handles "Lean already. Ship."**
   - Input: `Lean already. Ship.`
   - Assert: `findings.length === 0`, `summary === 'Lean already. Ship.'`

3. **parseSimplicityReport extracts net line**
   - Input: `L1: delete: unused util. Remove it.\nnet: -50 lines possible.`
   - Assert: `summary === 'net: -50 lines possible.'`

4. **buildSimplicityReviewPrompt includes all tags**
   - Assert: prompt string contains `delete:`, `stdlib:`, `native:`, `yagni:`, `shrink:`

5. **buildSimplicityAuditPrompt excludes node_modules**
   - Assert: prompt string does not contain `node_modules/`

### Integration Tests

文件：`packages/agent-core/src/code-review/__tests__/executor.test.ts` [C:INFERRED]

6. **executor uses simplicity prompt when focus=simplicity**
   - Mock `generate` 返回 Ponytail 格式文本
   - Input: `{ focus: 'simplicity', scope: 'diff', source: { kind: 'working-tree' } }`
   - Assert: `generate` called with userPrompt containing `反过度设计` 或 `simplicity`

7. **executor calls auditScanner when scope=repo**
   - Mock `auditScanner` 返回 digest
   - Input: `{ focus: 'simplicity', scope: 'repo' }`
   - Assert: `auditScanner` called with `cwd`, `generate` called with prompt containing `repo-wide`

### CLI/TUI Tests

8. **CLI parses --focus simplicity --scope repo**
   - 调用 `validateRequestCodeReviewOptions({ scope: 'repo', focus: 'simplicity' })`
   - Assert: 不抛错

9. **CLI rejects --scope repo with --base**
   - 调用 `validateRequestCodeReviewOptions({ scope: 'repo', base: 'HEAD~1' })`
   - Assert: throws `OptionConflictError`

10. **TUI parseArgs handles --focus and --scope**
    - Input: `"--focus simplicity --scope repo"`
    - Assert: `parsed.focus === 'simplicity'`, `parsed.scope === 'repo'`

### Done Criteria

```bash
pnpm nx typecheck agent-core
pnpm nx test agent-core -- --run
pnpm nx typecheck ody-code
pnpm nx test ody-code -- --run
```

## Self-Review

### Security
- 检查了 `GrepTool` 的敏感文件过滤逻辑，确认 audit 复用后可自动排除 `.env`、SSH key 等 [C:INFERRED]
- 未发现设计会将 diff 或 audit 摘要写入日志文件或文件名中包含敏感信息 [C:INFERRED]
- `--scope repo` 仍受 workspace path policy 约束，不会扫描工作目录外路径 [C:INFERRED]

### Test
- 每个行为都定义了 must-pass 断言（见 Test Plan） [C:INFERRED]
- must-reject 场景：CLI 拒绝 `--scope repo` + `--base`；TUI 拒绝未知 focus [C:INFERRED]
- 未发现断言与常量矛盾 [C:INFERRED]

### Ops
- Audit 调用 GrepTool 扫描全仓库，可能耗时较长；设计通过 `timeoutMs` 和 GrepTool 自身 20s 超时限制 [C:INFERRED]
- 并发：每次 review/audit 是独立 RPC 调用，无共享可变状态 [C:INFERRED]
- 标识符：`focus`/`scope` 使用字面量联合类型，避免碰撞 [C:INFERRED]

### Integration
- 验证了 `RequestCodeReviewPayload`、`CodeReviewRequestInput`、`createCodeReviewExecutor`、`resolveCodeReviewModel`、CLI/TUI 入口均存在且可扩展 [C:INFERRED]
- 目标路径：`packages/agent-core/src/code-review/` 和 `apps/ody-code/src/{cli,tui}/request-code-review.ts`，与用户命名的 `request-code-review` 命令一致，未静默改道 [C:USER]

### Scope
- 本设计仍是单一相干子系统：扩展 code-review 模块以支持 simplicity focus 和 repo audit [C:INFERRED]
- 未扩大到 A4 债务台账、A1 修改、或多 focus 并行 [C:USER]

## User Final Approval

- [x] Scope In/Out reviewed [C:USER]
- [x] Architecture reviewed [C:USER]
- [x] Data Models reviewed [C:USER]
- [x] Algorithms reviewed [C:USER]
- [x] Error Handling reviewed [C:USER]
- [x] Test Plan reviewed [C:USER]
- [x] Risk Register reviewed [C:USER]
- [x] Assumptions signed off (Deep audit) [C:USER]

**Approved by user via design-mode audit gate on 2026-06-18.** [C:USER]
