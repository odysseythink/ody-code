import { describe, expect, it, vi } from 'vitest';
import { resolveCodeReviewModel } from '../src/model-resolver';

function alwaysValid(_alias: string): boolean {
  return true;
}

describe('resolveCodeReviewModel', () => {
  const defaultModel = 'default-model';

  it('request: explicit override wins over everything', () => {
    const result = resolveCodeReviewModel(
      'request',
      {
        codeReviewRequest: 'req-specific',
        codeReview: 'general',
        review: 'old-reviewer',
      },
      defaultModel,
      { explicit: 'cli-model' },
      alwaysValid,
    );
    expect(result).toBe('cli-model');
  });

  it('request: falls back to codeReviewRequest then codeReview then review then default', () => {
    // No explicit, no codeReviewRequest — should use codeReview
    const result1 = resolveCodeReviewModel(
      'request',
      { codeReview: 'general', review: 'old-reviewer' },
      defaultModel,
      {},
      alwaysValid,
    );
    expect(result1).toBe('general');

    // Only review present
    const result2 = resolveCodeReviewModel(
      'request',
      { review: 'old-reviewer' },
      defaultModel,
      {},
      alwaysValid,
    );
    expect(result2).toBe('old-reviewer');

    // Session model falls back after modeModels
    const result3 = resolveCodeReviewModel(
      'request',
      {},
      undefined,
      { sessionModel: 'session-model' },
      alwaysValid,
    );
    expect(result3).toBe('session-model');

    // Default is last resort
    const result4 = resolveCodeReviewModel(
      'request',
      {},
      'last-resort',
      {},
      alwaysValid,
    );
    expect(result4).toBe('last-resort');
  });

  it('receive: does not accept explicit override', () => {
    const result = resolveCodeReviewModel(
      'receive',
      {
        codeReviewReceive: 'receive-model',
        codeReview: 'general',
      },
      defaultModel,
      { explicit: 'should-be-ignored' },
      alwaysValid,
    );
    expect(result).toBe('receive-model');
  });

  it('receive: falls back codeReviewReceive → codeReview → review → sessionModel → default', () => {
    const result1 = resolveCodeReviewModel(
      'receive',
      { codeReview: 'general' },
      defaultModel,
      {},
      alwaysValid,
    );
    expect(result1).toBe('general');

    const result2 = resolveCodeReviewModel(
      'receive',
      { review: 'old-reviewer', codeReviewReceive: 'rcv' },
      defaultModel,
      { sessionModel: 'sess' },
      alwaysValid,
    );
    expect(result2).toBe('rcv');

    const result3 = resolveCodeReviewModel(
      'receive',
      {},
      undefined,
      { sessionModel: 'sess' },
      alwaysValid,
    );
    expect(result3).toBe('sess');
  });

  it('skips invalid aliases and continues the chain', () => {
    let callCount = 0;
    const validate = (alias: string): boolean => {
      callCount += 1;
      return alias !== 'bad-alias';
    };
    const result = resolveCodeReviewModel(
      'request',
      { codeReviewRequest: 'bad-alias', codeReview: 'good-alias' },
      defaultModel,
      {},
      validate,
    );
    expect(result).toBe('good-alias');
    expect(callCount).toBeGreaterThanOrEqual(2);
  });

  it('throws error when all candidates are exhausted', () => {
    expect(() =>
      resolveCodeReviewModel(
        'request',
        {},
        undefined,
        {},
        () => false,
      ),
    ).toThrow();
  });
});
