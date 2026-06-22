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

import { SaveIdeaReportTool } from '../../../src/tools/builtin/idea/save-idea-report';
import { executeTool } from '../fixtures/execute-tool';
import type { Agent } from '../../../src/agent';
import type { ExecutableToolContext } from '../../../src/loop/types';

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
      expect(path).toBe('/workspace/.ody-code/ideas/2026-06-22-ai-客服系统.md');
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
        '/workspace/.ody-code/ideas/2026-06-22-ai-客服系统.md',
        '/workspace/.ody-code/ideas/2026-06-22-ai-客服系统-1.md',
      ]);
      const exists = vi.fn().mockImplementation((p: string) => Promise.resolve(existing.has(p)));
      const path = await generateIdeaFilePath(
        '/workspace/.ody-code/ideas',
        'AI 客服系统',
        FIXED_DATE,
        exists,
      );
      expect(path).toBe('/workspace/.ody-code/ideas/2026-06-22-ai-客服系统-2.md');
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
      expect(body).toContain("date: '2026-06-22T12:34:56.789Z'");
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
        { role: 'user', origin: { kind: 'skill_activation', skillName: 'idea-generator', activationId: 'a', trigger: 'user-slash' }, content: [], toolCalls: [] },
        { role: 'assistant', content: [{ type: 'text', text: '...' }], toolCalls: [] },
      ];
      expect(isIdeaSkillActive(history)).toBe(true);
    });

    it('returns false when a later non-idea skill activation overrides the context', () => {
      const history: ContextMessage[] = [
        { role: 'user', origin: { kind: 'skill_activation', skillName: 'idea-generator', activationId: 'a', trigger: 'user-slash' }, content: [], toolCalls: [] },
        { role: 'user', origin: { kind: 'skill_activation', skillName: 'simplicity-first', activationId: 'b', trigger: 'user-slash' }, content: [], toolCalls: [] },
      ];
      expect(isIdeaSkillActive(history)).toBe(false);
    });

    it('returns false when no skill activation exists', () => {
      const history: ContextMessage[] = [
        { role: 'assistant', content: [{ type: 'text', text: '...' }], toolCalls: [] },
        { role: 'user', origin: { kind: 'user' }, content: [], toolCalls: [] },
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

const signal = new AbortController().signal;

function toolContext(args: unknown): ExecutableToolContext & { readonly args: unknown } {
  return { turnId: '0', toolCallId: 'call_save_idea', args, signal };
}

function mockAgentWithHistory(
  overrides: Partial<Agent> = {},
  history: ContextMessage[] = [],
): Agent {
  return {
    kaos: createFakeKaos(),
    config: { cwd: '/workspace' } as unknown as Agent['config'],
    context: { history },
    ...overrides,
  } as unknown as Agent;
}

describe('SaveIdeaReportTool', () => {
  it('exposes correct metadata and schema', () => {
    const tool = new SaveIdeaReportTool(mockAgentWithHistory());
    expect(tool.name).toBe('SaveIdeaReport');
    expect(tool.description).toContain('idea-generation');
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
      { role: 'user', origin: { kind: 'user' }, content: [], toolCalls: [] },
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
        config: { cwd: '/workspace' } as unknown as Agent['config'],
      },
      [
        { role: 'user', origin: { kind: 'skill_activation', skillName: 'idea-generator', activationId: 'a', trigger: 'user-slash' }, content: [], toolCalls: [] },
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
    // First writeText call is for .gitignore (from ensureGitignore), second is the report
    expect(writeText).toHaveBeenCalledTimes(2);
    const gitignoreCall = writeText.mock.calls[0]!;
    const gitignorePath = gitignoreCall[0];
    const gitignoreContent = gitignoreCall[1];
    expect(gitignorePath).toBe('/workspace/.gitignore');
    expect(gitignoreContent).toBe('.ody-code/\n');
    const reportCall = writeText.mock.calls[1]!;
    const writtenPath = reportCall[0];
    const writtenContent = reportCall[1];
    expect(writtenPath).toMatch(/\d{4}-\d{2}-\d{2}-ai-客服系统\.md$/);
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
        config: { cwd: '/workspace' } as unknown as Agent['config'],
      },
      [
        { role: 'user', origin: { kind: 'skill_activation', skillName: 'idea-generator', activationId: 'a', trigger: 'user-slash' }, content: [], toolCalls: [] },
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
        config: { cwd: '/workspace' } as unknown as Agent['config'],
      },
      [
        { role: 'user', origin: { kind: 'skill_activation', skillName: 'idea-generator', activationId: 'a', trigger: 'user-slash' }, content: [], toolCalls: [] },
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
