import { describe, expect, it } from 'vitest';

import { runTsVsTs } from '../../src/parity/run-parity';

describe('TS-vs-TS parity harness', () => {
  it(
    'produces identical normalized snapshots for every scenario',
    async () => {
      const results = await runTsVsTs({ timeoutMs: 30000 });
      for (const result of results) {
        expect(result.equal).toBe(true);
      }
    },
    120000,
  );
});
