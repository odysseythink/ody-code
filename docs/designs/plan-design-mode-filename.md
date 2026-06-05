# Design: Topic-Based Filename for Plan/Design Mode

## Audit Level

**Deep** [C:USER] — 确认每个章节的关键断言，外加每个假设。

---

## Resolved Decisions

| # | Dimension | Decision | Source |
|---|---|---|---|
| 1 | Scope | Design 和 Plan 模式都使用主题化文件名 | [C:USER] |
| 2 | Data | 主题由 LLM 根据最近对话自动生成；允许用户手动传入 `topic` 参数覆盖 | [C:USER] |
| 3 | Data | planId 保持独立随机 hero slug；文件名使用 topic + 时间戳 | [C:USER] |
| 4 | Error | 文件名追加 UTC 时间 `YYYYMMDD-HHMMSS` 避免冲突 | [C:USER] |
| 5 | Error | LLM 生成失败时回退到 `design-YYYYMMDD-HHMMSS` 或 `plan-YYYYMMDD-HHMMSS` | [C:USER] |
| 6 | Security | Prompt 安全指令 + 代码层敏感词过滤 | [C:USER] |
| 7 | Observability | LLM 生成失败时记录 `topic_generation_failed` telemetry 事件 | [C:USER] |
| 8 | Operations | 不需要 experimental feature flag，直接替换现有行为 | [C:USER] |
| 9 | Integration | 主题生成在工具层（EnterDesignModeTool / EnterPlanModeTool）执行 | [C:USER] |
| 10 | Integration | 选择方案 A：工具层生成主题，PlanMode 只接收 fileStem | [C:USER] |

---

## Scope In / Out

### In

- [C:USER] EnterDesignModeTool 和 EnterPlanModeTool 新增可选 `topic?: string` 参数。
- [C:USER] 工具层调用 LLM 根据最近对话生成英文 kebab-case 主题 slug（最大 50 字符）。
- [C:USER] 文件名格式：`<topic>-YYYYMMDD-HHMMSS.md`（design 存 `designs/`，plan 存 `plans/`）。
- [C:USER] planId 保持独立随机 hero slug，用于 records / replay / 权限守卫 (`isWritableAdvancedSessionModePath`)。
- [C:USER] LLM 生成失败时回退到 `design-YYYYMMDD-HHMMSS.md` 或 `plan-YYYYMMDD-HHMMSS.md`。
- [C:USER] Prompt 安全指令 + 代码层敏感词过滤（`key`, `token`, `password`, `secret`, `credential` 等）。
- [C:USER] Telemetry：`topic_generation_failed` 事件，附 `reason` 字段。
- [C:INFERRED] 时间戳使用 UTC（`toISOString().slice(0, 19).replace(/[-T:]/g, '')` 的日期部分 + 时间部分）。

### Out (Deferred)

- [C:DEFERRED] 不支持 plan 文件拆分子系统时的子文件名主题化（子文件仍用 `planId-<subsystem>.md`）。
- [C:DEFERRED] 不支持用户事后重命名设计/计划文件。
- [C:DEFERRED] 不支持从多个历史消息中聚合主题（仅用最近一条用户消息的文本作为上下文）。
- [C:DEFERRED] 不添加 experimental feature flag（直接上线，替换现有随机 slug 行为）。

---

## Architecture & Data Flow

```
User → EnterDesignModeTool / EnterPlanModeTool
  │
  ├─ 用户传了 topic 参数？
  │     ├─ 是 → topic = 用户输入（经 slugify + 过滤）
  │     └─ 否 → TopicGenerator.generate(agent)
  │               │
  │               ├─ 从 agent.context.history 取最近一条 role='user' 消息
  │               ├─ 构建轻量 system prompt（见下方 Prompt 模板）
  │               ├─ agent.generate(provider, systemPrompt, [], userMessage)
  │               ├─ 清理结果：slugify + 敏感词过滤 + 截断 50 字符
  │               └─ 返回 topic slug 或 null
  │
  ├─ 如果 topic 为 null → topic = kind === 'design' ? 'design' : 'plan'
  ├─ timestamp = formatUtcTimestamp(new Date())  // YYYYMMDD-HHMMSS
  ├─ fileStem = `${topic}-${timestamp}`
  └─ PlanMode.enter(id, createFile, emitStatus, kind, fileStem)
        │
        ├─ planId = generateHeroSlug()  // 随机，不变
        ├─ _fileStem = fileStem ?? planId
        ├─ advancedSessionModeFilePath = advancedSessionModeFilePathFor(_fileStem)
        │     └─ 使用 _fileStem 作为文件名 stem（替代原来的 planId）
        ├─ _SessionModeFilePath = advancedSessionModeFilePath
        ├─ records.logRecord({ type: 'plan_mode.enter', id: planId, kind })
        └─ emitStatusUpdated()
```

[C:USER] 工具层负责主题生成；PlanMode 保持纯净，只接收 `_fileStem` 并据此生成路径。

---

## Interfaces

### TopicGenerator

```ts
// packages/agent-core/src/agent/plan/topic-generator.ts

export interface TopicGeneratorOptions {
  /** 最大主题长度（字符数），默认 50 */
  readonly maxLength?: number;
  /** 敏感词列表，默认内置列表 */
  readonly sensitiveWords?: readonly string[];
}

export class TopicGenerator {
  constructor(
    private readonly agent: Agent,
    private readonly options: TopicGeneratorOptions = {},
  ) {}

  /**
   * 根据最近对话生成主题 slug。
   * @returns kebab-case 英文主题；如果生成失败或内容不安全则返回 null。
   */
  async generate(): Promise<string | null>;
}
```

### PlanMode.enter 签名变更

```ts
// packages/agent-core/src/agent/plan/index.ts

async enter(
  id = this.createAdvancedSessionModeId(),
  createFile = false,
  emitStatus = true,
  kind: AdvancedSessionModeKind = 'plan',
  fileStem?: string,  // [C:USER] 新增
): Promise<void>
```

### EnterDesignModeInputSchema / EnterPlanModeInputSchema 变更

```ts
// packages/agent-core/src/tools/builtin/planning/enter-design-mode.ts
// packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts

export const EnterDesignModeInputSchema = z.object({
  topic: z.string().max(100).optional(),  // [C:USER] 新增
}).strict();

export const EnterPlanModeInputSchema = z.object({
  topic: z.string().max(100).optional(),  // [C:USER] 新增
}).strict();
```

---

## Non-Trivial Algorithms

### Topic Prompt Template

```ts
function buildTopicPrompt(userMessageText: string): string {
  return `You are a concise topic extractor. Based on the user's message below, generate a short English topic phrase (2-5 words) in kebab-case (lowercase, hyphen-separated).

Rules:
- Ignore API keys, passwords, tokens, secrets, credentials, or any sensitive information.
- Focus on the functional topic or feature being discussed.
- If the message is ambiguous, return "general".
- Output ONLY the kebab-case topic, nothing else.

User message: """${userMessageText}"""`;
}
```

[C:USER] Prompt 包含安全指令，要求 LLM 忽略敏感信息。

### Topic Cleanup Pipeline

```ts
function cleanupTopic(raw: string, maxLength = 50): string | null {
  // 1. Trim and lowercase
  let topic = raw.trim().toLowerCase();

  // 2. Replace non-alphanumeric with hyphens
  topic = topic.replace(/[^a-z0-9]+/g, '-');

  // 3. Remove leading/trailing hyphens
  topic = topic.replace(/^-+|-+$/g, '');

  // 4. Collapse multiple hyphens
  topic = topic.replace(/-+/g, '-');

  // 5. Check sensitive words
  const sensitiveWords = ['key', 'token', 'password', 'secret', 'credential', 'auth'];
  if (sensitiveWords.some(w => topic.includes(w))) {
    return null;
  }

  // 6. Truncate
  if (topic.length > maxLength) {
    topic = topic.slice(0, maxLength);
    // Ensure we don't end with a hyphen after truncation
    topic = topic.replace(/-+$/, '');
  }

  // 7. Validate minimum length
  if (topic.length < 2) {
    return null;
  }

  return topic;
}
```

[C:USER] 代码层敏感词过滤 + 截断 + 格式清理。

### UTC Timestamp Format

```ts
function formatUtcTimestamp(date: Date): string {
  const iso = date.toISOString(); // "2025-06-04T14:30:52.000Z"
  return iso.slice(0, 4) + iso.slice(5, 7) + iso.slice(8, 10) + '-'
       + iso.slice(11, 13) + iso.slice(14, 16) + iso.slice(17, 19);
  // => "20250604-143052"
}
```

[C:INFERRED] 使用 UTC 避免时区问题。

---

## Call-Site Integration

### 1. EnterDesignModeTool — `packages/agent-core/src/tools/builtin/planning/enter-design-mode.ts`

**Line range:** ~33-62 (resolveExecution 方法)

**变更前：**
```ts
execute: async () => {
  // Guard: already in plan/design mode
  if (this.agent.planMode.isActive) { ... }

  try {
    await this.agent.planMode.enter(undefined, undefined, undefined, 'design');
  } catch (error) { ... }
  ...
}
```

**变更后：**
```ts
execute: async () => {
  // Guard: already in plan/design mode
  if (this.agent.planMode.isActive) { ... }

  let fileStem: string | undefined;
  if (_args.topic !== undefined) {
    const cleaned = cleanupTopic(_args.topic);
    if (cleaned !== null) {
      fileStem = `${cleaned}-${formatUtcTimestamp(new Date())}`;
    }
  } else {
    const generator = new TopicGenerator(this.agent);
    const topic = await generator.generate();
    if (topic !== null) {
      fileStem = `${topic}-${formatUtcTimestamp(new Date())}`;
    }
    // 若 topic 为 null，fileStem 保持 undefined，PlanMode 会使用 planId 回退
  }

  try {
    await this.agent.planMode.enter(undefined, undefined, undefined, 'design', fileStem);
  } catch (error) { ... }
  ...
}
```

### 2. EnterPlanModeTool — `packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts`

**Line range:** 类似 enter-design-mode.ts 的结构

**变更：** 与 EnterDesignModeTool 相同模式，只是 `kind` 为 `'plan'`，回退主题为 `'plan'`。

### 3. PlanMode.enter — `packages/agent-core/src/agent/plan/index.ts`

**Line range:** ~35-73

**变更前：**
```ts
async enter(
  id = this.createAdvancedSessionModeId(),
  createFile = false,
  emitStatus = true,
  kind: AdvancedSessionModeKind = 'plan',
): Promise<void> {
  ...
  const advancedSessionModeFilePath = this.advancedSessionModeFilePathFor(id);
  ...
}
```

**变更后：**
```ts
async enter(
  id = this.createAdvancedSessionModeId(),
  createFile = false,
  emitStatus = true,
  kind: AdvancedSessionModeKind = 'plan',
  fileStem?: string,
): Promise<void> {
  ...
  this._fileStem = fileStem ?? id;  // [C:USER] 新增字段
  const advancedSessionModeFilePath = this.advancedSessionModeFilePathFor(this._fileStem);
  ...
}
```

### 4. PlanMode.advancedSessionModeFilePathFor — `packages/agent-core/src/agent/plan/index.ts`

**Line range:** ~181-189

**变更前：**
```ts
private advancedSessionModeFilePathFor(id: string): string {
  const cwdSubdir = this._kind === 'design' ? 'design' : 'plan';
  const homeSubdir = this._kind === 'design' ? 'designs' : 'plans';
  const plansDir =
    this.agent.homedir === undefined
      ? join(this.agent.config.cwd, cwdSubdir)
      : join(this.agent.homedir, homeSubdir);
  return join(plansDir, `${id}.md`);
}
```

**变更后：** 方法参数名从 `id` 改为 `stem` 以提高可读性（功能不变）：
```ts
private advancedSessionModeFilePathFor(stem: string): string {
  const cwdSubdir = this._kind === 'design' ? 'design' : 'plan';
  const homeSubdir = this._kind === 'design' ? 'designs' : 'plans';
  const plansDir =
    this.agent.homedir === undefined
      ? join(this.agent.config.cwd, cwdSubdir)
      : join(this.agent.homedir, homeSubdir);
  return join(plansDir, `${stem}.md`);
}
```

### 5. PlanMode 新增 `_fileStem` 字段

**Line range:** ~24-27 (类成员声明)

**新增：**
```ts
protected _fileStem: string | null = null;
```

**getter 新增：**
```ts
get fileStem(): string | null {
  return this._fileStem;
}
```

**reset 位置：** cancel/exit 方法中同步清空 `_fileStem`。

---

## Error & Degradation

| 错误场景 | 错误类 | 立即处理 | 降级路径 | 恢复条件 |
|---|---|---|---|---|
| LLM API 错误/超时 | `APIError` / `TimeoutError` | 捕获异常，记录 telemetry `topic_generation_failed`（reason: `api_error` / `timeout`），返回 null | 回退到 `design-YYYYMMDD-HHMMSS` 或 `plan-YYYYMMDD-HHMMSS` | 无，已降级完成 |
| LLM 返回空/无效主题 | `ValidationError` | 记录 telemetry（reason: `empty_result`），返回 null | 同上 | 无 |
| 主题包含敏感词 | `SecurityError`（内部） | 记录 telemetry（reason: `sensitive_content`），返回 null | 同上 | 无 |
| 主题超长 (>50 字符) | — | 截断到 50 字符，继续使用 | 截断后正常流程 | 无 |
| 文件系统写入失败 | `ENOENT` / `EACCES` | 抛出异常，cancel plan mode | 保持非 plan/design 模式 | 磁盘空间/权限恢复后重试 |
| 用户已处于 plan/design 模式 | `StateError` | 返回错误消息给调用者 | 提示用户先 exit 当前模式 | 用户 exit 后重试 |

[C:USER] 所有降级路径最终都保证能生成一个合法的文件名。

---

## Test Plan

| 测试文件 | 断言 |
|---|---|
| `packages/agent-core/test/agent/plan/topic-generator.test.ts`（新增） | `generate()` 从用户消息返回 kebab-case 主题；敏感词输入返回 `null`；超长输入截断到 50；空输入返回 `null`；非 ASCII 输入返回合法 slug 或 `null` |
| `packages/agent-core/test/tools/enter-design-mode.test.ts` | 无 `topic` 参数时，mock `TopicGenerator.generate` 返回 `'temp-dashboard'`，断言 `advancedSessionModeFilePath` 匹配 `/temp-dashboard-\d{8}-\d{6}\.md$/`；传入 `topic: 'Auth Refactor'` 断言路径包含 `auth-refactor` |
| `packages/agent-core/test/tools/enter-plan-mode.test.ts` | 同上，kind 为 `plan`，回退前缀为 `plan` |
| `packages/agent-core/test/agent/plan.test.ts` | `planId` 仍是随机 hero slug（如 `silver-surfer-deadpool`）；`fileStem` 是 topic + 时间戳；`isWritableAdvancedSessionModePath` 仍基于 `planId` |
| `packages/agent-core/test/agent/injection/design-mode.test.ts` | mock `advancedSessionModeFilePath` 现在包含 topic 时间戳格式而非 hero slug |
| `packages/agent-core/test/agent/injection/plan-mode.test.ts` | 同上 |

**Done Criteria：**
- `pnpm test --filter agent-core` 全部通过
- `pnpm tsc --noEmit --filter agent-core` 无类型错误
- `pnpm test --filter ody-code` 通过（Footer 渲染不依赖文件名格式，只透传 `advancedSessionModeFilePath`）

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | LLM 主题生成增加进入模式延迟（+200-500ms） | Medium | Medium | 轻量 prompt（<200 tokens）；设置 3s 超时；异步不阻塞 UI 渲染 |
| 2 | LLM 生成主题不可靠（乱码、非英文、含敏感词） | Medium | Low | Prompt 明确约束 + 代码层清理/过滤/截断；失败回退到默认文件名 |
| 3 | `planId` 与 `fileStem` 分离导致 `isWritableAdvancedSessionModePath` 不匹配 | Low | High | 保持 `isWritableAdvancedSessionModePath` 基于 `planId` 不变；`fileStem` 仅影响文件名生成 |
| 4 | 时间戳使用本地时区导致跨时区协作混乱 | Low | Low | 使用 UTC 时间 |
| 5 | designs/plans 目录文件堆积 | Medium | Low | 现有清理机制不变；文件名可读性提高反而便于用户手动清理 |

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if Wrong | How to Verify |
|---|---|---|---|---|
| 1 | `agent.generate` 可以在工具执行期间被同步调用（不引发重入问题） | Medium | High（主题生成无法进行） | 在测试中验证 `EnterDesignModeTool` 中调用 `agent.generate` 不会死锁或报错 |
| 2 | `agent.context.history` 中至少包含一条用户消息（否则 TopicGenerator 回退） | High | Low | 测试覆盖空历史场景 |
| 3 | 轻量 prompt（<200 tokens）的成本和延迟在用户可接受范围内 | Medium | Medium | 通过手动测试测量延迟；若 >1s 考虑添加 feature flag |
| 4 | 现有 `AdvancedSessionModeData.path` 的消费者只读取路径字符串，不解析文件名 stem | High | Low | 全局搜索 `advancedSessionModeFilePath` / `plan.path` 的使用点，确认无解析逻辑 |
| 5 | 内置敏感词列表足够覆盖常见场景，不会误杀合法主题 | Medium | Low | 审计敏感词列表；留好扩展接口 |

---

## Open Questions / Resolved Decisions

（见上方 Resolved Decisions 表格）
