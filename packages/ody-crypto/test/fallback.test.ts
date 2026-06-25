import { describe, expect, it } from 'vitest';
import { tsFallback } from '../src/fallback';

describe('tsFallback', () => {
  it('randomBytes returns requested length', () => {
    expect(tsFallback.randomBytes(16).length).toBe(16);
  });

  it('sha256 returns known vector', () => {
    expect(tsFallback.sha256('abc')).toBe(
      'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
    );
  });

  it('pkceChallenge default length is 43 and challenge is base64url', () => {
    const result = tsFallback.pkceChallenge();
    expect(result.codeVerifier.length).toBe(43);
    expect(result.codeChallenge).toMatch(/^[A-Za-z0-9_-]+$/);
  });

  it('pkceChallenge rejects 42 and 129', () => {
    expect(() => tsFallback.pkceChallenge(42)).toThrow(RangeError);
    expect(() => tsFallback.pkceChallenge(129)).toThrow(RangeError);
  });
});
