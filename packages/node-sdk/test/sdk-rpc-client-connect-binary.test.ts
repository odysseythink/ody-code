import { chmod, mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { SDKRpcClient } from '../src/rpc';

async function createMockHostScript(): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), 'ody-mock-host-'));
  const script = join(dir, 'mock-host.mjs');
  await writeFile(
    script,
    `#!/usr/bin/env node
import { createServer } from 'node:net';
const mode = process.argv.includes('--stdio') ? 'stdio' : 'socket';
const socketArg = process.argv.find((a, i) => i > 0 && process.argv[i - 1] === '--socket-path');
if (mode === 'socket' && socketArg) {
  const server = createServer((socket) => {
    socket.on('data', (chunk) => socket.write(chunk));
  });
  await new Promise((resolve) => server.listen(socketArg, resolve));
  console.error(JSON.stringify({ type: 'ready', socketPath: socketArg }));
} else {
  console.error(JSON.stringify({ type: 'ready', stdio: true }));
  process.stdin.on('data', (chunk) => process.stdout.write(chunk));
}
`,
  );
  await chmod(script, 0o755);
  return script;
}

describe('SDKRpcClient.connect with custom binary', () => {
  it('spawns stdio binary and passes --config/--home', async () => {
    const binaryPath = await createMockHostScript();
    const homeDir = await mkdtemp(join(tmpdir(), 'ody-home-'));
    const client = await SDKRpcClient.connect({
      transport: 'stdio',
      binaryPath,
      homeDir,
      configPath: join(homeDir, 'config.toml'),
    });
    expect(client.homeDir).toBe(homeDir);
    await client.close?.();
  });

  it('spawns socket binary and connects', async () => {
    const binaryPath = await createMockHostScript();
    const homeDir = await mkdtemp(join(tmpdir(), 'ody-home-'));
    const socketPath = join(homeDir, 'host.sock');
    const client = await SDKRpcClient.connect({
      transport: { socketPath, spawn: true },
      binaryPath,
      homeDir,
    });
    expect(client.homeDir).toBe(homeDir);
    await client.close?.();
  });
});
