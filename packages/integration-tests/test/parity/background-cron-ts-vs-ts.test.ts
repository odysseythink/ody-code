import { describe, expect, it } from 'vitest';

import {
  assertNoDiff,
  backgroundCronFixtures,
  runTsSnapshot,
} from '../../src/parity/background-cron-parity';

describe('background-cron TS↔TS parity', () => {
  it.each(backgroundCronFixtures)(
    '%s produces identical normalized snapshots on two TS runs',
    async (fixtureName) => {
      // Run serially to avoid concurrent mutation of
      // process.env ODY_CRON_CLOCK / ODY_CRON_MANUAL_TICK.
      const first = await runTsSnapshot(fixtureName);
      const second = await runTsSnapshot(fixtureName);

      expect(() => assertNoDiff(fixtureName, first, second)).not.toThrow();
    },
    120_000,
  );
});
