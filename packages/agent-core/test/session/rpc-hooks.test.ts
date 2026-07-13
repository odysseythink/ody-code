import { describe, expect, it } from 'vitest';

import { createBuiltinHookRegistry } from '../../src/session/hooks/builtin/registry';
import { HookEngine } from '../../src/session/hooks/engine';
import { SessionAPIImpl } from '../../src/session/rpc';
import type { Session } from '../../src/session';

describe('SessionAPIImpl.getHooksInfo', () => {
  it('returns profile, disabled hooks, summary, executions and counts', () => {
    const engine = new HookEngine(
      [
        { event: 'Stop', command: 'echo ok' },
        { event: 'PostToolUse', builtin: 'edit-accumulator' },
      ],
      {
        env: { ODY_CODE_HOOK_PROFILE: 'minimal', ODY_CODE_DISABLED_HOOKS: 'stop-format-typecheck' },
        builtins: createBuiltinHookRegistry(),
      },
    );
    const fakeSession = { hookEngine: engine } as unknown as Session;
    const api = new SessionAPIImpl(fakeSession);

    const info = api.getHooksInfo({});

    expect(info.profile).toBe('minimal');
    expect(info.disabled).toContain('stop-format-typecheck');
    expect(info.summary).toEqual({ Stop: 1, PostToolUse: 1 });
    expect(info.executions).toEqual([]);
    expect(info.counts).toEqual({
      allow: 0,
      block: 0,
      error: 0,
      timeout: 0,
      'skipped-profile': 0,
      dropped: 0,
    });
  });
});
