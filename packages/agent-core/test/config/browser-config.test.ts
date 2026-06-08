import { describe, expect, it } from 'vitest';
import {
  BrowserConfigSchema,
  KimiConfigSchema,
  KimiConfigPatchSchema,
} from '../../src/config/schema';

describe('BrowserConfigSchema', () => {
  it('parses valid browser config with all fields', () => {
    const parsed = BrowserConfigSchema.parse({
      enabled: true,
      chromePort: 9222,
      traceEnabled: true,
      traceRetentionDays: 7,
    });
    expect(parsed).toEqual({
      enabled: true,
      chromePort: 9222,
      traceEnabled: true,
      traceRetentionDays: 7,
    });
  });

  it('parses empty object as all undefined', () => {
    expect(BrowserConfigSchema.parse({})).toEqual({});
  });

  it('rejects chromePort = 0', () => {
    expect(() => BrowserConfigSchema.parse({ chromePort: 0 })).toThrow();
  });

  it('rejects chromePort > 65535', () => {
    expect(() => BrowserConfigSchema.parse({ chromePort: 70000 })).toThrow();
  });

  it('rejects traceRetentionDays = 0', () => {
    expect(() => BrowserConfigSchema.parse({ traceRetentionDays: 0 })).toThrow();
  });

  it('is accepted by KimiConfigSchema as optional field', () => {
    const config = KimiConfigSchema.parse({
      providers: {},
      browser: { enabled: true, chromePort: 9222 },
    });
    expect(config.browser).toEqual({ enabled: true, chromePort: 9222 });
  });

  it('is accepted by KimiConfigPatchSchema', () => {
    const patch = KimiConfigPatchSchema.parse({ browser: { enabled: false } });
    expect(patch.browser).toEqual({ enabled: false });
  });
});
