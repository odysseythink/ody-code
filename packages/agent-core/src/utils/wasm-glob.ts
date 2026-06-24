import picomatch from 'picomatch';
import { fileURLToPath } from 'node:url';

import { loadWasmModule, type WasmFlagId } from './wasm-loader';
import { callWasmU32Function } from './wasm-string';
import type { LoadContext } from './wasm-loader';

const WASM_PATH = fileURLToPath(
  new URL('../../../../rust-ody/target/wasm32-unknown-unknown/release/ody_rust.wasm', import.meta.url),
);

const GLOB_FLAG: WasmFlagId = 'wasm-glob';
const GLOB_ERROR = 0xFFFFFFFF;

export type GlobMatcher = (value: string, pattern: string, options?: { nocase?: boolean }) => boolean;

let wasmGlobMatcher: GlobMatcher | undefined;

export async function initGlobWasm(context?: LoadContext): Promise<void> {
  wasmGlobMatcher = await loadWasmGlobMatcher(undefined, context);
}

export async function loadWasmGlobMatcher(
  wasmBytes?: Uint8Array,
  context?: LoadContext,
): Promise<GlobMatcher> {
  return loadWasmModule(
    {
      wasmPath: WASM_PATH,
      fallback: globMatchJs,
      flagId: GLOB_FLAG,
      factory: (exports) => (value, pattern, options) => {
        const wasmResult = callWasmU32Function(
          exports,
          'glob_match',
          value,
          pattern,
          options?.nocase ? 'true' : 'false',
        );
        // Wasm only confirms positive matches; anything else (GLOB_ERROR or
        // genuine no-match) falls back to picomatch for byte-exact parity.
        if (wasmResult === 1) {
          return true;
        }
        return globMatchJs(value, pattern, options);
      },
    },
    wasmBytes,
    context,
  );
}

export function globMatch(value: string, pattern: string, options?: { nocase?: boolean }): boolean {
  const fn = wasmGlobMatcher;
  if (fn !== undefined) {
    try {
      return fn(value, pattern, options);
    } catch {
      // fallthrough to JS
    }
  }
  return globMatchJs(value, pattern, options);
}

function globMatchJs(value: string, pattern: string, options?: { nocase?: boolean }): boolean {
  if (picomatch.isMatch(value, pattern, options)) return true;

  const normalizedValue = stripLeadingDotSlash(value);
  const normalizedPattern = stripLeadingDotSlash(pattern);
  if (normalizedValue === value && normalizedPattern === pattern) return false;
  return picomatch.isMatch(normalizedValue, normalizedPattern, options);
}

function stripLeadingDotSlash(value: string): string {
  return value.startsWith('./') ? value.slice(2) : value;
}
