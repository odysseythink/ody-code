# 内置 idea-generator / idea-evaluator skills 与 SaveIdeaReport 工具设计

> 审计级别：Deep（需确认每个小节核心论断 + 全部 [C:INFERRED] 假设）

## Scope In/Out

### In Scope [C:USER]

1. 将 `/Users/ranwei/Downloads/skills/idea-generator.skill` 与 `idea-evaluator.skill` 中的 `SKILL.md` 内容作为内置 skill 加入 `packages/agent-core`，仅保留正常 Markdown 内容，去除 zip 包装。
2. 两个 skill 仅在 **normal** 会话模式下对模型可见；在 `plan` / `design` / `office-hours` / `game-design` 模式下隐藏。
3. 新增 `SaveIdeaReport` 内置工具，供 idea skill 在生成/评估完成后将报告持久化到 `.ody-code/ideas/`。
4. 工具写入 `.ody-code/ideas/` 时自动审批，无需用户逐次确认。
5. 文件名由工具根据报告标题自动生成：`YYYY-MM-DD-<slug>.md`，重名时加 `-1`、`-2` 后缀。
6. 工具输入包含 `title`、`content`、`type`（`'generator' | 'evaluator'`），以及可选的 `score` 和 `tags`；在文件正文前插入元数据头。
7. 保存失败或未调用时，报告内容仍保留在对话中，不阻塞用户。
8. `.ody-code/ideas/` 目录随首次保存自动创建，并确保 `.ody-code/` 被加入 `.gitignore`。

### Out of Scope [C:USER]

1. **不维护集中索引文件**：不生成/更新 `INDEX.md` 或数据库；仅保存单份 Markdown。
2. **不添加实验性 flag**：直接发布，不通过 `ODY_CODE_EXPERIMENTAL_*` 控制。
3. **不自动触发 idea skill**：仍由用户通过 `/idea-generator` 或 `/idea-evaluator` 显式调用，或由模型在 normal 模式下自行判断。
4. **不做跨会话的 idea 状态机**：不跟踪某一轮生成后必须评估、不强制工作流。
5. **不改动上游 skill 的核心方法论**：7 recipes、10 问评分、排雷等全部保留。

## Prior Art

本任务是内部功能移植，非引入新的开源方案； prior art 为上游 `.skill` 包中的 `SKILL.md` 以及本仓库现有的内置 skill / 内置工具 / session-mode 目录管理机制。

## Architecture

```text
User/Model ──activate──> idea-generator / idea-evaluator skill
                              │
                              ▼
                  SkillManager.activate / SkillTool.execution
                  (inject skill content as system reminder)
                              │
                              ▼
                         LLM follows skill
                         produces report
                              │
                              ▼
                  LLM calls SaveIdeaReport({ title, content, type, score?, tags? })
                              │
                              ▼
              SaveIdeaReportTool ──> 1. verify idea skill active in context
                                     2. ensure .ody-code/ideas/ exists
                                     3. generate unique filename
                                     4. write file via Kaos
                              │
                              ▼
                   PermissionPolicy: auto-approve writes under .ody-code/ideas/
```

### Components

- `IdeaGeneratorSkill` / `IdeaEvaluatorSkill`: builtin skill 定义，frontmatter 中声明 `hiddenInModes`。
- `SaveIdeaReportTool`: 内置工具，负责文件名生成与文件写入。
- `IdeaToolDirectoryPermissionPolicy`（或复用/扩展现有策略）: 对 `.ody-code/ideas/` 下的写操作自动审批。
- `SkillRegistry.registerBuiltinSkill`: 注册两个 skill。
- `ToolManager.initializeBuiltinTools`: 注册 `SaveIdeaReportTool`。
- Session-mode 风格的目录创建/`.gitignore` 逻辑复用或抽离。

## Reuse Analysis

| # | File / Module | What it solves | Reuse decision |
|---|---------------|----------------|----------------|
| 1 | `packages/agent-core/src/skill/builtin/index.ts` | 内置 skill 注册入口 | 直接使用：新增 `IDEA_GENERATOR_SKILL` / `IDEA_EVALUATOR_SKILL` 的 import 与 `registerBuiltinSkill` 调用 |
| 2 | `packages/agent-core/src/skill/builtin/*.ts` + `*.md` | 内置 skill 的 `.ts` 包装 + Markdown 内容 | 照抄模式：为每个 idea skill 创建 `.ts` 与 `.md`，用 `parseSkillText` 解析 |
| 3 | `packages/agent-core/src/skill/parser.ts:parseSkillText` | 解析 skill frontmatter / body | 直接使用 |
| 4 | `packages/agent-core/src/skill/types.ts:hiddenInModes` | 控制 skill 在指定 session mode 下隐藏 | 直接使用，frontmatter 中声明 `hiddenInModes: [plan, design, office-hours, game-design]` |
| 5 | `packages/agent-core/src/agent/tool/index.ts:initializeBuiltinTools` | 内置工具实例化/注册 | 直接扩展：在数组中加入 `new SaveIdeaReportTool(this.agent)` |
| 6 | `packages/agent-core/src/agent/permission/policies/index.ts` | 权限策略链组装 | 直接扩展：新增并注册自动审批策略 |
| 7 | `packages/agent-core/src/agent/session-mode/index.ts:resolveSessionModeDirectory` + `ensureGitignore` | 项目级 `.ody-code/<subdir>` 目录创建与 gitignore 维护 | 复用/抽离：把 `resolveSessionModeDirectory` 的目录解析逻辑或 `ensureGitignore` 泛化为 `.ody-code/ideas/` 使用 |
| 8 | `packages/agent-core/src/agent/session-mode/index.ts:findUniqueStemInDir` | 目录内唯一文件名生成 | 复用/抽离：保存工具需要相同逻辑 |
| 9 | `packages/agent-core/src/agent/kaos` | 文件 I/O 抽象 | 直接使用 `agent.kaos.mkdir` + `agent.kaos.writeText` |
| 10 | `packages/agent-core/src/agent/context/types.ts:SkillActivationOrigin` | 记录 skill 激活来源 | 直接使用：工具通过扫描上下文 history 的 origin 判断 idea skill 是否激活 |
| 11 | `packages/agent-core/test/skill/builtin-skills.test.ts` | 内置 skill 元数据测试 | 扩展：加入两个新 skill 的断言 |

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|------------|------------|-----------------|---------------|
| 1 | `hiddenInModes` 字段在 `SkillRegistry.listInvocableSkills(sessionMode)` 中已正确过滤非 normal 模式 [C:INFERRED] | High | skill 在非 normal 模式仍可见 | 阅读 `packages/agent-core/src/skill/registry.ts:118-132`（已验证） |
| 2 | 在 skill 提示词中显式提及工具名，LLM 会按要求调用 `SaveIdeaReport` [C:INFERRED] | Medium | 报告不被保存 | 通过集成测试 / 实际使用观察 |
| 3 | `ContextMessage.origin` 中 `skill_activation` 记录包含 `skillName`，可用于运行时守卫 [C:INFERRED] | High | 工具拒绝合法调用 | 阅读 `packages/agent-core/src/agent/context/types.ts:12-21`（已验证） |
| 4 | `.ody-code/` 被 `.gitignore` 后，`ideas/` 子目录自然被忽略 [C:INFERRED] | High | 用户意外提交 idea 文件 | 测试 `.gitignore` 规则 |
| 5 | `agent.kaos.writeText` 在父目录不存在时会失败；需要手动 `mkdir` [C:INFERRED] | High | 首次保存失败 | 阅读 `packages/agent-core/src/tools/builtin/file/write.ts:105-122`（已验证） |

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|------------|--------|------------|
| 1 | LLM 在 idea skill 未激活时调用 `SaveIdeaReport` | Low | 无关文件被写入 `.ody-code/ideas/` | 运行时守卫：检查上下文最近是否存在 idea skill 激活；拒绝并提示 |
| 2 | LLM 生成相同标题导致文件名冲突 | Medium | 覆盖旧报告 | `findUniqueStemInDir` 式自动加后缀 |
| 3 | 报告内容包含敏感/PII 信息被写入本地文件 | Medium | 隐私泄露 | 文件保存在项目本地 `.ody-code/`；已 gitignore；不做云端上传 |
| 4 | `SaveIdeaReport` 工具未按预期调用，用户找不到历史报告 | Medium | 功能不可用 | 保存为 best effort；skill 提示词中强调调用步骤 |
| 5 | skill 在非 normal 模式被误触发 | Low | 违反模式隔离 | `hiddenInModes: ['plan','design','office-hours','game-design']` |

## Data Models

### `IdeaReportType` [C:USER]

```typescript
type IdeaReportType = 'generator' | 'evaluator';
```

- `'generator'` 对应 `idea-generator` 生成的候选清单报告。
- `'evaluator'` 对应 `idea-evaluator` 生成的评分报告。

### `SaveIdeaReportInput` [C:USER]

```typescript
interface SaveIdeaReportInput {
  title: string;              // 报告标题，用于生成文件名 slug
  content: string;            // Markdown 格式正文
  type: IdeaReportType;       // 报告类型
  score?: number;             // 仅 evaluator 使用，0-10
  tags?: readonly string[];   // 可选标签，如 ['B2B', 'AI']
}
```

约束：
- `title` 去重后非空（trim 后长度 > 0）。
- `title` 不得包含敏感词（`key`/`token`/`password`/`secret`/`credential`），避免机密进入文件名。
- `score` 若提供，必须在 `[0, 10]` 区间。
- `tags` 元素去重、trim、过滤空字符串。

### Skill frontmatter [C:UPSTREAM] + [C:USER]

两个 skill 的 Markdown 顶部 frontmatter 示例：

```yaml
---
type: inline
name: idea-generator
description: >
  Systematically generate startup idea candidates ...
hiddenInModes:
  - plan
  - design
  - office-hours
  - game-design
---
```

`idea-evaluator` 同结构，`name` 为 `idea-evaluator`。

### Output file format [C:USER]

文件路径：`.ody-code/ideas/YYYY-MM-DD-<slug>.md`

文件内容结构：

```markdown
---
title: [title]
type: [generator | evaluator]
date: [ISO 8601]
score: [number | null]
tags: [tag1, tag2]
---

[content]
```

- `score` 省略或为空时不写入 frontmatter。
- `tags` 为空数组时写入 `tags: []`。
- `content` 保持 LLM 提供的原始 Markdown，不做额外包装。

## Algorithms

### A1. 生成唯一文件名 [C:USER]

```typescript
function generateIdeaFilePath(
  ideasDir: string,
  title: string,
  now: Date,
): string
```

伪代码：

```text
slug = slugifyTitle(title)                  // 小写、空格/特殊字符转连字符
slug = stripDatePrefix(slug)                // 防止用户标题自带日期前缀导致重复
baseStem = `${formatDatePrefix(now)}-${slug || 'untitled'}`
loop suffix = 1 .. MAX_SUFFIX:
  candidate = join(ideasDir, `${stem}.md`)
  if candidate 不存在:
    return candidate
  stem = `${baseStem}-${suffix}`
// 兜底：追加毫秒时间戳
return join(ideasDir, `${baseStem}-${Date.now()}.md`)
```

- `MAX_SUFFIX = 1000`（与 session-mode 一致）。
- 文件名冲突处理复用 `findUniqueStemInDir` 思路。

### A2. 运行时守卫：判断 idea skill 是否激活 [C:USER]

```typescript
function isIdeaSkillActive(history: readonly ContextMessage[]): boolean
```

伪代码：

```text
IDEA_SKILL_NAMES = ['idea-generator', 'idea-evaluator']
for msg in reverse(history):
  if msg.role == 'user' and msg.origin?.kind == 'skill_activation':
    if IDEA_SKILL_NAMES includes msg.origin.skillName:
      return true
    else:
      // 遇到了非 idea skill 的激活，说明 idea skill 上下文已被覆盖
      return false
return false
```

说明：
- 只检查 `role === 'user'` 的消息，因为 skill 内容以 system reminder 形式作为 user 消息注入。
- 从最近一条消息向前扫描；遇到任何 skill 激活即停止：若是 idea skill 则返回 true，否则返回 false。
- 不检查 compaction summary 或 injection origin，避免误报。

### A3. 目录创建与 `.gitignore` 维护 [C:INFERRED]

```typescript
async function ensureIdeasDirectory(cwd: string, kaos: Kaos): Promise<string>
```

伪代码：

```text
ideasDir = join(cwd, '.ody-code', 'ideas')
await kaos.mkdir(ideasDir, { parents: true, existOk: true })
await ensureGitignore(cwd, '.ody-code/')   // 复用 session-mode ensureGitignore
return ideasDir
```

### A4. 文件内容组装 [C:USER]

```typescript
function buildIdeaReportBody(input: SaveIdeaReportInput, now: Date): string
```

伪代码：

```text
frontmatter = {
  title: input.title,
  type: input.type,
  date: now.toISOString(),
}
if input.score is defined:
  frontmatter.score = input.score
if input.tags is defined and not empty:
  frontmatter.tags = input.tags
yaml = dumpYaml(frontmatter)
return `---\n${yaml}\n---\n\n${input.content.trim()}\n`
```

## Error Handling

| Error class | Immediate handling | Degradation path | Recovery condition |
|-------------|--------------------|--------------------|--------------------|
| `SaveIdeaReport` 在 idea skill 未激活时被调用 | 返回 `isError: true`，提示 "只能在 idea-generator 或 idea-evaluator skill 激活后使用" | 不写入文件；对话继续 | 用户重新激活 idea skill 后重试 |
| `title` 为空或仅空白 | 返回 `isError: true`，提示标题不能为空 | 不写入文件 | LLM 提供有效标题后重试 |
| `title` 包含敏感词（`key`/`token`/`password`/`secret`/`credential`） | 返回 `isError: true`，提示标题含敏感信息，要求换标题 | 不写入文件 | LLM 提供不含敏感词的标题后重试 |
| `score` 超出 `[0, 10]` | 返回 `isError: true`，提示分数范围 | 不写入文件 | LLM 提供有效分数后重试 |
| `.ody-code/ideas/` 目录创建失败 | 返回 `isError: true`，输出底层错误 | 不写入文件 | 用户修复权限/磁盘问题后重试 |
| 文件写入失败 | 返回 `isError: true`，输出底层错误 | 不写入文件；报告内容仍在对话中 | 用户修复后重试 |
| LLM 未调用 `SaveIdeaReport` | 无（best effort） | 报告仅以对话消息存在 | 无；用户可手动复制 |

## Call-site Integration

### 1. 注册内置 skill

文件：`packages/agent-core/src/skill/builtin/index.ts`（当前 18-34 行）

```typescript
import { IDEA_GENERATOR_SKILL } from './idea-generator';
import { IDEA_EVALUATOR_SKILL } from './idea-evaluator';

export function registerBuiltinSkills(registry: SkillRegistry): void {
  // ... existing skills ...
  registry.registerBuiltinSkill(IDEA_GENERATOR_SKILL);
  registry.registerBuiltinSkill(IDEA_EVALUATOR_SKILL);
}
```

### 2. 新增内置工具文件并导出

文件：`packages/agent-core/src/tools/builtin/idea/save-idea-report.ts`（新增）

```typescript
export const SaveIdeaReportInputSchema = z.object({ ... });
export class SaveIdeaReportTool implements BuiltinTool<SaveIdeaReportInput> { ... }
```

文件：`packages/agent-core/src/tools/builtin/index.ts`

```typescript
export * from './idea/save-idea-report';
```

### 3. 在 ToolManager 中实例化工具

文件：`packages/agent-core/src/agent/tool/index.ts:407-489`

在 `initializeBuiltinTools` 数组中加入：

```typescript
new b.SaveIdeaReportTool(this.agent),
```

### 4. 注册自动审批权限策略

文件：`packages/agent-core/src/agent/permission/policies/idea-tool-directory.ts`（新增）

```typescript
export class IdeaToolDirectoryApprovePermissionPolicy implements PermissionPolicy { ... }
```

文件：`packages/agent-core/src/agent/permission/policies/index.ts`

在策略链中 `DefaultToolApprovePermissionPolicy` 之前加入：

```typescript
new IdeaToolDirectoryApprovePermissionPolicy(agent),
```

### 5. skill 内容引用工具

文件：`packages/agent-core/src/skill/builtin/idea-generator.md` 与 `idea-evaluator.md`

在 `## Output Format` 之后追加段落：

```markdown
## 保存报告

完成上述报告后，调用 `SaveIdeaReport` 工具将其保存到项目目录：
- `title`: 报告的标题（一句话概括）。
- `content`: 上面的完整 Markdown 报告正文。
- `type`: 对于 idea-generator 填 `generator`，对于 idea-evaluator 填 `evaluator`。
- `score`: idea-evaluator 必填最终评分（0-10）；idea-generator 省略。
- `tags`: 可选标签数组，如 `["B2B", "AI"]`。
```

### 6. 测试扩展

文件：`packages/agent-core/test/skill/builtin-skills.test.ts`

- 将 `BUILTIN_SKILLS` 长度断言从 13 改为 15。
- 加入 `idea-generator` 与 `idea-evaluator` 的元数据断言。

新增测试文件：`packages/agent-core/test/tools/idea/save-idea-report.test.ts`

- 覆盖文件名生成、上下文守卫、frontmatter 组装、目录创建、错误返回。

## Test Plan

### 必过测试

1. `pnpm test packages/agent-core/test/skill/builtin-skills.test.ts`
   - 断言：内置 skill 总数 = 15。
   - 断言：`idea-generator` / `idea-evaluator` 的 `source === 'builtin'`，`path === 'builtin://idea-*'`，`description.length > 0`。
   - 断言：`hiddenInModes` 包含 `plan`, `design`, `office-hours`, `game-design`。

2. `pnpm test packages/agent-core/test/tools/idea/save-idea-report.test.ts`
   - 断言：输入 `title = "AI 客服系统"` 生成 `YYYY-MM-DD-ai-ke-fu-xi-tong.md`。
   - 断言：同目录已存在同名文件时生成 `YYYY-MM-DD-ai-ke-fu-xi-tong-1.md`。
   - 断言：上下文无 idea skill 激活时返回 `isError: true`。
   - 断言：上下文存在 `idea-generator` skill 激活时成功写入文件。
   - 断言：`score = 11` 返回 `isError: true`。
   - 断言：`title = "My secret API key idea"` 返回 `isError: true`。
   - 断言：输出文件包含正确的 YAML frontmatter。

3. `pnpm test packages/agent-core/test/permission/idea-tool-directory.test.ts`（或合并到现有权限测试）
   - 断言：对 `.ody-code/ideas/YYYY-MM-DD-foo.md` 的写操作返回 `approve`。
   - 断言：对 `.ody-code/plans/YYYY-MM-DD-foo.md` 的写操作不返回 `approve`（保持现有策略）。

### Done Criteria

- `pnpm test packages/agent-core` 全部通过。
- `pnpm lint` 无新增错误。
- `pnpm build` 成功。
- 在 normal 模式下手动激活 `/idea-generator`，验证 `.ody-code/ideas/` 下产生文件。

## Self-Review

### 最贵的 3 个决策与 adversarial 输入

#### D1. 运行时守卫 `isIdeaSkillActive`

| # | 输入历史（从旧到新） | 期望输出 | 说明 |
|---|----------------------|----------|------|
| 1 | `[{role:'user', origin:{kind:'skill_activation', skillName:'idea-generator'}}, {role:'assistant', content:'...'}]` | `true` | 正常：idea skill 激活后未出现其他 skill 覆盖 |
| 2 | `[{role:'user', origin:{kind:'skill_activation', skillName:'idea-generator'}}, {role:'user', origin:{kind:'skill_activation', skillName:'simplicity-first'}}]` | `false` | adversarial：后续其他 skill 激活应覆盖 idea 上下文 |
| 3 | `[{role:'assistant'}, {role:'user', origin:{kind:'user'}}]` | `false` | adversarial：无 skill 激活 |

结论：从最近消息反向扫描，遇到第一个 `skill_activation` 即停止，正确区分激活状态。

#### D2. 文件名生成

已用 `node -e` 验证现有 `slugifyTitle` + `stripDatePrefix` + `formatDatePrefix`：

| # | 输入 title | 期望 base | 实际 base |
|---|-----------|-----------|-----------|
| 1 | `AI 客服系统` | `2026-06-22-ai-客服系统` | `2026-06-22-ai-客服系统` |
| 2 | `2026-06-22-ai-kefu` | `2026-06-22-ai-kefu` | `2026-06-22-ai-kefu` |
| 3 | `2026-06-22` | `2026-06-22-untitled` | `2026-06-22-untitled` |

结论：日期前缀不会重复，CJK 标题保留，纯日期标题回退到 `untitled`。

#### D3. 权限策略自动审批范围

| # | 请求路径 | 期望结果 |
|---|----------|----------|
| 1 | `.ody-code/ideas/2026-06-22-foo.md` | `approve` |
| 2 | `.ody-code/plans/2026-06-22-foo.md` | 不 approve（fallthrough） |
| 3 | `.ody-code/ideas/../plans/foo.md` | 不 approve（归一化后不在 ideas 下） |

结论：策略先归一化路径，再判断是否以 `.ody-code/ideas/` 开头。

### 四镜扫描

- **Security**：检查了路径遍历、PII、自动审批范围。发现 `title` 若包含用户敏感词可能进入文件名；采用与 topic-generator 相同的 `sensitiveWords` 过滤策略，遇到 `key`/`token`/`password`/`secret`/`credential` 时拒绝生成文件名并要求 LLM 换标题。
- **Test**：每个行为都有 must-pass / must-reject：skill 元数据、文件名生成、上下文守卫、frontmatter 组装、权限策略。无与常量矛盾的断言。
- **Ops**：文件名冲突用 `findUniqueStemInDir` 式递增后缀解决；并发场景下存在极小概率 race，但保存为 best effort，可接受。每次调用仅一次 `mkdir` + 一次 `writeText`，无重试/退避。
- **Integration**：依赖的 hook/字段均已验证存在：`hiddenInModes` 过滤（`registry.ts:118-132`）、`SkillActivationOrigin`（`context/types.ts:12-21`）、`kaos.writeText` 行为（`write.ts:105-122`）、工具注册点（`agent/tool/index.ts:407-489`）、权限策略链（`permission/policies/index.ts`）。设计文件位于系统分配路径 `.ody-code/designs/2026-06-22-idea-skills.md`。
- **Scope**：单一功能（idea skills + 保存工具），未拆分成多个独立子项目；一个设计文件足以覆盖。

## User Final Approval

- 审计级别：Deep
- 假设确认：全部 5 条 [C:INFERRED] 假设均被用户接受，无需调整或推迟。
- 核心论断确认：用户确认 Scope、Architecture、Data Models、Algorithms、Error Handling 中的核心论断全部正确。
- 状态：待 ExitDesignMode 批准。
