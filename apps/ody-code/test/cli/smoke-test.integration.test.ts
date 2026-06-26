import { exec } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const __dirname = dirname(fileURLToPath(import.meta.url));
// apps/ody-code/test/cli -> apps/ody-code
const appDir = resolve(__dirname, '../..');
// apps/ody-code -> project root -> rust-ody/target/release/ody-host
const ODY_HOST_BINARY_PATH = resolve(appDir, '../../rust-ody/target/release/ody-host');
const ODY = 'node dist/main.mjs';

function runSmoke(args: string[]): Promise<{ exitCode: number; stdout: string; stderr: string }> {
  return new Promise((resolve) => {
    const cmd = `${ODY} ${args.join(' ')}`;
    exec(
      cmd,
      {
        env: process.env,
        cwd: appDir,
      },
      (error, stdout, stderr) => {
        resolve({ exitCode: error?.code ?? 0, stdout, stderr });
      },
    );
  });
}

describe('TUI smoke mode integration', () => {
  it('stdio transport exits 0 with SMOKE_OK', async () => {
    const { exitCode, stdout } = await runSmoke(['--host=rust', '--host-stdio', '--host-binary', ODY_HOST_BINARY_PATH, '--smoke-test']);
    expect(exitCode).toBe(0);
    expect(stdout).toMatch(/^SMOKE_OK stdio /);
  }, 60_000);

  it('socket transport exits 0 with SMOKE_OK', async () => {
    const { exitCode, stdout } = await runSmoke(['--host=rust', '--host-socket', '/tmp/ody-smoke-test.sock', '--host-binary', ODY_HOST_BINARY_PATH, '--smoke-test']);
    expect(exitCode).toBe(0);
    expect(stdout).toMatch(/^SMOKE_OK socket /);
  }, 60_000);

  it('tcp transport exits 0 with SMOKE_OK', async () => {
    const { exitCode, stdout } = await runSmoke(['--host=rust', '--host-tcp', '127.0.0.1:19095', '--host-binary', ODY_HOST_BINARY_PATH, '--smoke-test']);
    expect(exitCode).toBe(0);
    expect(stdout).toMatch(/^SMOKE_OK tcp /);
  }, 60_000);

  it('bad binary path exits 1 with SMOKE_FAIL', async () => {
    const cmd = `${ODY} --host=rust --host-stdio --host-binary /nonexistent/ody-host --smoke-test`;
    const { exitCode, stderr } = await new Promise<{ exitCode: number; stderr: string }>((resolve) => {
      exec(
        cmd,
        {
          env: process.env,
          cwd: appDir,
        },
        (error, _stdout, stderr) => {
          resolve({ exitCode: error?.code ?? 0, stderr });
        },
      );
    });
    expect(exitCode).not.toBe(0);
    expect(stderr).toMatch(/SMOKE_FAIL stdio:/);
  }, 60_000);
});
