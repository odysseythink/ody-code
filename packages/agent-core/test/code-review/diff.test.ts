import { describe, expect, it } from 'vitest';
import { execSync } from 'node:child_process';
import { parsePrNumber, buildDiffSource } from '../../src/code-review/diff';
import type { CodeReviewDiffSource } from '../../src/code-review/types';

describe('parsePrNumber', () => {
  it('parses full GitHub PR URL', () => {
    expect(parsePrNumber('https://github.com/owner/repo/pull/42')).toBe('42');
    expect(parsePrNumber('http://github.com/owner/repo/pull/123')).toBe('123');
  });

  it('parses bare PR number', () => {
    expect(parsePrNumber('789')).toBe('789');
  });

  it('throws on non-GitHub URL', () => {
    expect(() => parsePrNumber('https://gitlab.com/owner/repo/-/merge_requests/1'))
      .toThrow('PR URL must be a GitHub pull request URL');
  });

  it('throws on incomplete github.com URL missing owner/repo/pull/number', () => {
    expect(() => parsePrNumber('https://github.com/owner/pull/1'))
      .toThrow('PR URL must be a GitHub pull request URL');
  });
});

describe('buildDiffSource', () => {
  it('builds commits source with defaults', () => {
    const source: CodeReviewDiffSource = buildDiffSource({ base: 'HEAD~3', head: 'HEAD' });
    expect(source).toEqual({ kind: 'commits', base: 'HEAD~3', head: 'HEAD' });
  });

  it('builds working-tree source when no flags', () => {
    const source: CodeReviewDiffSource = buildDiffSource({});
    expect(source).toEqual({ kind: 'working-tree' });
  });

  it('defaults head to HEAD when only base is given', () => {
    const source: CodeReviewDiffSource = buildDiffSource({ base: 'main' });
    expect(source).toEqual({ kind: 'commits', base: 'main', head: 'HEAD' });
  });

  it('builds pr source', () => {
    const source: CodeReviewDiffSource = buildDiffSource({ pr: 'https://github.com/a/b/pull/5' });
    expect(source).toEqual({ kind: 'pr', prUrlOrNumber: 'https://github.com/a/b/pull/5' });
  });
});

describe('fetchDiff (smoke)', () => {
  it('returns non-empty diff for HEAD~1..HEAD in the current repo', async () => {
    const { fetchDiff } = await import('../../src/code-review/diff');
    const cwd = execSync('git rev-parse --show-toplevel', { encoding: 'utf-8' }).trim();
    const diff = await fetchDiff({ kind: 'commits', base: 'HEAD~1', head: 'HEAD' }, cwd);
    expect(typeof diff).toBe('string');
  });

  it('throws when gh is used but not available', async () => {
    const { fetchDiff } = await import('../../src/code-review/diff');
    await expect(
      fetchDiff({ kind: 'pr', prUrlOrNumber: '99999' }, '/tmp', {
        env: { ...process.env, PATH: '/tmp/no-gh' },
      }),
    ).rejects.toThrow(/gh/);
  });
});
