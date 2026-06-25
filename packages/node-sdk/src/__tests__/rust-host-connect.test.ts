import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createStreamTransport } from '@odysseythink/agent-core';
import { afterEach, describe, expect, test } from 'vitest';

const workspaceRoot = process.cwd();
const cargoDir = join(workspaceRoot, 'rust-ody');
const binaryPath = join(cargoDir, 'target/debug/ody-host');

function buildHost(): void {
  if (existsSync(binaryPath)) return;
  const result = spawnSync('cargo', ['build', '-p', 'ody-host'], {
    cwd: cargoDir,
    stdio: 'pipe',
    encoding: 'utf-8',
  });
  if (result.status !== 0) {
    throw new Error(`cargo build failed:\n${result.stderr}`);
  }
}

function makeTempConfig(): { configPath: string; homeDir: string } {
  const homeDir = mkdtempSync(join(tmpdir(), 'ody-host-test-'));
  const configPath = join(homeDir, 'ody.toml');
  const escaped = homeDir.replace(/\\/g, '\\\\');
  writeFileSync(
    configPath,
    `home_dir = "${escaped}"\nlog_level = "error"\n\n[provider]\napi_key = ""\ndefault_model = "mock"\n`,
  );
  return { configPath, homeDir };
}

function encodeRpcRequest(method: string, payload: unknown): Uint8Array {
  return new TextEncoder().encode(JSON.stringify({ method, args: [payload] }));
}

describe('rust host stdio transport', () => {
  let proc: ReturnType<typeof spawn> | undefined;
  let transport: ReturnType<typeof createStreamTransport> | undefined;
  let cleanup: (() => void) | undefined;
  let stderrBuf = '';

  afterEach(() => {
    transport?.close?.();
    proc?.kill();
    cleanup?.();
  });

  test(
    'getCoreInfo and createSession roundtrip',
    async () => {
      buildHost();
      const { configPath, homeDir } = makeTempConfig();
      cleanup = () => rmSync(homeDir, { recursive: true, force: true });

      proc = spawn(binaryPath, ['--config', configPath, '--stdio', '--log-level', 'error'], {
        stdio: ['pipe', 'pipe', 'pipe'],
      });

      proc.stderr!.on('data', (d: Buffer) => {
        stderrBuf += d.toString();
      });

      // Wait for host to finish initialization.
      await sleep(500);

      // If process already exited, something went wrong
      if (proc.exitCode !== null || proc.killed) {
        throw new Error(`ody-host exited early (code: ${proc.exitCode}), stderr: ${stderrBuf}`);
      }

      transport = createStreamTransport(
        proc.stdout!,
        proc.stdin!,
        async () => {
          // host's reverse RPC/emitEvent arrives here; test doesn't need to handle it.
          return new TextEncoder().encode(JSON.stringify({ ok: true, value: null }));
        },
        { framing: 'length-prefixed' },
      );

      const infoBytes = await transport.send(encodeRpcRequest('getCoreInfo', {}));
      const info = JSON.parse(new TextDecoder().decode(infoBytes));
      expect(info.ok).toBe(true);
      expect(info.value.version).toMatch(/^\d+\.\d+\.\d+/);

      const createBytes = await transport.send(
        encodeRpcRequest('createSession', { workDir: process.cwd() }),
      );
      const create = JSON.parse(new TextDecoder().decode(createBytes));
      expect(create.ok).toBe(true);
      expect(create.value.id).toBeDefined();
      expect(create.value.workDir).toBe(process.cwd());
    },
    60000,
  );
});
