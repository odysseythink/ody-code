import { fileURLToPath } from 'node:url';

import {
  loadWasmModule,
  wrapWithFallback,
  type WasmFlagId,
  type LoadContext,
} from '@odysseythink/agent-core-shared';
import { callWasmStringFunction } from '@odysseythink/agent-core-shared';

const WASM_PATH = fileURLToPath(
  new URL('../../../rust-ody/target/wasm32-unknown-unknown/release/ody_rust.wasm', import.meta.url),
);

const DIFF_FLAG: WasmFlagId = 'wasm-diff';

export interface DiffModule {
  readonly computeTextDiff: (oldText: string, newText: string) => string;
  readonly formatGitDiff: (rawDiff: string) => string;
}

export async function loadWasmDiffModule(wasmBytes?: Uint8Array, context?: LoadContext): Promise<DiffModule> {
  return loadWasmModule(
    {
      wasmPath: WASM_PATH,
      fallback: {
        computeTextDiff: computeTextDiffJs,
        formatGitDiff: formatGitDiffJs,
      },
      flagId: DIFF_FLAG,
      factory: (exports) => ({
        computeTextDiff: wrapWithFallback(
          (oldText: string, newText: string) =>
            callWasmStringFunction(exports, 'compute_diff', oldText, newText),
          computeTextDiffJs,
          DIFF_FLAG,
        ),
        formatGitDiff: wrapWithFallback(
          (rawDiff: string) => callWasmStringFunction(exports, 'format_git_diff', rawDiff),
          formatGitDiffJs,
          DIFF_FLAG,
        ),
      }),
    },
    wasmBytes,
    context,
  );
}

let wasmDiffModule: DiffModule | undefined;

export async function initDiffWasm(context?: LoadContext): Promise<void> {
  wasmDiffModule = await loadWasmDiffModule(undefined, context);
}

export function computeTextDiff(oldText: string, newText: string): string {
  return (wasmDiffModule?.computeTextDiff ?? computeTextDiffJs)(oldText, newText);
}

export function formatGitDiff(rawDiff: string): string {
  return (wasmDiffModule?.formatGitDiff ?? formatGitDiffJs)(rawDiff);
}

// JS fallback: identity — if Wasm is unavailable the raw diff is already usable.
function formatGitDiffJs(rawDiff: string): string {
  return rawDiff;
}

// JS fallback: simple line-based unified diff (no hunk optimization).
function computeTextDiffJs(oldText: string, newText: string): string {
  const oldLines = oldText.split('\n');
  const newLines = newText.split('\n');
  const lcs = longestCommonSubsequence(oldLines, newLines);

  let result = '--- old\n+++ new\n';
  let i = 0;
  let j = 0;
  for (const [oldIdx, newIdx] of lcs) {
    while (i < oldIdx) {
      result += `-${oldLines[i]}\n`;
      i += 1;
    }
    while (j < newIdx) {
      result += `+${newLines[j]}\n`;
      j += 1;
    }
    result += ` ${oldLines[i]}\n`;
    i += 1;
    j += 1;
  }
  while (i < oldLines.length) {
    result += `-${oldLines[i]}\n`;
    i += 1;
  }
  while (j < newLines.length) {
    result += `+${newLines[j]}\n`;
    j += 1;
  }
  return result;
}

function longestCommonSubsequence<T>(a: readonly T[], b: readonly T[]): Array<[number, number]> {
  const m = a.length;
  const n = b.length;
  if (m === 0 || n === 0) return [];

  const dp: number[][] = Array.from({ length: m + 1 }, () => Array(n + 1).fill(0));
  for (let i = 1; i <= m; i += 1) {
    const aPrev = a[i - 1];
    if (aPrev === undefined) break;
    for (let j = 1; j <= n; j += 1) {
      const bPrev = b[j - 1];
      if (bPrev === undefined) break;
      dp[i]![j] = aPrev === bPrev ? dp[i - 1]![j - 1]! + 1 : Math.max(dp[i - 1]![j]!, dp[i]![j - 1]!);
    }
  }

  const result: Array<[number, number]> = [];
  let i = m;
  let j = n;
  while (i > 0 && j > 0) {
    const dpPrev = dp[i - 1];
    const dpCurr = dp[i];
    const aPrev = a[i - 1];
    const bPrev = b[j - 1];
    if (dpPrev === undefined || dpCurr === undefined || aPrev === undefined || bPrev === undefined) break;
    if (aPrev === bPrev) {
      result.unshift([i - 1, j - 1]);
      i -= 1;
      j -= 1;
    } else if (dpPrev[j]! >= dpCurr[j - 1]!) {
      i -= 1;
    } else {
      j -= 1;
    }
  }
  return result;
}
