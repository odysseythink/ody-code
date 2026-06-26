# Code Review 命令化与可配置模型

## Scope

### In Scope

- 将内置的 `requesting-code-review` 与 `receiving-code-review` skill 暴露为可交互命令 [C:USER]
- 新增 CLI 子命令 `ody request-code-review` [C:USER]
- 新增 TUI slash 命令 `/request-code-review` 与 `/receive-code-review` [C:USER]
- 在 `config.toml` 的 `[mode_models]` 段增加 code-review 专用模型配置，并支持 fallback 链 [C:USER]
- `/request-code-review` 支持 `--base`/`--head` 与 `--pr` 两种 diff 来源 [C:USER]
- `/request-code-review` 默认直接 LLM 生成审查报告，可通过参数触发 subagent 深度审查 [C:USER]
- `/receive-code-review` 在当前 TUI 会话中临时切换模型并注入 `receiving-code-review` skill prompt，退出后恢复模型 [C:USER]

### Out of Scope

- `receiving-code-review` 不暴露 CLI 子命令：该命令依赖当前会话注入 skill，CLI 无会话难以实现 [C:USER]
- 不支持 GitLab / Bitbucket 等其它托管平台：PR diff 拉取仅通过 `gh` CLI 支持 GitHub [C:USER]
- 不新增独立的 `review` session mode：采用轻量 slash command 注入方式，避免改动 `SessionModeKind` 核心 [C:USER]
- 不增加专用 telemetry 事件：复用现有 LLM 调用与命令调用的通用埋点 [C:USER]
- 不作为实验性功能开关发布：直接作为正式命令上线 [C:USER]

## Prior Art

本次变更属于对现有内置 skill 的 CLI/TUI 封装，并扩展现有 `modeModels` 配置机制，无独立上游系统需要完整盘点。参考了现有 `/design-review` 与 `/plan-review` 的第二模型审查实现作为实现模板 [C:UPSTREAM]。

## Architecture

```
User
  ├─ CLI: ody request-code-review [options]
  │     └─ apps/ody-code/src/cli/sub/request-code-review.ts
  │           └─ 调用 agent-core 代码审查执行器
  │                 ├─ 从 config.modeModels 解析 review 专用模型
  │                 ├─ 获取 diff（git / gh pr）
  │                 ├─ 直接 LLM 生成报告
  │                 └─ 可选：派发 subagent 深度审查
  │
  └─ TUI
        ├─ /request-code-review [options]
        │     └─ apps/ody-code/src/tui/commands/request-code-review.ts
        │           └─ 复用同一执行器，结果以会话消息形式呈现
        │
        └─ /receive-code-review
              └─ apps/ody-code/src/tui/commands/receive-code-review.ts
                    ├─ 保存当前 modelAlias
                    ├─ 切换到 codeReviewReceive 模型
                    ├─ 注入 receiving-code-review skill prompt
                    └─ 用户下次普通输入前恢复原模型

Config
  └─ packages/agent-core/src/config/schema.ts
        └─ modeModels 新增 codeReview / codeReviewRequest / codeReviewReceive
```

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | [2026-06-17-code-review-commands/config.md](./2026-06-17-code-review-commands/config.md) | `config.toml` schema 扩展与模型 fallback 算法 | done |
| 2 | [2026-06-17-code-review-commands/core.md](./2026-06-17-code-review-commands/core.md) | 代码审查执行器、diff 获取、LLM prompt、subagent 派发 | done |
| 3 | [2026-06-17-code-review-commands/cli.md](./2026-06-17-code-review-commands/cli.md) | `ody request-code-review` CLI 子命令 | done |
| 4 | [2026-06-17-code-review-commands/tui.md](./2026-06-17-code-review-commands/tui.md) | `/request-code-review` 与 `/receive-code-review` slash 命令 | done |

## Data Models

跨模块新增/复用的核心数据结构：

```typescript
// config 层
interface ModeModels {
  plan?: string;
  design?: string;
  review?: string;
  codeReview?: string;
  codeReviewRequest?: string;
  codeReviewReceive?: string;
}

// core 层
interface CodeReviewRequestInput {
  source: { kind: 'commits'; base: string; head: string }
        | { kind: 'pr'; prUrlOrNumber: string }
        | { kind: 'working-tree' };
  modelAlias: string;
  description?: string;
  requirements?: string;
  deep?: boolean;
  timeoutMs?: number;
}

interface CodeReviewReport {
  ok: boolean;
  reviewerAlias: string;
  summary?: string;
  findings: CodeReviewFinding[];
  note?: string;
}

interface CodeReviewFinding {
  severity: 'critical' | 'important' | 'minor';
  title: string;
  detail: string;
  location?: string;
  suggestedFix?: string;
}

// TUI 层
interface ReceiveCodeReviewState {
  originalModelAlias: string;
  reviewModelAlias: string;
  active: boolean;
}
```

详见各 part 文件。

## Algorithms

### 模型解析 Fallback 链

```
resolveCodeReviewModel(kind, modeModels, defaultModel, overrides):
  candidates = []
  if kind == 'request' and overrides.explicit:
    candidates.push(overrides.explicit)
  if kind == 'request':
    candidates.push(modeModels.codeReviewRequest)
  else:
    candidates.push(modeModels.codeReviewReceive)
  candidates.push(modeModels.codeReview)
  candidates.push(modeModels.review)
  if overrides.sessionModel:
    candidates.push(overrides.sessionModel)
  candidates.push(defaultModel)

  for alias in candidates:
    if alias is valid and resolves:
      return alias
  throw ConfigInvalidError
```

### Diff 获取

```
fetchDiff(source, cwd):
  if source.kind == 'commits':
    return exec('git diff', source.base, source.head)
  if source.kind == 'working-tree':
    return exec('git diff --cached') + exec('git diff')
  if source.kind == 'pr':
    return exec('gh pr diff', parsePrNumber(source.prUrlOrNumber))
```

### `/receive-code-review` 模型切换与恢复

```
handleReceiveCodeReviewCommand(host):
  reviewModel = resolveCodeReviewModel('receive', ...)
  save originalModelAlias
  session.setModel(reviewModel)
  session.activateSkill('receiving-code-review')
  set active = true

maybeRestoreModelAfterReceiveReview(host):
  if active:
    session.setModel(originalModelAlias)
    active = false
```

各算法详细伪代码、边界条件与输入/输出见对应 part 文件。

## Error Handling

跨模块统一错误处理策略：

| 错误类 | 发生位置 | 立即处理 | 降级路径 |
|---|---|---|---|
| 参数冲突 | CLI/slash parser | 抛出 `OptionConflictError` / `host.showError` | 用户修正参数 |
| 模型解析失败 | `resolveCodeReviewModel` | 继续 fallback 链；全部失败则抛出 | 配置有效模型 |
| diff 获取失败 | `fetchDiff` | 返回 `ok=false` 并附 note | 使用 `--base/--head` 或检查 `gh` 登录 |
| diff 超过 token 上限 | `review` | 返回 `ok=false` 并提示 | 缩小 diff 范围 |
| LLM 调用失败 | `generate` | 返回 `ok=false` | 模型服务恢复后重试 |
| subagent 超时 | `runReviewerSubagent` | 返回 `ok=false` | 不使用 `--deep` 重试 |
| 无活跃会话 | TUI slash handler | `host.showError(NO_ACTIVE_SESSION_MESSAGE)` | 创建/恢复会话 |
| 模型切换失败 | `/receive-code-review` | `host.showError` | 保持原模型并仍注入 skill |

各子系统详细错误表见对应 part 文件。

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | `modeModels` 的现有字段 `review` 可以被保留作为向后兼容，同时新增 `codeReview` 等字段 [C:INFERRED] | Medium | 如果强行替换会破坏现有用户的 `review` 配置 | 检查现有测试与文档对 `modeModels.review` 的依赖 |
| 2 | `Session.setModel()` 在 TUI 中的切换可以立即生效，不会与正在进行的流冲突 [C:INFERRED] | Medium | 在流中切换模型可能导致状态不一致 | 检查 TUI 命令在 idle-only 还是 always 可用 |
| 3 | `gh pr diff` 与 `gh pr view` 的输出足以构造完整的代码审查上下文 [C:INFERRED] | Medium | 可能需要 GitHub API 补充评论/标签信息 | 实际运行 `gh` 命令验证 |
| 4 | 直接 LLM 单轮生成的审查报告已能满足主要场景，subagent 仅作为可选增强 [C:USER] | High | 设计过于复杂或过于简单 | 已在澄清阶段确认 |

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | 新增 `modeModels` 字段与现有 `review` 字段语义冲突 | Medium | Medium | 保留 `review` 作为通用 fallback，新增字段作为更细粒度配置；文档明确优先级 |
| 2 | `gh` CLI 未安装或未登录导致 `--pr` 失败 | High | Low | 清晰错误提示，并建议使用 `--base`/`--head` 手动提供 diff |
| 3 | 大 PR diff 超过模型上下文窗口 | Medium | High | 在 diff 获取后做 token 估算，超过阈值时提示用户并提供截断/分段策略 |
| 4 | `/receive-code-review` 模型切换后未正确恢复 | Low | Medium | 在状态管理中保存原模型，并在发送下一条非 skill 消息前恢复；增加恢复失败日志 |
| 5 | CLI 与 TUI 复用同一执行器时输出格式不一致 | Medium | Low | 执行器返回结构化数据，CLI 负责渲染为 markdown，TUI 负责渲染为会话消息 |

## Self-Review

- **Security**：检查了 `--pr` 参数解析逻辑，用 `node -e` 验证仅接受纯数字或 `github.com/<owner>/<repo>/pull/<number>` 格式，拒绝 `owner/repo/pull/789` 等不完整输入；API key 由现有 `ProviderManager`/`ProviderConfig` 机制解析，不进入代码审查命令日志；diff 内容仅在 LLM 调用/输出中流转，不持久化到磁盘。未发现新增 secret 泄漏点。
- **Test**：每个 part 都包含 must-pass 与 must-reject 断言；模型 fallback 链与 PR 解析逻辑已通过 `node -e` 验证；未发现断言与实现逻辑矛盾。
- **Ops**：大 diff 场景在 `core.md` 中做 token 估算并返回降级提示；`/receive-code-review` 通过 `AppState` 保存原模型并在下一条消息前恢复；subagent 深度审查支持 `--timeout`。未发现并发或重复调用问题。
- **Integration**：已验证 `modeModels` 字段存在于 `packages/agent-core/src/config/schema.ts`；`Session.setModel` 与 `Session.activateSkill` 存在于 `packages/node-sdk/src/session.ts`；`BUILTIN_SLASH_COMMANDS` 与 `handleBuiltInSlashCommand` 存在于 `apps/ody-code/src/tui/commands/`；Commander 子命令注册机制存在于 `apps/ody-code/src/cli/commands.ts`。所有设计依赖的数据源/钩子点均存在。
- **Scope**：本设计仍是“将两个 code-review skill 命令化并支持可配置模型”这一单一功能，拆分为 config/core/cli/tui 四个 part 仅因涉及多个代码层级，未裂变为多个独立产品。

## User Final Approval

- [x] 设计已获用户批准（2026-06-17）
