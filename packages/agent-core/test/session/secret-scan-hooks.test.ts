import { describe, expect, it } from 'vitest';

import { createBuiltinHookRegistry } from '../../src/session/hooks/builtin/registry';
import { HookEngine } from '../../src/session/hooks/engine';

function buildSecretScanHooks(enabled: boolean) {
  return enabled
    ? [
        {
          event: 'PreToolUse' as const,
          builtin: 'secret-leak-scanner',
          id: 'secret-leak-scanner:pre-tool',
        },
      ]
    : [];
}

describe('secret-scan default hook wiring', () => {
  it('warns on secret when hook is registered', async () => {
    const engine = new HookEngine(buildSecretScanHooks(true), {
      builtins: createBuiltinHookRegistry(),
    });
    const results = await engine.trigger('PreToolUse', {
      matcherValue: 'Bash',
      inputData: {
        toolName: 'Bash',
        toolInput: { command: 'export AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE' },
      },
    });
    expect(
      results.some((r) => r.action === 'allow' && r.reason?.includes('aws-access-key-id')),
    ).toBe(true);
  });

  it('does nothing when hook is not registered', async () => {
    const engine = new HookEngine(buildSecretScanHooks(false), {
      builtins: createBuiltinHookRegistry(),
    });
    const results = await engine.trigger('PreToolUse', {
      matcherValue: 'Bash',
      inputData: {
        toolName: 'Bash',
        toolInput: { command: 'export AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE' },
      },
    });
    expect(results).toHaveLength(0);
  });
});
