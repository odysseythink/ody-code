import { describe, expect, it } from 'vitest';

import {
  assertNoDiff,
  backgroundCronFixtures,
  runRustSnapshot,
  runTsSnapshot,
} from '../../src/parity/background-cron-parity';

describe('background-cron TS↔Rust parity', () => {
  it.each(backgroundCronFixtures)(
    '%s matches between TS driver and Rust binary',
    async (fixtureName) => {
      const tsSnapshot = await runTsSnapshot(fixtureName);
      const rustSnapshot = await runRustSnapshot(fixtureName);

      expect(() => assertNoDiff(fixtureName, tsSnapshot, rustSnapshot)).not.toThrow();
    },
    120_000,
  );
});
