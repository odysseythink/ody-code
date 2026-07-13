import { describe, expect, it } from 'vitest';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';

import { SecretLeakScannerBuiltin } from '../secret-leak-scanner';

const ctx = {
  cwd: '/tmp',
  env: {},
  timeout: 5,
};

async function makeTmpSessionDir(): Promise<string> {
  return mkdtemp(join(tmpdir(), 'secret-scan-hook-'));
}

describe('SecretLeakScannerBuiltin', () => {
  it('allows normal tool input', async () => {
    const hook = new SecretLeakScannerBuiltin();
    const result = await hook.run(
      { toolName: 'Read', toolInput: { path: 'README.md' }, session_id: 's1' },
      ctx,
    );
    expect(result.action).toBe('allow');
  });

  it('warns on secret in Bash command without blocking by default', async () => {
    const dir = await makeTmpSessionDir();
    const hook = new SecretLeakScannerBuiltin(undefined, join(dir, 'secret-scan.jsonl'));
    const result = await hook.run(
      {
        toolName: 'Bash',
        toolInput: {
          command:
            'curl -H "Authorization: ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" https://api.github.com',
        },
        session_id: 's1',
      },
      ctx,
    );
    expect(result.action).toBe('allow');
    expect(result.reason).toContain('github-pat');
    const lines = (await readFile(join(dir, 'secret-scan.jsonl'), 'utf8')).trim().split('\n');
    expect(lines).toHaveLength(1);
    const record = JSON.parse(lines[0]!);
    expect(record.ruleId).toBe('github-pat');
    expect(record.action).toBe('warn');
    await rm(dir, { recursive: true, force: true });
  });

  it('blocks when blockOnMatch is true', async () => {
    const dir = await makeTmpSessionDir();
    const hook = new SecretLeakScannerBuiltin({ blockOnMatch: true });
    const result = await hook.run(
      {
        toolName: 'Bash',
        toolInput: { command: 'export AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE' },
        session_id: 's1',
      },
      { ...ctx, env: { ODY_CODE_HOME: dir } },
    );
    expect(result.action).toBe('block');
    expect(result.reason).toContain('aws-access-key-id');
    await rm(dir, { recursive: true, force: true });
  });

  it('scans all string values in non-Bash tool input', async () => {
    const dir = await makeTmpSessionDir();
    const hook = new SecretLeakScannerBuiltin();
    const result = await hook.run(
      {
        toolName: 'WebSearch',
        toolInput: { query: 'ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa example' },
        session_id: 's1',
      },
      { ...ctx, env: { ODY_CODE_HOME: dir } },
    );
    expect(result.action).toBe('allow');
    expect(result.reason).toContain('github-pat');
    await rm(dir, { recursive: true, force: true });
  });
});
