import { existsSync } from 'node:fs';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { makeTsBackend, makeRustBackend } from '../../../src/parity/backends';
import { runParity } from '../../../src/parity/run-parity';
import { kaosOpsScenario } from '../../../src/parity/scenarios';
import { resolveRustBinaryPath } from '../../../src/parity/rust-binary';

function findProjectRoot(): string {
  let current = dirname(fileURLToPath(import.meta.url));
  while (current !== dirname(current)) {
    if (existsSync(join(current, '.git'))) return current;
    current = dirname(current);
  }
  return process.cwd();
}

const rootDir = findProjectRoot();
const binaryPath = (() => {
  try {
    return resolveRustBinaryPath(rootDir);
  } catch {
    return null;
  }
})();

describe.skipIf(binaryPath === null)('kaos ops L2 parity', () => {
  it('TS LocalKaos matches Rust kaos-rs via CoreHost env.*', async () => {
    const diff = await runParity({
      scenario: kaosOpsScenario,
      mockLlm: {} as any,
      makeA: (homeDir) => makeTsBackend({ homeDir }),
      makeB: (homeDir) =>
        makeRustBackend({
          homeDir,
          binaryPath: binaryPath!,
          transport: 'stdio',
          extraArgs: ['--mock-provider'],
        }),
      timeoutMs: 60000,
    });
    expect(diff, JSON.stringify(diff, null, 2)).toBeNull();
  }, 120000);
});
