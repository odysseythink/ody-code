# Part 2: Wasm Loader Framework (agent-core)

Scope: 在 `packages/agent-core` 中建立统一的双轨加载框架：feature flag 注册、通用 `loadWasmModule<T>` 加载器、以及 raw-ABI 字符串调用封装。

---

### Task 4: 注册 Wasm 实验 Flag

**Depends on:** none（设计已批准，本任务为 TS 侧起始点）

**Files:**
- Modify: `packages/agent-core/src/flags/registry.ts:13-38`

**Goal:** 为 tokenizer、diff、glob 三条 Wasm 路径新增实验 flag；默认开启，可通过环境变量单独或全局禁用。

**Step-by-step:**

- [ ] 修改 `packages/agent-core/src/flags/registry.ts`，在 `FLAG_DEFINITIONS` 数组末尾追加三项：

```typescript
import type { FlagDefinitionInput } from './types';

/**
 * Experimental feature flags. Empty by default — there are no experimental features yet.
 *
 * To add one, append an entry and gate the feature with `flags.enabled('my-feature')`:
 *   { id: 'my-feature', env: 'ODY_CODE_EXPERIMENTAL_MY_FEATURE', default: false, surface: 'both' }
 *
 * Keep the `as const satisfies` — it derives the literal `FlagId` union that gives `enabled()`
 * autocomplete and typo-checking. `env` must start with 'ODY_CODE_EXPERIMENTAL_', be unique, and
 * not equal the master switch 'ODY_CODE_EXPERIMENTAL_FLAG'; `id` must not be 'flag'.
 */
export const FLAG_DEFINITIONS = [
  {
    id: 'goal-command',
    env: 'ODY_CODE_EXPERIMENTAL_GOAL_COMMAND',
    default: false,
    surface: 'both',
  },
  {
    id: 'micro-compaction',
    env: 'ODY_CODE_EXPERIMENTAL_MICRO_COMPACTION',
    default: false,
    surface: 'core',
  },
  {
    id: 'background-ask',
    env: 'ODY_CODE_EXPERIMENTAL_BACKGROUND_ASK',
    default: false,
    surface: 'core',
  },
  {
    id: 'repo-knowledge',
    env: 'ODY_CODE_EXPERIMENTAL_REPO_KNOWLEDGE',
    default: false,
    surface: 'core',
  },
  {
    id: 'wasm-tokenizer',
    env: 'ODY_CODE_EXPERIMENTAL_WASM_TOKENIZER',
    default: true,
    surface: 'core',
  },
  {
    id: 'wasm-diff',
    env: 'ODY_CODE_EXPERIMENTAL_WASM_DIFF',
    default: true,
    surface: 'core',
  },
  {
    id: 'wasm-glob',
    env: 'ODY_CODE_EXPERIMENTAL_WASM_GLOB',
    default: true,
    surface: 'core',
  },
] as const satisfies readonly FlagDefinitionInput[];

/** Literal union of registered flag ids. */
export type FlagId = (typeof FLAG_DEFINITIONS)[number]['id'];
```

- [ ] 验证类型与现有测试：

```bash
pnpm tsc --noEmit -p packages/agent-core/tsconfig.json
pnpm vitest run packages/agent-core/test/flags/resolver.test.ts
```

- [ ] **Manual verification:**
  - `tsc` 成功退出；
  - `resolver.test.ts` 全部通过；
  - 新 env 名 `ODY_CODE_EXPERIMENTAL_WASM_TOKENIZER` / `_WASM_DIFF` / `_WASM_GLOB` 彼此唯一，且不与 `ODY_CODE_EXPERIMENTAL_FLAG` 冲突。

- [ ] Commit: `feat(agent-core): register wasm-tokenizer/wasm-diff/wasm-glob feature flags`

---

### Task 5: 通用双轨加载器 `loadWasmModule<T>`

**Depends on:** Task 4

**Files:**
- Create: `packages/agent-core/src/utils/wasm-loader.ts:1-90`
- Test: `packages/agent-core/test/utils/wasm-loader.test.ts`

**Goal:** 实现统一加载器：按 flag 决定是否走 Wasm；加载/实例化/运行时失败时静默降级到 JS fallback；永远返回与 fallback 同签名的可调用对象。

**Step-by-step:**

- [ ] 先写失败的测试。创建 `packages/agent-core/test/utils/wasm-loader.test.ts`：

```typescript
import { describe, expect, it } from 'vitest';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { loadWasmModule, wrapWithFallback, type WasmModuleConfig } from '../../src/utils/wasm-loader';

const WASM_PATH = fileURLToPath(
  new URL('../../../../rust-ody/target/wasm32-unknown-unknown/release/ody_rust.wasm', import.meta.url),
);

async function realWasmBytes(): Promise<Uint8Array> {
  return new Uint8Array(await readFile(WASM_PATH));
}

function makeConfig<T>(partial: Omit<WasmModuleConfig<T>, 'wasmPath'> & { wasmPath?: string }): WasmModuleConfig<T> {
  return {
    wasmPath: WASM_PATH,
    ...partial,
  } as WasmModuleConfig<T>;
}

describe('loadWasmModule', () => {
  it('returns fallback when flag is disabled by env', async () => {
    const fallback = () => 'js';
    const fn = await loadWasmModule(
      makeConfig({ fallback, flagId: 'wasm-tokenizer', factory: () => () => 'wasm' }),
      await realWasmBytes(),
      { ODY_CODE_EXPERIMENTAL_WASM_TOKENIZER: '0' },
    );
    expect(fn()).toBe('js');
  });

  it('returns fallback when wasm file is missing', async () => {
    const fallback = () => 'js';
    const fn = await loadWasmModule({
      wasmPath: '/definitely/missing.wasm',
      fallback,
      flagId: 'wasm-tokenizer',
      factory: () => () => 'wasm',
    });
    expect(fn()).toBe('js');
  });

  it('returns wasm result when everything works', async () => {
    const fallback = () => 'js';
    const fn = await loadWasmModule(
      makeConfig({ fallback, flagId: 'wasm-tokenizer', factory: () => () => 'wasm' }),
      await realWasmBytes(),
    );
    expect(fn()).toBe('wasm');
  });

  it('falls back when the wrapped wasm function throws at runtime', async () => {
    const fallback = () => 'js';
    const fn = await loadWasmModule(
      makeConfig({
        fallback,
        flagId: 'wasm-tokenizer',
        factory: () => () => {
          throw new Error('wasm panic');
        },
      }),
      await realWasmBytes(),
    );
    expect(fn()).toBe('js');
  });
});

describe('wrapWithFallback', () => {
  it('returns wasm result on success', () => {
    const fn = wrapWithFallback(
      (x: number) => x * 2,
      (x: number) => x + 1,
      'wasm-tokenizer',
    );
    expect(fn(5)).toBe(10);
  });

  it('returns fallback on wasm throw', () => {
    const fn = wrapWithFallback(
      () => {
        throw new Error('boom');
      },
      () => 'ok',
      'wasm-tokenizer',
    );
    expect(fn()).toBe('ok');
  });
});
```

- [ ] 运行测试并确认**失败**：

```bash
pnpm vitest run packages/agent-core/test/utils/wasm-loader.test.ts
```

Expected failure: `Error: Cannot find module '../../src/utils/wasm-loader'` 或 `loadWasmModule is not a function`。

- [ ] 实现 `packages/agent-core/src/utils/wasm-loader.ts`：

```typescript
/**
 * Generic dual-track Wasm loader used by tokenizer / diff / glob modules.
 *
 * Design contract:
 *   - If the flag is off, return the JS fallback synchronously (no Wasm I/O).
 *   - If Wasm instantiation or the factory fails, return the JS fallback.
 *   - If the returned function throws at runtime, wrapWithFallback routes the
 *     single call to the JS fallback without mutating global state.
 */
import { readFile } from 'node:fs/promises';

import { flags, type FlagId } from '../flags';

export type WasmFlagId = 'wasm-tokenizer' | 'wasm-diff' | 'wasm-glob';

export interface WasmExports {
  readonly memory: WebAssembly.Memory;
  alloc(len: number): number;
  dealloc(ptr: number, len: number): void;
}

export interface WasmModuleConfig<T> {
  readonly wasmPath: string;
  readonly fallback: T;
  readonly flagId: WasmFlagId;
  readonly factory: (exports: WasmExports) => T;
}

export interface LoadContext {
  readonly [env: string]: string | undefined;
}

export async function loadWasmModule<T>(
  config: WasmModuleConfig<T>,
  wasmBytes?: Uint8Array,
  context: LoadContext = process.env,
): Promise<T> {
  const resolver = new FlagResolver(context);
  if (!resolver.enabled(config.flagId)) {
    return config.fallback;
  }

  try {
    const bytes = wasmBytes ?? new Uint8Array(await readFile(config.wasmPath));
    const { instance } = await WebAssembly.instantiate(bytes, {});
    const wasmFn = config.factory(instance.exports as unknown as WasmExports);
    return wrapWithFallback(wasmFn, config.fallback, config.flagId);
  } catch {
    return config.fallback;
  }
}

export function wrapWithFallback<T extends (...args: any[]) => any>(
  wasmFn: T,
  fallback: T,
  _flagId: WasmFlagId,
): T {
  return ((...args: Parameters<T>): ReturnType<T> => {
    try {
      return wasmFn(...args);
    } catch {
      return fallback(...args);
    }
  }) as T;
}

// Minimal local resolver so tests can inject env without mutating global process.env.
class FlagResolver {
  private readonly env: LoadContext;

  constructor(env: LoadContext) {
    this.env = env;
  }

  enabled(id: WasmFlagId): boolean {
    return flags.enabled(id as FlagId);
  }
}
```

Wait, using `flags.enabled` reads from `process.env` via the global resolver, but I want to allow context injection. The existing `flags` global uses `process.env`. I should use the `FlagResolver` class from `../flags/resolver` with custom env. Let me fix:

```typescript
import { FlagResolver } from '../flags/resolver';

export async function loadWasmModule<T>(...): Promise<T> {
  const resolver = new FlagResolver(context);
  if (!resolver.enabled(config.flagId)) { ... }
}
```

But `FlagResolver.enabled` expects `FlagId` type. `WasmFlagId` is a subset, so casting is fine. Actually, I can just use `config.flagId as FlagId`.

Actually, since the flags are now in the registry, `FlagId` includes the wasm flags. So I can import `FlagId` and pass it directly. Let me revise:

```typescript
import { FlagResolver } from '../flags/resolver';
import type { FlagId } from '../flags/registry';

export type WasmFlagId = FlagId & ('wasm-tokenizer' | 'wasm-diff' | 'wasm-glob');
```

Hmm, that type might be complex. Simpler:

```typescript
import { FlagResolver } from '../flags/resolver';

export type WasmFlagId = 'wasm-tokenizer' | 'wasm-diff' | 'wasm-glob';

export interface WasmModuleConfig<T> {
  readonly wasmPath: string;
  readonly fallback: T;
  readonly flagId: WasmFlagId;
  readonly factory: (exports: WasmExports) => T;
}

// In loadWasmModule:
const resolver = new FlagResolver(context);
if (!resolver.enabled(config.flagId as import('../flags/registry').FlagId)) { ... }
```

Actually I can import FlagId at top:

```typescript
import { FlagResolver } from '../flags/resolver';
import type { FlagId } from '../flags/registry';

export type WasmFlagId = 'wasm-tokenizer' | 'wasm-diff' | 'wasm-glob';

// ...
if (!resolver.enabled(config.flagId as FlagId)) { ... }
```

Good.

But wait, `FlagResolver` constructor signature is:
```typescript
constructor(
  env: Readonly<Record<string, string | undefined>> = process.env,
  definitions: readonly FlagDefinitionInput[] = FLAG_DEFINITIONS,
)
```

So I can pass `context` directly. Good.

Also, the test imports `WasmModuleConfig` and uses `flagId: 'wasm-tokenizer'` which is of type `WasmFlagId`. The config type accepts `WasmFlagId`, so no cast needed in test.

Let me finalize the loader code:

```typescript
import { readFile } from 'node:fs/promises';

import { FlagResolver } from '../flags/resolver';
import type { FlagId } from '../flags/registry';

export type WasmFlagId = 'wasm-tokenizer' | 'wasm-diff' | 'wasm-glob';

export interface WasmExports {
  readonly memory: WebAssembly.Memory;
  alloc(len: number): number;
  dealloc(ptr: number, len: number): void;
}

export interface WasmModuleConfig<T> {
  readonly wasmPath: string;
  readonly fallback: T;
  readonly flagId: WasmFlagId;
  readonly factory: (exports: WasmExports) => T;
}

export interface LoadContext {
  readonly [env: string]: string | undefined;
}

export async function loadWasmModule<T>(
  config: WasmModuleConfig<T>,
  wasmBytes?: Uint8Array,
  context: LoadContext = process.env,
): Promise<T> {
  const resolver = new FlagResolver(context);
  if (!resolver.enabled(config.flagId as FlagId)) {
    return config.fallback;
  }

  try {
    const bytes = wasmBytes ?? new Uint8Array(await readFile(config.wasmPath));
    const { instance } = await WebAssembly.instantiate(bytes, {});
    const wasmFn = config.factory(instance.exports as unknown as WasmExports);
    return wrapWithFallback(wasmFn, config.fallback, config.flagId);
  } catch {
    return config.fallback;
  }
}

export function wrapWithFallback<T extends (...args: any[]) => any>(
  wasmFn: T,
  fallback: T,
  _flagId: WasmFlagId,
): T {
  return ((...args: Parameters<T>): ReturnType<T> => {
    try {
      return wasmFn(...args);
    } catch {
      return fallback(...args);
    }
  }) as T;
}
```

- [ ] 运行测试并确认**通过**：

```bash
pnpm vitest run packages/agent-core/test/utils/wasm-loader.test.ts
```

Expected: 6 tests passed.

- [ ] Commit: `feat(agent-core): generic wasm dual-track loader with fallback`

---

### Task 6: 共享 raw-ABI 字符串调用封装

**Depends on:** Task 5

**Files:**
- Create: `packages/agent-core/src/utils/wasm-string.ts:1-80`
- Test: `packages/agent-core/test/utils/wasm-string.test.ts`

**Goal:** 把所有 Wasm 字符串调用抽象为 `writeString` / `readCString` / `callWasmStringFunction`；Part 3/4/5 的模块代码不再直接操作 TextEncoder/TextDecoder。

**Step-by-step:**

- [ ] 先写失败的测试。创建 `packages/agent-core/test/utils/wasm-string.test.ts`：

```typescript
import { describe, expect, it } from 'vitest';

import {
  callWasmStringFunction,
  readCString,
  writeString,
  type WasmExports,
} from '../../src/utils/wasm-string';

function makeMockExports(): WasmExports {
  const memory = new WebAssembly.Memory({ initial: 1 });
  const buffer = new Uint8Array(memory.buffer);
  let nextPtr = 8;

  return {
    memory,
    alloc(len: number): number {
      if (len === 0) return 0;
      const ptr = nextPtr;
      nextPtr += len + 1;
      return ptr;
    },
    dealloc(_ptr: number, _len: number): void {
      // no-op in mock
    },
    concat(a: number, aLen: number, b: number, bLen: number): number {
      const textA = new TextDecoder().decode(buffer.subarray(a, a + aLen));
      const textB = new TextDecoder().decode(buffer.subarray(b, b + bLen));
      const out = `${textA}|${textB}`;
      const bytes = new TextEncoder().encode(out);
      const ptr = nextPtr;
      nextPtr += bytes.length + 1;
      buffer.set(bytes, ptr);
      buffer[ptr + bytes.length] = 0;
      return ptr;
    },
  };
}

describe('writeString + readCString', () => {
  it('round-trips empty string as ptr 0', () => {
    const exports = makeMockExports();
    const { ptr, len } = writeString(exports, '');
    expect(ptr).toBe(0);
    expect(len).toBe(0);
  });

  it('round-trips non-empty string', () => {
    const exports = makeMockExports();
    const { ptr, len } = writeString(exports, 'hello 世界');
    expect(ptr).not.toBe(0);
    expect(len).toBe(new TextEncoder().encode('hello 世界').length);
    expect(readCString(exports, ptr)).toBe('hello 世界');
  });

  it('reads null pointer as empty string', () => {
    const exports = makeMockExports();
    expect(readCString(exports, 0)).toBe('');
  });
});

describe('callWasmStringFunction', () => {
  it('passes multiple inputs and reads NUL-terminated output', () => {
    const exports = makeMockExports();
    const result = callWasmStringFunction(exports, 'concat', 'hello', 'world');
    expect(result).toBe('hello|world');
  });

  it('returns empty string when function returns null', () => {
    const exports = makeMockExports();
    (exports as any).nullFn = () => 0;
    expect(callWasmStringFunction(exports, 'nullFn', 'x')).toBe('');
  });
});
```

- [ ] 运行测试并确认**失败**：

```bash
pnpm vitest run packages/agent-core/test/utils/wasm-string.test.ts
```

Expected failure: `Error: Cannot find module '../../src/utils/wasm-string'`。

- [ ] 实现 `packages/agent-core/src/utils/wasm-string.ts`：

```typescript
/**
 * Raw-ABI string helpers shared by all Wasm modules.
 *
 * Convention (mirrors rust-ody/src/abi.rs):
 *   - writeString uses exports.alloc(len); empty strings use ptr 0.
 *   - Rust functions returning strings use alloc_cstring, which allocates
 *     len+1 bytes and writes a NUL terminator.
 *   - readCString reads until NUL; callWasmStringFunction then calls
 *     exports.dealloc(outPtr, decodedLen + 1).
 */
import type { WasmExports } from './wasm-loader';

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export interface StringAllocation {
  readonly ptr: number;
  readonly len: number;
}

export function writeString(exports: WasmExports, text: string): StringAllocation {
  const bytes = encoder.encode(text);
  const len = bytes.length;
  if (len === 0) {
    return { ptr: 0, len: 0 };
  }
  const ptr = exports.alloc(len);
  if (ptr === 0) {
    throw new Error('wasm alloc failed');
  }
  new Uint8Array(exports.memory.buffer, ptr, len).set(bytes);
  return { ptr, len };
}

export function readCString(exports: WasmExports, ptr: number): string {
  if (ptr === 0) {
    return '';
  }
  const view = new Uint8Array(exports.memory.buffer);
  let end = ptr;
  while (view[end] !== 0) {
    end += 1;
  }
  const bytes = view.subarray(ptr, end);
  return decoder.decode(bytes);
}

export function callWasmStringFunction(
  exports: WasmExports,
  fnName: string,
  ...inputStrings: string[]
): string {
  const allocations: StringAllocation[] = [];
  try {
    for (const str of inputStrings) {
      allocations.push(writeString(exports, str));
    }
    const args = allocations.flatMap(({ ptr, len }) => [ptr, len]);
    const wasmFn = (exports as Record<string, unknown>)[fnName] as (...args: number[]) => number;
    const outPtr = wasmFn(...args);
    const result = readCString(exports, outPtr);
    if (outPtr !== 0) {
      exports.dealloc(outPtr, result.length + 1);
    }
    return result;
  } finally {
    for (const { ptr, len } of allocations) {
      if (ptr !== 0) {
        exports.dealloc(ptr, len);
      }
    }
  }
}
```

- [ ] 运行测试并确认**通过**：

```bash
pnpm vitest run packages/agent-core/test/utils/wasm-string.test.ts
```

Expected: 5 tests passed.

- [ ] 运行全树类型检查，确保新模块与 flags/registry 变更无冲突：

```bash
pnpm -r typecheck
```

Expected: 成功退出。若失败，修复类型错误后再提交。

- [ ] Commit: `feat(agent-core): shared wasm string-call helper`

---

## Local Self-Review

- [ ] 1. Spec-coverage: Part 2 覆盖 "统一双轨加载框架"、"flag 禁用"、"Wasm 失败静默降级"、"raw-ABI 字符串调用约定"。
- [ ] 2. Placeholder scan: `wasm-loader.ts` 与 `wasm-string.ts` 无 TODO/TBD；测试文件包含完整断言。
- [ ] 3. No phantom tasks: Task 4 修改 registry 并验证；Task 5/6 创建模块与测试并运行通过。
- [ ] 4. Dependency soundness: Task 5 依赖 Task 4 注册的 flag id；Task 6 依赖 Task 5 的 `WasmExports` 类型；无向后依赖。
- [ ] 5. Caller & build soundness: Task 4 扩展 `FlagId` 联合类型，未破坏现有调用方；Task 6 以 `pnpm -r typecheck` 全树检查收尾。
- [ ] 6. Test-the-risk: 测试覆盖 flag 关闭、文件缺失、运行时 panic 三种降级路径；字符串封装测试覆盖空串、多字节 UTF-8、NUL 终止输出。
- [ ] 7. Type consistency: `WasmFlagId`、`WasmExports`、`StringAllocation` 命名与签名在 Part 3/4/5 中复用；`FlagResolver` 复用现有实现而非重写。
