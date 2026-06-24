import { describe, expect, it } from 'vitest';
import { getCoreVersion } from '../src/version';

describe('getCoreVersion', () => {
  it('returns a non-empty semver string', () => {
    const version = getCoreVersion();
    expect(version).toMatch(/^\d+\.\d+\.\d+/);
  });
});
