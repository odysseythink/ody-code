# Part 1 — Config Schema + Model Resolver

> Depends on: none. Phase A first deliverable: `config.toml` 解析/序列化支持三个新字段，并提供 fallback 链模型解析器。

## 文件列表

| 动作 | 文件 | 说明 |
|---|---|---|
| Modify | `packages/agent-core/src/config/schema.ts:315-370` | `KimiConfigSchema` / `KimiConfigPatchSchema` modeModels 扩展 |
| Modify | `packages/agent-core/src/config/toml.ts:535-544` | `modeModelsToToml` 增加 snake_case 回写 |
| Create | `packages/agent-core/src/code-review/model-resolver.ts` | `resolveCodeReviewModel` |
| Modify | `packages/agent-core/test/config/configs.test.ts` | Task 1 解析/round-trip 测试 |
| Create | `packages/agent-core/test/code-review/model-resolver.test.ts` | Task 2 fallback 链测试 |

---

## Task 1: config.toml `modeModels` 扩展并与 TOML 双向转换

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/config/schema.ts:315-370`
- Modify: `packages/agent-core/src/config/toml.ts:535-544`
- Modify: `packages/agent-core/test/config/configs.test.ts`

### 步骤

- [ ] **Write the failing test** — 在 `test/config/configs.test.ts` 末尾追加：

```ts
describe('modeModels code-review fields', () => {
  const TOML_WITH_CODE_REVIEW = `
default_model = "base"

[mode_models]
plan = "plan-model"
review = "generic-reviewer"
code_review = "deepseek-coder"
code_review_request = "claude-3-5-sonnet"
code_review_receive = "claude-3-5-sonnet"
`;

  it('parseConfigString parses new code_review fields as camelCase', () => {
    const config = parseConfigString(TOML_WITH_CODE_REVIEW, 'test.toml');
    expect(config.modeModels).toBeDefined();
    expect(config.modeModels!.plan).toBe('plan-model');
    expect(config.modeModels!.review).toBe('generic-reviewer');
    expect(config.modeModels!.codeReview).toBe('deepseek-coder');
    expect(config.modeModels!.codeReviewRequest).toBe('claude-3-5-sonnet');
    expect(config.modeModels!.codeReviewReceive).toBe('claude-3-5-sonnet');
  });

  it('configToTomlData round-trips code_review fields as snake_case', () => {
    const config = parseConfigString(TOML_WITH_CODE_REVIEW, 'test.toml');
    const data = configToTomlData(config);
    const modeModels = data['mode_models'] as Record<string, unknown>;
    expect(modeModels).toBeDefined();
    expect(modeModels['plan']).toBe('plan-model');
    expect(modeModels['review']).toBe('generic-reviewer');
    expect(modeModels['code_review']).toBe('deepseek-coder');
    expect(modeModels['code_review_request']).toBe('claude-3-5-sonnet');
    expect(modeModels['code_review_receive']).toBe('claude-3-5-sonnet');
  });

  it('writeConfigFile then readConfigFile round-trips all code_review fields', async () => {
    const dir = makeTempDir();
    const configPath = join(dir, 'config.toml');
    const config = parseConfigString(TOML_WITH_CODE_REVIEW, configPath);
    await writeConfigFile(configPath, config);
    const text = await readFile(configPath, 'utf-8');
    expect(text).toContain('code_review = "deepseek-coder"');
    expect(text).toContain('code_review_request = "claude-3-5-sonnet"');
    expect(text).toContain('code_review_receive = "claude-3-5-sonnet"');
    const roundTripped = parseConfigString(text, configPath);
    expect(roundTripped.modeModels!.codeReviewRequest).toBe('claude-3-5-sonnet');
    expect(roundTripped.modeModels!.codeReviewReceive).toBe('claude-3-5-sonnet');
  });
});
```

- [ ] **Run it and verify it FAILS:**

```bash
cd packages/agent-core && pnpm test
```

预期失败：`parseConfigString` 不会解析 `code_review` 等字段（schema 缺少定义），且 round-trip 的 TOML 中 key 为 `codeReview`（camelCase）而非 `code_review`（snake_case）。

- [ ] **Write the minimal implementation:**

**1. `packages/agent-core/src/config/schema.ts`** — `KimiConfigSchema` (line 315-319) 改为：

```ts
modeModels: z.object({
  plan: z.string().optional(),
  design: z.string().optional(),
  review: z.string().optional(),
  codeReview: z.string().optional(),
  codeReviewRequest: z.string().optional(),
  codeReviewReceive: z.string().optional(),
}).optional(),
```

`KimiConfigPatchSchema` (line 365-369) 同步增加相同字段：

```ts
modeModels: z.object({
  plan: z.string().optional(),
  design: z.string().optional(),
  review: z.string().optional(),
  codeReview: z.string().optional(),
  codeReviewRequest: z.string().optional(),
  codeReviewReceive: z.string().optional(),
}).optional(),
```

**2. `packages/agent-core/src/config/toml.ts`** — `modeModelsToToml` (line 535-543)：

```ts
function modeModelsToToml(
  modeModels: NonNullable<OdyConfig['modeModels']>,
  _raw: unknown,
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(modeModels)) {
    setDefined(out, camelToSnake(key), value);
  }
  return out;
}
```

注意：将 `setDefined(out, key, value)` 改为 `setDefined(out, camelToSnake(key), value)`。

- [ ] **Run it and verify it PASSES:**

```bash
cd packages/agent-core && pnpm test
```

两个新用例全部通过。已存在的 modeModels 测试仍通过（`plan`/`design`/`review` 是单次单词，camelToSnake 无变化）。

- [ ] **Commit.**

```bash
git add packages/agent-core/src/config/schema.ts packages/agent-core/src/config/toml.ts packages/agent-core/test/config/configs.test.ts
git commit -m "feat: add codeReview/codeReviewRequest/codeReviewReceive to modeModels and fix modeModelsToToml snake_case write-back"
```

---

## Task 2: 模型 fallback 解析器 `resolveCodeReviewModel`

**Depends on:** Task 1（用到了 config 中定义的 `modeModels` 类型）

**Files:**
- Create: `packages/agent-core/src/code-review/model-resolver.ts`
- Create: `packages/agent-core/test/code-review/model-resolver.test.ts`

### 步骤

- [ ] **Write the failing test** — 创建 `test/code-review/model-resolver.test.ts`：

```ts
import { describe, expect, it, vi } from 'vitest';
import { resolveCodeReviewModel } from '../../src/code-review/model-resolver';
import { OdyError, ErrorCodes } from '../../src/errors';

function alwaysValid(_alias: string): boolean {
  return true;
}

describe('resolveCodeReviewModel', () => {
  const defaultModel = 'default-model';

  it('request: explicit override wins over everything', () => {
    const result = resolveCodeReviewModel(
      'request',
      {
        codeReviewRequest: 'req-specific',
        codeReview: 'general',
        review: 'old-reviewer',
      },
      defaultModel,
      { explicit: 'cli-model' },
      alwaysValid,
    );
    expect(result).toBe('cli-model');
  });

  it('request: falls back to codeReviewRequest then codeReview then review then default', () => {
    // No explicit, no codeReviewRequest — should use codeReview
    const result1 = resolveCodeReviewModel(
      'request',
      { codeReview: 'general', review: 'old-reviewer' },
      defaultModel,
      {},
      alwaysValid,
    );
    expect(result1).toBe('general');

    // Only review present
    const result2 = resolveCodeReviewModel(
      'request',
      { review: 'old-reviewer' },
      defaultModel,
      {},
      alwaysValid,
    );
    expect(result2).toBe('old-reviewer');

    // Session model falls back after modeModels
    const result3 = resolveCodeReviewModel(
      'request',
      {},
      undefined,
      { sessionModel: 'session-model' },
      alwaysValid,
    );
    expect(result3).toBe('session-model');

    // Default is last resort
    const result4 = resolveCodeReviewModel(
      'request',
      {},
      'last-resort',
      {},
      alwaysValid,
    );
    expect(result4).toBe('last-resort');
  });

  it('receive: does not accept explicit override', () => {
    const result = resolveCodeReviewModel(
      'receive',
      {
        codeReviewReceive: 'receive-model',
        codeReview: 'general',
      },
      defaultModel,
      { explicit: 'should-be-ignored' },
      alwaysValid,
    );
    expect(result).toBe('receive-model');
  });

  it('receive: falls back codeReviewReceive → codeReview → review → sessionModel → default', () => {
    const result1 = resolveCodeReviewModel(
      'receive',
      { codeReview: 'general' },
      defaultModel,
      {},
      alwaysValid,
    );
    expect(result1).toBe('general');

    const result2 = resolveCodeReviewModel(
      'receive',
      { review: 'old-reviewer', codeReviewReceive: 'rcv' },
      defaultModel,
      { sessionModel: 'sess' },
      alwaysValid,
    );
    expect(result2).toBe('rcv');

    const result3 = resolveCodeReviewModel(
      'receive',
      {},
      undefined,
      { sessionModel: 'sess' },
      alwaysValid,
    );
    expect(result3).toBe('sess');
  });

  it('skips invalid aliases and continues the chain', () => {
    let callCount = 0;
    const validate = (alias: string): boolean => {
      callCount += 1;
      return alias !== 'bad-alias';
    };
    const result = resolveCodeReviewModel(
      'request',
      { codeReviewRequest: 'bad-alias', codeReview: 'good-alias' },
      defaultModel,
      {},
      validate,
    );
    expect(result).toBe('good-alias');
    expect(callCount).toBeGreaterThanOrEqual(2);
  });

  it('throws ConfigInvalidError when all candidates are exhausted', () => {
    expect(() =>
      resolveCodeReviewModel(
        'request',
        {},
        undefined,
        {},
        () => false,
      ),
    ).toThrow(OdyError);
    try {
      resolveCodeReviewModel('request', {}, undefined, {}, () => false);
    } catch (error) {
      expect(error).toBeInstanceOf(OdyError);
      expect((error as OdyError).code).toBe(ErrorCodes.CONFIG_INVALID);
    }
  });
});
```

- [ ] **Run it and verify it FAILS:**

```bash
cd packages/agent-core && pnpm test -- --reporter=verbose test/code-review/model-resolver.test.ts
```

预期失败：文件不存在（`../../src/code-review/model-resolver`）。

- [ ] **Write the minimal implementation** — 创建 `src/code-review/model-resolver.ts`：

```ts
import { ErrorCodes, OdyError } from '#/errors';
import type { OdyConfig } from '#/config';

export interface ResolveModelOverrides {
  readonly explicit?: string | undefined;
  readonly sessionModel?: string | undefined;
}

export function resolveCodeReviewModel(
  kind: 'request' | 'receive',
  modeModels: OdyConfig['modeModels'],
  defaultModel: string | undefined,
  overrides: ResolveModelOverrides,
  validate: (alias: string) => boolean,
): string {
  const candidates: (string | undefined)[] = [];

  if (kind === 'request' && overrides.explicit !== undefined) {
    candidates.push(overrides.explicit);
  }

  if (kind === 'request') {
    candidates.push(modeModels?.codeReviewRequest);
  } else {
    candidates.push(modeModels?.codeReviewReceive);
  }

  candidates.push(modeModels?.codeReview);
  candidates.push(modeModels?.review);

  if (overrides.sessionModel !== undefined) {
    candidates.push(overrides.sessionModel);
  }

  candidates.push(defaultModel);

  for (const alias of candidates) {
    if (isNonEmptyString(alias) && validate(alias)) {
      return alias;
    }
  }

  throw new OdyError(
    ErrorCodes.CONFIG_INVALID,
    'No usable model for code review. Configure [mode_models] with code_review, code_review_request, or code_review_receive, or set default_model.',
  );
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === 'string' && value.trim().length > 0;
}
```

- [ ] **Run it and verify it PASSES:**

```bash
cd packages/agent-core && pnpm test -- --reporter=verbose test/code-review/model-resolver.test.ts
```

全部 5 个用例通过。

- [ ] **Run full agent-core test suite to ensure no regression:**

```bash
cd packages/agent-core && pnpm test
```

- [ ] **Commit.**

```bash
git add packages/agent-core/src/code-review/model-resolver.ts packages/agent-core/test/code-review/model-resolver.test.ts
git commit -m "feat: add resolveCodeReviewModel with fallback chain for code review model selection"
```

---

## 本地 Self-Review

- [ ] 1. **Spec-coverage**: 本 Part 覆盖设计中的 `modeModels` schema 扩展（字段定义 + round-trip）和模型 fallback 算法（包括显式参数优先级、receive 不接收 explicit、跳过无效 candidate、全部失败抛错）。✅
- [ ] 2. **Placeholder scan**: 无 TODO/TBD，所有步骤含完整代码。✅
- [ ] 3. **No phantom tasks**: Task 1 经编译+测试验证 schema 变更与 TOML 写回；Task 2 新建模块+测试。无空 commit。✅
- [ ] 4. **Dependency soundness**: Task 2 依赖 Task 1 定义的 `OdyConfig.modeModels` 类型，Task 1 先完成。无循环/前置引用。✅
- [ ] 5. **Caller & build soundness**: `modeModelsToToml` 签名未变（只是内部实现改为 `camelToSnake(key)`），所有调用者不受影响。`KimiConfigPatchSchema` 超集新增字段，patch 合并逻辑不变。Task 1 结尾跑 `pnpm test` 覆盖全 agent-core 测试。✅
- [ ] 6. **Test-the-risk**: fallback 链顺序（显式 > codeReviewRequest > codeReview > review > sessionModel > default）含多个梯度断言；receive 不接收 explicit；invalid 跳过 + 全排尽抛错。所有边界路径覆盖。✅
- [ ] 7. **Type consistency**: `resolveCodeReviewModel` 的 `modeModels` 参数类型 `OdyConfig['modeModels']` 与 Task 1 新增的 schema 字段（`codeReviewRequest` 等）完全对应；`validate` 回调类型 `(alias: string) => boolean` 与后续 Task 4 core-impl 中的 `ProviderManager.resolveProviderConfig` 可自然对接。✅
