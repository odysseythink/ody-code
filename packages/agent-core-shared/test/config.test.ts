import { describe, expect, it } from 'vitest';
import { HookDefSchema, HOOK_PROFILES, OdyConfigSchema } from '../src/config';

describe('HookDefSchema', () => {
  it('accepts legacy hook with command only', () => {
    const parsed = HookDefSchema.parse({ event: 'PreToolUse', matcher: 'Shell', command: 'echo ok' });
    expect(parsed.command).toBe('echo ok');
    expect(parsed.profiles).toBeUndefined();
  });

  it('accepts builtin hook', () => {
    const parsed = HookDefSchema.parse({
      event: 'Stop',
      builtin: 'stop-format-typecheck',
      profiles: ['standard'],
    });
    expect(parsed.builtin).toBe('stop-format-typecheck');
  });

  it('accepts commands array', () => {
    const parsed = HookDefSchema.parse({
      event: 'PreToolUse',
      commands: ['shellcheck "$ODY_TOOL_INPUT"', 'node checks/no-verify.js'],
      id: 'pre:bash:quality',
    });
    expect(parsed.commands).toEqual(['shellcheck "$ODY_TOOL_INPUT"', 'node checks/no-verify.js']);
  });

  it.each([
    [{ command: 'a', builtin: 'b' }, 'command+builtin'],
    [{ command: 'a', commands: ['b'] }, 'command+commands'],
    [{ builtin: 'b', commands: ['c'] }, 'builtin+commands'],
    [{}, 'none'],
  ])('rejects %s (%s)', (extra, _label) => {
    expect(() => HookDefSchema.parse({ event: 'Stop', ...extra })).toThrow(
      'hook: exactly one of command / builtin / commands',
    );
  });

  it('rejects single-element commands array', () => {
    expect(() => HookDefSchema.parse({ event: 'Stop', commands: ['a'] })).toThrow();
  });

  it('exports the three profile names', () => {
    expect(HOOK_PROFILES).toEqual(['minimal', 'standard', 'strict']);
  });
});

describe('sessionMemory config schema', () => {
  it('accepts config without sessionMemory', () => {
    const parsed = OdyConfigSchema.parse({});
    expect(parsed.sessionMemory).toBeUndefined();
  });

  it('accepts full sessionMemory values', () => {
    const parsed = OdyConfigSchema.parse({
      sessionMemory: { maxChars: 4000, retentionDays: 7 },
    });
    expect(parsed.sessionMemory).toEqual({ maxChars: 4000, retentionDays: 7 });
  });

  it('accepts partial sessionMemory values', () => {
    const parsed = OdyConfigSchema.parse({
      sessionMemory: { maxChars: 12000 },
    });
    expect(parsed.sessionMemory).toEqual({ maxChars: 12000 });
  });

  it('rejects negative maxChars', () => {
    expect(() =>
      OdyConfigSchema.parse({ sessionMemory: { maxChars: -1 } }),
    ).toThrow();
  });

  it('rejects negative retentionDays', () => {
    expect(() =>
      OdyConfigSchema.parse({ sessionMemory: { retentionDays: -1 } }),
    ).toThrow();
  });
});

describe('secretScan config schema', () => {
  it('accepts config without secretScan', () => {
    const parsed = OdyConfigSchema.parse({});
    expect(parsed.secretScan).toBeUndefined();
  });

  it('accepts full secretScan values', () => {
    const parsed = OdyConfigSchema.parse({
      secretScan: {
        enabled: true,
        blockOnMatch: false,
        maxScanBytes: 8192,
        entropyThreshold: 4.5,
        allowList: ['EXAMPLE_KEY'],
        profiles: ['strict'],
      },
    });
    expect(parsed.secretScan).toEqual({
      enabled: true,
      blockOnMatch: false,
      maxScanBytes: 8192,
      entropyThreshold: 4.5,
      allowList: ['EXAMPLE_KEY'],
      profiles: ['strict'],
    });
  });

  it('accepts partial secretScan values', () => {
    const parsed = OdyConfigSchema.parse({
      secretScan: { enabled: true },
    });
    expect(parsed.secretScan).toEqual({ enabled: true });
  });

  it('rejects negative maxScanBytes', () => {
    expect(() => OdyConfigSchema.parse({ secretScan: { maxScanBytes: -1 } })).toThrow();
  });

  it('rejects entropyThreshold above 8', () => {
    expect(() => OdyConfigSchema.parse({ secretScan: { entropyThreshold: 8.1 } })).toThrow();
  });
});
