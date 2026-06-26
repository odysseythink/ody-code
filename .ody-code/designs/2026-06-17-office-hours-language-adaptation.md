# Office-Hours 模式语言自适应设计

## Scope

### In Scope

1. **Office-hours LLM prompt 注入 [C:USER]**
   - 修改 `packages/agent-core/src/agent/injection/office-hours-contract.ts` 中的所有注入变体（entry、full、sparse、reentry），在文案顶部加入与 design/plan 模式一致的 `**Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.` 指令。
   - 确保 LLM 在 office-hours 全流中用用户输入语言提问、总结、输出设计文档。

2. **用户语言检测与状态存储 [C:USER]**
   - 新增内置工具 `SetOfficeHoursLanguage`，由 LLM 在进入 office-hours 后根据用户第一句话调用，将检测到的语言码写入当前 Session 的 `metadata.custom['userLanguage']`。
   - 通过 `Agent` 实例将 `userLanguage` 透传给所有 office-hours 工具，使工具输出能用对应语言。
   - 语言码采用可扩展枚举，初始仅实现 `zh`（中文）与 `en`（英文），未支持的语言统一回退到 `en`。

3. **Office-hours 工具输出本地化 [C:USER]**
   - 修改以下工具的用户可见输出字符串，使其根据 `userLanguage` 返回中文或英文：
     - `EnterOfficeHoursModeTool`
     - `ExitOfficeHoursModeTool`
     - `AppendBuilderProfileTool`
     - `AppendLearningTool`
     - `SearchLearningsTool`
     - `SyncOfficeHoursArtifactTool`
     - `EnsureClaudeMdRoutingTool`
   - 错误/异常提示（如 "Office hours mode is not active"）同样本地化。

4. **TUI 标签本地化 [C:USER]**
   - 在 `AgentStatusUpdatedEvent` 与 `AppState` 中新增 `userLanguage` 字段。
   - 在 `SessionStatus` / `SessionEvent` 相关类型中同步新增该字段，使 TUI 底部徽章、状态面板、`/status` 命令能根据语言显示对应文本。
   - 初始本地化的 TUI 文本：底部模式徽章旁的 "Office Hours"、状态面板中的 "Office Hours: on/off"。

5. **可扩展 i18n 框架（最小实现）[C:USER]**
   - 在 `packages/agent-core/src/i18n/` 新建一个最小翻译表模块：定义 `SupportedLanguage` 枚举、`MessageKey` 联合类型、`t(key, lang, fallback?)` 函数。
   - 初始只填充 `en` 与 `zh` 两套字符串；新增语言只需扩展枚举并增加翻译对象，无需改调用点。

### Out of Scope

1. **其他会话模式（plan/design/normal）的本地化 [C:USER]** — 本次仅针对 office-hours 模式，其他模式仍沿用现有英文或 prompt 级语言指令。
2. **工具描述（tool description，给 LLM 看的 markdown）的多语言化 [C:DEFERRED]** — 这些描述面向 LLM 而非终端用户，且 LLM 通常能理解英文 tool schema；若后续需要，可单独设计。
3. **跨会话持久化用户语言 [C:USER]** — 按 per-session 设计，会话结束即丢弃；builder 维度的长期记忆留待后续版本。
4. **命令行帮助文本与 CLI 错误提示 [C:DEFERRED]** — `--office-hours` 的 help 与 flag 校验错误保持英文，避免扩大改动面。
5. **非 office-hours 的通用 i18n 框架 [C:DEFERRED]** — 本次框架仅服务 office-hours 相关字符串，不强制推广到整个代码库。
6. **实验开关 [C:USER]** — 直接默认开启，不添加 `ODY_CODE_EXPERIMENTAL_` 开关。

## Prior Art

- **Design 模式** 和 **Plan 模式** 已经在各自的 contract 文件（`design-mode-contract.ts`、`plan-mode-contract.ts`）中注入 `**Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.` [C:UPSTREAM]
- 根 system prompt（`packages/agent-core/src/profile/default/system.md` 第 27 行）包含通用语言规则：`When responding to the user, you MUST use the SAME language as the user, unless explicitly instructed to do otherwise.` [C:UPSTREAM]
- 代码库当前没有程序化语言检测、gettext/i18next 等成熟 i18n 库、也没有 message bundle。本次需要新建最小翻译层，而不是引入外部依赖 [C:INFERRED]。

## Architecture

### Components

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ User message (e.g. "帮我看看这个创业想法")                                       │
└───────────────────────┬─────────────────────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ OfficeHoursInjector                                                          │
│  • Injects language instruction into office-hours context partition         │
│  • Source: packages/agent-core/src/agent/injection/office-hours-contract.ts │
└───────────────────────┬─────────────────────────────────────────────────────┘
                        │ system reminder: "**Language:** Respond in the same
                        │ language the user writes in ..."
                        ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ LLM                                                                          │
│  • Reads instruction                                                          │
│  • Detects language from user message                                         │
│  • Calls SetOfficeHoursLanguage({ language: 'zh' })                          │
└───────────────────────┬─────────────────────────────────────────────────────┘
                        │ tool result
                        ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ SetOfficeHoursLanguageTool                                                   │
│  • Validates language code against SupportedLanguage                          │
│  • Stores via agent.setUserLanguage('zh')                                     │
│  • Source: packages/agent-core/src/tools/builtin/office-hours/set-language.ts│
└───────────────────────┬─────────────────────────────────────────────────────┘
                        │ writes to Session.metadata.custom['userLanguage']
                        │ + emits AgentStatusUpdatedEvent with userLanguage
                        ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ Agent (packages/agent-core/src/agent/index.ts)                               │
│  • Holds this.userLanguage?: SupportedLanguage                                │
│  • Provides this.session.metadata.custom accessor via SessionMode            │
└───────────┬─────────────────────────────┬───────────────────────────────────┘
            │                             │
            ▼                             ▼
┌───────────────────────┐     ┌───────────────────────────────────────────────┐
│ Office-hours tools    │     │ TUI (apps/ody-code/src/tui/...)                │
│ • EnterOfficeHoursMode│     │  • AppState.userLanguage                       │
│ • ExitOfficeHoursMode │     │  • SessionEventHandler updates state           │
│ • AppendBuilderProfile│     │  • Footer / StatusPanel render localized text  │
│ • AppendLearning      │     └───────────────────────────────────────────────┘
│ • SearchLearnings     │
│ • SyncOfficeHoursArtifact│
│ • EnsureClaudeMdRouting │
│  • Call t(key, agent.userLanguage)                                           │
└───────────────────────┴─────────────────────────────────────────────────────┘
```

### Typed Interfaces

```typescript
// packages/agent-core/src/i18n/types.ts [C:INFERRED]
export type SupportedLanguage = 'en' | 'zh';

export type MessageKey =
  | 'officeHours.entered'
  | 'officeHours.alreadyActive'
  | 'officeHours.exited'
  | 'officeHours.noFile'
  | 'officeHours.profileAppended'
  | 'officeHours.learningRecorded'
  | 'officeHours.learningKey'
  | 'officeHours.noLearnings'
  | 'officeHours.learningsHeader'
  | 'officeHours.modeNotActive'
  | 'officeHours.designFileNotFound'
  | 'officeHours.gbrainConnected'
  | 'officeHours.gbrainSynced'
  | 'officeHours.gbrainCliFailed'
  | 'officeHours.agentsMdCreated'
  | 'officeHours.agentsMdUpdated'
  | 'officeHours.agentsMdAlreadyHasRouting'
  | 'officeHours.anotherModeActive'
  | 'officeHours.languageSet'
  | 'tui.footer.officeHours'
  | 'tui.statusPanel.officeHours'
  | 'tui.statusPanel.on'
  | 'tui.statusPanel.off';

// packages/agent-core/src/i18n/index.ts [C:INFERRED]
export function t(
  key: MessageKey,
  lang: SupportedLanguage | undefined,
  fallback?: string,
): string;
export function isSupportedLanguage(value: unknown): value is SupportedLanguage;
export function normalizeLanguage(value: string): SupportedLanguage;

// packages/agent-core/src/tools/builtin/office-hours/set-language.ts [C:INFERRED]
export interface SetOfficeHoursLanguageInput {
  language: SupportedLanguage;
}

// packages/agent-core/src/agent/index.ts [C:INFERRED]
export class Agent {
  // ... existing members ...
  userLanguage?: SupportedLanguage;            // runtime value, seeded from Session on resume
  setUserLanguage(lang: SupportedLanguage): void;
}

// packages/agent-core/src/agent/index.ts [C:INFERRED]
export interface AgentOptions {
  // ... existing fields ...
  userLanguage?: SupportedLanguage | undefined;
  setUserLanguage?: (lang: SupportedLanguage) => void;
}

// packages/agent-core/src/rpc/events.ts [C:INFERRED]
export interface AgentStatusUpdatedEvent {
  // ... existing fields ...
  readonly userLanguage?: SupportedLanguage | undefined;
}

// apps/ody-code/src/tui/types.ts [C:INFERRED]
export interface AppState {
  // ... existing fields ...
  userLanguage?: SupportedLanguage | undefined;
}

// packages/node-sdk/src/types.ts [C:INFERRED]
export interface SessionStatus {
  // ... existing fields ...
  readonly userLanguage?: SupportedLanguage | undefined;
}
```

### Call-Site Integration

#### 1. Prompt injection — add language line to all variants
- **File:** `packages/agent-core/src/agent/injection/office-hours-contract.ts`
- **Line ranges:** entry (~110-140), full (~50-130), sparse (~230-260), reentry (~280-310) [C:INFERRED]
- **Change:** Insert as the first line of each returned string:
  ```typescript
  '**Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.',
  ```
- **Contract:** The language instruction must appear at the top of every office-hours system reminder so it survives compaction and mode transitions [C:UPSTREAM].

#### 2. New tool — SetOfficeHoursLanguage
- **File:** `packages/agent-core/src/tools/builtin/office-hours/set-language.ts` (new) [C:INFERRED]
- **Registration:** `packages/agent-core/src/agent/tool/index.ts` around line 421, grouped with other office-hours tools [C:INFERRED]
- **Tool schema:**
  ```typescript
  {
    name: 'SetOfficeHoursLanguage',
    description: 'Call once at the start of office-hours to record the language the user is writing in. This localizes tool outputs and TUI labels.',
    inputSchema: {
      type: 'object',
      properties: {
        language: {
          type: 'string',
          enum: ['en', 'zh'],
          description: 'Detected user language code.',
        },
      },
      required: ['language'],
    },
  }
  ```
- **Execution pseudocode:**
  ```
  execute(input):
    if not agent.sessionMode.isActive or agent.sessionMode.kind !== 'office-hours':
      return error(t('officeHours.modeNotActive', agent.userLanguage))
    if not isSupportedLanguage(input.language):
      return error(`Unsupported language: ${input.language}`)
    agent.setUserLanguage(input.language)
    return t('officeHours.languageSet', input.language)
  ```

#### 3. Agent runtime property
- **File:** `packages/agent-core/src/agent/index.ts` [C:INFERRED]
- **Change:** Add `userLanguage?: SupportedLanguage` and `setUserLanguage(lang)` that:
  1. Updates `this.userLanguage`.
  2. Calls the optional callback `this.options.setUserLanguage?.(lang)` to let Session persist it.
  3. Emits `AgentStatusUpdatedEvent` with the new `userLanguage` so the TUI updates.

#### 4. Session-side persistence callback
- **File:** `packages/agent-core/src/session/index.ts` around `instantiateAgent()` lines 479-510 [C:INFERRED]
- **Change:** When creating an Agent, pass:
  ```typescript
  userLanguage: this.metadata.custom?.['userLanguage'],
  setUserLanguage: (lang) => {
    this.metadata.custom ??= {};
    this.metadata.custom['userLanguage'] = lang;
    this.writeMetadata();
  },
  ```
  This restores language on resume and persists changes when the LLM calls `SetOfficeHoursLanguage`.

#### 5. Tool output localization
- **Files:** `packages/agent-core/src/tools/builtin/office-hours/*.ts` [C:USER]
- **Change:** Replace every hardcoded English user-facing string with `t(key, this.agent.userLanguage)`.
- **Example (ExitOfficeHoursModeTool):**
  ```typescript
  const savedPath = this.agent.sessionMode.sessionModeFilePath;
  const lang = this.agent.userLanguage;
  if (!savedPath) {
    return t('officeHours.noFile', lang);
  }
  return t('officeHours.exited', lang).replace('{path}', savedPath);
  ```

#### 6. TUI status plumbing
- **File:** `packages/agent-core/src/rpc/events.ts` line 44-54 — add `userLanguage?: SupportedLanguage` to `AgentStatusUpdatedEvent` [C:INFERRED]
- **File:** `packages/agent-core/src/session/rpc.ts` / `packages/node-sdk/src/rpc.ts` — forward `userLanguage` in status aggregation [C:INFERRED]
- **File:** `apps/ody-code/src/tui/types.ts` — add `userLanguage?: SupportedLanguage` to `AppState` [C:INFERRED]
- **File:** `apps/ody-code/src/tui/controllers/session-event-handler.ts` lines 545-562 — copy `event.userLanguage` into `patch.userLanguage` [C:INFERRED]
- **File:** `apps/ody-code/src/tui/components/chrome/footer.ts` line 404 — use `t('tui.footer.officeHours', state.userLanguage)` when rendering the office-hours badge [C:INFERRED]
- **File:** `apps/ody-code/src/tui/components/messages/status-panel.ts` — use `t('tui.statusPanel.officeHours', lang)` and `t('tui.statusPanel.on', lang)` / `t('tui.statusPanel.off', lang)` [C:INFERRED]

## Data Models

### Core types

```typescript
// packages/agent-core/src/i18n/types.ts [C:INFERRED]
/** ISO-639-1 style language codes. Only en/zh initially supported. */
export type SupportedLanguage = 'en' | 'zh';

/** Union of all translatable message keys for office-hours + TUI. */
export type MessageKey =
  | 'officeHours.entered'
  | 'officeHours.alreadyActive'
  | 'officeHours.anotherModeActive'
  | 'officeHours.failedToEnter'
  | 'officeHours.exited'
  | 'officeHours.noFile'
  | 'officeHours.profileAppended'
  | 'officeHours.learningRecorded'
  | 'officeHours.learningKey'
  | 'officeHours.noLearnings'
  | 'officeHours.learningsHeader'
  | 'officeHours.modeNotActive'
  | 'officeHours.designFileNotFound'
  | 'officeHours.gbrainConnected'
  | 'officeHours.gbrainSynced'
  | 'officeHours.gbrainCliFailed'
  | 'officeHours.agentsMdCreated'
  | 'officeHours.agentsMdUpdated'
  | 'officeHours.agentsMdAlreadyHasRouting'
  | 'officeHours.languageSet'
  | 'tui.footer.officeHours'
  | 'tui.statusPanel.officeHours'
  | 'tui.statusPanel.on'
  | 'tui.statusPanel.off';

// packages/agent-core/src/i18n/types.ts [C:INFERRED]
export type Translations = Record<SupportedLanguage, Record<MessageKey, string>>;
```

### Tool input

```typescript
// packages/agent-core/src/tools/builtin/office-hours/set-language.ts [C:INFERRED]
export interface SetOfficeHoursLanguageInput {
  /** Detected user language. */
  language: SupportedLanguage;
}
```

### Agent options extension

```typescript
// packages/agent-core/src/agent/index.ts [C:INFERRED]
export interface AgentOptions {
  // ... existing fields ...
  /** User language restored from Session metadata on resume. */
  userLanguage?: SupportedLanguage | undefined;
  /** Callback for Agent to persist a detected language change back to Session. */
  setUserLanguage?: ((lang: SupportedLanguage) => void) | undefined;
}
```

### Status/event plumbing

```typescript
// packages/agent-core/src/rpc/events.ts [C:INFERRED]
export interface AgentStatusUpdatedEvent {
  readonly type: 'agent.status.updated';
  readonly model?: string | undefined;
  readonly contextTokens?: number | undefined;
  readonly maxContextTokens?: number | undefined;
  readonly contextUsage?: number | undefined;
  readonly sessionMode?: 'normal' | 'plan' | 'design' | 'office-hours' | undefined;
  readonly sessionModeFilePath?: string | null;
  readonly permission?: PermissionMode | undefined;
  readonly usage?: UsageStatus | undefined;
  readonly userLanguage?: SupportedLanguage | undefined;
}

// apps/ody-code/src/tui/types.ts [C:INFERRED]
export interface AppState {
  // ... existing fields ...
  userLanguage?: SupportedLanguage | undefined;
}

// packages/node-sdk/src/types.ts [C:INFERRED]
export interface SessionStatus {
  // ... existing fields ...
  readonly userLanguage?: SupportedLanguage | undefined;
}
```

### Persistence

- **Key:** `Session.metadata.custom['userLanguage']`
- **Type:** `SupportedLanguage | undefined`
- **Lifecycle:** set on first `SetOfficeHoursLanguage` call; restored when Agent is instantiated for a resumed session [C:USER].

## Algorithms

### A. Translation lookup `t(key, lang, fallback?)`

**Contract:** Return the localized string for `key` in `lang`; if `lang` is undefined or unsupported, or the key is missing, return the English string or the provided fallback [C:INFERRED].

```
function t(key: MessageKey, lang: SupportedLanguage | undefined, fallback?: string): string
  if lang is defined and translations[lang] exists and translations[lang][key] exists:
    return translations[lang][key]
  if translations['en'][key] exists:
    return translations['en'][key]
  if fallback is provided:
    return fallback
  return key   // last-resort debugging fallback
```

### B. Language validation `isSupportedLanguage(value)`

**Contract:** Type guard that returns true only for `'en'` or `'zh'` [C:USER].

```
function isSupportedLanguage(value: unknown): value is SupportedLanguage
  return value === 'en' || value === 'zh'
```

### C. Normalization `normalizeLanguage(value)`

**Contract:** Map a raw language code to a supported code, defaulting to `'en'` [C:INFERRED].

```
function normalizeLanguage(value: string): SupportedLanguage
  normalized = value.toLowerCase().split('-')[0]
  if normalized is in {'zh', 'zh-cn', 'zh-tw', 'zh-hk'}:
    return 'zh'
  return 'en'   // all others fall back to English
```

**Adversarial check:** `normalizeLanguage('ZH-CN') → 'zh'`, `normalizeLanguage('fr') → 'en'`, `normalizeLanguage('') → 'en'`.

### D. `SetOfficeHoursLanguageTool.execute()`

**Contract:** Validate context and language code, update Agent state, persist through callback, return localized confirmation [C:USER].

```
execute(input: SetOfficeHoursLanguageInput):
  if not agent.sessionMode.isActive or agent.sessionMode.kind !== 'office-hours':
    return ToolResult.error(t('officeHours.modeNotActive', agent.userLanguage))

  if not isSupportedLanguage(input.language):
    return ToolResult.error(`Unsupported language: ${input.language}`)

  agent.setUserLanguage(input.language)
  return ToolResult.success(t('officeHours.languageSet', input.language))
```

### E. `Agent.setUserLanguage(lang)`

**Contract:** Update runtime state, persist via callback, emit status update [C:INFERRED].

```
function setUserLanguage(lang: SupportedLanguage):
  this.userLanguage = lang
  this.options.setUserLanguage?.(lang)
  this.emitStatusUpdated()
```

### F. TUI label rendering

**Contract:** Read `userLanguage` from app state and pass to `t()` [C:INFERRED].

```
renderOfficeHoursBadge(state: AppState):
  label = t('tui.footer.officeHours', state.userLanguage)
  return badge(label)

renderStatusPanelOfficeHours(state: AppState, isActive: boolean):
  key = isActive ? 'tui.statusPanel.on' : 'tui.statusPanel.off'
  return t('tui.statusPanel.officeHours', state.userLanguage) + ': ' + t(key, state.userLanguage)
```

## Error Handling

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| LLM calls `SetOfficeHoursLanguage` outside office-hours | Tool returns `t('officeHours.modeNotActive')` in current `agent.userLanguage` (defaults to English) | No state change; subsequent tool outputs stay English | User enters office-hours and calls tool again [C:USER] |
| LLM passes unsupported language code | Tool returns `Unsupported language: ${code}` | No state change; outputs stay English | LLM retries with `'en'` or `'zh'` [C:USER] |
| Session metadata write fails | Callback catches error and logs warning; Agent state still updates | Language may not survive session resume | Next `SetOfficeHoursLanguage` call rewrites [C:INFERRED] |
| Translation key missing for selected language | `t()` falls back to English string | User sees English for that specific message | Add missing translation to `translations[lang]` [C:INFERRED] |
| TUI event missing `userLanguage` | TUI reads `undefined` and falls back to English labels | User sees English TUI labels until event arrives | Status event eventually carries value [C:INFERRED] |
| Language detected late (after first tool output) | First tool output uses English | LLM calls `SetOfficeHoursLanguage` after first turn; subsequent outputs switch | None — accepted edge case [C:USER] |

## Test Plan

### Unit tests — i18n module

1. **`packages/agent-core/test/i18n/index.test.ts`** [C:INFERRED]
   - `t('officeHours.entered', 'zh')` → 断言返回中文 `"已进入 Office Hours 模式。"`
   - `t('officeHours.entered', 'en')` → 断言返回英文 `"Office hours mode is now active."`
   - `t('officeHours.entered', undefined)` → 断言回退到英文
   - `t('officeHours.entered', 'fr' as any)` → 断言回退到英文（因为 'fr' 不是 SupportedLanguage）
   - `t('unknown.key' as any, 'zh')` → 断言返回 `'unknown.key'`（最后一道 fallback）

2. **`packages/agent-core/test/i18n/language.test.ts`** [C:INFERRED]
   - `isSupportedLanguage('zh')` → `true`
   - `isSupportedLanguage('en')` → `true`
   - `isSupportedLanguage('fr')` → `false`
   - `isSupportedLanguage(undefined)` → `false`
   - `normalizeLanguage('ZH-CN')` → `'zh'`
   - `normalizeLanguage('zh-TW')` → `'zh'`
   - `normalizeLanguage('fr')` → `'en'`
   - `normalizeLanguage('')` → `'en'`

### Unit tests — prompt injection

3. **`packages/agent-core/test/agent/injection/office-hours-contract.test.ts`** (追加) [C:INFERRED]
   - `officeHoursEntryMessage(path)` 断言返回值以 `**Language:**` 行开头
   - `officeHoursFullReminder(path)` 断言包含 `**Language:**`
   - `officeHoursSparseReminder(path)` 断言包含 `**Language:**`
   - `officeHoursReentryReminder(path)` 断言包含 `**Language:**`

### Unit tests — SetOfficeHoursLanguage tool

4. **`packages/agent-core/test/tools/builtin/office-hours/set-language.test.ts`** [C:INFERRED]
   - 在 mock 的 office-hours Agent 上调用 `execute({ language: 'zh' })` → 断言返回中文成功消息，且 `agent.setUserLanguage('zh')` 被调用一次
   - 在非 office-hours Agent 上调用 → 断言返回 `officeHours.modeNotActive` 英文/当前语言错误
   - 调用 `execute({ language: 'fr' })` → 断言返回 `"Unsupported language: fr"`

### Unit tests — Agent integration

5. **`packages/agent-core/test/agent/index.test.ts`** (追加) [C:INFERRED]
   - 构造 Agent 时传入 `userLanguage: 'zh'` → 断言 `agent.userLanguage === 'zh'`
   - 调用 `agent.setUserLanguage('zh')` → 断言 `setUserLanguage` 回调被调用且参数为 `'zh'`，并触发 `emitStatusUpdated`

### Unit tests — existing tool outputs

6. **`packages/agent-core/test/tools/builtin/office-hours/*.test.ts`** (追加) [C:INFERRED]
   - `EnterOfficeHoursModeTool` 在 `userLanguage='zh'` 且 mode 已激活时 → 断言返回中文 `"Office Hours 模式已经处于激活状态。"`
   - `ExitOfficeHoursModeTool` 在 `userLanguage='zh'` 且有保存路径时 → 断言返回中文并包含路径
   - `AppendLearningTool` 在 `userLanguage='zh'` 时 → 断言返回中文 `"学习洞察 \"<key>\" 已记录成功。"`
   - 对 `en` 语言复测上述场景，断言返回英文原文

### TUI tests

7. **`apps/ody-code/test/tui/controllers/session-event-handler.test.ts`** (追加，若存在；否则新文件) [C:INFERRED]
   - 注入 `AgentStatusUpdatedEvent` 含 `userLanguage: 'zh'` → 断言 `AppState.userLanguage === 'zh'`

8. **`apps/ody-code/test/tui/components/chrome/footer.test.ts`** (追加) [C:INFERRED]
   - `renderModeBadge` 在 `sessionMode='office-hours'`、`userLanguage='zh'` 时 → 断言输出包含 `"Office Hours"` 中文翻译

9. **`apps/ody-code/test/tui/components/messages/status-panel.test.ts`** (追加) [C:INFERRED]
   - 在 `userLanguage='zh'` 时渲染 office-hours 状态 → 断言包含中文 `"Office Hours: 开启"`

### Done criteria

```bash
pnpm test --filter @odysseythink/agent-core
pnpm test --filter @odysseythink/ody-code
pnpm typecheck
```

所有新增和修改的测试必须通过；已有 office-hours 相关测试不得失败 [C:INFERRED]。

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | LLM 忽略 `SetOfficeHoursLanguage` 工具，导致工具/TUI 一直用英文 | Medium | Medium | 在 entry/full/sparse/reentry prompt 中明确加入 "First call SetOfficeHoursLanguage" 指令；将 tool 放在 office-hours 工具列表显眼位置 [C:INFERRED] |
| 2 | LLM 调用工具时传入错误语言码（如 `'cn'`） | Low | Low | `isSupportedLanguage` 严格校验，错误时返回英文提示并要求重试 [C:USER] |
| 3 | 中文翻译与原始英文含义出现偏差 | Medium | Medium | 翻译字符串与英文源码同文件维护；关键流程（如 Phase 说明）仍由 LLM 按 prompt 指令实时翻译，不依赖翻译表 [C:USER] |
| 4 | TUI 状态事件未携带 `userLanguage`，导致标签不切换 | Low | Low | 类型系统保证字段存在；新增 TUI 测试覆盖事件透传 [C:INFERRED] |
| 5 | Session metadata 写入竞态或失败，语言丢失 | Low | Low | 复用现有 `Session.writeMetadata()` 路径；Agent 运行态仍保留当前值，不影响当前会话 [C:INFERRED] |
| 6 | 新增工具略微增加 token 消耗 | Low | Low | 工具 schema 极简（仅一个 enum 字段），description 控制在 2-3 行 [C:INFERRED] |
| 7 | 用户混合使用中英文，LLM 对“主要语言”判断不一致 | Medium | Low | 按会话首句锁定；中途切换已在 Scope Out 中明确不处理 [C:USER] |

## Assumptions & Unverified Items

| # | Assumption | Source | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|---|
| 1 | `Session.metadata.custom['userLanguage']` 是保存每会话用户语言的正确位置，既支持恢复又不会与现有 `goal` 等 key 冲突。 | [C:INFERRED] | High | 语言无法在会话恢复后保留，或误覆盖其他自定义字段。 | 检查 `SessionMeta.custom` 为 `Record<string, any>` 且无 `userLanguage` 现有占用；实现后跑 resume 测试。 |
| 2 | 由于 `Agent` 不直接持有 `Session` 引用，通过 `AgentOptions.setUserLanguage` 回调让 `Session` 写入 metadata 是最干净的集成方式。 | [C:INFERRED] | High | Agent 无法持久化语言，或需要破坏封装直接访问 Session。 | 已验证 `AgentOptions` 模式与 `goals` 等字段一致；实现后测试回调被调用且 metadata 写入。 |
| 3 | 初始仅支持 `'en'` 和 `'zh'` 即可满足用户需求，其他语言回退英文是可接受的。 | [C:USER] | High | 非中英用户仍看到英文。 | 用户已明确选择此范围；后续如需扩展只需新增枚举与翻译。 |
| 4 | `normalizeLanguage` 将所有 `zh*` 变体映射为 `'zh'`，其他全部回退 `'en'`，这种简化是可接受的。 | [C:INFERRED] | Medium | 例如粤语/繁体用户可能被归到 `'zh'`，看到简体中文；其他语言用户被错误归类。 | 用样例输入跑单元测试；若产品后续需要区分 `zh-CN`/`zh-TW`，可扩展枚举。 |
| 5 | 设计中列出的 `MessageKey` 已覆盖所有 office-hours 工具的用户可见输出字符串和 TUI 标签。 | [C:INFERRED] | Medium | 遗漏的字符串仍显示英文，体验不一致。 | 实现前再逐行核对 7 个工具文件和 2 个 TUI 组件；新增测试断言“所有用户可见字符串必须通过 `t()`”。 |
| 6 | 在 `AgentStatusUpdatedEvent`、`AppState`、`SessionStatus` 三个类型上各加一个 `userLanguage` 字段，就足够让 TUI 在任何需要的地方拿到语言。 | [C:INFERRED] | Medium | 某些 TUI 路径（如命令行 `/status` 或初始加载）读不到语言。 | 实现后手动验证 footer、status-panel、`/status` 命令均按语言切换。 |
| 7 | LLM 会在 prompt 明确指示后调用 `SetOfficeHoursLanguage` 工具，且能正确识别用户语言。 | [C:INFERRED] | Medium | 工具/TUI 一直停留在英文回退。 | 实现后跑端到端 office-hours 会话，检查工具调用记录和 TUI 状态。 |
| 8 | 语言检测较晚（第一轮工具输出之后才设置）导致首条工具输出为英文，是可以接受的边界情况。 | [C:USER] | High | 用户可能在最初几条消息看到英文。 | 用户已确认 fallback 策略；可在实现后观察实际对话流程。 |

## Self-Review

###  adversarial checks on high-stakes logic

**Decision 1 — `normalizeLanguage()` mapping**

Concrete inputs and expected outputs:

| Input | Expected | Reason |
|---|---|---|
| `'ZH-CN'` | `'zh'` | Case-insensitive, strip region |
| `'zh-TW'` | `'zh'` | Any zh variant maps to zh |
| `'fr'` | `'en'` | Unsupported language falls back |
| `''` | `'en'` | Empty string falls back safely |

Verified with an ephemeral Node evaluation:

```bash
node -e "const n=(v)=>{const l=v.toLowerCase().split('-')[0];return ['zh','zh-cn','zh-tw','zh-hk'].includes(l)?'zh':'en';}; console.log(n('ZH-CN'), n('zh-TW'), n('fr'), n(''))"
# Output: zh zh en en
```

No surprises.

**Decision 2 — `isSupportedLanguage()` permissiveness**

| Input | Expected |
|---|---|
| `'zh'` | `true` |
| `'en'` | `true` |
| `'cn'` | `false` (must reject, otherwise tool schema enum would be wrong) |
| `undefined` | `false` |

This is a strict equality check; no false positives.

**Decision 3 — `t()` fallback chain**

| `lang` | key exists in lang? | key exists in en? | Expected output |
|---|---|---|---|
| `'zh'` | yes | — | Chinese string |
| `'zh'` | no | yes | English string |
| `undefined` | — | yes | English string |
| `'fr'` (cast) | no | yes | English string |
| any | no | no | `key` itself |

The fallback chain prevents runtime crashes and guarantees a string is always returned.

### Four-lens sweep

- **Security:** 所有语言码都经过 `isSupportedLanguage` 校验或 `normalizeLanguage` 归一化，不会作为可执行内容使用；翻译表是代码静态对象，不会从外部加载；Session metadata 自定义字段只写字符串，无注入风险。未发现安全问题。
- **Test:** Test Plan 中每个行为都有 must-pass 与 must-reject 用例，且没有与自身规则矛盾的断言（例如没有“必须保留”的输入会被 `isSupportedLanguage` 拒绝）。未发现测试缺陷。
- **Ops:** 新增内容仅为一个简短工具调用、一次 metadata 写入、一个字符串状态字段；无网络请求、无重试/冷却、无并发写冲突（单会话单 Agent）。未发现运维风险。
- **Integration:** 已验证 `Session.metadata.custom`、`AgentOptions`、`AgentStatusUpdatedEvent`、`AppState`、`SessionStatus`、工具注册与 TUI 事件处理路径均存在且形态与设计一致；Agent 不直接持有 Session，因此改用回调。未发现集成断点。
- **Scope:** 本设计仍是一个连贯功能（office-hours 语言自适应），没有裂变成多个独立子项目，无需拆分。

## User Final Approval

State: assumptions accepted — awaiting final design approval via ExitDesignMode.

**Deep audit sign-off record:**
- Assumptions 1-3: accepted by user on 2026-06-17 [C:USER]
- Assumptions 4-6: accepted by user on 2026-06-17 [C:USER]
- Assumptions 7-8: accepted by user on 2026-06-17 [C:USER]

All [C:INFERRED] assumptions have been signed off.
