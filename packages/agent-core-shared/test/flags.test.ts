import { describe, expect, it } from 'vitest';
import { FlagResolver } from '../src/flags/resolver';
import { FLAG_DEFINITIONS } from '../src/flags/registry';

describe('session-memory flag', () => {
  it('is registered with the correct env name and defaults to off', () => {
    const def = FLAG_DEFINITIONS.find((d) => d.id === 'session-memory');
    expect(def).toBeDefined();
    expect(def!.env).toBe('ODY_CODE_EXPERIMENTAL_SESSION_MEMORY');
    expect(def!.default).toBe(false);
    expect(def!.surface).toBe('core');
  });

  it('can be enabled via env override', () => {
    const resolver = new FlagResolver(
      { ODY_CODE_EXPERIMENTAL_SESSION_MEMORY: 'true' },
      FLAG_DEFINITIONS,
    );
    expect(resolver.enabled('session-memory')).toBe(true);
  });

  it('defaults to off when env is absent', () => {
    const resolver = new FlagResolver({}, FLAG_DEFINITIONS);
    expect(resolver.enabled('session-memory')).toBe(false);
  });

  it('can be forced on by the master switch', () => {
    const resolver = new FlagResolver(
      { ODY_CODE_EXPERIMENTAL_FLAG: '1' },
      FLAG_DEFINITIONS,
    );
    expect(resolver.enabled('session-memory')).toBe(true);
  });
});
