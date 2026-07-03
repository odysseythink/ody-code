import { performance } from 'node:perf_hooks';

import type { ChatProvider } from '@odysseythink/kosong';

import { makeTsBackend, makeRustBackend, createTempHome, cleanupHome } from './backends';
import { MockChatProvider } from './fixtures/mock-provider';
import { resolveRustBinaryPath } from './rust-binary';

export interface BenchmarkResult {
  readonly backend: 'ts' | 'rust';
  readonly firstTokenMs: number;
  readonly totalMs: number;
  readonly tokens: number;
  readonly fullText: string;
  readonly throughputTokensPerSec: number;
}

export const BENCHMARK_TOKENS = 50;

const mockLlm: ChatProvider = new MockChatProvider(
  Array.from({ length: BENCHMARK_TOKENS }, (_, i) => ({ type: 'text' as const, text: `tok${i} ` })),
);

function benchmarkResponse(): string {
  return Array.from({ length: BENCHMARK_TOKENS }, (_, i) => `tok${i}`).join(' ');
}

async function runBackend(backend: 'ts' | 'rust', homeDir: string): Promise<BenchmarkResult> {
  const binaryPath = resolveRustBinaryPath();
  const makeBackend =
    backend === 'ts'
      ? () => makeTsBackend({ homeDir, mockLlm })
      : () =>
          makeRustBackend({
            homeDir,
            binaryPath,
            transport: 'stdio',
            extraArgs: ['--mock-provider'],
          });

  const b = await makeBackend();
  try {
    const summary = await b.client.createSession({ workDir: homeDir });

    let firstTokenAt: number | undefined;
    let lastTokenAt: number | undefined;
    const chunks: string[] = [];
    const unsubscribe = b.client.onEvent((event) => {
      if (event.type === 'assistant.delta') {
        const now = performance.now();
        if (firstTokenAt === undefined) firstTokenAt = now;
        lastTokenAt = now;
        const delta = (event as { delta?: string }).delta;
        if (typeof delta === 'string') {
          chunks.push(delta);
        }
      }
    });

    const start = performance.now();
    await b.client.prompt({
      sessionId: summary.id,
      input: [{ type: 'text', text: 'benchmark' }],
    });

    // Wait a short grace period for all deltas to arrive.
    await new Promise((resolve) => setTimeout(resolve, 500));
    unsubscribe();
    const totalMs = performance.now() - start;

    const fullText = chunks.join('');
    const tokens = fullText.trim().length > 0 ? fullText.trim().split(/\s+/).length : 0;

    return {
      backend,
      firstTokenMs: firstTokenAt !== undefined ? firstTokenAt - start : totalMs,
      totalMs,
      tokens,
      fullText,
      throughputTokensPerSec: totalMs > 0 ? (tokens / totalMs) * 1000 : 0,
    };
  } finally {
    await b.close();
  }
}

export async function runBenchmark(): Promise<{
  readonly ts: BenchmarkResult;
  readonly rust: BenchmarkResult;
}> {
  const tsHome = await createTempHome('parity-bench-ts-');
  const rustHome = await createTempHome('parity-bench-rust-');
  process.env['ODY_MOCK_RESPONSE'] = benchmarkResponse();
  try {
    const [ts, rust] = await Promise.all([
      runBackend('ts', tsHome),
      runBackend('rust', rustHome),
    ]);
    return { ts, rust };
  } finally {
    delete process.env['ODY_MOCK_RESPONSE'];
    await cleanupHome(tsHome);
    await cleanupHome(rustHome);
  }
}

export function formatBenchmark(results: {
  readonly ts: BenchmarkResult;
  readonly rust: BenchmarkResult;
}): string {
  const { ts, rust } = results;
  return [
    '| backend | firstTokenMs | totalMs | tokens | throughput (tok/s) |',
    '|---|---|---|---|---|',
    `| ts | ${ts.firstTokenMs.toFixed(2)} | ${ts.totalMs.toFixed(2)} | ${ts.tokens} | ${ts.throughputTokensPerSec.toFixed(2)} |`,
    `| rust | ${rust.firstTokenMs.toFixed(2)} | ${rust.totalMs.toFixed(2)} | ${rust.tokens} | ${rust.throughputTokensPerSec.toFixed(2)} |`,
  ].join('\n');
}
