import { EventEmitter } from 'node:events';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../../../src/agent';
import type { OdyConfig } from '@odysseythink/agent-core-shared';
import {
  AdvancedSessionReviewer,
  type AdvancedSessionReviewResult,
} from '../../../../src/agent/session-mode/reviewer';
import {
  buildReviewEntries,
  formatReport,
  resolveTestReviewerAlias,
  ReviewTestsTool,
} from '../../../../src/tools/builtin/test-review/review-tests';
import { createFakeKaos } from '../../fixtures/fake-kaos';

describe('resolveTestReviewerAlias', () => {
  const cfg = (c: Partial<OdyConfig>): OdyConfig => c as OdyConfig;

  it('prefers the dedicated test_review alias over the active model', () => {
    expect(
      resolveTestReviewerAlias(cfg({ modeModels: { testReview: 'm-test' }, defaultModel: 'm-default' }), 'm-active'),
    ).toBe('m-test');
  });

  it("falls back to the current mode's active model when test_review is absent", () => {
    expect(resolveTestReviewerAlias(cfg({ modeModels: { review: 'm-review' }, defaultModel: 'm-default' }), 'm-active')).toBe(
      'm-active',
    );
    expect(resolveTestReviewerAlias(cfg({}), 'm-active')).toBe('m-active');
  });

  it('falls back to the default model when there is no active model', () => {
    expect(resolveTestReviewerAlias(cfg({ defaultModel: 'm-default' }), undefined)).toBe('m-default');
  });

  it('returns undefined only when nothing at all is available', () => {
    expect(resolveTestReviewerAlias(cfg({}), undefined)).toBeUndefined();
    expect(resolveTestReviewerAlias(undefined, undefined)).toBeUndefined();
  });
});

describe('buildReviewEntries', () => {
  it('pairs each test file with its same-directory sibling implementation, in order', () => {
    const entries = buildReviewEntries(['src/sum.test.ts'], ['src/sum.test.ts', 'src/sum.ts']);
    expect(entries).toEqual([
      { label: 'TEST FILE', path: 'src/sum.test.ts' },
      { label: 'IMPLEMENTATION FILE', path: 'src/sum.ts' },
    ]);
  });

  it('includes changed non-test source files that are not the derived sibling', () => {
    const entries = buildReviewEntries(
      ['src/a.test.ts'],
      ['src/a.test.ts', 'src/a.ts', 'src/helper.ts'],
    );
    expect(entries).toEqual([
      { label: 'TEST FILE', path: 'src/a.test.ts' },
      { label: 'IMPLEMENTATION FILE', path: 'src/a.ts' },
      { label: 'IMPLEMENTATION FILE', path: 'src/helper.ts' },
    ]);
  });

  it('dedupes the sibling against the changed-source sweep and never labels a test as implementation', () => {
    // a.ts is both the derived sibling AND a changed source → appears once.
    // b.spec.ts is a test → must never be pulled in as an implementation file.
    const entries = buildReviewEntries(['src/a.test.ts'], ['src/a.test.ts', 'src/a.ts', 'src/b.spec.ts']);
    expect(entries).toEqual([
      { label: 'TEST FILE', path: 'src/a.test.ts' },
      { label: 'IMPLEMENTATION FILE', path: 'src/a.ts' },
    ]);
  });

  it('derives the sibling implementation even when it was not itself changed (context for the test)', () => {
    // Only the test changed; we still want its implementation read for context.
    // buildReviewContent tolerates the sibling being unreadable/absent. README.md
    // is not a source file and must be excluded.
    const entries = buildReviewEntries(['src/x.test.ts'], ['src/x.test.ts', 'README.md']);
    expect(entries).toEqual([
      { label: 'TEST FILE', path: 'src/x.test.ts' },
      { label: 'IMPLEMENTATION FILE', path: 'src/x.ts' },
    ]);
  });
});

/** Build a fake Kaos whose git status returns `gitShort` and whose readText serves `files`. */
function makeKaos(gitShort: string, files: Record<string, string>) {
  return createFakeKaos({
    getcwd: () => '/repo',
    exec: (async () => {
      const stdout = new EventEmitter();
      return {
        stdout,
        wait: async () => {
          stdout.emit('data', Buffer.from(gitShort));
        },
      };
    }) as unknown as ReturnType<typeof createFakeKaos>['exec'],
    readText: (async (path: string) => {
      if (path in files) return files[path];
      throw new Error(`ENOENT: ${path}`);
    }) as unknown as ReturnType<typeof createFakeKaos>['readText'],
  });
}

function makeToolAgent(over: { kimiConfig?: Partial<OdyConfig>; modelAlias?: string }): Agent {
  return {
    kimiConfig: over.kimiConfig as OdyConfig | undefined,
    config: { modelAlias: over.modelAlias },
  } as unknown as Agent;
}

function runTool(tool: ReviewTestsTool) {
  const execution = tool.resolveExecution({});
  if (!('execute' in execution)) throw new Error('expected a runnable execution');
  const ctx = { signal: new AbortController().signal, turnId: 't', toolCallId: 'c' };
  return execution.execute(ctx as unknown as Parameters<typeof execution.execute>[0]);
}

describe('ReviewTestsTool.execution', () => {
  afterEach(() => vi.restoreAllMocks());

  it('errors when no reviewer model is available anywhere', async () => {
    const tool = new ReviewTestsTool(makeKaos('', {}), makeToolAgent({ kimiConfig: {} }));
    const result = await runTool(tool);
    expect(result.isError).toBe(true);
    expect(String(result.output)).toContain('No reviewer model available');
  });

  it('returns a benign message when no test files changed', async () => {
    const review = vi.spyOn(AdvancedSessionReviewer.prototype, 'review');
    const tool = new ReviewTestsTool(
      makeKaos(' M src/foo.ts\n', { 'src/foo.ts': 'export const x = 1;' }),
      makeToolAgent({ modelAlias: 'm-active' }),
    );
    const result = await runTool(tool);
    expect(String(result.output)).toContain('No changed test files');
    expect(review).not.toHaveBeenCalled(); // no point spending a model call
  });

  it('feeds the changed test + its implementation to the reviewer and formats the report', async () => {
    const review = vi
      .spyOn(AdvancedSessionReviewer.prototype, 'review')
      .mockResolvedValue({
        auditLevel: 'Standard',
        findings: [{ severity: 'high', confidence: 'certain', title: 'Tautology', detail: 'expect(x).toBe(x)' }],
        mutationProbes: [{ location: 'src/sum.ts:2', mutation: 'return a - b', expectedCatch: 'adds' }],
        ok: true,
      } satisfies AdvancedSessionReviewResult);

    const tool = new ReviewTestsTool(
      makeKaos(' M src/sum.ts\n M src/sum.test.ts\n', {
        'src/sum.test.ts': "test('adds', () => expect(sum(1,2)).toBe(3));",
        'src/sum.ts': 'export const sum = (a, b) => a + b;',
      }),
      makeToolAgent({ kimiConfig: { modeModels: { testReview: 'm-judge' } }, modelAlias: 'm-active' }),
    );
    const result = await runTool(tool);

    // The reviewer saw both the test AND its implementation (so it can judge tautology).
    expect(review).toHaveBeenCalledTimes(1);
    const content = review.mock.calls[0]?.[0] ?? '';
    expect(content).toContain('TEST FILE: src/sum.test.ts');
    expect(content).toContain('IMPLEMENTATION FILE: src/sum.ts');

    // The report surfaces findings + the runnable probe.
    const out = String(result.output);
    expect(out).toContain('Tautology');
    expect(out).toContain('return a - b');
    expect(out).toContain('reviewer: m-judge'); // dedicated alias preferred over m-active
  });

  it('degrades gracefully when the reviewer cannot run', async () => {
    vi.spyOn(AdvancedSessionReviewer.prototype, 'review').mockResolvedValue({
      auditLevel: 'Standard',
      findings: [],
      ok: false,
      note: 'reviewer alias unavailable',
    });
    const tool = new ReviewTestsTool(
      makeKaos(' M src/a.test.ts\n', { 'src/a.test.ts': 'test("x", () => {});' }),
      makeToolAgent({ modelAlias: 'm-active' }),
    );
    const result = await runTool(tool);
    expect(result.isError).toBeFalsy();
    expect(String(result.output)).toContain('Test review could not run');
    expect(String(result.output)).toContain('reviewer alias unavailable');
  });
});

describe('formatReport', () => {
  const base = (over: Partial<AdvancedSessionReviewResult>): AdvancedSessionReviewResult => ({
    auditLevel: 'Standard',
    findings: [],
    ok: true,
    ...over,
  });

  it('renders findings with severity/confidence and an ESCALATE tag for high-certain at Standard', () => {
    const report = formatReport(
      base({
        findings: [
          { severity: 'high', confidence: 'certain', title: 'Tautological assert', detail: 'expect(x).toBe(x)' },
        ],
      }),
      'm-test',
      ['src/a.test.ts'],
    );
    expect(report).toContain('reviewer: m-test');
    expect(report).toContain('src/a.test.ts');
    expect(report).toContain('Tautological assert');
    expect(report).toContain('HIGH');
    expect(report).toContain('ESCALATE');
  });

  it('does NOT escalate a speculative finding even at high severity', () => {
    const report = formatReport(
      base({
        findings: [{ severity: 'high', confidence: 'speculative', title: 'Maybe weak', detail: 'unverified' }],
      }),
      'm',
      ['t.test.ts'],
    );
    expect(report).not.toContain('ESCALATE');
  });

  it('renders runnable mutation probes with the run-then-revert instruction', () => {
    const report = formatReport(
      base({
        mutationProbes: [
          { location: 'src/sum.ts:3', mutation: 'return a - b', expectedCatch: 'adds two numbers' },
        ],
      }),
      'm',
      ['src/sum.test.ts'],
    );
    expect(report).toContain('Mutation probes (1)');
    expect(report).toContain('return a - b');
    expect(report).toContain('src/sum.ts:3');
    expect(report).toContain('adds two numbers');
    expect(report).toContain('REVERT');
    expect(report).toContain('stays GREEN');
  });

  it('states plainly when there are no findings and no probes', () => {
    const report = formatReport(base({}), 'm', ['t.test.ts']);
    expect(report).toContain('none');
    expect(report).toContain('No mutation probes');
  });
});
