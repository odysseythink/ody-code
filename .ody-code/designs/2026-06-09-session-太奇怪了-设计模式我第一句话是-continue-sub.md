# 改进 Session Mode 文件名生成逻辑

## Scope

### In

1. **Design 模式文件名生成** [C:USER]
   - 进入 design 模式时，调用 LLM 从用户消息中提取设计主题，作为文件名 stem。
   - LLM 提取失败时回退到本地 `extractTopicFromMessage()`（取前 5 个词）。
   - 不根据设计文件最终 H1 重命名——初始文件名即最终文件名。

2. **Plan 模式文件名生成（引用设计文件场景）** [C:USER]
   - 当用户消息中明确引用了 `.ody-code/designs/*.md` 路径时，提取该设计文件的 stem 作为 plan 文件名 stem。
   - 验证引用的设计文件存在于磁盘；不存在时回退到 LLM 提取主题。
   - 复用 `_lastCompletedDesignFilePath` 机制作为无引用时的后备策略。

3. **修改范围**：`packages/agent-core/src/agent/session-mode/index.ts` 和 `topic-generator.ts` [C:USER]

### Out

1. **Review 模式** — 本次不涉及 reviewer 文件名。 [C:INFERRED]
2. **已有文件的自动重命名** — 设计文件写完后不因 H1 变化而重命名。 [C:USER]
3. **Plan 模式无引用时的行为变更** — 保持现有 `_lastCompletedDesignFilePath` 回退逻辑不变。 [C:USER]
4. **UI/TUI 改动** — 纯后端逻辑修改，不涉及前端展示。 [C:INFERRED]

---

## Architecture

```
SessionMode.enter(kind)
  └── resolveFilePathEagerly(dir)
        ├── if kind === 'design':
        │     └── resolveDesignFilePathEagerly(dir)
        │           ├── TopicGenerator.generate() → slug (LLM)
        │           └── fallback: extractTopicFromMessage(strippedText)
        │           └── findUniqueStemInDir(dir, datePrefix + slug)
        ├── if kind === 'plan':
        │     └── resolvePlanFilePathEagerly(dir)
        │           ├── extractDesignFileRefFromMessage(text) → path | null
        │           ├── if path exists on disk:
        │           │     └── use basename(path, '.md') as stem
        │           ├── else if _lastCompletedDesignFilePath !== null:
        │           │     └── use its stem
        │           └── else:
        │                 └── TopicGenerator.generate() → slug (LLM)
        │                 └── fallback: extractTopicFromMessage(strippedText)
        └── findUniqueStemInDir(dir, stem)
```

**关键变更点** [C:INFERRED]：
- `resolveFilePathEagerly()` 拆分为 design 和 plan 两条路径。
- 新增 `extractDesignFileRefFromMessage()` 工具函数（放在 `topic-generator.ts`）。
- `TopicGenerator.generate()` 现有 LLM 提取能力复用于 eager resolution。

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | `TopicGenerator.generate()` 可以在 `SessionMode` 内部直接调用（`agent.generate` 可用）。 | High | LLM 提取不可用，回退到本地提取，功能降级但仍工作。 | 代码已验证：`TopicGenerator` 接收 `Agent` 构造，`SessionMode` 持有 `agent`。 |
| 2 | 用户消息中引用的设计文件路径格式为 `.ody-code/designs/<stem>.md`（相对路径）。 | Medium | 可能漏掉绝对路径引用（如 `/Users/.../.ody-code/designs/...`）。 | 实现时用正则同时匹配相对和绝对路径中的 `.ody-code/designs/*.md` 模式。 |
| 3 | 多个设计文件路径引用时，取第一个匹配即可。 | Medium | 如果用户引用了多个设计文件，可能选错。 | 已通过 AskUserQuestion 确认（见 Resolved decisions）。 |
| 4 | 进入 design 模式时增加一次 LLM 调用（~3s timeout）对用户体验可接受。 | Medium | 如果网络慢，进入 design 模式会有明显延迟。 | timeout 设为 3s（与 `TopicGenerator` 一致），超时不阻塞，回退本地提取。 |
| 5 | `_lastCompletedDesignFilePath` 仍会在 `exit()` 时正确设置。 | High | Plan 模式无引用时无法继承 design 文件名。 | 代码已验证：`exit()` 第 172-173 行设置 `_lastCompletedDesignFilePath`。 |

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | LLM 提取主题超时，导致进入 design 模式延迟。 | Medium | 用户体验差 | Timeout 3s，超时不抛错，静默回退本地提取。 |
| 2 | `extractDesignFileRefFromMessage` 正则误匹配非设计文件路径。 | Low | Plan 文件名错误 | 正则严格限定 `.ody-code/designs/` 前缀 + `.md` 后缀。 |
| 3 | 修改破坏现有测试 `expect(ctx.llmCalls).toHaveLength(0)`（进入 plan 模式无 LLM 调用断言）。 | High | CI 失败 | 更新测试：design 模式进入时断言 `llmCalls` 长度为 1（LLM 提取主题）。 |
| 4 | 文件名 stem 冲突导致 `findUniqueStemInDir` 生成 `-1`、`-2` 后缀，用户困惑。 | Low | 文件名不符合预期 | 保持现有去重逻辑不变，这是已知行为。 |

---

## Resolved decisions

1. **审计级别**：Deep [C:USER]
2. **Design 模式主题提取**：进入时调用 LLM，失败回退本地提取 [C:USER]
3. **Plan 模式引用设计文件**：优先从消息匹配路径，验证存在后使用其 stem；不存在则回退 LLM [C:USER]
4. **Plan 模式无引用时**：保持现有 `_lastCompletedDesignFilePath` 机制 [C:USER]
5. **设计文件 H1 重命名**：不自动重命名 [C:USER]
6. **多文件引用处理**：取第一个匹配路径 [C:INFERRED]

---

## Interfaces & Types

### `extractDesignFileRefFromMessage(text: string): string | null` [C:INFERRED]

```typescript
// 从用户消息文本中提取引用的设计文件路径（相对或绝对路径）。
// 返回第一个匹配的路径字符串（含 .md 后缀），无匹配返回 null。
function extractDesignFileRefFromMessage(text: string): string | null;
```

**契约**：只匹配以 `.ody-code/designs/` 为路径片段、以 `.md` 结尾的路径。支持相对路径（`./` 或无前导）和绝对路径（`/` 或盘符开头）。

### `SessionMode.resolveFilePathEagerly(dir: string): Promise<void>` [C:INFERRED]

```typescript
// 在 session mode 进入时提前解析文件名。
// 根据 kind 分派到 design 或 plan 专用解析逻辑。
private async resolveFilePathEagerly(dir: string): Promise<void>;
```

### `SessionMode.resolveDesignFilePathEagerly(dir: string): Promise<void>` [C:INFERRED]

```typescript
// Design 模式专用：先 LLM 提取主题，失败后回退本地提取。
private async resolveDesignFilePathEagerly(dir: string): Promise<void>;
```

### `SessionMode.resolvePlanFilePathEagerly(dir: string): Promise<void>` [C:INFERRED]

```typescript
// Plan 模式专用：先检查设计文件引用，再 _lastCompletedDesignFilePath，最后 LLM/本地提取。
private async resolvePlanFilePathEagerly(dir: string): Promise<void>;
```

---

## Algorithms

### Algorithm 1: Design 模式文件名解析 [C:USER]

```
resolveDesignFilePathEagerly(dir):
  if _sessionModeFilePath !== null: return

  // 1. 尝试 LLM 提取主题
  slug = null
  try:
    generator = new TopicGenerator(agent, { maxWords: 5, maxLength: 50 })
    slug = await generator.generate()
  catch (error):
    log.warn('LLM topic extraction failed for design mode', { error })

  // 2. LLM 失败或返回空 → 回退本地提取
  if slug === null:
    text = topicSlugFromHistory()  // 取最新用户消息，stripLocators 后提取
    if text !== null:
      slug = text
    else:
      return  // 无可用主题，推迟到 lazy resolution

  // 3. 组装路径
  datePrefix = formatDatePrefix(new Date())
  stem = await findUniqueStemInDir(dir, `${datePrefix}-${slug}`)
  _sessionModeFilePath = join(dir, `${stem}.md`)
```

### Algorithm 2: Plan 模式文件名解析（引用设计文件场景） [C:USER]

```
resolvePlanFilePathEagerly(dir):
  if _sessionModeFilePath !== null: return

  // 1. 检查用户消息中是否引用了设计文件
  lastUserMessage = getLatestUserMessageText()
  designPath = extractDesignFileRefFromMessage(lastUserMessage)

  if designPath !== null:
    // 2. 验证文件存在
    try:
      await agent.kaos.stat(designPath)
      designStem = basename(designPath, '.md')
      stem = await findUniqueStemInDir(dir, designStem)
      _sessionModeFilePath = join(dir, `${stem}.md`)
      return
    catch (error):
      if isMissingFileError(error):
        log.warn('Referenced design file not found, falling back', { designPath })
      else:
        throw error

  // 3. 无引用或文件不存在 → 复用现有逻辑
  if _lastCompletedDesignFilePath !== null:
    designBase = basename(_lastCompletedDesignFilePath)
    designStem = designBase.endsWith('.md') ? designBase.slice(0, -3) : designBase
    _lastCompletedDesignFilePath = null
    stem = await findUniqueStemInDir(dir, designStem)
    _sessionModeFilePath = join(dir, `${stem}.md`)
    return

  // 4. 最后回退：LLM 提取 → 本地提取
  slug = null
  try:
    generator = new TopicGenerator(agent, { maxWords: 5, maxLength: 50 })
    slug = await generator.generate()
  catch (error):
    log.warn('LLM topic extraction failed for plan mode', { error })

  if slug === null:
    slug = topicSlugFromHistory()

  if slug === null:
    return  // 推迟到 lazy resolution

  datePrefix = formatDatePrefix(new Date())
  stem = await findUniqueStemInDir(dir, `${datePrefix}-${slug}`)
  _sessionModeFilePath = join(dir, `${stem}.md`)
```

### Algorithm 3: 提取设计文件引用 [C:INFERRED]

```
extractDesignFileRefFromMessage(text):
  prefix = '.ody-code/designs/'
  idx = text.indexOf(prefix)
  if idx === -1: return null

  // 向前回溯：路径字符只能是字母、数字、.、/、~、-、_
  start = idx
  while start > 0:
    prevChar = text[start - 1]
    if prevChar is whitespace: break
    if prevChar not in [a-zA-Z0-9._/~\-]: break
    start--

  // 向后匹配：路径字符 + .md
  afterPrefix = text.slice(idx)
  mdMatch = afterPrefix.match(/^[a-zA-Z0-9._\/~\-]+\.md/)
  if mdMatch === null: return null

  path = text.slice(start, idx + mdMatch[0].length)

  // 清理首尾引号/括号
  path = path.replace(/^["'`(]+|["'`)]+$/g, '')

  return path
```

**验证**（ ephemeral `node -e` ）：

```bash
node -e "
const tests = [
  { input: '将设计.ody-code/designs/2026-06-09-foo.md转换为计划', expected: '.ody-code/designs/2026-06-09-foo.md' },
  { input: '参考 /Users/x/.ody-code/designs/bar.md 做计划', expected: '/Users/x/.ody-code/designs/bar.md' },
  { input: '继续之前的计划', expected: null },
  { input: '请看.ody-code/designs/a.md和.ody-code/designs/b.md', expected: '.ody-code/designs/a.md' },
  { input: '\"./.ody-code/designs/quoted.md\"', expected: './.ody-code/designs/quoted.md' },
  { input: '将设计.ody-code/designs/2026-06-09-sub-project-2-phaseD.md转换为完整的执行计划', expected: '.ody-code/designs/2026-06-09-sub-project-2-phaseD.md' },
];
function extractDesignFileRefFromMessage(text) {
  const prefix = '.ody-code/designs/';
  const idx = text.indexOf(prefix);
  if (idx === -1) return null;
  let start = idx;
  while (start > 0) {
    const prevChar = text[start - 1];
    if (/\\s/.test(prevChar)) break;
    if (!/[a-zA-Z0-9._\\/~\-]/.test(prevChar)) break;
    start--;
  }
  const afterPrefix = text.slice(idx);
  const mdMatch = afterPrefix.match(/^[a-zA-Z0-9._\\/~\-]+\\.md/);
  if (!mdMatch) return null;
  let path = text.slice(start, idx + mdMatch[0].length);
  path = path.replace(/^[\"'\`(]+|[\"'\`)]+$/g, '');
  return path;
}
for (const t of tests) {
  const got = extractDesignFileRefFromMessage(t.input);
  console.log(got === t.expected ? 'PASS' : 'FAIL', JSON.stringify(t.input), 'got:', got, 'expected:', t.expected);
}
"
```

---

## Call-site Integration

### 修改点 1: `SessionMode.resolveFilePathEagerly` [C:USER]

**文件**: `packages/agent-core/src/agent/session-mode/index.ts`  
**行范围**: 235-261（现有 `resolveFilePathEagerly` 方法）

```typescript
// BEFORE: 统一的 resolveFilePathEagerly，按 kind 做简单分支
private async resolveFilePathEagerly(dir: string): Promise<void> {
  if (this._sessionModeFilePath !== null) return;
  if (this._kind === 'plan' && this._lastCompletedDesignFilePath !== null) { ... }
  const slug = this.topicSlugFromHistory();
  ...
}

// AFTER: 分派到 kind 专用方法
private async resolveFilePathEagerly(dir: string): Promise<void> {
  if (this._sessionModeFilePath !== null) return;
  if (this._kind === 'design') {
    await this.resolveDesignFilePathEagerly(dir);
  } else {
    await this.resolvePlanFilePathEagerly(dir);
  }
}
```

### 修改点 2: 新增 `resolveDesignFilePathEagerly` [C:USER]

**文件**: `packages/agent-core/src/agent/session-mode/index.ts`  
**插入位置**: `resolveFilePathEagerly` 之后

```typescript
private async resolveDesignFilePathEagerly(dir: string): Promise<void> {
  let slug: string | null = null;
  try {
    const generator = new TopicGenerator(this.agent, { maxWords: 5, maxLength: 50 });
    slug = await generator.generate();
  } catch (error) {
    this.agent.log?.warn('LLM topic extraction failed for design mode', { error });
  }
  if (slug === null) {
    slug = this.topicSlugFromHistory();
  }
  if (slug === null) return;
  const datePrefix = formatDatePrefix(new Date());
  const stem = await this.findUniqueStemInDir(dir, `${datePrefix}-${slug}`);
  this._sessionModeFilePath = join(dir, `${stem}.md`);
}
```

### 修改点 3: 替换现有的 plan 分支为 `resolvePlanFilePathEagerly` [C:USER]

**文件**: `packages/agent-core/src/agent/session-mode/index.ts`

```typescript
private async resolvePlanFilePathEagerly(dir: string): Promise<void> {
  // 1. 检查设计文件引用
  const lastUserText = this.getLatestUserMessageText();
  if (lastUserText !== null) {
    const designPath = extractDesignFileRefFromMessage(lastUserText);
    if (designPath !== null) {
      try {
        await this.agent.kaos.stat(designPath);
        const designStem = basename(designPath, '.md');
        const stem = await this.findUniqueStemInDir(dir, designStem);
        this._sessionModeFilePath = join(dir, `${stem}.md`);
        return;
      } catch (error) {
        if (!isMissingFileError(error)) throw error;
        this.agent.log?.warn('Referenced design file not found', { designPath });
      }
    }
  }

  // 2. 复用 _lastCompletedDesignFilePath（现有逻辑）
  if (this._lastCompletedDesignFilePath !== null) {
    const designBase = basename(this._lastCompletedDesignFilePath);
    const designStem = designBase.endsWith('.md') ? designBase.slice(0, -3) : designBase;
    this._lastCompletedDesignFilePath = null;
    const stem = await this.findUniqueStemInDir(dir, designStem);
    this._sessionModeFilePath = join(dir, `${stem}.md`);
    return;
  }

  // 3. LLM → 本地回退
  let slug: string | null = null;
  try {
    const generator = new TopicGenerator(this.agent, { maxWords: 5, maxLength: 50 });
    slug = await generator.generate();
  } catch (error) {
    this.agent.log?.warn('LLM topic extraction failed for plan mode', { error });
  }
  if (slug === null) {
    slug = this.topicSlugFromHistory();
  }
  if (slug === null) return;
  const datePrefix = formatDatePrefix(new Date());
  const stem = await this.findUniqueStemInDir(dir, `${datePrefix}-${slug}`);
  this._sessionModeFilePath = join(dir, `${stem}.md`);
}
```

### 修改点 4: `topic-generator.ts` 新增函数 [C:INFERRED]

**文件**: `packages/agent-core/src/agent/session-mode/topic-generator.ts`

```typescript
/**
 * 从用户消息中提取引用的设计文件路径。
 * 匹配 .ody-code/designs/*.md 模式（相对或绝对路径）。
 */
export function extractDesignFileRefFromMessage(text: string): string | null {
  const pattern = /(?:^|[\s"'`(])((?:\.\/|~\/|\/)?[^\s"'`(]*\.ody-code\/designs\/[^\s"'`(]+\.md)(?=[\s"'`)]|$)/gi;
  const match = pattern.exec(text);
  if (match === null) return null;
  return match[1].replace(/^["'`(]+|["'`)]+$/g, '');
}
```

**注意**: `TopicGenerator` 类需要从 `topic-generator.ts` 导入到 `index.ts`（当前只导入了函数，未导入类）。

---

## Error & Degradation

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| LLM timeout (3s) | `log.warn`，忽略错误 | 回退到 `topicSlugFromHistory()` 本地提取 | 本地提取成功则正常继续；否则推迟到 lazy resolution |
| LLM 返回空/无效主题 | `log.warn`，标记为失败 | 同上 | 同上 |
| 引用的设计文件不存在 (`ENOENT`) | `log.warn`，忽略该引用 | 回退到 `_lastCompletedDesignFilePath` 或 LLM/本地提取 | 后续路径解析成功则正常继续 |
| 引用的设计文件无权限访问 (`EACCES`) | 抛出错误（视为严重问题） | — | 用户修复文件权限后重试 |
| 磁盘上已有同名文件 | `findUniqueStemInDir` 添加 `-1`、`-2` 后缀 | 文件名带后缀，不影响内容 | 无（正常行为） |

---

## Test Plan

### 必须更新的现有测试

1. **`plan.test.ts:836` — "derives the eagerly-reserved path from the latest user message topic"** [C:INFERRED]
   - **变更**: 这是 plan 模式测试，现有断言 `expect(ctx.llmCalls).toHaveLength(0)` 仍应成立（plan 模式没有改成本次 design 模式的 LLM 提取）。
   - **实际**: 此测试**不需要修改**，因为 plan 模式的 eager resolution 仅在无设计文件引用且无 `_lastCompletedDesignFilePath` 时才调用 LLM，而测试场景（`add a billing module`）不满足这些条件。
   - **等等，重新分析**: plan 模式现在也会在有用户消息但无引用、无 `_lastCompletedDesignFilePath` 时调用 LLM。这个测试 seeded prompt 后直接 `enter('topic-plan')`——`_lastCompletedDesignFilePath` 为 null，也没有引用设计文件。按新逻辑，它会调用 LLM！
   - **结论**: 这个测试需要更新：mock LLM 响应或调整断言。

2. **新增 design 模式测试** [C:INFERRED]
   - **断言**: `await ctx.agent.sessionMode.enter('design-id', false, false, 'design')` 后，`ctx.llmCalls` 长度为 1。
   - **断言**: 生成的路径包含 LLM 返回的主题（如 `api-design`）而非 `continue`。

### 新增测试

3. **Design 模式 LLM 提取文件名** [C:USER]
   ```typescript
   const ctx = testAgent({ kaos: createPlanKaos() });
   ctx.configure();
   ctx.mockNextResponse({ type: 'text', text: 'sub-project-2-phase-d' });
   (ctx.agent.context.history as unknown[]).push({
     role: 'user', origin: { kind: 'user' },
     content: [{ type: 'text', text: 'continue Sub-project 2 Phase D' }],
   });
   await ctx.agent.sessionMode.enter('design-id', false, false, 'design');
   const today = formatDatePrefix(new Date());
   expect(ctx.agent.sessionMode.sessionModeFilePath).toBe(
     `/workspace/.ody-code/designs/${today}-sub-project-2-phase-d.md`,
   );
   expect(ctx.llmCalls).toHaveLength(1);
   ```

4. **Design 模式 LLM 失败回退** [C:USER]
   ```typescript
   const generate = vi.fn(async () => { throw new Error('timeout'); });
   const ctx = testAgent({ kaos: createPlanKaos(), generate });
   (ctx.agent.context.history as unknown[]).push({
     role: 'user', origin: { kind: 'user' },
     content: [{ type: 'text', text: 'continue Sub-project 2 Phase D' }],
   });
   await ctx.agent.sessionMode.enter('design-id', false, false, 'design');
   const today = formatDatePrefix(new Date());
   // Fallback: extractTopicFromMessage 取前5词 → "continue-sub-project-2-phase"
   expect(ctx.agent.sessionMode.sessionModeFilePath).toBe(
     `/workspace/.ody-code/designs/${today}-continue-sub-project-2-phase.md`,
   );
   ```

5. **Plan 模式引用设计文件路径** [C:USER]
   ```typescript
   const files = new Map<string, string>();
   files.set('/workspace/.ody-code/designs/2026-06-09-sub-project-2-phased.md', '# Design');
   const stat = vi.fn(async (path: string) => {
     if (files.has(path)) return { stMode: 0o100644 } as never;
     throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
   });
   const ctx = testAgent({ kaos: createPlanKaos({ stat }) });
   (ctx.agent.context.history as unknown[]).push({
     role: 'user', origin: { kind: 'user' },
     content: [{ type: 'text', text: '将设计 "/workspace/.ody-code/designs/2026-06-09-sub-project-2-phased.md" 转换为执行计划' }],
   });
   await ctx.agent.sessionMode.enter('plan-id', false, false, 'plan');
   expect(ctx.agent.sessionMode.sessionModeFilePath).toBe(
     '/workspace/.ody-code/plans/2026-06-09-sub-project-2-phased.md',
   );
   ```

6. **Plan 模式引用设计文件不存在时回退 LLM** [C:USER]
   ```typescript
   const stat = vi.fn(async () => {
     throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
   });
   const ctx = testAgent({ kaos: createPlanKaos({ stat }) });
   ctx.configure();
   ctx.mockNextResponse({ type: 'text', text: 'convert-design-to-plan' });
   (ctx.agent.context.history as unknown[]).push({
     role: 'user', origin: { kind: 'user' },
     content: [{ type: 'text', text: '将设计 "/workspace/.ody-code/designs/nonexistent.md" 转换为执行计划' }],
   });
   await ctx.agent.sessionMode.enter('plan-id', false, false, 'plan');
   const today = formatDatePrefix(new Date());
   expect(ctx.agent.sessionMode.sessionModeFilePath).toBe(
     `/workspace/.ody-code/plans/${today}-convert-design-to-plan.md`,
   );
   expect(ctx.llmCalls).toHaveLength(1);
   ```

### Done Criteria

```bash
# 测试必须通过
pnpm test packages/agent-core/test/agent/plan.test.ts
pnpm test packages/agent-core/test/agent/injection/design-mode.test.ts
pnpm test packages/agent-core/test/tools/enter-plan-mode.test.ts
pnpm test packages/agent-core/test/tools/enter-design-mode.test.ts

# TypeScript 编译必须通过
pnpm typecheck
```

---

## Self-Review

### Security
- **检查**: `extractDesignFileRefFromMessage` 是否可能误匹配或泄露敏感路径。
- **发现**: 正则严格限定 `.ody-code/designs/` 前缀和 `.md` 后缀，只取第一个匹配。不会泄露非设计文件路径。`TopicGenerator` 已有的敏感词过滤（`DEFAULT_SENSITIVE_WORDS`）继续生效。
- **修复**: 无。

### Test
- **检查**: 每个行为是否有 must-pass 和 must-reject 案例。
- **发现**:
  - must-pass: Design LLM 提取成功 → 文件名含 LLM 主题；Plan 引用存在的设计文件 → 复用其 stem。
  - must-reject: Design LLM 超时 → 回退本地提取（`continue-sub-project-2-phase` 而非 `continue`）；Plan 引用不存在的设计文件 → 回退 LLM/本地。
- **修复**: 更新了测试计划中的断言，明确 `plan.test.ts:836` 需要修改（plan 模式无引用无 `_lastCompletedDesignFilePath` 时也会调用 LLM）。

### Ops
- **检查**: 新增的 LLM 调用成本/延迟，文件名冲突行为。
- **发现**:
  - Design 模式进入时增加 1 次 LLM 调用（timeout 3s），延迟可接受。
  - Plan 模式引用设计文件时增加 1 次 `kaos.stat` 调用（本地文件系统，极快）。
  - `findUniqueStemInDir` 去重逻辑不变，`-1`、`-2` 后缀行为保持一致。
- **修复**: 无。

### Integration
- **检查**: 设计依赖的每个数据源/字段/事件/hook 是否实际存在。
- **发现**:
  - `TopicGenerator` 类存在于 `topic-generator.ts`，构造函数接收 `Agent` —— 已验证 ✅
  - `SessionMode.agent` 持有 `Agent` 实例 —— 已验证 ✅（`index.ts:38`）
  - `agent.generate()` 方法用于 LLM 调用 —— 已验证 ✅（`TopicGenerator.generate()` 使用它）
  - `agent.kaos.stat()` 用于文件存在性检查 —— 已验证 ✅
  - `_lastCompletedDesignFilePath` 在 `exit()` 第 172-173 行设置 —— 已验证 ✅
  - `basename(designPath, '.md')` —— `pathe` 的 `basename` 支持第二个参数 —— 已验证 ✅
- **修复**: 无。

### Scope
- **检查**: 是否仍是单一连贯设计，还是应分解。
- **发现**: 两个模式（design/plan）的文件名生成都属于 `SessionMode` 类的职责，修改集中在同一模块，是单一连贯设计。
- **修复**: 无。
