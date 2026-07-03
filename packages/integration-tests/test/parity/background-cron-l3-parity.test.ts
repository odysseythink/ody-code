import { execFile } from 'node:child_process';
import { dirname, join } from 'node:path';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import { assertParity } from '../../src/parity/assert-parity';
import { runBackgroundCronL3Fixture } from '../../src/parity/background-cron-l3-driver';
import { normalizeBackgroundCronSnapshot } from '../../src/parity/normalize-background-cron';

const execFileAsync = promisify(execFile);

const __dirname = dirname(fileURLToPath(import.meta.url));
const fixturesDir = join(__dirname, 'fixtures', 'background-cron');
const projectRoot = dirname(dirname(dirname(__dirname)));

const fixtures = [
  'cron-fire.json',
  'background-process-completes.json',
  'cron-remove-last.json',
];

async function runRustFixture(fixtureName: string): Promise<unknown> {
  const fixturePath = join(fixturesDir, fixtureName);
  const { stdout } = await execFileAsync(
    'cargo',
    ['run', '--quiet', '--bin', 'background_cron_l3', '--', fixturePath],
    { cwd: join(projectRoot, 'rust-ody') },
  );
  return JSON.parse(stdout) as unknown;
}

describe('background-cron L3 TS-vs-Rust parity', () => {
  it.each(fixtures)(
    '%s matches the Rust golden binary',
    async (fixtureName) => {
      const fixturePath = join(fixturesDir, fixtureName);
      const tsSnapshot = normalizeBackgroundCronSnapshot(
        await runBackgroundCronL3Fixture(fixturePath),
      );
      const rustSnapshot = normalizeBackgroundCronSnapshot(await runRustFixture(fixtureName));
      const diff = assertParity(fixtureName, tsSnapshot as never, rustSnapshot as never);
      expect(diff).toBeNull();
    },
    120_000,
  );
});
