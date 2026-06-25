import { chmod, mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { RustHostConnector } from '../../src/host';

async function createMockRustHost(): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), 'ody-mock-rust-host-'));
  const script = join(dir, 'ody-host');
  await writeFile(
    script,
    `#!/usr/bin/env node
process.stderr.write(JSON.stringify({ type: 'ready', stdio: true }) + '\\n');
process.stdin.on('data', (chunk) => process.stdout.write(chunk));
`,
  );
  await chmod(script, 0o755);
  return script;
}

describe('RustHostConnector', () => {
  it('connects via stdio and reports homeDir', async () => {
    const binaryPath = await createMockRustHost();
    const homeDir = await mkdtemp(join(tmpdir(), 'ody-home-'));
    const client = await RustHostConnector.connect({
      mode: 'stdio',
      binaryPath,
      homeDir,
      configPath: join(homeDir, 'config.toml'),
    });
    expect(client.homeDir).toBe(homeDir);
    (client as any).close?.();
  });

  it('emits disconnect when the host process exits', async () => {
    const binaryPath = await createMockRustHost();
    const homeDir = await mkdtemp(join(tmpdir(), 'ody-home-'));
    const connector = new RustHostConnector();
    const client = await connector.connect({ mode: 'stdio', binaryPath, homeDir });
    const disconnected = new Promise<Error>((resolve) => connector.onDisconnect(resolve));
    (client as any).close?.();
    const error = await disconnected;
    expect(error.message).toMatch(/disconnected/i);
  });
});
