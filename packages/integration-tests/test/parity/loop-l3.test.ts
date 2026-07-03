import { execFile } from 'node:child_process';
import { existsSync } from 'node:fs';
import { promisify } from 'node:util';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import { assertParity } from '../../src/parity/assert-parity';
import { runLoopL3Fixture } from '../../src/parity/loop-l3-driver';
import { normalizeLoopSnapshot } from '../../src/parity/normalize-loop';

const execFileAsync = promisify(execFile);

const __dirname = dirname(fileURLToPath(import.meta.url));
const fixturesDir = join(__dirname, '../../src/parity/fixtures/loop');

const fixtures = [
  'end-turn.json',
  'single-tool-call.json',
  'tool-not-found.json',
  'tool-stops-turn.json',
  'two-tool-calls.json',
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
    ['run', '--quiet', '--bin', 'loop_l3', '--', fixturePath],
    { cwd: join(root, 'rust-ody') },
  );
  return JSON.parse(stdout) as unknown;
}

describe('loop L3 TS-vs-Rust parity', () => {
  it.each(fixtures)(
    '%s matches the Rust golden binary',
    async (fixtureName) => {
      const fixturePath = join(fixturesDir, fixtureName);

      const tsSnapshot = normalizeLoopSnapshot(await runLoopL3Fixture(fixturePath));
      const rustSnapshot = normalizeLoopSnapshot(
        (await runRustFixture(fixtureName)) as {
          readonly turnResult: unknown;
          readonly recordedEvents: unknown;
          readonly liveEvents: unknown;
        },
      );

      const diff = assertParity(fixtureName, tsSnapshot as never, rustSnapshot as never);
      expect(diff).toBeNull();
    },
    120000,
  );
});
