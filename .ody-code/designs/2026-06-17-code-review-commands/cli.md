# Code Review 命令 — CLI 层

## 设计目标

为 `ody request-code-review` 提供非交互式命令入口，直接输出 markdown 审查报告到 stdout [C:USER]。

## 命令注册

在 `apps/ody-code/src/cli/commands.ts` 中新增子命令注册 [C:INFERRED]：

```typescript
import { registerRequestCodeReviewCommand } from './sub/request-code-review';

// 在 registerProviderCommand(program) 之后调用：
registerRequestCodeReviewCommand(program);
```

## 命令参数

```typescript
interface RequestCodeReviewCLIOptions {
  /** 本地 commit range：base..head */
  readonly base?: string;
  readonly head?: string;
  /** GitHub PR URL 或 number */
  readonly pr?: string;
  /** 模型别名，覆盖配置 */
  readonly model?: string;
  /** 一句话描述改动 */
  readonly description?: string;
  /** 需求/计划文件路径或字符串 */
  readonly requirements?: string;
  /** 派发 reviewer subagent 做深度审查 */
  readonly deep?: boolean;
  /** 超时秒数 */
  readonly timeout?: number;
}
```

对应 Commander 注册（`apps/ody-code/src/cli/sub/request-code-review.ts`）：

```typescript
export function registerRequestCodeReviewCommand(program: Command): void {
  program
    .command('request-code-review')
    .description('Request a code review for the current changes.')
    .option('--base <sha>', 'Base commit for the review range.')
    .option('--head <sha>', 'Head commit for the review range. Defaults to HEAD.')
    .option('--pr <url-or-number>', 'Review a GitHub PR instead of a local range.')
    .option('-m, --model <model>', 'Model alias to use for this review.')
    .option('-d, --description <text>', 'Short description of what was built.')
    .option('-r, --requirements <text>', 'Requirements or plan the changes should meet.')
    .option('--deep', 'Dispatch a reviewer subagent for deeper analysis.', false)
    .option('-t, --timeout <seconds>', 'Timeout for the review in seconds.', parseInt)
    .action(async (opts: RequestCodeReviewCLIOptions) => {
      await runRequestCodeReviewCommand(opts);
    });
}
```

## 参数校验算法

```
function validateRequestCodeReviewOptions(opts: RequestCodeReviewCLIOptions): void {
  if (opts.pr !== undefined && (opts.base !== undefined || opts.head !== undefined)) {
    throw OptionConflictError('Cannot combine --pr with --base/--head.')
  }

  if (opts.pr === undefined && opts.base === undefined) {
    // 默认使用 HEAD~1..HEAD
    opts.base = 'HEAD~1'
    opts.head = opts.head ?? 'HEAD'
  }

  if (opts.base !== undefined && opts.head === undefined) {
    opts.head = 'HEAD'
  }

  if (opts.timeout !== undefined && (opts.timeout <= 0 || !Number.isInteger(opts.timeout))) {
    throw OptionConflictError('--timeout must be a positive integer (seconds).')
  }
}
```

## 主执行流程

```
async function runRequestCodeReviewCommand(opts: RequestCodeReviewCLIOptions): Promise<void> {
  validateRequestCodeReviewOptions(opts)

  harness = createKimiHarness({ uiMode: 'print', ... })
  await harness.ensureConfigFile()
  config = await harness.getConfig()

  source = buildDiffSource(opts)

  modelAlias = resolveCodeReviewModel(
    'request',
    config.modeModels,
    config.defaultModel,
    { explicit: opts.model }
  )

  executor = createCodeReviewExecutor(harness)
  report = await executor.review({
    source,
    modelAlias,
    description: opts.description,
    requirements: opts.requirements,
    deep: opts.deep,
    timeoutMs: opts.timeout !== undefined ? opts.timeout * 1000 : undefined,
  })

  if (!report.ok) {
    console.error(report.note ?? 'Code review failed.')
    process.exitCode = 1
    return
  }

  console.log(renderReportToMarkdown(report))
}
```

## 报告渲染

```
function renderReportToMarkdown(report: CodeReviewReport): string {
  lines = [
    `# Code Review Report (${report.reviewerAlias})`,
    '',
    report.summary ?? '',
    '',
    `## Findings (${report.findings.length})`,
    '',
  ]

  for (const finding of report.findings) {
    lines.push(`### [${finding.severity.toUpperCase()}] ${finding.title}`)
    if (finding.location) lines.push(`- **Location:** ${finding.location}`)
    lines.push(finding.detail)
    if (finding.suggestedFix) lines.push(`- **Suggested fix:** ${finding.suggestedFix}`)
    lines.push('')
  }

  return lines.join('\n')
}
```

## 调用位置

| 文件 | 行范围 | 说明 |
|---|---|---|
| `apps/ody-code/src/cli/commands.ts` | 93-94 附近 | 新增 `registerRequestCodeReviewCommand(program)` [C:INFERRED] |
| `apps/ody-code/src/cli/sub/request-code-review.ts` | 新建 | 命令定义、参数解析、执行入口 [C:INFERRED] |

## 错误与降级

| 错误类 | 立即处理 | 降级路径 | 恢复条件 |
|---|---|---|---|
| 参数冲突 | 抛出 `OptionConflictError` | 无 | 用户修正参数 |
| 模型解析失败 | stderr 输出错误，exit code 1 | 无 | 用户配置有效模型 |
| diff 获取失败 | stderr 输出错误，exit code 1 | 无 | 修正 git/gh 环境 |
| 报告生成失败 | stderr 输出错误，exit code 1 | 无 | 模型服务恢复 |

## 测试断言

1. `ody request-code-review --base HEAD~1 --head HEAD` 在 git 仓库中返回 markdown 报告且 exit code 0。
2. `ody request-code-review --pr 123 --base HEAD~1` 返回参数冲突错误且 exit code 非 0。
3. `ody request-code-review` 无参数时默认使用 `HEAD~1..HEAD`。
4. `ody request-code-review --model unknown-model` 在模型无效时返回清晰错误。
5. `ody request-code-review --deep` 触发 subagent 路径。
