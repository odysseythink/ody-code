# Code Review 命令 — 配置层

## 设计目标

让 code-review 相关命令可以使用与默认会话不同的模型，同时保持向后兼容现有 `modeModels.review` 字段 [C:USER]。

## 数据模型

### `KimiConfig.modeModels` 扩展 [C:USER]

```typescript
export const KimiConfigSchema = z.object({
  // ... existing fields ...
  modeModels: z.object({
    plan: z.string().optional(),
    design: z.string().optional(),
    review: z.string().optional(),        // [C:UPSTREAM] 保留既有字段
    codeReview: z.string().optional(),    // [C:USER] code-review 通用模型
    codeReviewRequest: z.string().optional(),  // [C:USER] request 专用模型
    codeReviewReceive: z.string().optional(),  // [C:USER] receive 专用模型
  }).optional(),
  // ...
});
```

`KimiConfigPatchSchema` 中同步增加相同字段 [C:INFERRED]。

### TOML 映射

在 `config.toml` 中体现为：

```toml
[mode_models]
review = "kimi-v1"              # 现有字段，继续生效
code_review = "deepseek-coder"  # code-review 通用模型
code_review_request = "claude-3-5-sonnet"  # request 专用
code_review_receive = "claude-3-5-sonnet"  # receive 专用
```

`transformTomlData` 与 `configToTomlData` 中 `modeModels` 段保持 `transformPlainObject` / `modeModelsToToml` 处理，新增的 camelCase 键会自动 snake_case 化 [C:INFERRED]。

## 模型解析与 Fallback 算法

### 接口

```typescript
interface CodeReviewModelResolver {
  /**
   * 按优先级解析出可用的模型别名。
   * 如果解析出的别名无效，会抛出 ConfigInvalidError。
   */
  resolve(kind: 'request' | 'receive', overrides?: { explicit?: string; sessionModel?: string }): string;
}
```

### Fallback 链 [C:USER]

对于 `request`：

```
1. --model <model> 显式参数（CLI / slash 命令传入）
2. modeModels.codeReviewRequest
3. modeModels.codeReview
4. modeModels.review
5. overrides.sessionModel（TUI 当前会话模型，仅 TUI 调用时存在）
6. kimiConfig.defaultModel
```

对于 `receive`：

```
1. modeModels.codeReviewReceive
2. modeModels.codeReview
3. modeModels.review
4. overrides.sessionModel（TUI 当前会话模型）
5. kimiConfig.defaultModel
```

> receive 不接受 `--model` 参数，因为 slash 命令本身没有参数设计 [C:USER]。

### 解析伪代码

```
function resolveCodeReviewModel(
  kind: 'request' | 'receive',
  modeModels: ModeModels | undefined,
  defaultModel: string | undefined,
  overrides: { explicit?: string; sessionModel?: string }
): string {
  const candidates: (string | undefined)[] = []

  if (kind === 'request' && overrides.explicit !== undefined) {
    candidates.push(overrides.explicit)
  }

  if (kind === 'request') {
    candidates.push(modeModels?.codeReviewRequest)
  } else {
    candidates.push(modeModels?.codeReviewReceive)
  }

  candidates.push(modeModels?.codeReview)
  candidates.push(modeModels?.review)

  if (overrides.sessionModel !== undefined) {
    candidates.push(overrides.sessionModel)
  }

  candidates.push(defaultModel)

  for (const alias of candidates) {
    if (isNonEmptyString(alias)) {
      validateModelAlias(alias)  // 通过 modelProvider.resolveProviderConfig 验证存在性
      return alias
    }
  }

  throw ConfigInvalidError('No usable model for code review. Configure [mode_models] or default_model.')
}
```

### 验证策略

- 解析到候选别名后，调用 `modelProvider.resolveProviderConfig(alias)` 验证该别名是否已在 `[models]` 中配置 [C:INFERRED]。
- 若验证失败，继续尝试下一个候选；若全部失败，抛出错误。

## 变更位置

| 文件 | 变更内容 |
|---|---|
| `packages/agent-core/src/config/schema.ts` | `KimiConfigSchema` 与 `KimiConfigPatchSchema` 的 `modeModels` 对象新增三个可选字段 [C:USER] |
| `packages/agent-core/src/config/toml.ts` | 无需额外变更，`modeModels` 已走 `transformPlainObject` / `modeModelsToToml`，新增 camelCase 字段自动序列化为 snake_case [C:INFERRED] |
| `packages/agent-core/src/config/index.ts`（或新建 `packages/agent-core/src/code-review/model-resolver.ts`） | 新增 `resolveCodeReviewModel` 与 `CodeReviewModelResolver` [C:INFERRED] |

## 本地错误/降级

| 错误类 | 立即处理 | 降级路径 | 恢复条件 |
|---|---|---|---|
| `codeReviewRequest` 专用模型无效 | 尝试 `codeReview` | 继续 fallback 链 | 任一候选模型有效 |
| `codeReviewReceive` 专用模型无效 | 尝试 `codeReview` | 继续 fallback 链 | 任一候选模型有效 |
| 全部候选无效 | 抛出 `ConfigInvalidError` | 无 | 用户在 config.toml 中配置有效模型 |

## 本地测试断言

1. `parseConfigString` 能正确解析包含 `code_review_request` 与 `code_review_receive` 的配置。
2. `configToTomlData` 写回磁盘后，新增字段以 snake_case 保留。
3. `resolveCodeReviewModel('request', { codeReviewRequest: 'a', codeReview: 'b' }, 'default', { explicit: 'c' })` 返回 `'c'`。
4. `resolveCodeReviewModel('request', { codeReview: 'b' }, 'default', {})` 返回 `'b'`。
5. `resolveCodeReviewModel('receive', {}, undefined, {})` 抛出 `ConfigInvalidError`。
