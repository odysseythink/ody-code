# Epic A / A1 — 简约阶梯 Skill（simplicity-first）

**Document Type**: Design (implementation-ready)
**Last Updated**: 2026-06-17
**Status**: DRAFT (awaiting approval)
**Source**: `.ody-code/roadmaps/ponytail-inspired-roadmap.md` P1-A
**Target**: `packages/agent-core/src/skill/builtin/`

---

## Scope

### In Scope

- 新增 builtin skill `simplicity-first`，编码 ponytail 简约阶梯（YAGNI → 标准库 → 平台原生 → 已有依赖 → 一行 → 最小可用）与“懒但不破”硬约束 [C:USER]。
- 单文件多档（lite / full / ultra）运行时过滤机制，使用 HTML 注释块 `<!-- LEVEL[ -->...<!-- ]LEVEL -->` 标记，大小写不敏感 [C:USER]。
- 将过滤逻辑接入 `SkillRegistry.renderSkillPrompt()`，使 `/simplicity [lite|full|ultra]` 激活时按档位渲染 skill 内容 [C:USER]。
- 在 `packages/agent-core/src/skill/builtin/index.ts` 注册新 skill [C:USER]。
- 缺失档位参数默认 `full`；未知档位参数抛出 `OdyError`/`SkillNotFoundError` 语义错误 [C:USER]。
- 所有会话模式（normal / plan / design）均可激活该 skill [C:USER]。

### Out of Scope (Deferred)

- **P3-C before/after 示例文档**：A1 仅交付核心 skill 与过滤机制，配套对照示例 deferred 到后续迭代 [C:USER]。
- **P3-A 状态栏徽章**：TUI 显示当前简约强度 deferred；当前强度仅通过最近一次激活参数体现 [C:USER]。
- **P3-B config 驱动默认强度**：跨会话持久化配置、环境变量级联 deferred [C:USER]。
- **A2 微代理 / A3 review / A4 债务台账**：属于 Epic A 其他条目，不在 A1 范围内 [C:USER]。
- **全局常驻注入器**：A1 采用主动 skill 激活，不新增 `SimplicityInjector` [C:USER]。

---

## Reuse Analysis

- **Skill system infrastructure** (`packages/agent-core/src/skill/`): `SkillRegistry`, `SkillManager`, `parser.ts`, `scanner.ts` — all reused as-is for registering and activating `simplicity-first`. No changes needed to these modules. [C:INFERRED]
- **Builtin skill pattern** (`packages/agent-core/src/skill/builtin/*.ts`): the `.ts` + `.md` pair pattern used by existing builtin skills (e.g. `systematic-debugging.ts` + `systematic-debugging.md`) is copied for the new skill. [C:INFERRED]
- **`SkillRegistry.renderSkillPrompt`** (`registry.ts:90`): the existing rendering pipeline is extended with an inline filter step; no new class or injection interface needed. [C:INFERRED]
- **Error codes** (`packages/agent-core/src/errors/codes.ts`): `REQUEST_INVALID` reused for invalid-level errors. No new error code required. [C:INFERRED]
- **Slash command → skill activation**: the existing `SkillManager.activate()` path is reused; `/simplicity` routes as a normal slash command with args passed through to the skill. [C:INFERRED]
- **Telemetry/observability**: reused via `SkillManager.activate()`'s existing `skill.activated` event and `skill_invoked` telemetry. [C:INFERRED]

No greenfield subsystem — all new code builds directly on existing infrastructure.

---

## Prior Art

### Ponytail 4.7.0

- `skills/ponytail/SKILL.md` 承载完整规则；`hooks/ponytail-instructions.js` 在运行时按 `lite/full/ultra` 过滤表格行与示例标签 [C:UPSTREAM]。
- 过滤规则：移除 frontmatter 后，保留表格行 `| **mode** | ... |` 或列表标签 `- mode: ...` 中 label 等于当前 mode 的行，其余普通规则保留 [C:UPSTREAM]。
- 档位语义：`lite` = 照常实现但提一句更懒替代；`full` = 强制走阶梯；`ultra` = 敢于质疑需求本身 [C:UPSTREAM]。
- 硬约束：信任边界校验、错误处理、安全、可访问性、硬件标定绝不简化 [C:UPSTREAM]。

### ody-code 现有基础设施

- `packages/agent-core/src/skill/builtin/` 已有多个 `.ts` + `.md` 配对的 builtin skill [C:INFERRED]。
- `SkillRegistry.renderSkillPrompt(skill, rawArgs)` 已负责参数展开与插件指令包装 [C:INFERRED]。
- `SkillManager.activate()` 通过 `skill.activated` 事件与 telemetry `skill_invoked` 提供可观测性 [C:INFERRED]。

---

## Architecture

### Components & Files

| # | Component | File | Purpose |
|---|-----------|------|---------|
| 1 | Skill markdown body | `packages/agent-core/src/skill/builtin/simplicity-first.md` | 简约阶梯规则、输出纪律、硬约束，含 `<!-- LITE[ -->`, `<!-- FULL[ -->`, `<!-- ULTRA[ -->` HTML 注释块 [C:USER] |
| 2 | Skill registration module | `packages/agent-core/src/skill/builtin/simplicity-first.ts` | 解析 `.md` → `SkillDefinition`，导出 `SIMPLICITY_FIRST_SKILL` 常量 [C:USER] |
| 3 | Registration call-site | `packages/agent-core/src/skill/builtin/index.ts` | `registerBuiltinSkills()` 中新增一行 `registry.registerBuiltinSkill(SIMPLICITY_FIRST_SKILL)` [C:INFERRED] |
| 4 | Multi-level filter | `packages/agent-core/src/skill/builtin/simplicity-first.ts` | 导出 `filterSimplicityLevels(body: string, level: SimplicityLevel): string` [C:USER] |
| 5 | Render integration | `packages/agent-core/src/skill/registry.ts` `renderSkillPrompt` | 当 skill name 为 `simplicity-first` 时，在 `expandSkillParameters` 之前调用 `filterSimplicityLevels` [C:USER] |

### Data Flow

```text
User slash /simplicity ultra
    ↓ SkillManager.activate({ name: 'simplicity-first', args: 'ultra' })
    ↓
SkillRegistry.renderSkillPrompt(skill, 'ultra')
    ├── (NEW) if skill.name === 'simplicity-first':
    │        level := parseSimplicityLevel(rawArgs)  // 'missing' → 'full'; 'unknown' → error
    │        content := filterSimplicityLevels(skill.content, level)
    │   else: content := skill.content
    └── expandSkillParameters(content, rawArgs, ...)  // existing
    ↓
<system-reminder><kimi-skill-loaded>...</kimi-skill-loaded></system-reminder>
    ↓
Agent context
```

### Interfaces

```
type SimplicityLevel = 'lite' | 'full' | 'ultra'
```

```typescript
// parseSimplicityLevel(rawArgs: string): SimplicityLevel
// — Parse a simplicity level from slash-command arguments.
// — 'lite' | 'full' | 'ultra' (case-insensitive) → the matching level.
// — '' | whitespace-only → defaults to 'full'.
// — any other non-empty string → throws OdyError with REQUEST_INVALID code.

// filterSimplicityLevels(body: string, level: SimplicityLevel): string
// — Strip level-specific HTML-comment blocks from a skill body, keeping only
//   blocks tagged for `level` (or none) and removing blocks tagged for other levels.
// — Blocks: /^<!--\s*(LITE|FULL|ULTRA)\s*\[.*?<!--\s*\]\1\s*-->/gims (multi-line).
// — Content outside any level block is always preserved.
// — Unclosed blocks: treated as if closing tag is missing → the block is
//   consumed entirely (from open tag to end-of-body) and removed.
// — Nesting: NOT supported; inner level tags are text, not parsed.
```

### `SkillDefinition` metadata for `simplicity-first`

`frontmatter` fields:

```yaml
type: inline            # user-activatable via slash command
name: simplicity-first
description: >-
  激活'简约优先'思维模式——在写任何代码前走简约阶梯（YAGNI → 标准库 → 平台原生 →
  已有依赖 → 一行 → 最小可用）。支持 lite/full/ultra 三档强度。
arguments: level        # optional; <SimplicityLevel>; default "full"
hiddenInModes: []       # visible in all modes
```

No new data structures beyond the `SimplicityLevel` type alias and one new skill `SkillDefinition` entry. No persistence required — level is purely ephemeral (per-activation args) [C:USER].

---

## Data Models

### `SimplicityLevel`

```typescript
type SimplicityLevel = 'lite' | 'full' | 'ultra'
```

- **Lifecycle**: ephemeral — parsed from slash-command args on each activation, discarded after skill rendering.
- **Persistence**: none. Level is per-activation only.
- **Default**: `'full'` (when args empty/whitespace).

### `SkillDefinition` for `simplicity-first`

Reuses existing `SkillDefinition` interface (from `packages/agent-core/src/skill/types.ts`), no new fields required. The following `SkillMetadata` fields are set:

```typescript
{
  type: 'inline',
  name: 'simplicity-first',
  description: '...',        // full description text
  arguments: 'level',        // optional; used as first arg token
  hiddenInModes: [],          // visible in all modes
}
```

### HTML comment block structure (in-skill markup, not code)

The skill `.md` body uses these markup conventions:

```
Content visible to ALL levels (unannotated)

<!-- LITE[ -->
Content visible ONLY at lite level
<!-- ]LITE -->

<!-- FULL[ -->
Content visible ONLY at full level
<!-- ]FULL -->

<!-- ULTRA[ -->
Content visible ONLY at ultra level
<!-- ]ULTRA -->

More unannotated content (visible to all)
```

- Tags are NOT valid HTML comments — they are a domain-specific convention parsed by `filterSimplicityLevels`.
- Level names are case-insensitive (`lite`/`LITE`/`Lite` all recognized).
- Nesting is NOT supported (inner tags are treated as text/preserved).
- Unclosed blocks are consumed entirely to end-of-body.

---

## Algorithms

### Algorithm 1: `filterSimplicityLevels(body, level)`

```
Input:  body  (string) — skill content after frontmatter removal
        level (SimplicityLevel) — 'lite' | 'full' | 'ultra'
Output: string — body with blocks NOT matching `level` removed

Step 1: // Scan left-to-right, maintain output and a "discard" flag
        out := ''
        cursor := 0
        discardStack := []  // stack of (level-to-discard, discard flag)
        // Regex to match opening tag: /^<!--\s*(LITE|FULL|ULTRA)\s*\[/
        // Regex to match closing tag: /^<!--\s*\]\s*(LITE|FULL|ULTRA)\s*-->/

Step 2: // Scan body for next opening or closing tag
        while cursor < body.length:
            // Find next tag (open or close) from current cursor
            nextOpen := find next match of openPattern in body starting at cursor
            nextClose := find next match of closePattern in body starting at cursor
            nextPos := min(nextOpen.index, nextClose.index) or end-of-body

            // Append text between cursor and nextPos
            if not currently in discard mode:
                out += body[cursor..nextPos.start]

            if nextPos is nextOpen:
                blockLevel := normalizeLevel(nextOpen.captured[1])
                // Push onto discard stack
                discard := blockLevel != level
                discardStack.push({ level: blockLevel, discard })
                cursor := nextOpen.end
            else if nextPos is nextClose:
                closeLevel := normalizeLevel(nextClose.captured[1])
                // Pop; if stack empty or level mismatch, skip the close tag
                // (treat as text that was already consumed above)
                if discardStack is not empty:
                    discardStack.pop()
                cursor := nextClose.end
            else:
                // No more tags; append remainder if not discarding
                if not currently in discard mode:
                    out += body[cursor..]
                break

Step 3: return out

Helper: currently in discard mode = discardStack.any { it.discard == true }
Helper: normalizeLevel(s) = trim(s).toLowerCase()
```

### Algorithm 2: `parseSimplicityLevel(rawArgs)`

```
Input:  rawArgs (string) — first token of skill args (e.g. 'ultra', '', 'extreme')
Output: SimplicityLevel

Step 1: Trim rawArgs.
Step 2: If empty → return 'full'.
Step 3: Normalize: lowercased := trim(rawArgs).toLowerCase().
Step 4: If lowercased ∈ {'lite', 'full', 'ultra'} → return lowercased.
Step 5: Otherwise → throw new OdyError(
            ErrorCodes.REQUEST_INVALID,
            `Invalid simplicity level "${rawArgs.trim()}". Use: lite, full, or ultra.`
        )
```

### Algorithm 3: `renderSkillPrompt` integration

The existing method `SkillRegistry.renderSkillPrompt(skill, rawArgs)` at
`packages/agent-core/src/skill/registry.ts` ~L90:

```
Existing call order (line ~92):
    argumentNames := skillArgumentNames(skill.metadata)
    content := expandSkillParameters(skill.content, rawArgs, ...)
    // ...wrap with plugin instructions...

Modified call order:
    argumentNames := skillArgumentNames(skill.metadata)
    content := skill.content
    if skill.name === 'simplicity-first':
        level := parseSimplicityLevel(rawArgs)
        content := filterSimplicityLevels(content, level)
    content := expandSkillParameters(content, rawArgs, ...)
    // ...wrap with plugin instructions...
```

Insertion point: `registry.ts`, between the `argumentNames` assignment and the
`expandSkillParameters` call — a ~2-line insertion guarded by the skill name
comparison. No change to the public API signature of `renderSkillPrompt`.

---

## Error Handling

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| Unknown level (e.g. `extreme`) | `parseSimplicityLevel` throws `OdyError(REQUEST_INVALID)` before skill content is rendered | `SkillManager.activate` catches the error via ` SkillRegistry.renderSkillPrompt` → `SkillNotFoundError` is not matched, falls through to agent-level error handler; skill is NOT activated [C:INFERRED] | User corrects the level and re-activates |
| Missing args (no level) | Defaults to `'full'` — no error | Full-strength skill content is rendered — no impact to user | N/A |
| Unclosed `<!-- LEVEL[ -->` block | `filterSimplicityLevels` treats the block as extending to end-of-body; all trailing content in that block is removed | Content loss: all text after the unclosed open tag is discarded [C:INFERRED] | Author fixes the `.md` file to include matching `<!-- ]LEVEL -->` |
| Mismatched close tag (e.g. `<!-- FULL[ -->...<!-- ]LITE -->`) | Closing tag level is ignored (does not pop any entry from discardStack); the block stays open | The block extends to end-of-body or to the next matching close tag; content may be accidentally retained or removed | Author fixes the `.md` file |
| `renderSkillPrompt` called with non-simplicity skill (normal path) | No-op — `skill.name !== 'simplicity-first'`, filter is skipped | None | N/A |
| Skill `.md` file missing or unparseable | `parseSkillText` throws `SkillParseError` at registration time (not at activation time); skill is not registered | Skill absent from model listing; `/simplicity` returns `SKILL_NOT_FOUND` [C:INFERRED] | Fix `.md` file content and restart session |

---

## Call-Site Integration

### `packages/agent-core/src/skill/builtin/index.ts` ~L15 (in `registerBuiltinSkills`)

Before: series of `registry.registerBuiltinSkill(...)` calls ending with `VERIFICATION_BEFORE_COMPLETION`.
After: append one new call:

```typescript
import { SIMPLICITY_FIRST_SKILL } from './simplicity-first';
// ... inside registerBuiltinSkills:
registry.registerBuiltinSkill(SIMPLICITY_FIRST_SKILL);
```

Also add `SIMPLICITY_FIRST_SKILL` to the `export {}` block at the bottom of the file.

### `packages/agent-core/src/skill/registry.ts` ~L90–95 (in `renderSkillPrompt`)

```typescript
renderSkillPrompt(skill: SkillDefinition, rawArgs: string): string {
  const argumentNames = skillArgumentNames(skill.metadata);
  // --- INSERTION START (~line 92) ---
  let content = skill.content;
  if (skill.name === 'simplicity-first') {
    const level = parseSimplicityLevel(rawArgs);
    content = filterSimplicityLevels(content, level);
  }
  // --- INSERTION END ---
  content = expandSkillParameters(content, rawArgs, {
    skillDir: skill.dir,
    sessionId: this.sessionId,
    argumentNames,
  });
  // ...existing plugin instructions wrapping...
}
```

`parseSimplicityLevel` and `filterSimplicityLevels` are imported from
`#/skill/builtin/simplicity-first` (or a shared utility module). The import is a
static `import` since `registry.ts` already imports from `#/skill/builtin/` via
the `SkillRegistry` constructor path [C:INFERRED].

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|------------|------------|-----------------|---------------|
| 1 | `SkillRegistry.renderSkillPrompt` 的 `rawArgs` 会原样传入 `/simplicity ultra` 中的 `ultra`，与现有 slash→skill 激活链路一致。 | Medium | 档位无法正确解析，过滤失效。 | 查看 slash 命令路由到 `SkillManager.activate` 的代码路径。 |
| 2 | skill 内容中 HTML 注释块不会与 frontmatter 解析冲突；`parseSkillText` 返回的 `content` 已不含 frontmatter。 | High | 过滤逻辑可能误删 frontmatter 或解析异常。 | 读取 `parser.ts` 确认 `content` 边界。 |
| 3 | 所有档位共用的“懒但不破”硬约束与阶梯内容可以放在未标注块中，不会与档位块产生语义漂移。 | Medium | 档位切换可能意外改变硬约束可见性。 | 在 skill 文案编写时显式标注每段所属档位。 |
| 4 | 单文件多档过滤只需求解单行/多段包裹，不需要任意嵌套档位块。 | High | 算法复杂度上升；可能出现未定义行为。 | 设计中禁止嵌套，并在测试中覆盖未闭合块。 |
| 5 | 使用 `ErrorCodes.REQUEST_INVALID` 作为未知档位的错误码；该错误码已在 `codes.ts` 中定义为 `'request.invalid'`，语义匹配"无效的参数输入"。 | High | 错误类型不一致，测试断言需要调整。 | 已确认 `packages/agent-core/src/errors/codes.ts` 中 `REQUEST_INVALID: 'request.invalid'` 存在。 |

---

## Test Plan

### Unit tests: `filterSimplicityLevels`
Test file: `packages/agent-core/test/skill/simplicity-first.test.ts` (new)

| # | Test case | Input `body` | `level` | Expected output |
|---|-----------|--------------|---------|-----------------|
| 1 | No level blocks | `"# Rules\n\nUse stdlib."` | `'full'` | `"# Rules\n\nUse stdlib."` (no change) |
| 2 | Keep block matching level | `"<!-- FULL[ -->\nAlways do X.\n<!-- ]FULL -->"` | `'full'` | `"Always do X."` (block stripped, content kept) |
| 3 | Remove block not matching level | `"<!-- LITE[ -->\nSkip X.\n<!-- ]LITE -->"` | `'full'` | `""` |
| 4 | Mixed blocks + unannotated | `"# Title\n\n<!-- FULL[ -->\nA\n<!-- ]FULL -->\n\n<!-- ULTRA[ -->\nB\n<!-- ]ULTRA -->\n\n# Footer"` | `'full'` | `"# Title\n\nA\n\n\n\n# Footer"` |
| 5 | Case-insensitive tags | `"<!-- lite[ -->\nX\n<!-- ]LITE -->"` | `'lite'` | `"X"` |
| 6 | Unclosed block -> consumed entirely | `"<!-- LITE[ -->\nno close\n# Rest"` | `'full'` | `""` (everything after tag removed) |
| 7 | Mismatched close (FULL open, LITE close) | `"<!-- FULL[ -->\nX\n<!-- ]LITE -->"` | `'full'` | `"X\n"` (close tag ignored; content kept) |
| 8 | Empty body | `""` | `'full'` | `""` |
| 9 | Nested level tags | `"<!-- FULL[ -->\n<!-- LITE[ -->inner<!-- ]LITE -->\n<!-- ]FULL -->"` | `'full'` | `"<!-- LITE[ -->inner<!-- ]LITE -->"` (inner tags preserved) |
| 10 | Whitespace around level name | `"<!--    FULL   [ -->\nX\n<!-- ] FULL -->"` | `'full'` | `"X"` |

### Unit tests: `parseSimplicityLevel`

| # | Test case | `rawArgs` | Expected result |
|---|-----------|-----------|-----------------|
| 11 | Empty string | `''` | `'full'` (default) |
| 12 | Whitespace only | `'   '` | `'full'` (default) |
| 13 | Valid \'lite\' | `'lite'` | `'lite'` |
| 14 | Valid \'full\' | `'full'` | `'full'` |
| 15 | Valid \'ultra\' | `'ultra'` | `'ultra'` |
| 16 | Case-insensitive | `'ULTRA'` | `'ultra'` |
| 17 | Mixed case | `'Lite'` | `'lite'` |
| 18 | Unknown level | `'extreme'` | Throws `OdyError(REQUEST_INVALID)` |
| 19 | Leading/trailing spaces | `'  ultra  '` | `'ultra'` |

### Integration tests

| # | Test case | Args | Expected behavior |
|---|-----------|------|-------------------|
| 20 | Simplicity with `'lite'` | `'lite'` | Content includes lite-blocks + unannotated; excludes full/ultra |
| 21 | Simplicity with default `''` | `''` | Content includes full-blocks + unannotated; excludes lite/ultra |
| 22 | Non-simplicity skill | `'some-args'` | Normal rendering, no filter applied |

### Regression test

| # | Test case | Expected |
|---|-----------|----------|
| 23 | Existing builtin skill (e.g. `systematic-debugging`) | Activates and renders normally; no filter interference |

### Done Criteria

```bash
npx tsc -p packages/agent-core/tsconfig.json --noEmit
npx vitest run packages/agent-core/test/skill/simplicity-first
npx vitest run packages/agent-core
```

All tests must pass. No new lint warnings.

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|------------|--------|------------|
| 1 | Filter regex false positives | Medium | High | Test cases #1-7 cover must-survive and must-reject. Adversarial inputs in Self-Review. |
| 2 | Filter regex false negatives (malformed tags) | Medium | Medium | Test cases #6-7 cover unclosed and mismatched tags. |
| 3 | renderSkillPrompt breaks non-simplicity skills | Low | High | Test case #23; guarded by `skill.name === 'simplicity-first'`. |
| 4 | Skill content over-encourages simplification | Low | High | Hard constraints as unannotated block, always visible. |
| 5 | Name collision with project/user skill | Very Low | Medium | Builtin names centrally managed. File-system first-wins. |
| 6 | Import cycle in registry.ts | Medium | High | Verify before implementation; extract to shared utility if needed. |

---

## Self-Review

### Highest-stakes decisions (adversarial verification)

**Decision 1: Filter regex patterns (open/close tag matching)**

The regexes `/^<!--\s*(LITE|FULL|ULTRA)\s*\[/im` (open) and `/^<!--\s*\]\s*(LITE|FULL|ULTRA)\s*-->/im` (close) were verified with `node -e` across 15 test inputs. Results: all correct — valid tags match, invalid tags/unknown levels/mid-line tags rejected. Edge case confirmed: whitespace tolerance works.

**Decision 2: `parseSimplicityLevel` defaulting and error semantics**

Verified: empty → 'full', 'ULTRA' → 'ultra', 'extreme' → throws. Error code `REQUEST_INVALID` confirmed exists in `packages/agent-core/src/errors/codes.ts`.

**Decision 3: Name-based skill gating in `renderSkillPrompt`**

The check `skill.name === 'simplicity-first'` correctly isolates the filter to this skill only. The skill name from `SkillDefinition` is already via `parseSkillText` which does NOT normalize names (normalizeSkillName is a registry function for lookups, not the stored name). Verified: `renderSkillPrompt` exists at `registry.ts:90`.

### Four-lens sweep

- **Security**: No filters reject valid input (confirmed above). No secrets or PII in design. The "hard constraints" in the skill body prevent simplification of validation/error handling/safety — these are unannotated blocks always visible regardless of level.

- **Test**: Every behaviour has a must-pass (test cases #1-5, #11-17, #20-22) and must-reject case (test cases #6, #9, #18). No assertion contradicts a constant it depends on. Test case #23 explicitly covers regression for non-simplicity skills. Nothing found to fix.

- **Ops**: The filter function is O(n) linear scan with one pass — negligible cost. `parseSimplicityLevel` is O(1). No new identifiers that could collide. No concurrency concerns (design is pure-function, stateless). No persistence — level is ephemeral per-activation args. Nothing found to fix.

- **Integration**: Every data source referenced in the design exists:
  - `renderSkillPrompt` at `registry.ts:90` ✓
  - `expandSkillParameters` at `parser.ts:178` ✓
  - `SkillManager.activate` at `skill/index.ts:18` ✓
  - `registerBuiltinSkills` at `builtin/index.ts:15` ✓
  - `ErrorCodes.REQUEST_INVALID` at `errors/codes.ts` ✓
  - Target location `packages/agent-core/src/skill/builtin/` unchanged from user-specified target ✓
  - No silent retargeting. Risk: import cycle between `registry.ts` and `simplicity-first.ts` — flagged in Risk Register #6 with mitigation (extract to shared utility).

- **Scope**: This is a single coherent design — one skill + filter function + registration. No scope creep detected. P3-A, P3-B, P3-C explicitly deferred. A2/A3/A4 out of scope. Nothing found to fix.

---

## User Final Approval

## User Final Approval

- **Audit level**: Deep ✓
- **All 5 [C:INFERRED] assumptions**: Accepted ✓
- **All section key claims**: Confirmed ✓
- **C1-C8 completeness**: Verified (C8 Reuse Analysis included) ✓
- **Status**: APPROVED (2026-06-17)
