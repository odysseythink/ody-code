import { describe, expect, it } from 'vitest';
import { buildReviewPrompt, parseReviewReport } from '../../src/code-review/prompt';
import type { CodeReviewReport } from '../../src/code-review/types';

describe('buildReviewPrompt', () => {
  it('contains diff and Assessment instruction', () => {
    const prompt = buildReviewPrompt('--- a/file.ts\n+++ b/file.ts', 'added feature X', 'Requirement Y');
    expect(prompt).toContain('## Diff');
    expect(prompt).toContain('added feature X');
    expect(prompt).toContain('Requirement Y');
    expect(prompt).toContain('Assessment');
  });

  it('handles missing description and requirements', () => {
    const prompt = buildReviewPrompt('diff', undefined, undefined);
    expect(prompt).toContain('[not provided]');
  });
});

describe('parseReviewReport', () => {
  const sampleOutput = `
Strengths:
- Clean code structure

Findings:
Critical:
- [broken null check] (src/foo.ts:42)
  Missing null check on result
  fix: Add if (result === null) guard

Important:
- [edge case not covered] (src/bar.ts)
  Negative input not handled
  fix: Add input validation

Minor:
- [naming] (src/baz.ts:10)
  Variable name too short
  fix: Rename to meaningful name

Assessment: Needs fixes
`;

  it('parses strengths as summary', () => {
    const report = parseReviewReport(sampleOutput, 'test-model');
    expect(report.ok).toBe(true);
    expect(report.reviewerAlias).toBe('test-model');
    expect(report.summary).toContain('Clean code structure');
  });

  it('parses findings by severity', () => {
    const report = parseReviewReport(sampleOutput, 'test-model');
    expect(report.findings).toHaveLength(3);
    expect(report.findings[0]!.severity).toBe('critical');
    expect(report.findings[0]!.title).toContain('broken null check');
    expect(report.findings[0]!.location).toBe('src/foo.ts:42');
    expect(report.findings[0]!.suggestedFix).toContain('null) guard');

    expect(report.findings[1]!.severity).toBe('important');
    expect(report.findings[2]!.severity).toBe('minor');
  });

  it('returns ok=true with empty findings when no issues found', () => {
    const output = `Strengths:\n- Great work\n\nFindings:\nCritical:\n\nImportant:\n\nMinor:\n\nAssessment: Ready to proceed`;
    const report = parseReviewReport(output, 'x');
    expect(report.ok).toBe(true);
    expect(report.findings).toHaveLength(0);
    expect(report.summary).toContain('Great work');
  });
});
