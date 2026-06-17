import { describe, expect, it } from 'vitest';
import { OptionConflictError } from '../../src/cli/options';
import { buildDiffSource, validateRequestCodeReviewOptions } from '../../src/cli/sub/request-code-review';

describe('validateRequestCodeReviewOptions', () => {
  it('throws OptionConflictError when --pr is combined with --base', () => {
    expect(() =>
      validateRequestCodeReviewOptions({ pr: '42', base: 'HEAD~1' }),
    ).toThrow(OptionConflictError);
  });

  it('throws OptionConflictError when --timeout is not a positive integer', () => {
    expect(() =>
      validateRequestCodeReviewOptions({ timeout: 0 }),
    ).toThrow('--timeout must be a positive integer (seconds).');

    expect(() =>
      validateRequestCodeReviewOptions({ timeout: -1 }),
    ).toThrow('--timeout must be a positive integer (seconds).');
  });

  it('defaults base to HEAD~1 and head to HEAD when neither flag nor --pr given', () => {
    const opts: { base?: string; head?: string; pr?: string } = {};
    validateRequestCodeReviewOptions(opts);
    expect(opts.base).toBe('HEAD~1');
    expect(opts.head).toBe('HEAD');
  });

  it('defaults head to HEAD when only base is given', () => {
    const opts = { base: 'main', head: undefined as string | undefined };
    validateRequestCodeReviewOptions(opts);
    expect(opts.base).toBe('main');
    expect(opts.head).toBe('HEAD');
  });

  it('accepts --pr alone without conflict', () => {
    expect(() =>
      validateRequestCodeReviewOptions({ pr: 'https://github.com/a/b/pull/1' }),
    ).not.toThrow();
  });

  it('accepts --base and --head together', () => {
    expect(() =>
      validateRequestCodeReviewOptions({ base: 'main', head: 'feature' }),
    ).not.toThrow();
  });

  it('accepts valid positive integer timeout', () => {
    expect(() =>
      validateRequestCodeReviewOptions({ timeout: 120 }),
    ).not.toThrow();
  });
});

describe('buildDiffSource (CLI)', () => {
  it('builds working-tree when no flags', () => {
    expect(buildDiffSource({})).toEqual({ kind: 'working-tree' });
  });

  it('builds pr source', () => {
    expect(buildDiffSource({ pr: '1' })).toEqual({ kind: 'pr', prUrlOrNumber: '1' });
  });

  it('builds commits source', () => {
    expect(buildDiffSource({ base: 'HEAD~3', head: 'HEAD' })).toEqual({
      kind: 'commits',
      base: 'HEAD~3',
      head: 'HEAD',
    });
  });
});
