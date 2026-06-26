# Code Review 命令 — 核心执行器

## 设计目标

为 `request-code-review` 命令提供可复用的代码审查执行能力，支持本地 git diff 与 GitHub PR diff 两种来源，默认单轮 LLM 输出 markdown 报告，可选 subagent 深度审查 [C:USER]。

## 数据模型

### `CodeReviewRequestInput`

```typescript
interface CodeReviewRequestInput {
  /** diff 来源：本地 commit range 或 GitHub PR */
  readonly source: CodeReviewDiffSource;
  /** 模型别名，已由 resolver 解析为有效值 */
  readonly modelAlias: string;
  /** 一句话描述本次改动是做什么的 */
  readonly description?: string;
  /** 原始需求或计划文件路径/内容 */
  readonly requirements?: string;
  /** 是否派发 reviewer subagent 做深度审查 */
  readonly deep?: boolean;
  /** LLM 调用超时（毫秒） */
  readonly timeoutMs?: number;
}

type CodeReviewDiffSource =
  | { kind: 'commits'; base: string; head: string }
  | { kind: 'pr'; prUrlOrNumber: string }
  | { kind: 'working-tree' };
```

### `CodeReviewReport`

```typescript
interface CodeReviewReport {
  readonly ok: boolean;
  readonly reviewerAlias: string;
  readonly summary?: string;
  readonly findings: readonly CodeReviewFinding[];
  readonly note?: string;  // 失败或降级时的可读说明
}

interface CodeReviewFinding {
  readonly severity: 'critical' | 'important' | 'minor';
  readonly title: string;
  readonly detail: string;
  readonly location?: string;
  readonly suggestedFix?: string;
}
```

## 接口

```typescript
interface CodeReviewExecutor {
  /**
   * 执行代码审查并返回结构化报告。
   * 失败时返回 ok=false 的 report，不会直接抛错。
   */
  review(input: CodeReviewRequestInput): Promise<CodeReviewReport>;
}
```

## 算法

### Diff 获取

```
function fetchDiff(source: CodeReviewDiffSource, cwd: string): Promise<string> {
  switch (source.kind) {
    case 'commits':
      return exec('git', ['diff', source.base, source.head], { cwd })

    case 'working-tree':
      staged = exec('git', ['diff', '--cached'], { cwd })
      unstaged = exec('git', ['diff'], { cwd })
      return staged + '\n' + unstaged

    case 'pr':
      prNumber = parsePrNumber(source.prUrlOrNumber)
      // 使用 gh CLI 拉取 diff 与 PR 描述
      diff = exec('gh', ['pr', 'diff', prNumber], { cwd })
      if (diff.trim().length === 0) {
        throw CodeReviewError('PR diff is empty or gh CLI is not authenticated.')
      }
      return diff
  }
}
```

### Prompt 构造

```
function buildReviewPrompt(diff: string, description: string, requirements: string): string {
  return [
    'You are a code reviewer. Review the following changes.',
    '',
    '## Context',
    description ? `What was built: ${description}` : 'What was built: [not provided]',
    requirements ? `Requirements: ${requirements}` : 'Requirements: [not provided]',
    '',
    '## Diff',
    '```diff',
    diff,
    '```',
    '',
    '## Your Task',
    '1. Evaluate the changes against the requirements.',
    '2. Categorize findings as Critical / Important / Minor.',
    '3. For each finding, give a title, detail, file/line location if available, and suggested fix.',
    '4. Conclude with an assessment: Ready to proceed / Needs fixes.',
    '',
    'Output format:',
    '```',
    'Strengths:',
    '- ...',
    '',
    'Findings:',
    'Critical:',
    '- [title] (location)\n  detail\n  fix: ...',
    '',
    'Important:',
    '- ...',
    '',
    'Minor:',
    '- ...',
    '',
    'Assessment: Ready to proceed / Needs fixes',
    '```',
  ].join('\n')
}
```

### 报告解析

```
function parseReviewReport(raw: string, reviewerAlias: string): CodeReviewReport {
  assessment = extractAssessment(raw)      // 最后一行 Assessment: ...
  strengths = extractSection(raw, 'Strengths')
  critical = extractFindings(raw, 'Critical')
  important = extractFindings(raw, 'Important')
  minor = extractFindings(raw, 'Minor')

  findings = [
    ...critical.map(f => { ...f, severity: 'critical' }),
    ...important.map(f => { ...f, severity: 'important' }),
    ...minor.map(f => { ...f, severity: 'minor' }),
  ]

  return {
    ok: true,
    reviewerAlias,
    summary: strengths.join('\n'),
    findings,
  }
}
```

### 主执行流程

```
function review(input: CodeReviewRequestInput): Promise<CodeReviewReport> {
  try {
    diff = await fetchDiff(input.source, cwd)
  } catch (error) {
    return { ok: false, reviewerAlias: input.modelAlias, findings: [], note: error.message }
  }

  if (tokenEstimate(diff) > MAX_DIFF_TOKENS) {
    return {
      ok: false,
      reviewerAlias: input.modelAlias,
      findings: [],
      note: `Diff too large (~${tokenEstimate(diff)} tokens). Try a smaller range or use --base/--head.`
    }
  }

  if (input.deep) {
    return runReviewerSubagent(diff, input)
  }

  provider = resolveProvider(input.modelAlias)
  prompt = buildReviewPrompt(diff, input.description, input.requirements)
  response = await generate(provider, '', [], [{ role: 'user', content: prompt }], {}, { signal })
  reportText = assembleResponseText(response)
  return parseReviewReport(reportText, input.modelAlias)
}
```

### Subagent 深度审查

```
function runReviewerSubagent(diff: string, input: CodeReviewRequestInput): Promise<CodeReviewReport> {
  taskPrompt = buildReviewerSubagentPrompt(diff, input)
  task = await subagentHost.dispatch({
    type: 'general-purpose',
    prompt: taskPrompt,
    modelAlias: input.modelAlias,
  })
  result = await task.waitForCompletion({ timeoutMs: input.timeoutMs })
  return parseReviewReport(result.output, input.modelAlias)
}
```

Subagent prompt 基于现有 `requesting-code-review.md` 中的模板 [C:UPSTREAM]。

## 调用位置

| 调用方 | 路径 | 调用方式 |
|---|---|---|
| CLI 子命令 | `apps/ody-code/src/cli/sub/request-code-review.ts` | `executor.review({ source, modelAlias, description, requirements, deep })` |
| TUI slash | `apps/ody-code/src/tui/commands/request-code-review.ts` | 同上，结果通过 `host.sendNormalUserInput(reportText)` 注入会话 |

## 错误与降级

| 错误类 | 立即处理 | 降级路径 | 恢复条件 |
|---|---|---|---|
| `git` 不在仓库中 | 返回 `ok=false` | 提示用户 `cd` 到 git 仓库 | 用户在 git 仓库中运行 |
| `gh` 未安装/未登录 | 返回 `ok=false` | 提示使用 `--base`/`--head` 或登录 gh | gh 可用 |
| diff 超过 token 上限 | 返回 `ok=false` | 提示缩小范围 | 用户提供更小的 diff |
| LLM 调用失败 | 返回 `ok=false` | 无 | 模型服务恢复 |
| subagent 超时 | 返回 `ok=false` | 提示用户可重试或不使用 `--deep` | 网络/服务恢复 |

## 测试断言

1. `fetchDiff({ kind: 'commits', base: 'HEAD~1', head: 'HEAD' }, cwd)` 返回非空字符串（在真实 git 仓库中）。
2. `buildReviewPrompt('diff', 'desc', 'req')` 包含 '## Diff' 与 'Assessment' 指令。
3. `parseReviewReport` 对标准格式输入返回 3 条 finding 且 severity 正确。
4. `review()` 在 diff 过大时返回 `ok=false` 且 note 包含 token 估算。
5. `--deep` 为 true 时调用 `subagentHost.dispatch` 而非直接 `generate`。
