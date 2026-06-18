import { describe, expect, it, vi } from 'vitest';
import { createCodeReviewExecutor } from '../../src/code-review/executor';
import type { CodeReviewRequestInput, CodeReviewReport } from '../../src/code-review/types';
import type { RepoAuditDigest } from '../../src/code-review/simplicity';

function fakeGenerate(text: string) {
  return vi.fn(async () => ({
    message: { role: 'assistant', content: [{ type: 'text', text }] },
    usage: { input: 100, output: 50 },
    stopReason: 'end_turn',
  }));
}

describe('createCodeReviewExecutor', () => {
  const cwd = '/app';
  const modelAlias = 'reviewer';

  it('returns ok=false when diff fetch fails', async () => {
    const executor = createCodeReviewExecutor({
      cwd,
      fetchDiff: vi.fn(async () => { throw new Error('not a git repo'); }),
      generate: fakeGenerate('') as unknown as (opts: { modelAlias: string; systemPrompt: string; userPrompt: string; signal?: AbortSignal }) => Promise<{ message: { role: string; content: Array<{ type: string; text: string }> } }>,
      resolveProviderConfig: vi.fn(() => ({})),
      estimateTokens: vi.fn(() => 0),
    });
    const report: CodeReviewReport = await executor.review({
      source: { kind: 'commits', base: 'x', head: 'y' },
      modelAlias,
    });
    expect(report.ok).toBe(false);
    expect(report.note).toContain('not a git repo');
  });

  it('returns ok=false when diff exceeds token limit', async () => {
    const executor = createCodeReviewExecutor({
      cwd,
      fetchDiff: vi.fn(async () => 'x'.repeat(100_000)),
      generate: fakeGenerate('') as unknown as (opts: { modelAlias: string; systemPrompt: string; userPrompt: string; signal?: AbortSignal }) => Promise<{ message: { role: string; content: Array<{ type: string; text: string }> } }>,
      resolveProviderConfig: vi.fn(() => ({})),
      estimateTokens: vi.fn(() => 200_000),
    });
    const report: CodeReviewReport = await executor.review({
      source: { kind: 'working-tree' },
      modelAlias,
    });
    expect(report.ok).toBe(false);
    expect(report.note).toContain('token');
  });

  it('generates a report on successful LLM response', async () => {
    const llmText = [
      'Strengths:',
      '- Good code',
      '',
      'Findings:',
      'Critical:',
      '',
      'Important:',
      '- [edge case] (src/foo.ts)',
      '  No null check',
      '  fix: add guard',
      '',
      'Minor:',
      '',
      'Assessment: Ready to proceed',
    ].join('\n');

    const executor = createCodeReviewExecutor({
      cwd,
      fetchDiff: vi.fn(async () => 'mock diff'),
      generate: fakeGenerate(llmText) as unknown as (opts: { modelAlias: string; systemPrompt: string; userPrompt: string; signal?: AbortSignal }) => Promise<{ message: { role: string; content: Array<{ type: string; text: string }> } }>,
      resolveProviderConfig: vi.fn(() => ({})),
      estimateTokens: vi.fn(() => 10),
    });
    const report: CodeReviewReport = await executor.review({
      source: { kind: 'working-tree' },
      modelAlias,
    });
    expect(report.ok).toBe(true);
    expect(report.findings).toHaveLength(1);
    expect(report.findings[0]!.severity).toBe('important');
    expect(report.findings[0]!.title).toBe('edge case');
  });

  it('calls deepRunner when deep is true', async () => {
    const deepRunner = vi.fn(async () => ({
      ok: true,
      reviewerAlias: 'deep-reviewer',
      findings: [{ severity: 'critical' as const, title: 'deep finding', detail: 'found by subagent' }],
    }));
    const executor = createCodeReviewExecutor({
      cwd,
      fetchDiff: vi.fn(async () => 'mock diff'),
      generate: fakeGenerate('') as unknown as (opts: { modelAlias: string; systemPrompt: string; userPrompt: string; signal?: AbortSignal }) => Promise<{ message: { role: string; content: Array<{ type: string; text: string }> } }>,
      resolveProviderConfig: vi.fn(() => ({})),
      estimateTokens: vi.fn(() => 10),
      deepRunner,
    });
    const report: CodeReviewReport = await executor.review({
      source: { kind: 'working-tree' },
      modelAlias,
      deep: true,
    });
    expect(deepRunner).toHaveBeenCalledOnce();
    expect(report.reviewerAlias).toBe('deep-reviewer');
  });

  it('returns ok=false when deepRunner not provided but deep is true', async () => {
    const executor = createCodeReviewExecutor({
      cwd,
      fetchDiff: vi.fn(async () => 'mock diff'),
      generate: fakeGenerate('') as unknown as (opts: { modelAlias: string; systemPrompt: string; userPrompt: string; signal?: AbortSignal }) => Promise<{ message: { role: string; content: Array<{ type: string; text: string }> } }>,
      resolveProviderConfig: vi.fn(() => ({})),
      estimateTokens: vi.fn(() => 10),
    });
    const report: CodeReviewReport = await executor.review({
      source: { kind: 'working-tree' },
      modelAlias,
      deep: true,
    });
    expect(report.ok).toBe(false);
    expect(report.note).toContain('Deep review is not available');
  });

  describe('simplicity focus', () => {
    const cwd = '/app';
    const modelAlias = 'reviewer';

    it('uses simplicity prompt when focus=simplicity', async () => {
      const ponytailOutput = 'src/foo.ts:L42: delete: dead code. Remove it.\nnet: -10 lines possible.';
      const executor = createCodeReviewExecutor({
        cwd,
        fetchDiff: vi.fn(async () => 'mock diff'),
        generate: vi.fn(async () => ({
          message: { role: 'assistant', content: [{ type: 'text', text: ponytailOutput }] },
          usage: { input: 100, output: 50 },
        })),
        resolveProviderConfig: vi.fn(() => ({})),
        estimateTokens: vi.fn(() => 10),
      });
      const report: CodeReviewReport = await executor.review({
        source: { kind: 'working-tree' },
        modelAlias,
        focus: 'simplicity',
      });
      expect(report.ok).toBe(true);
      expect(report.findings).toHaveLength(1);
      expect(report.findings[0]!.title).toContain('[DELETE]');
    });

    it('calls auditScanner when scope=repo', async () => {
      const digest: RepoAuditDigest = {
        workspaceDir: '/app',
        fileCount: 5,
        files: ['src/a.ts'],
        dependencies: [],
        snippets: [],
      };
      const auditScanner = vi.fn(async () => digest);
      const ponytailOutput = 'stdlib: custom clone. Use structuredClone. [src/a.ts]\nnet: -15 lines possible.';
      const executor = createCodeReviewExecutor({
        cwd,
        fetchDiff: vi.fn(async () => 'unused'),
        generate: vi.fn(async () => ({
          message: { role: 'assistant', content: [{ type: 'text', text: ponytailOutput }] },
          usage: { input: 100, output: 50 },
        })),
        resolveProviderConfig: vi.fn(() => ({})),
        estimateTokens: vi.fn(() => 10),
        auditScanner,
      });
      const report: CodeReviewReport = await executor.review({
        source: { kind: 'working-tree' },
        modelAlias,
        focus: 'simplicity',
        scope: 'repo',
      });
      expect(auditScanner).toHaveBeenCalledWith('/app', undefined);
      expect(report.ok).toBe(true);
      expect(report.findings).toHaveLength(1);
      expect(report.findings[0]!.title).toContain('[STDLIB]');
    });

    it('returns ok=false when scope=repo but auditScanner not provided', async () => {
      const executor = createCodeReviewExecutor({
        cwd,
        fetchDiff: vi.fn(async () => 'unused'),
        generate: vi.fn(async () => ({
          message: { role: 'assistant', content: [{ type: 'text', text: '' }] },
          usage: { input: 0, output: 0 },
        })),
        resolveProviderConfig: vi.fn(() => ({})),
        estimateTokens: vi.fn(() => 0),
      });
      const report: CodeReviewReport = await executor.review({
        source: { kind: 'working-tree' },
        modelAlias,
        scope: 'repo',
      });
      expect(report.ok).toBe(false);
      expect(report.note).toContain('Repo audit is not available');
    });

    it('returns Lean already report for no-finding simplicity output', async () => {
      const executor = createCodeReviewExecutor({
        cwd,
        fetchDiff: vi.fn(async () => 'mock diff'),
        generate: vi.fn(async () => ({
          message: { role: 'assistant', content: [{ type: 'text', text: 'Lean already. Ship.' }] },
          usage: { input: 100, output: 50 },
        })),
        resolveProviderConfig: vi.fn(() => ({})),
        estimateTokens: vi.fn(() => 10),
      });
      const report: CodeReviewReport = await executor.review({
        source: { kind: 'working-tree' },
        modelAlias,
        focus: 'simplicity',
      });
      expect(report.ok).toBe(true);
      expect(report.findings).toHaveLength(0);
      expect(report.summary).toBe('Lean already. Ship.');
    });
  });
});
