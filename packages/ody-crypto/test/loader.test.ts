import { describe, expect, it, vi } from 'vitest';

const mockState = vi.hoisted(() => {
  let currentMock: Record<string, unknown> | undefined;
  let shouldFail = false;
  return {
    setSuccess: (mock: Record<string, unknown>) => {
      currentMock = mock;
      shouldFail = false;
    },
    setFail: () => {
      shouldFail = true;
    },
    doRequire: () => {
      if (shouldFail) throw new Error('dlopen');
      return currentMock;
    },
  };
});

vi.mock('node:module', () => ({
  createRequire: () => mockState.doRequire,
}));

import { getOdyCrypto, loadNative } from '../src/loader';
import { tsFallback } from '../src/fallback';

describe('loadNative', () => {
  it('returns native when .node loads', () => {
    const mock = {
      randomBytes: vi.fn(),
      sha256: vi.fn(),
      pkceChallenge: vi.fn(),
      verifyIdToken: vi.fn(),
    };
    mockState.setSuccess(mock);
    const result = loadNative();
    expect(result?.randomBytes).toBe(mock.randomBytes);
    expect(result?.sha256).toBe(mock.sha256);
  });

  it('falls back to ts on require failure', () => {
    mockState.setFail();
    const result = getOdyCrypto();
    const challenge = result.pkceChallenge();
    expect(challenge.codeVerifier.length).toBe(43);
    expect(result).toBe(tsFallback);
  });
});
