import { describe, expect, it, vi } from 'vitest';
import type { SessionSubagentHost } from '#/session/subagent-host';
import { runReviewerSubagent } from '#/tools/builtin/code-review/request-code-review';

const REVIEW_RESULT = [
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

function fakeHost(result: string): { host: SessionSubagentHost; spawn: ReturnType<typeof vi.fn> } {
  const spawn = vi.fn().mockResolvedValue({
    agentId: 'sub_1',
    profileName: 'reviewer',
    resumed: false,
    completion: Promise.resolve({ result }),
  });
  return { host: { spawn } as unknown as SessionSubagentHost, spawn };
}

describe('runReviewerSubagent', () => {
  it('spawns the reviewer profile on the given model and parses structured findings', async () => {
    const { host, spawn } = fakeHost(REVIEW_RESULT);

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

    // Parsed the subagent's final summary into structured findings.
    expect(report.ok).toBe(true);
    expect(report.reviewerAlias).toBe('reviewer-model-x');
    const critical = report.findings.find((f) => f.severity === 'critical');
    expect(critical?.title).toBe('Nil deref in handler');
    expect(critical?.location).toBe('server/handler.go:42');
    expect(critical?.suggestedFix).toContain('nil body');
    expect(report.findings.some((f) => f.severity === 'important')).toBe(true);
  });
});
