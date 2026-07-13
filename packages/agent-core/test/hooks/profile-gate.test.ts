import { describe, expect, it } from 'vitest';

import { createProfileGate } from '../../src/session/hooks/profile-gate';
import type { HookDef } from '../../src/session/hooks/types';

describe('createProfileGate', () => {
  it('defaults to strict profile', () => {
    expect(createProfileGate({}).profile).toBe('strict');
  });

  it('reads ODY_CODE_HOOK_PROFILE', () => {
    expect(createProfileGate({ ODY_CODE_HOOK_PROFILE: 'minimal' }).profile).toBe('minimal');
  });

  it('falls back to strict for unknown profile values', () => {
    expect(createProfileGate({ ODY_CODE_HOOK_PROFILE: 'aggressive' }).profile).toBe('strict');
  });

  it('enables a hook when current profile is in hook.profiles', () => {
    const gate = createProfileGate({ ODY_CODE_HOOK_PROFILE: 'standard' });
    expect(
      gate.isEnabled({
        event: 'Stop',
        builtin: 'stop-format-typecheck',
        profiles: ['standard'],
      } as unknown as HookDef),
    ).toBe(true);
  });

  it('disables a hook when current profile is not in hook.profiles', () => {
    const gate = createProfileGate({ ODY_CODE_HOOK_PROFILE: 'minimal' });
    expect(
      gate.isEnabled({
        event: 'Stop',
        builtin: 'stop-format-typecheck',
        profiles: ['standard', 'strict'],
      } as unknown as HookDef),
    ).toBe(false);
  });

  it('disables a hook by id', () => {
    const gate = createProfileGate({ ODY_CODE_DISABLED_HOOKS: 'pre:bash:quality' });
    expect(
      gate.isEnabled({
        event: 'PreToolUse',
        command: 'echo ok',
        id: 'pre:bash:quality',
      } as unknown as HookDef),
    ).toBe(false);
  });

  it('disables a hook case-insensitively by builtin id', () => {
    const gate = createProfileGate({ ODY_CODE_DISABLED_HOOKS: 'Stop-Format-TypeCheck' });
    expect(
      gate.isEnabled({
        event: 'Stop',
        builtin: 'stop-format-typecheck',
      } as unknown as HookDef),
    ).toBe(false);
  });

  it('preserves a must-survive hook id that contains the word "stop" as a substring', () => {
    const gate = createProfileGate({ ODY_CODE_DISABLED_HOOKS: 'stop-format-typecheck' });
    expect(
      gate.isEnabled({
        event: 'Stop',
        builtin: 'nonstop-reporter',
      } as unknown as HookDef),
    ).toBe(true);
  });
});
