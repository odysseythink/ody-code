# Simplicity-First Skill — Implementation Plan

**Goal:** Add a `simplicity-first` builtin skill with three intensity levels (lite/full/ultra) encoded as HTML-comment blocks in a single `.md` file, filtered at render time via a new `filterSimplicityLevels` function in `SkillRegistry.renderSkillPrompt`.

**Architecture:** A new `.md` + `.ts` pair under `packages/agent-core/src/skill/builtin/` follows the existing builtin-skill pattern. The `.ts` module exports the `SkillDefinition`, a `filterSimplicityLevels` pure function (regex-based block stripping), and a `parseSimplicityLevel` argument parser. `SkillRegistry.renderSkillPrompt` gains a 2-line guard that applies the filter before `expandSkillParameters` when `skill.name === 'simplicity-first'`. No new data structures, no persistence — level is ephemeral per-activation args.

**Tech Stack:** TypeScript 6.0, Vitest 4.1, Node ≥24.15, pnpm 10.33. Target: `packages/agent-core`.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| Task | Create | Modify | Test |
|------|--------|--------|------|
| 1 | `packages/agent-core/src/skill/builtin/simplicity-first.md` | — | — (manual verification) |
| 2 | `packages/agent-core/src/skill/builtin/simplicity-first.ts` | — | `packages/agent-core/test/skill/simplicity-first.test.ts` (new) |
| 3 | — | `packages/agent-core/src/skill/builtin/index.ts` L1-43 | `packages/agent-core/test/skill/builtin-skills.test.ts` L33-35 |
| 4 | — | `packages/agent-core/src/skill/registry.ts` L90-96 | `packages/agent-core/test/skill/simplicity-first.test.ts` (add integration tests) |
| 5 | — | — (verify only) | — |

---

## Dependency Overview

```
Task 1 (skill .md body)
  └─ Task 2 (simplicity-first.ts + unit tests)
       ├─ Task 3 (builtin/index.ts registration + test count update)
       └─ Task 4 (registry.ts integration + integration tests)
            └─ Task 5 (whole-tree verify + changeset)
```

Tasks 3 and 4 are independent of each other and can run in parallel after Task 2.

---

## Risks & Open Questions

| # | Risk | Mitigation |
|---|------|------------|
| 1 | Import cycle between `registry.ts` and `simplicity-first.ts` | Use a dynamic `import()` inside the guard block, or extract filter/parse to a shared utility. Plan uses dynamic import. |
| 2 | Regex false positives on malformed HTML comments | 10+ test cases in Task 2 cover must-survive and must-reject inputs. |
| 3 | `rawArgs` may not be the plain level string if slash-command routing pre-processes args | Verified in design: `rawArgs` is the first token of skill args, same as `argumentNames` parsing. |
| 4 | `.md` import may not resolve until the file exists (Task 1 complete) | Task 2 depends on Task 1. |

---

### Task 1: Create the `simplicity-first.md` skill body

**Depends on:** none

**Files:**
- Create: `packages/agent-core/src/skill/builtin/simplicity-first.md`

This is a non-testable content file — the skill body with YAML frontmatter and HTML-comment level blocks.

- [ ] Write the skill `.md` file:

```markdown
---
type: inline
name: simplicity-first
description: >-
  激活'简约优先'思维模式——在写任何代码前走简约阶梯（YAGNI → 标准库 → 平台原生 →
  已有依赖 → 一行 → 最小可用）。你的输出默认必须偏向更简单（更懒）、更少外部依赖、更少代码行数的方案。
  支持 lite（温和提醒）、full（强制走阶梯，默认）、ultra（质疑需求本身）三档强度。
arguments: level
---

<!-- FULL[ -->
## 简约强度档位

当前已激活 `simplicity-first /full`：你 **必须** 遵守下方阶梯。禁止跳过某一层直接跃迁到实现。
<!-- ]FULL -->

<!-- LITE[ -->
## 简约强度档位

当前已激活 `simplicity-first /lite`：正常实现即可，但在提交方案前简单提一句是否有更懒的替代。
<!-- ]LITE -->

<!-- ULTRA[ -->
## 简约强度档位

当前已激活 `simplicity-first /ultra`：**敢于质疑需求本身**。如果任务本身可以通过改变需求、去掉功能、推迟或完全不做来避免编码 — 首先提出来。然后仍然走完整阶梯。
<!-- ]ULTRA -->

## 懒但不破（硬约束，无视档位永远生效）

以下约束在 **任何档位下都不得违反**：

1. **信任边界**：不简化认证、授权、审计、输入校验、跨域隔离和安全边界。任何从外部接收的数据都要校验。
2. **错误处理**：不吞异常。每个外部调用必须有超时和错误回退路径。
3. **可访问性**：不为了简约而移除语义化 HTML、ARIA label、键盘导航等必要结构。
4. **安全**：避免已知的危险操作（eval、sql 拼接、shell 注入、XSS 等）。遵循最小权限原则。
5. **硬件/物理标定**：任何涉及硬件的配置、校准或阈值不经简化——物理世界不关心我们是否偏爱简单。
6. **数据完整性**：生产数据不丢失、不损坏、不静默截断。

## 简约阶梯（必须按顺序走）

<!-- FULL[ -->
对于 **每一个要解决的问题**，你必须按照以下顺序思考并执行，每层走不下去才能下一层：

1. **YAGNI 自问**：真的需要写代码吗？能否通过改变需求、改配置、或复用已有功能来避免编码？
2. **标准库优先**：语言/运行时标准库已提供该能力吗？
3. **平台原生**：OS / 浏览器 / Node.js / Web API 原生支持吗？
4. **已有依赖约束**：项目中已有依赖（`package.json` 中已安装的包）能覆盖吗？
5. **一行方案**：能用一行代码或一个简单表达式解决吗？如果不能 ——
6. **最小可用实现**：用最少的代码行数写出满足需求的最小方案。避免不必要的抽象、类、接口、中间件和配置。**禁止** "为了未来扩展" 而添加当前不需要的代码。

每一步都必须在输出中 **明确记录结论**（为什么该层不能解决问题，或者该层怎么解决了问题），然后再进入下一层。
<!-- ]FULL -->

<!-- LITE[ -->
对于 **每一个要解决的问题**，按照常识实现，但完成后简要提一下：是否存在更懒的替代（不改代码、用标准库、复用已有依赖，等）。
<!-- ]LITE -->

<!-- ULTRA[ -->
对于 **每一个要解决的问题**，你必须：

1. **首先质疑需求本身**——这个任务真的需要做吗？能否通过改需求、推迟或完全不做来避免编码？
2. **如果必须做**，走完整简约阶梯（YAGNI → 标准库 → 平台原生 → 已有依赖 → 一行 → 最小可用），**每一步都必须在输出中明确记录结论**。
3. 对每个结论，问自己：**我是不是下意识跳过了某一层**？是的话，回去重走。
<!-- ]ULTRA -->

## 输出纪律（所有档位）

- 在开始写代码前，在分析中明确声明当前档位和选择的方案路径。
- 在方案被质疑或不确定时，优先选 **更少代码** 的路径，而非更"优雅"的路径。
- 避免引入新的第三方依赖，除非已有依赖无法覆盖且标准库也不支持该功能。
```

- [ ] Manual verification:

```bash
wc -l packages/agent-core/src/skill/builtin/simplicity-first.md
# Expected: ~87 lines
grep -c '<!--' packages/agent-core/src/skill/builtin/simplicity-first.md
# Expected: >= 8 (four open/close pairs for lite/full/ultra plus description opening line)
```

- [ ] Commit.

```
git add packages/agent-core/src/skill/builtin/simplicity-first.md
git commit -m "feat: add simplicity-first skill markdown body"
```

---

### Task 2: Create `simplicity-first.ts` with filter/parse + unit tests

**Depends on:** Task 1 (the `.md` file must exist for the `import ... from './simplicity-first.md'` to resolve)

**Files:**
- Create: `packages/agent-core/src/skill/builtin/simplicity-first.ts`
- Create: `packages/agent-core/test/skill/simplicity-first.test.ts`

#### 2a. Test-first: `filterSimplicityLevels` (no .ts file yet, test imports fail → expected FAIL)

- [ ] Write the failing test file `packages/agent-core/test/skill/simplicity-first.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { filterSimplicityLevels, parseSimplicityLevel } from '../../src/skill/builtin/simplicity-first';
import type { SimplicityLevel } from '../../src/skill/builtin/simplicity-first';

// ============================================================
// filterSimplicityLevels
// ============================================================

describe('filterSimplicityLevels', () => {
  it('returns body unchanged when no level blocks present', () => {
    const body = '# Rules\n\nUse stdlib.';
    expect(filterSimplicityLevels(body, 'full')).toBe('# Rules\n\nUse stdlib.');
  });

  it('keeps content inside a block matching the current level', () => {
    const body = '<!-- FULL[ -->\nAlways do X.\n<!-- ]FULL -->';
    const result = filterSimplicityLevels(body, 'full');
    expect(result).toBe('Always do X.');
    expect(result).not.toContain('<!--');
  });

  it('removes content inside a block not matching the current level', () => {
    const body = '<!-- LITE[ -->\nSkip X.\n<!-- ]LITE -->';
    const result = filterSimplicityLevels(body, 'full');
    expect(result).toBe('');
  });

  it('keeps unannotated content and filters level-specific blocks', () => {
    const body = [
      '# Title',
      '',
      '<!-- FULL[ -->',
      'A',
      '<!-- ]FULL -->',
      '',
      '<!-- ULTRA[ -->',
      'B',
      '<!-- ]ULTRA -->',
      '',
      '# Footer',
    ].join('\n');
    const result = filterSimplicityLevels(body, 'full');
    expect(result).toBe('# Title\n\nA\n\n\n\n# Footer');
  });

  it('handles case-insensitive tags', () => {
    const body = '<!-- lite[ -->\nX\n<!-- ]LITE -->';
    expect(filterSimplicityLevels(body, 'lite')).toBe('X');
  });

  it('consumes unclosed block entirely (to end-of-body)', () => {
    const body = '<!-- LITE[ -->\nno close\n# Rest';
    expect(filterSimplicityLevels(body, 'full')).toBe('');
  });

  it('ignores mismatched close tag (FULL open, LITE close)', () => {
    const body = '<!-- FULL[ -->\nX\n<!-- ]LITE -->';
    expect(filterSimplicityLevels(body, 'full')).toBe('X\n');
  });

  it('handles empty body', () => {
    expect(filterSimplicityLevels('', 'full')).toBe('');
  });

  it('preserves nested level tags as text (no nesting support)', () => {
    const body = '<!-- FULL[ -->\n<!-- LITE[ -->inner<!-- ]LITE -->\n<!-- ]FULL -->';
    expect(filterSimplicityLevels(body, 'full')).toBe('<!-- LITE[ -->inner<!-- ]LITE -->');
  });

  it('tolerates whitespace around level name in open tag', () => {
    const body = '<!--    FULL   [ -->\nX\n<!-- ] FULL -->';
    expect(filterSimplicityLevels(body, 'full')).toBe('X');
  });

  // --- Must-survive adversarial inputs ---
  it('does not match level names mid-sentence', () => {
    const body = 'The word "full" should survive and "lite" too.';
    expect(filterSimplicityLevels(body, 'full')).toBe(body);
  });

  it('does not match level names in regular HTML comments', () => {
    const body = '<!-- This is a normal comment about full mode -->\n<!-- Another comment -->';
    expect(filterSimplicityLevels(body, 'full')).toBe(body);
  });

  it('does not match unclosed bracket without opening tag pattern', () => {
    const body = '<!-- ]FULL --> without an opener';
    // The close tag alone (no matching open on stack) should be preserved as text
    // because it won't be matched at start-of-body — but it IS a valid close pattern.
    // The algo: since cursor is at start, no open tag on stack → close tag pops nothing
    // → it IS consumed (removed) because we skip it when no matching open.
    // Actually, per the design algorithm: nextClose is found and if discardStack is empty,
    // we pop nothing and advance cursor past the tag. The text before the close tag
    // was already appended. So the close tag text itself is consumed/removed.
    // Fix: the open tag regex matches `<!--\s*(LITE|FULL|ULTRA)\s*\[` which requires `[`.
    // `<!-- ]FULL -->` does NOT match the open pattern. Let me trace:
    // nextOpen = find `<!--\s*(LITE|FULL|ULTRA)\s*\[` from cursor=0 → no match
    // nextClose = find `<!--\s*\]\s*(LITE|FULL|ULTRA)\s*-->` → matches at position 0
    // cursor=0, no discard → append body[0..0] = '' → advance cursor past close tag
    // Result: '' — the close tag is skipped. But the design says: "if stack empty or level
    // mismatch, skip the close tag". So the close tag is removed. That means:
    expect(filterSimplicityLevels(body, 'full')).toBe('');
    // This is expected — orphan close tags are silently consumed.
  });
});

// ============================================================
// parseSimplicityLevel
// ============================================================

describe('parseSimplicityLevel', () => {
  it('defaults to full on empty string', () => {
    expect(parseSimplicityLevel('')).toBe('full');
  });

  it('defaults to full on whitespace-only', () => {
    expect(parseSimplicityLevel('   ')).toBe('full');
  });

  it('parses "lite"', () => {
    expect(parseSimplicityLevel('lite')).toBe('lite');
  });

  it('parses "full"', () => {
    expect(parseSimplicityLevel('full')).toBe('full');
  });

  it('parses "ultra"', () => {
    expect(parseSimplicityLevel('ultra')).toBe('ultra');
  });

  it('handles case-insensitive input', () => {
    expect(parseSimplicityLevel('ULTRA')).toBe('ultra');
    expect(parseSimplicityLevel('Lite')).toBe('lite');
  });

  it('trims leading/trailing spaces', () => {
    expect(parseSimplicityLevel('  ultra  ')).toBe('ultra');
  });

  it('throws OdyError with REQUEST_INVALID for unknown level', () => {
    expect(() => parseSimplicityLevel('extreme')).toThrow('Invalid simplicity level');
    try {
      parseSimplicityLevel('extreme');
    } catch (e: any) {
      expect(e.code).toBe('request.invalid');
    }
  });
});
```

- [ ] Run tests — expect FAIL (module not found):

```bash
npx vitest run packages/agent-core/test/skill/simplicity-first.test.ts 2>&1 | tail -20
# Expected: FAIL — "Cannot find module '../../src/skill/builtin/simplicity-first'" or similar
```

#### 2b. Write the minimal implementation `packages/agent-core/src/skill/builtin/simplicity-first.ts`:

```typescript
import { ErrorCodes } from '../../errors/codes';
import { OdyError } from '../../errors/classes';
import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import SIMPLICITY_FIRST_BODY from './simplicity-first.md';

// ---- Types ----

export type SimplicityLevel = 'lite' | 'full' | 'ultra';

// ---- Skill definition (following builtin skill pattern) ----

const PSEUDO_PATH = 'builtin://simplicity-first';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/simplicity-first.md',
  skillDirName: 'simplicity-first',
  source: 'builtin',
  text: SIMPLICITY_FIRST_BODY,
});

export const SIMPLICITY_FIRST_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};

// ---- Level parsing ----

/** Parse a simplicity level from slash-command arguments. */
export function parseSimplicityLevel(rawArgs: string): SimplicityLevel {
  const trimmed = rawArgs.trim();
  if (trimmed.length === 0) return 'full';
  const lower = trimmed.toLowerCase();
  if (lower === 'lite' || lower === 'full' || lower === 'ultra') {
    return lower;
  }
  throw new OdyError(
    ErrorCodes.REQUEST_INVALID,
    `Invalid simplicity level "${rawArgs.trim()}". Use: lite, full, or ultra.`,
  );
}

// ---- Level filtering ----

const VALID_LEVELS = new Set<string>(['lite', 'full', 'ultra']);

// Matches opening tags: <!-- LITE[ -->, <!--    FULL   [ -->
const OPEN_RE = /<!--\s*(LITE|FULL|ULTRA)\s*\[/gi;
// Matches closing tags: <!-- ]LITE -->, <!-- ] FULL -->
const CLOSE_RE = /<!--\s*\]\s*(LITE|FULL|ULTRA)\s*-->/gi;

/**
 * Strip level-specific HTML-comment blocks from a skill body.
 *
 * Blocks tagged for `level` (or no tag) are kept; blocks tagged for other
 * levels are removed.  Content outside any level block is always preserved.
 */
export function filterSimplicityLevels(body: string, level: SimplicityLevel): string {
  const out: string[] = [];
  let cursor = 0;
  const discardStack: boolean[] = [];

  while (cursor < body.length) {
    // Reset lastIndex for global regexes used with .exec on substring
    OPEN_RE.lastIndex = 0;
    CLOSE_RE.lastIndex = 0;

    // Find next open and close tags from current cursor
    const remainingSlice = body.slice(cursor);
    const nextOpen = OPEN_RE.exec(remainingSlice);
    const nextClose = CLOSE_RE.exec(remainingSlice);

    const openIdx = nextOpen !== null ? cursor + nextOpen.index : Infinity;
    const closeIdx = nextClose !== null ? cursor + nextClose.index : Infinity;

    if (openIdx === Infinity && closeIdx === Infinity) {
      // No more tags — append remainder if not discarding
      if (!discardStack.includes(true)) {
        out.push(body.slice(cursor));
      }
      break;
    }

    if (openIdx <= closeIdx) {
      // Append text before the open tag
      if (!discardStack.includes(true)) {
        out.push(body.slice(cursor, openIdx));
      }
      // Determine if this block's level should be discarded
      const blockLevel = nextOpen![1].toLowerCase();
      const discard = blockLevel !== level;
      discardStack.push(discard);
      cursor = openIdx + nextOpen![0].length;
    } else {
      // Append text before the close tag
      if (!discardStack.includes(true)) {
        out.push(body.slice(cursor, closeIdx));
      }
      // Pop from stack; ignore if empty or level mismatch
      if (discardStack.length > 0) {
        discardStack.pop();
      }
      cursor = closeIdx + nextClose![0].length;
    }
  }

  return out.join('');
}
```

- [ ] Run tests — expect PASS:

```bash
npx vitest run packages/agent-core/test/skill/simplicity-first.test.ts 2>&1
```

Expected: all 21 tests pass (10 filter + 8 parse + 3 adversarial).

- [ ] Commit.

```
git add packages/agent-core/src/skill/builtin/simplicity-first.ts packages/agent-core/test/skill/simplicity-first.test.ts
git commit -m "feat: add simplicity-first skill module with filter/parse logic"
```

---

### Task 3: Register skill in `builtin/index.ts` + update builtin-skills test count

**Depends on:** Task 2 (needs `SIMPLICITY_FIRST_SKILL` export)

**Files:**
- Modify: `packages/agent-core/src/skill/builtin/index.ts` L1-4, L15-28, L30-43
- Modify: `packages/agent-core/test/skill/builtin-skills.test.ts` L18-20, L33

No shared signature change; the only public surface is the new export.

#### 3a. Test-first: update builtin-skills count (this FAILS until the import + registration is added)

- [ ] Modify `packages/agent-core/test/skill/builtin-skills.test.ts`:

At L18, add the new entry to `BUILTIN_SKILLS` array:

```typescript
import {
  DISPATCHING_PARALLEL_AGENTS_SKILL,
  EXECUTING_PLANS_SKILL,
  FINISHING_A_DEVELOPMENT_BRANCH_SKILL,
  MCP_CONFIG_SKILL,
  RECEIVING_CODE_REVIEW_SKILL,
  REQUESTING_CODE_REVIEW_SKILL,
  SIMPLICITY_FIRST_SKILL,          // <-- ADD
  SUBAGENT_DRIVEN_DEVELOPMENT_SKILL,
  SYNC_CHANGELOG_SKILL,
  SYSTEMATIC_DEBUGGING_SKILL,
  TEST_DRIVEN_DEVELOPMENT_SKILL,
  USING_GIT_WORKTREES_SKILL,
  VERIFICATION_BEFORE_COMPLETION_SKILL,
} from '../../src/skill/builtin';
```

At L18-L31 area, add to array:

```typescript
const BUILTIN_SKILLS = [
  { skill: DISPATCHING_PARALLEL_AGENTS_SKILL, name: 'dispatching-parallel-agents' },
  { skill: EXECUTING_PLANS_SKILL, name: 'executing-plans' },
  { skill: FINISHING_A_DEVELOPMENT_BRANCH_SKILL, name: 'finishing-a-development-branch' },
  { skill: MCP_CONFIG_SKILL, name: 'mcp-config' },
  { skill: RECEIVING_CODE_REVIEW_SKILL, name: 'receiving-code-review' },
  { skill: REQUESTING_CODE_REVIEW_SKILL, name: 'requesting-code-review' },
  { skill: SIMPLICITY_FIRST_SKILL, name: 'simplicity-first' },   // <-- ADD
  { skill: SUBAGENT_DRIVEN_DEVELOPMENT_SKILL, name: 'subagent-driven-development' },
  { skill: SYNC_CHANGELOG_SKILL, name: 'sync-changelog' },
  { skill: SYSTEMATIC_DEBUGGING_SKILL, name: 'systematic-debugging' },
  { skill: TEST_DRIVEN_DEVELOPMENT_SKILL, name: 'test-driven-development' },
  { skill: USING_GIT_WORKTREES_SKILL, name: 'using-git-worktrees' },
  { skill: VERIFICATION_BEFORE_COMPLETION_SKILL, name: 'verification-before-completion' },
];
```

At L33, update the count:

```typescript
  it('has exactly 13 built-in skills', () => {   // was 12
    expect(BUILTIN_SKILLS).toHaveLength(13);      // was 12
  });
```

- [ ] Run — expect FAIL (import `SIMPLICITY_FIRST_SKILL` not yet exported from `builtin/index.ts`):

```bash
npx vitest run packages/agent-core/test/skill/builtin-skills.test.ts 2>&1 | tail -5
# Expected: FAIL — "does not provide an export named 'SIMPLICITY_FIRST_SKILL'"
```

#### 3b. Wire the registration

- [ ] Modify `packages/agent-core/src/skill/builtin/index.ts`:

Add import at top (after existing imports, before the function):

```typescript
import { SIMPLICITY_FIRST_SKILL } from './simplicity-first';
```

Add registration call inside `registerBuiltinSkills` (after `VERIFICATION_BEFORE_COMPLETION_SKILL` line):

```typescript
  registry.registerBuiltinSkill(SIMPLICITY_FIRST_SKILL);
```

Add to the export block:

```typescript
export {
  DISPATCHING_PARALLEL_AGENTS_SKILL,
  EXECUTING_PLANS_SKILL,
  FINISHING_A_DEVELOPMENT_BRANCH_SKILL,
  MCP_CONFIG_SKILL,
  RECEIVING_CODE_REVIEW_SKILL,
  REQUESTING_CODE_REVIEW_SKILL,
  SIMPLICITY_FIRST_SKILL,          // <-- ADD
  SUBAGENT_DRIVEN_DEVELOPMENT_SKILL,
  SYNC_CHANGELOG_SKILL,
  SYSTEMATIC_DEBUGGING_SKILL,
  TEST_DRIVEN_DEVELOPMENT_SKILL,
  USING_GIT_WORKTREES_SKILL,
  VERIFICATION_BEFORE_COMPLETION_SKILL,
};
```

- [ ] Run — expect PASS:

```bash
npx vitest run packages/agent-core/test/skill/builtin-skills.test.ts 2>&1
```

Expected: all 3 tests pass, including "has exactly 13 built-in skills".

- [ ] Commit.

```
git add packages/agent-core/src/skill/builtin/index.ts packages/agent-core/test/skill/builtin-skills.test.ts
git commit -m "feat: register simplicity-first as builtin skill"
```

---

### Task 4: Integrate level filter into `SkillRegistry.renderSkillPrompt`

**Depends on:** Task 2 (needs `filterSimplicityLevels` and `parseSimplicityLevel` from `simplicity-first.ts`)

**Files:**
- Modify: `packages/agent-core/src/skill/registry.ts` L90-96
- Modify: `packages/agent-core/test/skill/simplicity-first.test.ts` (add integration tests)

**Risk:** Import cycle between `registry.ts` and `simplicity-first.ts`. `registry.ts` already imports from `./parser`, `./scanner`, `./types` — none of which import from `registry.ts`. Adding `import { filterSimplicityLevels, parseSimplicityLevel } from './builtin/simplicity-first'` to `registry.ts` creates no cycle because `simplicity-first.ts` only imports from `../parser`, `../types`, `../../errors/*` — none of which import back to `registry.ts`. **Safe static import.**

No shared signature change: `renderSkillPrompt(skill, rawArgs): string` interface unchanged.

#### 4a. Test-first: integration tests (FAIL until registry.ts is modified)

- [ ] Append to `packages/agent-core/test/skill/simplicity-first.test.ts`:

```typescript
import { SkillRegistry } from '../../src/skill/registry';
import { SIMPLICITY_FIRST_SKILL } from '../../src/skill/builtin/simplicity-first';

// ============================================================
// Integration: SkillRegistry.renderSkillPrompt with simplicity-first
// ============================================================

describe('SkillRegistry integration with simplicity-first', () => {
  it('renders full-level content (default) when no args provided', () => {
    const registry = new SkillRegistry();
    registry.registerBuiltinSkill(SIMPLICITY_FIRST_SKILL);
    const output = registry.renderSkillPrompt(SIMPLICITY_FIRST_SKILL, '');
    // Must contain full-only content
    expect(output).toContain('简约阶梯（必须按顺序走）');
    // Must NOT contain lite-only or ultra-only content markers
    expect(output).not.toContain('正常实现即可');
    expect(output).not.toContain('首先质疑需求本身');
    // Hard constraints always present
    expect(output).toContain('懒但不破');
    // Output discipline always present
    expect(output).toContain('输出纪律');
  });

  it('renders lite-level content when "lite" arg provided', () => {
    const registry = new SkillRegistry();
    registry.registerBuiltinSkill(SIMPLICITY_FIRST_SKILL);
    const output = registry.renderSkillPrompt(SIMPLICITY_FIRST_SKILL, 'lite');
    expect(output).toContain('正常实现即可');
    expect(output).not.toContain('简约阶梯（必须按顺序走）');
    expect(output).not.toContain('首先质疑需求本身');
    expect(output).toContain('懒但不破');
  });

  it('renders ultra-level content when "ultra" arg provided', () => {
    const registry = new SkillRegistry();
    registry.registerBuiltinSkill(SIMPLICITY_FIRST_SKILL);
    const output = registry.renderSkillPrompt(SIMPLICITY_FIRST_SKILL, 'ultra');
    expect(output).toContain('首先质疑需求本身');
    expect(output).not.toContain('简约阶梯（必须按顺序走）');
    expect(output).not.toContain('正常实现即可');
    expect(output).toContain('懒但不破');
  });

  it('throws OdyError for invalid level', () => {
    const registry = new SkillRegistry();
    registry.registerBuiltinSkill(SIMPLICITY_FIRST_SKILL);
    expect(() => registry.renderSkillPrompt(SIMPLICITY_FIRST_SKILL, 'extreme'))
      .toThrow('Invalid simplicity level');
  });

  it('does not interfere with non-simplicity skills', () => {
    const registry = new SkillRegistry();
    // Use an existing builtin skill as the control
    const { SYSTEMATIC_DEBUGGING_SKILL } = require('../../src/skill/builtin/systematic-debugging');
    registry.registerBuiltinSkill(SYSTEMATIC_DEBUGGING_SKILL);
    const output = registry.renderSkillPrompt(SYSTEMATIC_DEBUGGING_SKILL, 'some-args');
    expect(output).toContain('systematic');
    expect(output.length).toBeGreaterThan(0);
  });
});
```

- [ ] Run — expect FAIL (integration tests call `registry.renderSkillPrompt` but filter not yet wired):

```bash
npx vitest run packages/agent-core/test/skill/simplicity-first.test.ts 2>&1 | tail -30
# Expected: 3 new integration tests FAIL — content assertions will fail
# because renderSkillPrompt returns unfiltered content (all blocks visible).
```

#### 4b. Implement the integration in `registry.ts`

- [ ] Modify `packages/agent-core/src/skill/registry.ts` L90-96:

Current code (L90-96):
```typescript
  renderSkillPrompt(skill: SkillDefinition, rawArgs: string): string {
    const argumentNames = skillArgumentNames(skill.metadata);
    const content = expandSkillParameters(skill.content, rawArgs, {
      skillDir: skill.dir,
      sessionId: this.sessionId,
      argumentNames,
    });
```

New code:
```typescript
  renderSkillPrompt(skill: SkillDefinition, rawArgs: string): string {
    const argumentNames = skillArgumentNames(skill.metadata);
    let content = skill.content;
    if (skill.name === 'simplicity-first') {
      const { parseSimplicityLevel, filterSimplicityLevels } =
        require('./builtin/simplicity-first') as typeof import('./builtin/simplicity-first');
      const level = parseSimplicityLevel(rawArgs);
      content = filterSimplicityLevels(content, level);
    }
    content = expandSkillParameters(content, rawArgs, {
      skillDir: skill.dir,
      sessionId: this.sessionId,
      argumentNames,
    });
```

**Note:** Using `require()` rather than top-level `import` to avoid any potential import cycle, even though static analysis shows no cycle exists. This is a defensive choice. If the engineer confirms no cycle, a static `import` at the top of the file is cleaner — add this line after L4:

```typescript
import { filterSimplicityLevels, parseSimplicityLevel } from './builtin/simplicity-first';
```

And then the guard block simplifies to:

```typescript
    let content = skill.content;
    if (skill.name === 'simplicity-first') {
      const level = parseSimplicityLevel(rawArgs);
      content = filterSimplicityLevels(content, level);
    }
```

Either approach is valid. The plan uses the static import (cleaner, no cycle risk per static analysis above).

- [ ] Run integration tests — expect PASS:

```bash
npx vitest run packages/agent-core/test/skill/simplicity-first.test.ts 2>&1
```

Expected: all 24 tests pass (21 from Task 2 + 3 integration tests).

- [ ] Also confirm existing registry tests still pass:

```bash
npx vitest run packages/agent-core/test/skill/registry.test.ts 2>&1
```

Expected: all existing tests pass (no regressions).

- [ ] Commit.

```
git add packages/agent-core/src/skill/registry.ts packages/agent-core/test/skill/simplicity-first.test.ts
git commit -m "feat: integrate simplicity level filtering into SkillRegistry.renderSkillPrompt"
```

---

### Task 5: Whole-tree typecheck, full test suite, and changeset

**Depends on:** Task 3 AND Task 4

**Files:**
- Create: `.changeset/simplicity-first-skill.md` (changeset)
- No source/test modifications.

#### 5a. Whole-tree typecheck

- [ ] Run full-workspace typecheck:

```bash
pnpm -r run typecheck 2>&1
```

Expected: zero errors. All packages typecheck clean.

#### 5b. Full agent-core test suite

- [ ] Run the full `agent-core` test suite:

```bash
npx vitest run packages/agent-core 2>&1
```

Expected: all tests pass, including:
- `packages/agent-core/test/skill/simplicity-first.test.ts` — 24 tests
- `packages/agent-core/test/skill/builtin-skills.test.ts` — 3 tests (including updated count)
- `packages/agent-core/test/skill/registry.test.ts` — all existing tests
- All other test files — no regressions

#### 5c. Generate changeset

- [ ] Run:

```bash
pnpm changeset
```

Select:
- `@odysseythink/agent-core` → `minor`
- `@odysseythink/ody-code` (CLI bundle) → `minor`

Changelog message: `Add simplicity-first builtin skill with lite/full/ultra intensity levels.`

- [ ] Commit.

```
git add .changeset/
git commit -m "chore: add changeset for simplicity-first skill"
```

---

## Self-Review

- [ ] 1. **Spec-coverage table**: map every spec section/requirement → Task(s), marked covered / GAP / no-op.

| Spec Requirement | Task(s) | Status |
|---|---|---|
| New builtin skill `simplicity-first` | T1 (.md) + T2 (.ts definition) | covered |
| Single-file multi-level filtering via HTML comment blocks | T1 (markup) + T2 (filterSimplicityLevels) | covered |
| `filterSimplicityLevels` pure function with regex block stripping | T2 | covered |
| `parseSimplicityLevel` argument parser | T2 | covered |
| Integration into `SkillRegistry.renderSkillPrompt` | T4 | covered |
| Registration in `builtin/index.ts` | T3 | covered |
| Missing level args → default `full` | T2 (parseSimplicityLevel test #1-2) | covered |
| Unknown level args → throws `OdyError(REQUEST_INVALID)` | T2 (parseSimplicityLevel test #8) | covered |
| All session modes (normal/plan/design) can activate | T2 (`hiddenInModes: undefined`, inherited from frontmatter — no `hiddenInModes` in YAML = visible in all modes) | covered |
| Case-insensitive level tags | T2 (filter test #5, parse test #6-7) | covered |
| Unclosed block → consumed entirely | T2 (filter test #6) | covered |
| Mismatched close tag → ignored | T2 (filter test #7) | covered |
| Nesting NOT supported (inner tags preserved as text) | T2 (filter test #9) | covered |
| Whitespace tolerance in tags | T2 (filter test #10, parse test #9) | covered |
| Hard constraints always visible (unannotated block) | T1 (content structure) + T4 (integration test assertions for 懒但不破) | covered |
| No new error codes (reuses REQUEST_INVALID) | T2 (parseSimplicityLevel uses ErrorCodes.REQUEST_INVALID) | covered |
| No persistence / stateless | T2 (ephemeral per-activation args) | covered |
| Non-simplicity skills unaffected | T4 (integration test #5) | covered |
| Regression: existing builtin skills still work | T5b (full suite) | covered |
| P3-A (status bar badge) | — | no-op (deferred) |
| P3-B (config-driven default) | — | no-op (deferred) |
| P3-C (before/after example docs) | — | no-op (deferred) |
| Global constant injector | — | no-op (deferred) |

- [ ] 2. **Placeholder scan**: no `TODO`/`TBD`/"implement later" in any task step. All code is shown inline. The `require()` vs `import` note in Task 4b is a style choice, not a placeholder — both forms are given.

- [ ] 3. **No phantom tasks**: every task produces a verifiable change. T1 creates a file; T2 creates two files; T3 modifies two files; T4 modifies two files; T5 produces a changeset file and a test pass. Zero `--allow-empty` commits.

- [ ] 4. **Dependency soundness**: T1→T2→{T3, T4}→T5. Every `Depends on:` references an earlier task. Task 2 imports `./simplicity-first.md` (created in T1). Task 3 imports `SIMPLICITY_FIRST_SKILL` (exported in T2). Task 4 imports `filterSimplicityLevels` and `parseSimplicityLevel` (exported in T2). No forward references.

- [ ] 5. **Caller & build soundness**: No shared-signature method signatures change. `renderSkillPrompt(skill, rawArgs): string` retains its exact signature — internal implementation changed, no caller update needed. T5 runs `pnpm -r run typecheck` (whole-tree).

- [ ] 6. **Test-the-risk**:
  - `filterSimplicityLevels`: 10 behavioral tests + 3 adversarial must-survive tests. Must-survive inputs verified against regex: (a) `The word "full" should survive` — no `<!--\s*(LITE|FULL|ULTRA)\s*\[` pattern, survives; (b) `<!-- This is a normal comment` — no `[` after the level name, survives; (c) orphan close `<!-- ]FULL -->` — matches close pattern but with empty stack, correctly consumed per design.
  - `parseSimplicityLevel`: 8 tests covering empty, whitespace, valid levels, case insensitivity, trimming, and error throw with code assertion.
  - Integration: 5 tests covering all three levels, error propagation, and regression for non-simplicity skills.
  - Risk covered: regex false positives (adversarial inputs), false negatives (unclosed/mismatched tags), default behavior, error handling, and regression isolation.

- [ ] 7. **Type consistency**: `SimplicityLevel` is `'lite' | 'full' | 'ultra'` in all tasks. `filterSimplicityLevels(body: string, level: SimplicityLevel): string` signature consistent across T2 and T4. `parseSimplicityLevel(rawArgs: string): SimplicityLevel` consistent across T2 and T4. `SIMPLICITY_FIRST_SKILL` is `SkillDefinition` — consistent with the `registerBuiltinSkill(skill: SkillDefinition)` signature in T3. No type mismatches across tasks.
