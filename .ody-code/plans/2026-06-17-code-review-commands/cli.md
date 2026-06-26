# Part 4 — `ody request-code-review` CLI 子命令

> Depends on: Part 3 Task 6（`KimiHarness.requestCodeReview()` 可用）。

## 文件列表

| 动作 | 文件 | 说明 |
|---|---|---|
| Create | `apps/ody-code/src/cli/sub/request-code-review.ts` | 命令定义、参数校验、执行入口 |
| Modify | `apps/ody-code/src/cli/commands.ts:93-94` | 注册子命令 |
| Create | `apps/ody-code/test/cli/request-code-review.test.ts` | CLI 测试（mock harness） |

---

## Task 7: `ody request-code-review` CLI 子命令

**Depends on:** Part 3 Task 6

**Files:**
- Create: `apps/ody-code/src/cli/sub/request-code-review.ts`
- Modify: `apps/ody-code/src/cli/commands.ts:93-94`
- Create: `apps/ody-code/test/cli/request-code-review.test.ts`

### 步骤

- [ ] **Write failing test** — 创建 `test/cli/request-code-review.test.ts`：

```ts
import { describe, expect, it, vi } from 'vitest';
import { OptionConflictError } from '../../../src/cli/options';
import { buildDiffSource, validateRequestCodeReviewOptions } from '../../../src/cli/sub/request-code-review';

describe('validateRequestCodeReviewOptions', () => {
  it('throws OptionConflictError when --pr is combined with --base', () => {
    expect(() =>
      validateRequestCodeReviewOptions({ pr: '42', base: 'HEAD~1' }),
    ).toThrow(OptionConflictError);
  });

  it('throws OptionConflictError when --timeout is not a positive integer', () => {
    expect(() =>
      validateRequestCodeReviewOptions({ timeout: 0 }),
    ).toThrow('--timeout must be a positive integer (seconds)');

    expect(() =>
      validateRequestCodeReviewOptions({ timeout: -1 }),
    ).toThrow('--timeout must be a positive integer (seconds)');
  });

  it('defaults base to HEAD~1 and head to HEAD when neither flag nor --pr given', () => {
    const opts = { base: undefined, head: undefined, pr: undefined };
    validateRequestCodeReviewOptions(opts);
    expect(opts.base).toBe('HEAD~1');
    expect(opts.head).toBe('HEAD');
  });

  it('defaults head to HEAD when only base is given', () => {
    const opts = { base: 'main', head: undefined };
    validateRequestCodeReviewOptions(opts);
    expect(opts.base).toBe('main');
    expect(opts.head).toBe('HEAD');
  });

  it('accepts --pr alone without conflict', () => {
    expect(() =>
      validateRequestCodeReviewOptions({ pr: 'https://github.com/a/b/pull/1' }),
    ).not.toThrow();
  });

  it('accepts --base and --head together', () => {
    expect(() =>
      validateRequestCodeReviewOptions({ base: 'main', head: 'feature' }),
    ).not.toThrow();
  });

  it('accepts valid positive integer timeout', () => {
    expect(() =>
      validateRequestCodeReviewOptions({ timeout: 120 }),
    ).not.toThrow();
  });
});

describe('buildDiffSource (CLI)', () => {
  it('builds working-tree when no flags', () => {
    expect(buildDiffSource({})).toEqual({ kind: 'working-tree' });
  });

  it('builds pr source', () => {
    expect(buildDiffSource({ pr: '1' })).toEqual({ kind: 'pr', prUrlOrNumber: '1' });
  });

  it('builds commits source', () => {
    expect(buildDiffSource({ base: 'HEAD~3', head: 'HEAD' })).toEqual({
      kind: 'commits',
      base: 'HEAD~3',
      head: 'HEAD',
    });
  });
});
```

- [ ] **Verify FAILS:**

```bash
cd apps/ody-code && pnpm test -- --reporter=verbose test/cli/request-code-review.test.ts
```

- [ ] **Write implementation:**

**`src/cli/sub/request-code-review.ts`：**

```ts
import {
  KimiHarness,
  renderCodeReviewReportToMarkdown,
} from '@odysseythink/ody-code-sdk';
import type { Command } from 'commander';
import type { Writable } from 'node:stream';

import { OptionConflictError } from '#/cli/options';
import { createKimiCodeHostIdentity } from '#/cli/version';

interface RequestCodeReviewCLIOptions {
  base?: string | undefined;
  head?: string | undefined;
  pr?: string | undefined;
  model?: string | undefined;
  description?: string | undefined;
  requirements?: string | undefined;
  deep?: boolean | undefined;
  timeout?: number | undefined;
}

export interface RequestCodeReviewDeps {
  readonly getHarness: () => KimiHarness;
  readonly stdout: Pick<Writable, 'write'>;
  readonly stderr: Pick<Writable, 'write'>;
  readonly exit: (code: number) => never;
}

export function validateRequestCodeReviewOptions(
  opts: RequestCodeReviewCLIOptions,
): void {
  if (opts.pr !== undefined && (opts.base !== undefined || opts.head !== undefined)) {
    throw new OptionConflictError('Cannot combine --pr with --base/--head.');
  }

  if (opts.pr === undefined && opts.base === undefined && opts.head === undefined) {
    opts.base = 'HEAD~1';
    opts.head = 'HEAD';
  }

  if (opts.base !== undefined && opts.head === undefined) {
    opts.head = 'HEAD';
  }

  if (opts.timeout !== undefined) {
    if (!Number.isInteger(opts.timeout) || opts.timeout <= 0) {
      throw new OptionConflictError('--timeout must be a positive integer (seconds).');
    }
  }
}

export function buildDiffSource(opts: {
  base?: string | undefined;
  head?: string | undefined;
  pr?: string | undefined;
}) {
  if (opts.pr !== undefined) {
    return { kind: 'pr' as const, prUrlOrNumber: opts.pr };
  }
  if (opts.base !== undefined || opts.head !== undefined) {
    return {
      kind: 'commits' as const,
      base: opts.base ?? 'HEAD~1',
      head: opts.head ?? 'HEAD',
    };
  }
  return { kind: 'working-tree' as const };
}

export async function handleRequestCodeReview(
  opts: RequestCodeReviewCLIOptions,
  deps: RequestCodeReviewDeps,
): Promise<void> {
  validateRequestCodeReviewOptions(opts);

  const harness = deps.getHarness();
  await harness.ensureConfigFile();

  const source = buildDiffSource(opts);

  try {
    const report = await harness.requestCodeReview({
      source,
      modelAlias: opts.model,
      description: opts.description,
      requirements: opts.requirements,
      deep: opts.deep,
      timeoutMs: opts.timeout !== undefined ? opts.timeout * 1000 : undefined,
    });

    if (!report.ok) {
      deps.stderr.write(`${report.note ?? 'Code review failed.'}\n`);
      deps.exit(1);
    }

    deps.stdout.write(`${renderCodeReviewReportToMarkdown(report)}\n`);
  } catch (error) {
    deps.stderr.write(
      `Code review failed: ${error instanceof Error ? error.message : String(error)}\n`,
    );
    deps.exit(1);
  }
}

export function registerRequestCodeReviewCommand(parent: Command): void {
  parent
    .command('request-code-review')
    .description('Request a code review for the current changes.')
    .option('--base <sha>', 'Base commit for the review range.')
    .option('--head <sha>', 'Head commit for the review range. Defaults to HEAD.')
    .option('--pr <url-or-number>', 'Review a GitHub PR (URL or number).')
    .option('-m, --model <model>', 'Model alias to use for this review.')
    .option('-d, --description <text>', 'Short description of what was built.')
    .option('-r, --requirements <text>', 'Requirements or plan the changes should meet.')
    .option('--deep', 'Dispatch a reviewer subagent for deeper analysis.', false)
    .option('-t, --timeout <seconds>', 'Timeout for the review in seconds.', parseInt)
    .action(async (opts: RequestCodeReviewCLIOptions) => {
      const identity = createKimiCodeHostIdentity();
      const harness = new KimiHarness({ identity });
      await handleRequestCodeReview(opts, {
        getHarness: () => harness,
        stdout: process.stdout,
        stderr: process.stderr,
        exit: (code: number) => process.exit(code),
      });
    });
}
```

- [ ] **在 `commands.ts` 中注册** — 修改 `apps/ody-code/src/cli/commands.ts`：

在 import 区新增：

```ts
import { registerRequestCodeReviewCommand } from './sub/request-code-review';
```

在 `registerExportCommand(program); registerProviderCommand(program);` 之后新增：

```ts
registerRequestCodeReviewCommand(program);
```

- [ ] **Run test + build:**

```bash
cd apps/ody-code && pnpm test -- --reporter=verbose test/cli/request-code-review.test.ts
```

或全量：

```bash
cd apps/ody-code && pnpm test
```

- [ ] **手动验证** — 在任意 git 仓库中运行：

```bash
node apps/ody-code/dist/cli.mjs request-code-review --base HEAD~1 --head HEAD
```

预期：输出 markdown 审查报告到 stdout，exit code 0（需已配置 provider 和 default_model）。无参数默认使用 `HEAD~1..HEAD`。

- [ ] **Commit.**

```bash
git add apps/ody-code/src/cli/sub/request-code-review.ts apps/ody-code/src/cli/commands.ts apps/ody-code/test/cli/request-code-review.test.ts
git commit -m "feat: add \"ody request-code-review\" CLI subcommand"
```

---

## 本地 Self-Review

- [ ] 1. **Spec-coverage**: 本 Part 覆盖 CLI `ody request-code-review` 命令注册、参数解析（`--base`/`--head`/`--pr`/`--model`/`--description`/`--requirements`/`--deep`/`--timeout`）、参数冲突校验（`--pr` 与 `--base` 互斥、timeout 正整）、默认行为（无参数时默认 `HEAD~1..HEAD`）。✅
- [ ] 2. **Placeholder scan**: 无 TODO/TBD。✅
- [ ] 3. **No phantom tasks**: 产出可测试的子命令模块（参数校验+diff 构建的单元测试）+ 可编译的子命令注册。✅
- [ ] 4. **Dependency soundness**: 依赖 Part 3 的 `KimiHarness.requestCodeReview()` 和 `renderCodeReviewReportToMarkdown` 导出。✅
- [ ] 5. **Caller & build soundness**: 仅修改 `commands.ts` 添加一行 `registerRequestCodeReviewCommand(program)`，不影响现有命令。`apps/ody-code` 测试全量通过。✅
- [ ] 6. **Test-the-risk**: 参数冲突所有组合有断言（`--pr`+`--base`、timeout 非正整数、timeout 零值）；默认行为（空参数→`HEAD~1..HEAD`、仅 base→head 默认 HEAD）。全部路径覆盖。✅
- [ ] 7. **Type consistency**: CLI opts 类型 `RequestCodeReviewCLIOptions` 与 `buildDiffSource` 参数对齐；调用 `harness.requestCodeReview({ source, ... })` 与 Part 3 SDK 签名一致。✅
