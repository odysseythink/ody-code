import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, rmSync } from 'node:fs';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const __dirname = fileURLToPath(new URL('.', import.meta.url));
const repoRoot = dirname(dirname(dirname(dirname(__dirname))));
const outPath = `${repoRoot}/scripts/generated/rpc-schema.json`;

describe('gen-rpc-schema', () => {
  it('generates a schema covering CoreAPI and SDKAPI methods', { timeout: 120_000 }, () => {
    rmSync(outPath, { force: true });
    expect(existsSync(outPath)).toBe(false);

    execFileSync('pnpm', ['-w', 'exec', 'tsx', 'scripts/gen-rpc-schema.ts'], {
      cwd: repoRoot,
      stdio: 'ignore',
    });

    expect(existsSync(outPath)).toBe(true);
    const schema = JSON.parse(readFileSync(outPath, 'utf-8')) as {
      title: string;
      protocols: {
        core: { methods: Record<string, { payload: unknown; returns: unknown }> };
        sdk: { methods: Record<string, { payload: unknown; returns: unknown }> };
      };
    };

    expect(schema.title).toBe('Ody Code RPC API');
    expect(schema.protocols.core.methods['createSession']).toBeDefined();
    expect(schema.protocols.sdk.methods['emitEvent']).toBeDefined();
  });
});
