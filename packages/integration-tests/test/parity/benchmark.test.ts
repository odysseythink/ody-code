import { describe, expect, it } from 'vitest';

import { BENCHMARK_TOKENS, formatBenchmark, runBenchmark } from '../../src/parity/benchmark';

describe('parity benchmark', () => {
  it('ts and rust produce the same number of mock tokens', async () => {
    const results = await runBenchmark();
    expect(results.ts.tokens).toBe(BENCHMARK_TOKENS);
    expect(results.rust.tokens).toBe(BENCHMARK_TOKENS);
    expect(results.ts.fullText.trim()).toBe(results.rust.fullText.trim());
    console.log('\n' + formatBenchmark(results));
  }, 60000);
});
