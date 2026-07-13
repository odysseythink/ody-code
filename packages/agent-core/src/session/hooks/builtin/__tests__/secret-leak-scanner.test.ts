import { describe, expect, it } from 'vitest';

import { SecretLeakScannerBuiltin } from '../secret-leak-scanner';

const ctx = {
  cwd: '/tmp',
  env: {},
  timeout: 5,
};

describe('SecretLeakScannerBuiltin', () => {
  it('allows normal tool input', async () => {
    const hook = new SecretLeakScannerBuiltin();
    const result = await hook.run({ toolName: 'Read', toolInput: { path: 'README.md' } }, ctx);
    expect(result.action).toBe('allow');
  });

  it('warns on secret in Bash command without blocking by default', async () => {
    const hook = new SecretLeakScannerBuiltin();
    const result = await hook.run(
      {
        toolName: 'Bash',
        toolInput: {
          command: 'curl -H "Authorization: ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" https://api.github.com',
        },
      },
      ctx,
    );
    expect(result.action).toBe('allow');
    expect(result.reason).toContain('github-pat');
  });

  it('blocks when blockOnMatch is true', async () => {
    const hook = new SecretLeakScannerBuiltin({ blockOnMatch: true });
    const result = await hook.run(
      { toolName: 'Bash', toolInput: { command: 'export AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE' } },
      ctx,
    );
    expect(result.action).toBe('block');
    expect(result.reason).toContain('aws-access-key-id');
  });

  it('scans all string values in non-Bash tool input', async () => {
    const hook = new SecretLeakScannerBuiltin();
    const result = await hook.run(
      { toolName: 'WebSearch', toolInput: { query: 'ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa example' } },
      ctx,
    );
    expect(result.action).toBe('allow');
    expect(result.reason).toContain('github-pat');
  });
});
