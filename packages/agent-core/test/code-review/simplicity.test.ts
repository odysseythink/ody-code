import { describe, expect, it } from 'vitest';
import {
  parseSimplicityReport,
  buildSimplicityReviewPrompt,
  buildSimplicityAuditPrompt,
} from '../../src/code-review/simplicity';
import type { CodeReviewReport } from '../../src/code-review/types';

describe('parseSimplicityReport', () => {
  it('parses well-formed Ponytail lines with file location', () => {
    const raw = 'src/foo.ts:L12: stdlib: 27-line validator class. Use String.prototype.includes, 1 line.';
    const report: CodeReviewReport = parseSimplicityReport(raw, 'simplicity-model');
    expect(report.ok).toBe(true);
    expect(report.findings).toHaveLength(1);
    expect(report.findings[0]!.severity).toBe('important'); // stdlib = important
    expect(report.findings[0]!.location).toBe('src/foo.ts:12');
    expect(report.findings[0]!.title).toContain('[STDLIB]');
    expect(report.findings[0]!.detail).toContain('stdlib: 27-line validator class. Use String.prototype.includes, 1 line.');
    expect(report.findings[0]!.suggestedFix).toBe('Use String.prototype.includes, 1 line');
  });

  it('parses shrink tag as minor severity', () => {
    const raw = 'L5: shrink: long function. Extract helper.';
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.findings[0]!.severity).toBe('minor');
    expect(report.findings[0]!.title).toContain('[SHRINK]');
  });

  it('handles Lean already. Ship.', () => {
    const raw = 'Lean already. Ship.';
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.findings).toHaveLength(0);
    expect(report.summary).toBe('Lean already. Ship.');
  });

  it('extracts net line as summary', () => {
    const raw = 'L1: delete: unused util. Remove it.\nnet: -50 lines possible.';
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.summary).toBe('net: -50 lines possible.');
    expect(report.findings).toHaveLength(1);
  });

  it('skips unparseable lines', () => {
    const raw = 'Some random text\nL1: delete: unused code. Remove it.\nMore noise';
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.findings).toHaveLength(1);
    expect(report.findings[0]!.title).toContain('[DELETE]');
  });

  it('handles delete and yagni tags as important severity', () => {
    const raw = [
      'L1: delete: dead code class. Remove the entire file.',
      'L5: yagni: premature abstraction. Inline the two call sites.',
    ].join('\n');
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.findings).toHaveLength(2);
    expect(report.findings[0]!.severity).toBe('important');
    expect(report.findings[1]!.severity).toBe('important');
  });

  it('handles native tag as important severity', () => {
    const raw = 'L3: native: custom deep clone. Use structuredClone, 1 line.';
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.findings[0]!.severity).toBe('important');
    expect(report.findings[0]!.title).toContain('[NATIVE]');
  });

  it('handles line without file prefix', () => {
    const raw = 'L8: delete: unused import. Remove.';
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.findings).toHaveLength(1);
    // location is just the line number when no file prefix
    expect(report.findings[0]!.location).toBe(':8');
  });

  it('extracts trailing [path] from audit output format', () => {
    const raw = 'stdlib: custom clone. Use structuredClone. [src/a.ts]';
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.findings).toHaveLength(1);
    expect(report.findings[0]!.location).toBe('src/a.ts');
    expect(report.findings[0]!.suggestedFix).toBe('Use structuredClone');
    expect(report.findings[0]!.detail).toBe('stdlib: custom clone. Use structuredClone.');
  });

  it('extracts trailing [path] with brackets in path', () => {
    const raw = 'delete: dead code. Remove it. [src/utils/helper.ts]';
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.findings).toHaveLength(1);
    expect(report.findings[0]!.location).toBe('src/utils/helper.ts');
    expect(report.findings[0]!.suggestedFix).toBe('Remove it');
  });

  it('handles empty input as ok', () => {
    const report: CodeReviewReport = parseSimplicityReport('', 'x');
    expect(report.findings).toHaveLength(0);
    expect(report.ok).toBe(true);
  });

  // Must-survive inputs: verify parser does not match false positives
  it('does not parse a line starting with a tag-like word that is not a Ponytail tag', () => {
    const raw = 'This is a normal sentence. delete: is not a real tag here.';
    // "delete:" after "sentence." should not start a valid tag line
    // The parser only matches tag: at the start of a trimmed line
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.findings).toHaveLength(0);
  });
});

describe('buildSimplicityReviewPrompt', () => {
  it('contains all five Ponytail tags in the prompt', () => {
    const prompt = buildSimplicityReviewPrompt('mock diff', 'desc', 'reqs');
    expect(prompt).toContain('delete:');
    expect(prompt).toContain('stdlib:');
    expect(prompt).toContain('native:');
    expect(prompt).toContain('yagni:');
    expect(prompt).toContain('shrink:');
  });

  it('contains the diff content', () => {
    const prompt = buildSimplicityReviewPrompt('--- a/file.ts\n+++ b/file.ts\n+new line', undefined, undefined);
    expect(prompt).toContain('--- a/file.ts');
    expect(prompt).toContain('+new line');
  });

  it('includes optional description and requirements', () => {
    const prompt = buildSimplicityReviewPrompt('diff', 'added login', 'must use OAuth');
    expect(prompt).toContain('added login');
    expect(prompt).toContain('must use OAuth');
  });

  it('handles missing description and requirements gracefully', () => {
    const prompt = buildSimplicityReviewPrompt('diff', undefined, undefined);
    expect(prompt).toContain('[not provided]');
  });

  it('includes Lean already. Ship. instruction', () => {
    const prompt = buildSimplicityReviewPrompt('diff', undefined, undefined);
    expect(prompt).toContain('Lean already. Ship.');
  });

  it('includes net line instruction', () => {
    const prompt = buildSimplicityReviewPrompt('diff', undefined, undefined);
    expect(prompt).toContain('net:');
  });
});

describe('buildSimplicityAuditPrompt', () => {
  const digest = {
    workspaceDir: '/app',
    fileCount: 3,
    files: ['src/a.ts', 'src/b.ts', 'src/c.ts'],
    dependencies: ['lodash', 'express'],
    snippets: [
      { path: 'src/a.ts', lines: 'import * as _ from "lodash";\nclass Foo {}' },
    ],
  };

  it('contains all five tags', () => {
    const prompt = buildSimplicityAuditPrompt(digest);
    expect(prompt).toContain('delete:');
    expect(prompt).toContain('stdlib:');
    expect(prompt).toContain('native:');
    expect(prompt).toContain('yagni:');
    expect(prompt).toContain('shrink:');
  });

  it('includes file list', () => {
    const prompt = buildSimplicityAuditPrompt(digest);
    expect(prompt).toContain('src/a.ts');
  });

  it('includes dependency list', () => {
    const prompt = buildSimplicityAuditPrompt(digest);
    expect(prompt).toContain('lodash');
    expect(prompt).toContain('express');
  });

  it('does not contain node_modules (sensitive filter)', () => {
    const prompt = buildSimplicityAuditPrompt(digest);
    expect(prompt).not.toContain('node_modules');
  });

  it('includes net line instruction for deps', () => {
    const prompt = buildSimplicityAuditPrompt(digest);
    expect(prompt).toContain('deps possible');
  });
});
