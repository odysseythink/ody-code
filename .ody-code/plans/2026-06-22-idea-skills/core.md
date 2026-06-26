# Phase A — Core runtime: helpers + `SaveIdeaReportTool` + permission policy

This phase builds the reusable logic and runtime pieces that the skills will depend on. It produces testable software on its own: the helpers, the tool, and the permission policy class are all exercised by unit tests before any wiring occurs.

## Shared constants used in this part

- `IDEA_SKILL_NAMES = ['idea-generator', 'idea-evaluator']`
- `SENSITIVE_TITLE_WORDS = ['key', 'token', 'password', 'secret', 'credential']`
- `MAX_SUFFIX = 1000`
- Output directory: `<cwd>/.ody-code/ideas/`
- Filename pattern: `YYYY-MM-DD-<slug>.md`, suffix `-1`, `-2`, … on collision

---

### Task A1: Idea report helpers

**Depends on:** none

**Files:**
- Create: `packages/agent-core/src/tools/builtin/idea/report-helpers.ts`
- Create: `packages/agent-core/src/utils/gitignore.ts`
- Modify: `packages/agent-core/src/agent/session-mode/index.ts` (replace private `ensureGitignore` with call to shared helper)
- Test: `packages/agent-core/test/tools/idea/save-idea-report.test.ts`

This task extracts the filename generation, frontmatter assembly, input validation, context guard, and `.ody-code/` gitignore logic into reusable helpers. It also extracts `ensureGitignore` so both `SessionMode` and `SaveIdeaReportTool` can share it.

- [ ] **Write the failing test.** Create `packages/agent-core/test/tools/idea/save-idea-report.test.ts` with the helper tests:

```typescript
import { describe, expect, it, vi } from 'vitest';
import { join } from 'pathe';

import {
  buildIdeaReportBody,
  generateIdeaFilePath,
  ensureIdeasDirectory,
  isIdeaSkillActive,
  validateIdeaReportInput,
  type SaveIdeaReportInput,
} from '../../../src/tools/builtin/idea/report-helpers';
import { createFakeKaos } from '../fixtures/fake-kaos';
import type { ContextMessage } from '../../../src/agent/context/types';

const FIXED_DATE = new Date('2026-06-22T12:34:56.789Z');

describe('idea report helpers', () => {
  describe('validateIdeaReportInput', () => {
    it('accepts a valid generator input', () => {
      const result = validateIdeaReportInput({
        title: 'AI 客服系统',
        content: '## Report',
        type: 'generator',
      });
      expect(result).toEqual({
        ok: true,
        data: {
          title: 'AI 客服系统',
          content: '## Report',
          type: 'generator',
        },
      });
    });

    it('accepts a valid evaluator input with score and tags', () => {
      const result = validateIdeaReportInput({
        title: 'B2B 发票助手',
        content: '## Report',
        type: 'evaluator',
        score: 7.5,
        tags: ['B2B', 'AI', '  AI  ', ''],
      });
      expect(result).toEqual({
        ok: true,
        data: {
          title: 'B2B 发票助手',
          content: '## Report',
          type: 'evaluator',
          score: 7.5,
          tags: ['B2B', 'AI'],
        },
      });
    });

    it('rejects empty titles', () => {
      const result = validateIdeaReportInput({
        title: '   ',
        content: '## Report',
        type: 'generator',
      });
      expect(result.ok).toBe(false);
      if (result.ok) return;
      expect(result.error).toContain('title');
    });

    it('rejects titles containing sensitive words (must-survive: "monkey" stays valid)', () => {
      const survive = validateIdeaReportInput({
        title: 'My monkey idea',
        content: 'x',
        type: 'generator',
      });
      expect(survive.ok).toBe(true);

      const fail = validateIdeaReportInput({
        title: 'My secret API key idea',
        content: 'x',
        type: 'generator',
      });
      expect(fail.ok).toBe(false);
      if (fail.ok) return;
      expect(fail.error).toContain('sensitive');
    });

    it('rejects scores outside [0, 10]', () => {
      const result = validateIdeaReportInput({
        title: 'Good idea',
        content: 'x',
        type: 'evaluator',
        score: 11,
      });
      expect(result.ok).toBe(false);
    });

    it('rejects invalid types', () => {
      const result = validateIdeaReportInput({
        title: 'Good idea',
        content: 'x',
        type: 'reviewer',
      });
      expect(result.ok).toBe(false);
    });
  });

  describe('generateIdeaFilePath', () => {
    it('generates YYYY-MM-DD-<slug>.md from title', async () => {
      const exists = vi.fn().mockResolvedValue(false);
      const path = await generateIdeaFilePath(
        '/workspace/.ody-code/ideas',
        'AI 客服系统',
        FIXED_DATE,
        exists,
      );
      expect(path).toBe('/workspace/.ody-code/ideas/2026-06-22-ai-ke-fu-xi-tong.md');
    });

    it('strips an existing date prefix from the title to avoid doubling', async () => {
      const exists = vi.fn().mockResolvedValue(false);
      const path = await generateIdeaFilePath(
        '/workspace/.ody-code/ideas',
        '2026-06-22-ai-kefu',
        FIXED_DATE,
        exists,
      );
      expect(path).toBe('/workspace/.ody-code/ideas/2026-06-22-ai-kefu.md');
    });

    it('falls back to untitled when the title yields an empty slug', async () => {
      const exists = vi.fn().mockResolvedValue(false);
      const path = await generateIdeaFilePath(
        '/workspace/.ody-code/ideas',
        '2026-06-22',
        FIXED_DATE,
        exists,
      );
      expect(path).toBe('/workspace/.ody-code/ideas/2026-06-22-untitled.md');
    });

    it('adds -1, -2 suffixes on collision', async () => {
      const existing = new Set([
        '/workspace/.ody-code/ideas/2026-06-22-ai-ke-fu-xi-tong.md',
        '/workspace/.ody-code/ideas/2026-06-22-ai-ke-fu-xi-tong-1.md',
      ]);
      const exists = vi.fn().mockImplementation((p: string) => Promise.resolve(existing.has(p)));
      const path = await generateIdeaFilePath(
        '/workspace/.ody-code/ideas',
        'AI 客服系统',
        FIXED_DATE,
        exists,
      );
      expect(path).toBe('/workspace/.ody-code/ideas/2026-06-22-ai-ke-fu-xi-tong-2.md');
    });
  });

  describe('buildIdeaReportBody', () => {
    it('builds frontmatter with score and tags', () => {
      const input: SaveIdeaReportInput = {
        title: 'B2B 发票助手',
        content: '## 想法质检报告\n\n内容',
        type: 'evaluator',
        score: 7.5,
        tags: ['B2B', 'AI'],
      };
      const body = buildIdeaReportBody(input, FIXED_DATE);
      expect(body).toContain('title: B2B 发票助手');
      expect(body).toContain('type: evaluator');
      expect(body).toContain('date: 2026-06-22T12:34:56.789Z');
      expect(body).toContain('score: 7.5');
      expect(body).toContain('tags:');
      expect(body).toContain('- B2B');
      expect(body).toContain('- AI');
      expect(body).toContain('## 想法质检报告\n\n内容');
    });

    it('omits score when undefined and writes empty tags as []', () => {
      const input: SaveIdeaReportInput = {
        title: 'AI 客服系统',
        content: '## Report',
        type: 'generator',
        tags: [],
      };
      const body = buildIdeaReportBody(input, FIXED_DATE);
      expect(body).not.toContain('score:');
      expect(body).toContain('tags: []');
    });

    it('trims trailing whitespace from content', () => {
      const input: SaveIdeaReportInput = {
        title: 'x',
        content: '  content  ',
        type: 'generator',
      };
      const body = buildIdeaReportBody(input, FIXED_DATE);
      expect(body).toMatch(/---\n\ncontent\n$/);
    });
  });

  describe('isIdeaSkillActive', () => {
    it('returns true when the most recent skill activation is an idea skill', () => {
      const history: ContextMessage[] = [
        { role: 'user', origin: { kind: 'skill_activation', skillName: 'idea-generator', activationId: 'a', trigger: 'user-slash' }, content: [] },
        { role: 'assistant', content: [{ type: 'text', text: '...' }] },
      ];
      expect(isIdeaSkillActive(history)).toBe(true);
    });

    it('returns false when a later non-idea skill activation overrides the context', () => {
      const history: ContextMessage[] = [
        { role: 'user', origin: { kind: 'skill_activation', skillName: 'idea-generator', activationId: 'a', trigger: 'user-slash' }, content: [] },
        { role: 'user', origin: { kind: 'skill_activation', skillName: 'simplicity-first', activationId: 'b', trigger: 'user-slash' }, content: [] },
      ];
      expect(isIdeaSkillActive(history)).toBe(false);
    });

    it('returns false when no skill activation exists', () => {
      const history: ContextMessage[] = [
        { role: 'assistant', content: [{ type: 'text', text: '...' }] },
        { role: 'user', origin: { kind: 'user' }, content: [] },
      ];
      expect(isIdeaSkillActive(history)).toBe(false);
    });
  });

  describe('ensureIdeasDirectory', () => {
    it('creates .ody-code/ideas and appends .ody-code/ to .gitignore', async () => {
      const mkdir = vi.fn().mockResolvedValue(undefined);
      const readText = vi.fn().mockResolvedValue('node_modules/\n');
      const writeText = vi.fn().mockResolvedValue(undefined);
      const kaos = createFakeKaos({ mkdir, readText, writeText });

      const dir = await ensureIdeasDirectory('/workspace', kaos);

      expect(dir).toBe('/workspace/.ody-code/ideas');
      expect(mkdir).toHaveBeenCalledWith('/workspace/.ody-code/ideas', { parents: true, existOk: true });
      expect(writeText).toHaveBeenCalledWith(
        '/workspace/.gitignore',
        'node_modules/\n.ody-code/\n',
      );
    });

    it('creates .gitignore when it does not exist', async () => {
      const mkdir = vi.fn().mockResolvedValue(undefined);
      const readText = vi.fn().mockRejectedValue(Object.assign(new Error('ENOENT'), { code: 'ENOENT' }));
      const writeText = vi.fn().mockResolvedValue(undefined);
      const kaos = createFakeKaos({ mkdir, readText, writeText });

      await ensureIdeasDirectory('/workspace', kaos);

      expect(writeText).toHaveBeenCalledWith('/workspace/.gitignore', '.ody-code/\n');
    });

    it('does not duplicate the gitignore entry', async () => {
      const mkdir = vi.fn().mockResolvedValue(undefined);
      const readText = vi.fn().mockResolvedValue('node_modules/\n.ody-code/\n');
      const writeText = vi.fn().mockResolvedValue(undefined);
      const kaos = createFakeKaos({ mkdir, readText, writeText });

      await ensureIdeasDirectory('/workspace', kaos);

      expect(writeText).not.toHaveBeenCalled();
    });
  });
});
```

- [ ] **Run it and verify it FAILS.**

```bash
pnpm test packages/agent-core/test/tools/idea/save-idea-report.test.ts
```

Expected failure: modules `../../../src/tools/builtin/idea/report-helpers` and `../../../src/utils/gitignore` cannot be resolved.

- [ ] **Write the minimal implementation.**

Create `packages/agent-core/src/utils/gitignore.ts`:

```typescript
import { join } from 'pathe';
import type { Kaos } from '@odysseythink/kaos';

const GITIGNORE_ENTRY = '.ody-code/';

export async function ensureGitignore(
  cwd: string,
  kaos: Pick<Kaos, 'readText' | 'writeText'>,
): Promise<void> {
  const gitignorePath = join(cwd, '.gitignore');
  try {
    const content = await kaos.readText(gitignorePath);
    if (content.trim().length === 0) {
      await kaos.writeText(gitignorePath, GITIGNORE_ENTRY + '\n');
      return;
    }
    const lines = content.split('\n');
    for (const line of lines) {
      if (line.trim() === GITIGNORE_ENTRY) {
        return;
      }
    }
    const separator = content.endsWith('\n') ? '' : '\n';
    await kaos.writeText(gitignorePath, content + separator + GITIGNORE_ENTRY + '\n');
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      await kaos.writeText(gitignorePath, GITIGNORE_ENTRY + '\n');
    } else {
      throw error;
    }
  }
}
```

Create `packages/agent-core/src/tools/builtin/idea/report-helpers.ts`:

```typescript
import { dump as dumpYaml } from 'js-yaml';
import { join } from 'pathe';
import type { Kaos } from '@odysseythink/kaos';

import type { ContextMessage } from '../../../agent/context/types';
import {
  formatDatePrefix,
  slugifyTitle,
  stripDatePrefix,
} from '../../../agent/session-mode/topic-generator';
import { ensureGitignore } from '../../../utils/gitignore';

export type IdeaReportType = 'generator' | 'evaluator';

export interface SaveIdeaReportInput {
  readonly title: string;
  readonly content: string;
  readonly type: IdeaReportType;
  readonly score?: number;
  readonly tags?: readonly string[];
}

export const IDEA_SKILL_NAMES = ['idea-generator', 'idea-evaluator'] as const;

export const SENSITIVE_TITLE_WORDS = [
  'key',
  'token',
  'password',
  'secret',
  'credential',
] as const;

export const MAX_SUFFIX = 1000;

export function validateIdeaReportInput(
  input: unknown,
): { ok: true; data: SaveIdeaReportInput } | { ok: false; error: string } {
  if (input === null || typeof input !== 'object') {
    return { ok: false, error: 'Input must be an object' };
  }
  const record = input as Record<string, unknown>;

  if (typeof record.title !== 'string' || record.title.trim().length === 0) {
    return { ok: false, error: 'title is required and must be non-empty' };
  }
  const title = record.title.trim();
  const lowerTitle = title.toLowerCase();
  if (SENSITIVE_TITLE_WORDS.some((word) => lowerTitle.includes(word))) {
    return { ok: false, error: 'title contains sensitive words; provide a different title' };
  }

  if (typeof record.content !== 'string') {
    return { ok: false, error: 'content must be a string' };
  }

  if (record.type !== 'generator' && record.type !== 'evaluator') {
    return { ok: false, error: 'type must be "generator" or "evaluator"' };
  }

  if (record.score !== undefined) {
    if (
      typeof record.score !== 'number' ||
      !Number.isFinite(record.score) ||
      record.score < 0 ||
      record.score > 10
    ) {
      return { ok: false, error: 'score must be a number between 0 and 10' };
    }
  }

  let tags: string[] | undefined;
  if (record.tags !== undefined) {
    if (!Array.isArray(record.tags)) {
      return { ok: false, error: 'tags must be an array of strings' };
    }
    const seen = new Set<string>();
    tags = [];
    for (const raw of record.tags) {
      if (typeof raw !== 'string') continue;
      const tag = raw.trim();
      if (tag.length === 0 || seen.has(tag)) continue;
      seen.add(tag);
      tags.push(tag);
    }
  }

  return {
    ok: true,
    data: {
      title,
      content: record.content,
      type: record.type,
      score: record.score,
      tags,
    },
  };
}

export async function generateIdeaFilePath(
  ideasDir: string,
  title: string,
  now: Date,
  exists: (path: string) => Promise<boolean>,
): Promise<string> {
  let slug = slugifyTitle(title);
  slug = stripDatePrefix(slug);
  const baseStem = `${formatDatePrefix(now)}-${slug || 'untitled'}`;

  let stem = baseStem;
  for (let suffix = 1; suffix <= MAX_SUFFIX; suffix++) {
    const candidate = join(ideasDir, `${stem}.md`);
    if (!(await exists(candidate))) {
      return candidate;
    }
    stem = `${baseStem}-${suffix}`;
  }
  return join(ideasDir, `${baseStem}-${Date.now()}.md`);
}

export function buildIdeaReportBody(input: SaveIdeaReportInput, now: Date): string {
  const frontmatter: Record<string, unknown> = {
    title: input.title,
    type: input.type,
    date: now.toISOString(),
  };
  if (input.score !== undefined) {
    frontmatter.score = input.score;
  }
  if (input.tags !== undefined) {
    frontmatter.tags = input.tags;
  }
  const yaml = dumpYaml(frontmatter, { lineWidth: -1, noRefs: true }).trim();
  return `---\n${yaml}\n---\n\n${input.content.trim()}\n`;
}

export function isIdeaSkillActive(history: readonly ContextMessage[]): boolean {
  for (let index = history.length - 1; index >= 0; index--) {
    const message = history[index];
    if (message?.role !== 'user') continue;
    const origin = message.origin;
    if (origin?.kind !== 'skill_activation') continue;
    return IDEA_SKILL_NAMES.includes(origin.skillName);
  }
  return false;
}

export async function ensureIdeasDirectory(
  cwd: string,
  kaos: Pick<Kaos, 'mkdir' | 'readText' | 'writeText'>,
): Promise<string> {
  const ideasDir = join(cwd, '.ody-code', 'ideas');
  await kaos.mkdir(ideasDir, { parents: true, existOk: true });
  await ensureGitignore(cwd, kaos);
  return ideasDir;
}
```

Modify `packages/agent-core/src/agent/session-mode/index.ts` (replace the private `ensureGitignore` method body with the shared helper). Locate the existing private method at lines 691–715 and replace it with:

```typescript
import { ensureGitignore } from '../../utils/gitignore';
// ... existing imports ...

  private async ensureGitignore(cwd: string): Promise<void> {
    await ensureGitignore(cwd, this.agent.kaos);
  }
```

Keep the method as a thin wrapper so the rest of `SessionMode` does not change.

- [ ] **Run it and verify it PASSES.**

```bash
pnpm test packages/agent-core/test/tools/idea/save-idea-report.test.ts
```

Expected: all helper tests pass.

- [ ] **Commit.**

```bash
git add packages/agent-core/src/utils/gitignore.ts packages/agent-core/src/tools/builtin/idea/report-helpers.ts packages/agent-core/src/agent/session-mode/index.ts packages/agent-core/test/tools/idea/save-idea-report.test.ts
git commit -m "feat(agent-core): add idea report helpers"
```

---

### Task A2: `SaveIdeaReportTool`

**Depends on:** Task A1

**Files:**
- Create: `packages/agent-core/src/tools/builtin/idea/save-idea-report.ts`
- Create: `packages/agent-core/src/tools/builtin/idea/save-idea-report.md`
- Test: `packages/agent-core/test/tools/idea/save-idea-report.test.ts` (append tool tests)

This task implements the built-in tool that the idea skills will instruct the model to call.

- [ ] **Write the failing test.** Append the following tests to `packages/agent-core/test/tools/idea/save-idea-report.test.ts`:

```typescript
import { SaveIdeaReportTool } from '../../../src/tools/builtin/idea/save-idea-report';
import { executeTool } from '../fixtures/execute-tool';
import type { Agent } from '../../../src/agent';
import type { ContextMessage } from '../../../src/agent/context/types';

const signal = new AbortController().signal;

function toolContext(args: unknown) {
  return { turnId: '0', toolCallId: 'call_save_idea', args, signal };
}

function mockAgentWithHistory(
  overrides: Partial<Agent> = {},
  history: ContextMessage[] = [],
): Agent {
  return {
    kaos: createFakeKaos(),
    config: { cwd: '/workspace' },
    context: { history },
    ...overrides,
  } as unknown as Agent;
}

describe('SaveIdeaReportTool', () => {
  it('exposes correct metadata and schema', () => {
    const tool = new SaveIdeaReportTool(mockAgentWithHistory());
    expect(tool.name).toBe('SaveIdeaReport');
    expect(tool.description).toContain('SaveIdeaReport');
    expect(tool.parameters).toMatchObject({
      type: 'object',
      required: expect.arrayContaining(['title', 'content', 'type']),
      properties: {
        title: { type: 'string' },
        content: { type: 'string' },
        type: { enum: ['generator', 'evaluator'] },
        score: { type: 'number' },
        tags: { type: 'array', items: { type: 'string' } },
      },
    });
  });

  it('returns an error when no idea skill is active', async () => {
    const agent = mockAgentWithHistory(undefined, [
      { role: 'user', origin: { kind: 'user' }, content: [] },
    ]);
    const tool = new SaveIdeaReportTool(agent);

    const result = await executeTool(tool, toolContext({
      title: 'AI 客服系统',
      content: '## Report',
      type: 'generator',
    }));

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('idea-generator');
    expect(result.output).toContain('idea-evaluator');
  });

  it('writes the report when idea-generator is active', async () => {
    const mkdir = vi.fn().mockResolvedValue(undefined);
    const writeText = vi.fn().mockResolvedValue(42);
    const stat = vi.fn().mockRejectedValue(Object.assign(new Error('ENOENT'), { code: 'ENOENT' }));
    const readText = vi.fn().mockResolvedValue('');
    const agent = mockAgentWithHistory(
      {
        kaos: createFakeKaos({ mkdir, writeText, stat, readText }),
        config: { cwd: '/workspace' },
      },
      [
        { role: 'user', origin: { kind: 'skill_activation', skillName: 'idea-generator', activationId: 'a', trigger: 'user-slash' }, content: [] },
      ],
    );
    const tool = new SaveIdeaReportTool(agent);

    const result = await executeTool(tool, toolContext({
      title: 'AI 客服系统',
      content: '## 想法生成报告\n\n内容',
      type: 'generator',
      tags: ['AI'],
    }));

    expect(result.isError).toBeUndefined();
    expect(mkdir).toHaveBeenCalledWith('/workspace/.ody-code/ideas', { parents: true, existOk: true });
    expect(writeText).toHaveBeenCalledTimes(1);
    const [writtenPath, writtenContent] = writeText.mock.calls[0];
    expect(writtenPath).toMatch(/2026-06-22-ai-ke-fu-xi-tong\.md$/);
    expect(writtenContent).toContain('title: AI 客服系统');
    expect(writtenContent).toContain('type: generator');
    expect(writtenContent).toContain('## 想法生成报告');
    expect(result.output).toContain(writtenPath);
  });

  it('returns an error for invalid input without writing', async () => {
    const writeText = vi.fn().mockResolvedValue(1);
    const agent = mockAgentWithHistory(
      {
        kaos: createFakeKaos({ writeText }),
        config: { cwd: '/workspace' },
      },
      [
        { role: 'user', origin: { kind: 'skill_activation', skillName: 'idea-generator', activationId: 'a', trigger: 'user-slash' }, content: [] },
      ],
    );
    const tool = new SaveIdeaReportTool(agent);

    const result = await executeTool(tool, toolContext({
      title: '',
      content: '## Report',
      type: 'generator',
    }));

    expect(result).toMatchObject({ isError: true });
    expect(writeText).not.toHaveBeenCalled();
  });

  it('surfaces I/O errors without crashing', async () => {
    const mkdir = vi.fn().mockRejectedValue(new Error('permission denied'));
    const agent = mockAgentWithHistory(
      {
        kaos: createFakeKaos({ mkdir }),
        config: { cwd: '/workspace' },
      },
      [
        { role: 'user', origin: { kind: 'skill_activation', skillName: 'idea-generator', activationId: 'a', trigger: 'user-slash' }, content: [] },
      ],
    );
    const tool = new SaveIdeaReportTool(agent);

    const result = await executeTool(tool, toolContext({
      title: 'Good idea',
      content: '## Report',
      type: 'generator',
    }));

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('permission denied');
  });
});
```

- [ ] **Run it and verify it FAILS.**

```bash
pnpm test packages/agent-core/test/tools/idea/save-idea-report.test.ts
```

Expected failure: module `../../../src/tools/builtin/idea/save-idea-report` cannot be resolved.

- [ ] **Write the minimal implementation.**

Create `packages/agent-core/src/tools/builtin/idea/save-idea-report.md`:

```markdown
Save the current idea-generation or idea-evaluation report to the project under `.ody-code/ideas/`. Only available after the `idea-generator` or `idea-evaluator` skill has been activated in this conversation. The file name is generated automatically from `title`; do not include dates or sensitive words in the title.
```

Create `packages/agent-core/src/tools/builtin/idea/save-idea-report.ts`:

```typescript
import { z } from 'zod';
import type { Agent } from '../../../agent';
import type { BuiltinTool } from '../../../agent/tool';
import { ToolAccesses } from '../../../loop/tool-access';
import type { ExecutableToolResult, ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import {
  buildIdeaReportBody,
  ensureIdeasDirectory,
  generateIdeaFilePath,
  isIdeaSkillActive,
  validateIdeaReportInput,
  type SaveIdeaReportInput,
} from './report-helpers';
import SAVE_IDEA_REPORT_DESCRIPTION from './save-idea-report.md';

export const SaveIdeaReportInputSchema = z.object({
  title: z.string().describe('Short, filesystem-safe title for the report.'),
  content: z.string().describe('Full Markdown report body.'),
  type: z.enum(['generator', 'evaluator']).describe('Report kind.'),
  score: z
    .number()
    .min(0)
    .max(10)
    .optional()
    .describe('Final 0-10 score; required for evaluator reports.'),
  tags: z
    .array(z.string())
    .optional()
    .describe('Optional tags such as ["B2B", "AI"].'),
});

export type SaveIdeaReportInputValidated = z.infer<typeof SaveIdeaReportInputSchema>;

export class SaveIdeaReportTool implements BuiltinTool<SaveIdeaReportInputValidated> {
  readonly name = 'SaveIdeaReport' as const;
  readonly description = SAVE_IDEA_REPORT_DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(SaveIdeaReportInputSchema);

  constructor(private readonly agent: Agent) {}

  async resolveExecution(args: SaveIdeaReportInputValidated): Promise<ToolExecution> {
    if (!isIdeaSkillActive(this.agent.context?.history ?? [])) {
      return {
        isError: true,
        output:
          'SaveIdeaReport can only be used after idea-generator or idea-evaluator has been activated.',
      };
    }

    const validation = validateIdeaReportInput(args);
    if (!validation.ok) {
      return {
        isError: true,
        output: validation.error,
      };
    }

    const { data } = validation;
    const cwd = this.agent.config.cwd;
    const ideasDir = await ensureIdeasDirectory(cwd, this.agent.kaos);
    const filePath = await generateIdeaFilePath(ideasDir, data.title, new Date(), async (p) => {
      try {
        await this.agent.kaos.stat(p);
        return true;
      } catch {
        return false;
      }
    });

    const body = buildIdeaReportBody(data, new Date());

    return {
      accesses: ToolAccesses.writeFile(filePath),
      description: `Saving idea report to ${filePath}`,
      display: { kind: 'file_io', operation: 'write', path: filePath, content: body },
      approvalRule: this.name,
      execute: async () => this.execution(filePath, body),
    };
  }

  private async execution(
    filePath: string,
    body: string,
  ): Promise<ExecutableToolResult> {
    try {
      await this.agent.kaos.writeText(filePath, body);
      return { output: `Saved idea report to ${filePath}` };
    } catch (error) {
      return {
        isError: true,
        output: error instanceof Error ? error.message : String(error),
      };
    }
  }
}
```

- [ ] **Run it and verify it PASSES.**

```bash
pnpm test packages/agent-core/test/tools/idea/save-idea-report.test.ts
```

Expected: all helper and tool tests pass.

- [ ] **Commit.**

```bash
git add packages/agent-core/src/tools/builtin/idea/save-idea-report.ts packages/agent-core/src/tools/builtin/idea/save-idea-report.md packages/agent-core/test/tools/idea/save-idea-report.test.ts
git commit -m "feat(agent-core): add SaveIdeaReport tool"
```

---

### Task A3: `IdeaToolDirectoryApprovePermissionPolicy`

**Depends on:** none (only depends on existing permission types; it does not depend on the tool or helpers)

**Files:**
- Create: `packages/agent-core/src/agent/permission/policies/idea-tool-directory.ts`
- Test: `packages/agent-core/test/agent/permission/idea-tool-directory.test.ts`

This task implements the permission policy that auto-approves writes under `.ody-code/ideas/`. Registration into the policy chain happens in Phase C.

- [ ] **Write the failing test.** Create `packages/agent-core/test/agent/permission/idea-tool-directory.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';

import { IdeaToolDirectoryApprovePermissionPolicy } from '../../../src/agent/permission/policies/idea-tool-directory';
import type { PermissionPolicyContext } from '../../../src/agent/permission/types';
import type { Agent } from '../../../src/agent';
import { createFakeKaos } from '../../tools/fixtures/fake-kaos';

function mockContext(toolName: string, paths: string[]): PermissionPolicyContext {
  return {
    toolCall: { name: toolName, id: 'call_1', arguments: {} },
    execution: {
      accesses: paths.map((path) => ({ kind: 'file' as const, operation: 'write' as const, path })),
      approvalRule: toolName,
    },
  } as unknown as PermissionPolicyContext;
}

function mockAgent(cwd: string): Agent {
  return {
    config: { cwd },
    kaos: createFakeKaos(),
  } as unknown as Agent;
}

describe('IdeaToolDirectoryApprovePermissionPolicy', () => {
  it('approves writes directly under .ody-code/ideas/', () => {
    const agent = mockAgent('/workspace');
    const policy = new IdeaToolDirectoryApprovePermissionPolicy(agent);
    const result = policy.evaluate(mockContext('SaveIdeaReport', [
      '/workspace/.ody-code/ideas/2026-06-22-foo.md',
    ]));
    expect(result).toEqual({ kind: 'approve' });
  });

  it('approves writes in nested subdirectories of .ody-code/ideas/', () => {
    const agent = mockAgent('/workspace');
    const policy = new IdeaToolDirectoryApprovePermissionPolicy(agent);
    const result = policy.evaluate(mockContext('SaveIdeaReport', [
      '/workspace/.ody-code/ideas/archive/2026-06-22-foo.md',
    ]));
    expect(result).toEqual({ kind: 'approve' });
  });

  it('does not approve writes to .ody-code/plans/', () => {
    const agent = mockAgent('/workspace');
    const policy = new IdeaToolDirectoryApprovePermissionPolicy(agent);
    const result = policy.evaluate(mockContext('SaveIdeaReport', [
      '/workspace/.ody-code/plans/2026-06-22-foo.md',
    ]));
    expect(result).toBeUndefined();
  });

  it('does not approve writes that escape ideas via traversal', () => {
    const agent = mockAgent('/workspace');
    const policy = new IdeaToolDirectoryApprovePermissionPolicy(agent);
    const result = policy.evaluate(mockContext('SaveIdeaReport', [
      '/workspace/.ody-code/ideas/../plans/foo.md',
    ]));
    expect(result).toBeUndefined();
  });

  it('does not approve reads under .ody-code/ideas/', () => {
    const agent = mockAgent('/workspace');
    const policy = new IdeaToolDirectoryApprovePermissionPolicy(agent);
    const context = {
      toolCall: { name: 'Read', id: 'call_1', arguments: {} },
      execution: {
        accesses: [{ kind: 'file', operation: 'read', path: '/workspace/.ody-code/ideas/foo.md' }],
        approvalRule: 'Read',
      },
    } as unknown as PermissionPolicyContext;
    const result = policy.evaluate(context);
    expect(result).toBeUndefined();
  });
});
```

- [ ] **Run it and verify it FAILS.**

```bash
pnpm test packages/agent-core/test/agent/permission/idea-tool-directory.test.ts
```

Expected failure: module `../../../src/agent/permission/policies/idea-tool-directory` cannot be resolved.

- [ ] **Write the minimal implementation.**

Create `packages/agent-core/src/agent/permission/policies/idea-tool-directory.ts`:

```typescript
import { join, normalize } from 'pathe';

import type { Agent } from '../..';
import type { PermissionPolicy, PermissionPolicyContext, PermissionPolicyResult } from '../types';

export class IdeaToolDirectoryApprovePermissionPolicy implements PermissionPolicy {
  readonly name = 'idea-tool-directory-approve';

  constructor(private readonly agent: Agent) {}

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    const cwd = this.agent.config.cwd;
    if (cwd.length === 0) return;

    const ideasDir = normalize(join(cwd, '.ody-code', 'ideas'));
    const prefix = ideasDir.endsWith('/') ? ideasDir : `${ideasDir}/`;

    const accesses = context.execution.accesses ?? [];
    for (const access of accesses) {
      if (access.kind !== 'file') continue;
      if (access.operation !== 'write' && access.operation !== 'readwrite') continue;
      const normalizedPath = normalize(access.path);
      if (!normalizedPath.startsWith(prefix)) {
        return;
      }
    }

    if (accesses.length === 0) return;
    return { kind: 'approve' };
  }
}
```

- [ ] **Run it and verify it PASSES.**

```bash
pnpm test packages/agent-core/test/agent/permission/idea-tool-directory.test.ts
```

Expected: all policy tests pass.

- [ ] **Commit.**

```bash
git add packages/agent-core/src/agent/permission/policies/idea-tool-directory.ts packages/agent-core/test/agent/permission/idea-tool-directory.test.ts
git commit -m "feat(agent-core): add idea-tool-directory permission policy"
```

---

## Local Self-Review (Phase A)

- [ ] **Spec coverage:** Task A1 covers filename generation, frontmatter, validation, context guard, directory creation, and gitignore maintenance. Task A2 covers the tool and error handling. Task A3 covers the auto-approve policy scope. All Phase A responsibilities from the design are present.
- [ ] **No placeholders:** Every step contains real file paths, code, commands, and expected output. No TODO/TBD.
- [ ] **No phantom tasks:** Each task creates/modifies files and ends with a passing test + commit.
- [ ] **Dependency soundness:** A2 depends on A1 (uses helpers). A3 is independent. No task references a later symbol.
- [ ] **Caller & build soundness:** Task A1 modifies `SessionMode` to call the extracted `ensureGitignore`. This is a private-method refactor, not a shared-signature change. Still, end Phase A with a single-package typecheck:

```bash
pnpm --filter @odysseythink/agent-core typecheck
```

- [ ] **Test-the-risk:**
  - Filename collisions: `-1`/`-2` suffix test.
  - Sensitive title filtering: must-survive `"monkey"` is checked.
  - Context guard: adversarial override-by-later-skill test.
  - Policy scope: traversal escape (`../plans/`) and wrong-directory tests.
  - I/O failure: permission-denied error propagation test.
- [ ] **Type consistency:** `SaveIdeaReportInput` in helpers matches the Zod schema in the tool. `IdeaReportType` is used consistently as `'generator' | 'evaluator'`.
