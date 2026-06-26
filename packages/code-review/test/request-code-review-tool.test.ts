import { describe, expect, it, vi } from 'vitest';
import {
  runReviewerSubagent,
  type CodeReviewSubagentHost,
} from '../src/request-code-review-tool';
import { parseReviewReport } from '../src/prompt';

const REVIEW_RESULT_NEEDS_FIXES = [
  'Strengths:',
  '- clear separation of concerns',
  '',
  'Findings:',
  'Critical:',
  '- [Nil deref in handler] (server/handler.go:42)',
  '  the request body may be nil before Decode',
  '  fix: guard against a nil body',
  '',
  'Important:',
  '- [Missing handler test] (server/handler.go)',
  '  the new endpoint has no interface-level test',
  '',
  'Assessment: Needs fixes',
].join('\n');

const REVIEW_RESULT_APPROVED = [
  'Strengths:',
  '- clean implementation',
  '',
  'Findings:',
  'Critical:',
  'Important:',
  'Minor:',
  '',
  'Assessment: Approved',
].join('\n');

function fakeHost(result: string): { host: CodeReviewSubagentHost; spawn: ReturnType<typeof vi.fn> } {
  const spawn = vi.fn().mockResolvedValue({
    agentId: 'sub_1',
    profileName: 'reviewer',
    resumed: false,
    completion: Promise.resolve({ result }),
  });
  return { host: { spawn } as unknown as CodeReviewSubagentHost, spawn };
}

describe('runReviewerSubagent', () => {
  it('spawns the reviewer profile on the given model and parses structured findings', async () => {
    const { host, spawn } = fakeHost(REVIEW_RESULT_NEEDS_FIXES);

    const report = await runReviewerSubagent(host, {
      diff: 'diff --git a/server/handler.go b/server/handler.go\n+new code',
      reviewerAlias: 'reviewer-model-x',
      description: 'add /preferences endpoint',
      requirements: 'GET returns saved prefs as JSON',
      parentToolCallId: 'call_1',
      signal: new AbortController().signal,
    });

    // Spawned the read-only reviewer profile on the dedicated reviewer model.
    expect(spawn).toHaveBeenCalledTimes(1);
    const [profileName, options] = spawn.mock.calls[0]!;
    expect(profileName).toBe('reviewer');
    expect(options.modelAlias).toBe('reviewer-model-x');
    expect(options.runInBackground).toBe(false);
    expect(options.prompt).toContain('diff --git a/server/handler.go');
    expect(options.prompt).toContain('add /preferences endpoint');
    expect(options.prompt).toContain('GET returns saved prefs as JSON');

    // Parsed the subagent's final summary into structured findings.
    expect(report.ok).toBe(true);
    expect(report.reviewerAlias).toBe('reviewer-model-x');
    expect(report.findings).toHaveLength(2);
    const critical = report.findings.find((f) => f.severity === 'critical');
    expect(critical?.title).toBe('Nil deref in handler');
    expect(critical?.location).toBe('server/handler.go:42');
    expect(critical?.suggestedFix).toBe('guard against a nil body');
    const important = report.findings.find((f) => f.severity === 'important');
    expect(important?.title).toBe('Missing handler test');
  });

  it('parses an approved review with no findings', async () => {
    const { host } = fakeHost(REVIEW_RESULT_APPROVED);

    const report = await runReviewerSubagent(host, {
      diff: 'diff --git a/server/handler.go b/server/handler.go\n+new code',
      reviewerAlias: 'reviewer-model-x',
      parentToolCallId: 'call_2',
      signal: new AbortController().signal,
    });

    // A successful review parse returns ok:true with zero findings.
    expect(report.ok).toBe(true);
    expect(report.reviewerAlias).toBe('reviewer-model-x');
    expect(report.findings).toHaveLength(0);
    expect(report.summary).toBe('clean implementation');
  });

  it('propagates when the subagent completion rejects', async () => {
    const spawn = vi.fn().mockResolvedValue({
      agentId: 'sub_1',
      profileName: 'reviewer',
      resumed: false,
      completion: Promise.reject(new Error('subagent crashed')),
    });
    const host = { spawn } as unknown as CodeReviewSubagentHost;

    await expect(
      runReviewerSubagent(host, {
        diff: 'diff --git a/x b/x',
        reviewerAlias: 'reviewer-model-x',
        parentToolCallId: 'call_3',
        signal: new AbortController().signal,
      }),
    ).rejects.toThrow('subagent crashed');
  });
});

describe('parseReviewReport', () => {
  it('returns empty findings for malformed output', () => {
    const report = parseReviewReport('not a review', 'model-x');
    expect(report.ok).toBe(true);
    expect(report.reviewerAlias).toBe('model-x');
    expect(report.findings).toHaveLength(0);
    expect(report.summary).toBeUndefined();
  });
});
