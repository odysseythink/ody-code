import { execFile } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

import { assertParity } from './assert-parity';
import { runBackgroundCronL3Fixture } from './background-cron-l3-driver';
import { normalizeBackgroundCronSnapshot } from './normalize-background-cron';
import type { BackgroundCronSnapshot } from './background-cron-fixture';
import type { NormalizedSnapshot } from './types';

const execFileAsync = promisify(execFile);

const __dirname = dirname(fileURLToPath(import.meta.url));
const fixturesDir = join(__dirname, '../../test/parity/fixtures/background-cron');
const projectRoot = dirname(dirname(dirname(dirname(__dirname))));

export const backgroundCronFixtures = [
  'cron-fire.json',
  'background-process-completes.json',
  'cron-remove-last.json',
];

export async function runTsSnapshot(fixtureName: string): Promise<NormalizedSnapshot> {
  const snapshot = await runBackgroundCronL3Fixture(join(fixturesDir, fixtureName));
  return normalize(snapshot);
}

export async function runRustSnapshot(fixtureName: string): Promise<NormalizedSnapshot> {
  const fixturePath = join(fixturesDir, fixtureName);
  const binaryPath = process.env['ODY_BACKGROUND_CRON_BINARY_PATH'];

  let stdout: string;
  if (binaryPath !== undefined && binaryPath.length > 0) {
    ({ stdout } = await execFileAsync(binaryPath, [fixturePath]));
  } else {
    ({ stdout } = await execFileAsync(
      'cargo',
      ['run', '--quiet', '--bin', 'background_cron_l3', '--', fixturePath],
      { cwd: join(projectRoot, 'rust-ody') },
    ));
  }

  const snapshot = JSON.parse(stdout) as BackgroundCronSnapshot;
  return normalize(snapshot);
}

export function assertNoDiff(name: string, a: NormalizedSnapshot, b: NormalizedSnapshot): void {
  const diff = assertParity(name, a, b);
  if (diff !== null) {
    throw new Error(`parity diff in ${name}: ${JSON.stringify(diff.diffs, null, 2)}`);
  }
}

function normalize(snapshot: BackgroundCronSnapshot): NormalizedSnapshot {
  return normalizeBackgroundCronSnapshot(snapshot) as NormalizedSnapshot;
}
