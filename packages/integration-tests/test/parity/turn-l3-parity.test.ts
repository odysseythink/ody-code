import { execFile } from 'node:child_process';
import { existsSync } from 'node:fs';
import { promisify } from 'node:util';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import { assertParity } from '../../src/parity/assert-parity';
import { normalizeTurnSnapshot } from '../../src/parity/normalize-turn';
import { runTurnL3Fixture } from '../../src/parity/turn-l3-driver';

const execFileAsync = promisify(execFile);

const __dirname = dirname(fileURLToPath(import.meta.url));
const fixturesDir = join(__dirname, '../../src/parity/fixtures/turn');

const fixtures = [
  'end-turn.json',
  'single-tool-call.json',
  'tool-not-found.json',
  'steer-buffer.json',
  'cancel-mid-step.json',
  'same-step-dedup.json',
  'cross-step-dedup.json',
  'goal-continuation.json',
];

function findProjectRoot(): string {
  let current = __dirname;
  while (current !== dirname(current)) {
    if (existsSync(join(current, '.git'))) {
      return current;
    }
    current = dirname(current);
  }
  return process.cwd();
}

async function runRustFixture(fixtureName: string): Promise<unknown> {
  const root = findProjectRoot();
  const fixturePath = join(fixturesDir, fixtureName);
  const { stdout } = await execFileAsync(
    'cargo',
    ['run', '--quiet', '--bin', 'turn_l3', '--', fixturePath],
    { cwd: join(root, 'rust-ody') },
  );
  return JSON.parse(stdout) as unknown;
}

describe('turn L3 TS-vs-Rust parity', () => {
  it.each(fixtures)(
    '%s matches the Rust golden binary',
    async (fixtureName) => {
      const fixturePath = join(fixturesDir, fixtureName);

      const tsSnapshot = normalizeTurnSnapshot(await runTurnL3Fixture(fixturePath), 'ts');
      const rustSnapshot = normalizeTurnSnapshot(
        (await runRustFixture(fixtureName)) as {
          readonly name: string;
          readonly turns: unknown;
          readonly events: unknown;
          readonly records: unknown;
          readonly contextInputs: unknown;
          readonly telemetry: unknown;
          readonly goalState?: unknown;
        },
        'rust',
      );

      const diff = assertParity(fixtureName, tsSnapshot as never, rustSnapshot as never);
      expect(diff).toBeNull();
    },
    120000,
  );
});
