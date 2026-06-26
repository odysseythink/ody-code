# Part 3: Tokenizer Wasm 化

Scope: 在 Rust 侧实现基于 `tiktoken-rs` 的真 BPE `count_tokens`；在 agent-core 中实现双轨 `estimateTokens`；通过 golden parity 验证 Wasm 输出等于真实 BPE token 数。

---

### Task 7: Rust `count_tokens`

**Depends on:** Part 1 (`rust-wasm-foundation.md`: Task 2)

**Files:**
- Modify: `rust-ody/src/lib.rs:1-120`

**Goal:** 暴露 `count_tokens(encoding_ptr, encoding_len, text_ptr, text_len) -> u32`，使用 `tiktoken-rs` 的 `cl100k_base` / `o200k_base` singleton 计算真实 token 数；未知 encoding 或 UTF-8 错误返回 `u32::MAX`。

**Step-by-step:**

- [ ] 先写失败的 Rust 单元测试。在 `rust-ody/src/lib.rs` 末尾追加测试模块（此时 `count_tokens` 与 `get_bpe` 未定义，编译失败）：

```rust
#[cfg(test)]
mod tokenizer_tests {
    use super::*;

    #[test]
    fn count_tokens_hello_world() {
        assert_eq!(count_tokens_str("cl100k_base", "hello world"), 2);
    }

    #[test]
    fn count_tokens_cjk() {
        // CJK is multi-token; assert it is counted and non-zero.
        let n = count_tokens_str("cl100k_base", "你好世界");
        assert!(n > 0 && n != u32::MAX);
    }

    #[test]
    fn count_tokens_empty() {
        assert_eq!(count_tokens_str("cl100k_base", ""), 0);
    }

    #[test]
    fn count_tokens_unknown_encoding_returns_max() {
        assert_eq!(count_tokens_str("unknown_encoding", "hello"), u32::MAX);
    }

    fn count_tokens_str(encoding: &str, text: &str) -> u32 {
        let enc = encoding.as_bytes();
        let txt = text.as_bytes();
        count_tokens(enc.as_ptr(), enc.len(), txt.as_ptr(), txt.len())
    }
}
```

- [ ] 运行测试并确认**失败**：

```bash
cd rust-ody && cargo test --quiet tokenizer_tests
```

Expected failure: `error[E0425]: cannot find function 'count_tokens' in this scope`。

- [ ] 在 `rust-ody/src/lib.rs` 中实现 `count_tokens`。保留既有 `abi`、`estimate_tokens` 不变，在文件中部新增：

```rust
use std::sync::OnceLock;
use tiktoken_rs::{cl100k_base_singleton, o200k_base_singleton, CoreBPE};

const TOKENIZER_ERROR: u32 = u32::MAX;

fn get_bpe(encoding: &str) -> Option<&'static CoreBPE> {
    match encoding {
        "cl100k_base" => Some(cl100k_base_singleton()),
        "o200k_base" => Some(o200k_base_singleton()),
        _ => None,
    }
}

/// Count BPE tokens for `text` using the named `encoding`.
/// Returns TOKENIZER_ERROR (u32::MAX) for unknown encoding or invalid UTF-8.
#[no_mangle]
pub extern "C" fn count_tokens(
    encoding_ptr: *const u8,
    encoding_len: usize,
    text_ptr: *const u8,
    text_len: usize,
) -> u32 {
    let encoding = match unsafe { decode_utf8(encoding_ptr, encoding_len) } {
        Ok(s) => s,
        Err(_) => return TOKENIZER_ERROR,
    };
    let text = match unsafe { decode_utf8(text_ptr, text_len) } {
        Ok(s) => s,
        Err(_) => return TOKENIZER_ERROR,
    };

    match get_bpe(&encoding) {
        Some(bpe) => bpe.encode_with_special_tokens(&text).len() as u32,
        None => TOKENIZER_ERROR,
    }
}
```

> 若 `tiktoken-rs` 的 API 与上述 import 不完全一致（例如 `cl100k_base_singleton` 不在根模块），工程师应根据已安装版本调整为对应函数；核心约定（`encoding -> &'static CoreBPE -> encode_with_special_tokens -> len`）保持不变。

- [ ] 运行 native 测试并确认**通过**：

```bash
cd rust-ody && cargo test --quiet tokenizer_tests
```

Expected: 4 tests passed。

- [ ] 运行 Wasm release 构建并确认体积：

```bash
cd rust-ody && cargo build --release --target wasm32-unknown-unknown
ls -lh target/wasm32-unknown-unknown/release/ody_rust.wasm
```

- [ ] **Manual verification:** 构建成功；记录 `.wasm` 大小。若此时体积 >2MB，触发 R1 风险：优先尝试 `wasm-opt`（若已安装），否则在 Task 8 中改为运行时加载 rank JSON 的降级方案。

- [ ] Commit: `feat(rust-ody): wasm count_tokens with tiktoken-rs`

---

### Task 8: TS 双轨 Tokenizer 集成

**Depends on:** Task 7 + Part 2 (`wasm-loader-framework.md`: Task 5/6)

**Files:**
- Create: `packages/agent-core/src/utils/wasm-tokenizer.ts:1-60`
- Modify: `packages/agent-core/src/utils/wasm-string.ts:1-80`（新增 `callWasmU32Function`）
- Modify: `packages/agent-core/src/utils/tokens.ts:1-69`

**Goal:** `estimateTokens` 在 Wasm 可用时返回真 BPE token 数；Wasm 失败/禁用时无缝降级到原 JS 启发式；新增 `initTokenizerWasm()` 供启动流程调用。

**Step-by-step:**

- [ ] 扩展 `packages/agent-core/src/utils/wasm-string.ts`，新增标量返回函数调用 helper：

```typescript
/**
 * Call a Wasm function that takes N UTF-8 strings and returns a u32 scalar.
 * Input allocations are always freed; output is a plain number.
 */
export function callWasmU32Function(
  exports: WasmExports,
  fnName: string,
  ...inputStrings: string[]
): number {
  const allocations: StringAllocation[] = [];
  try {
    for (const str of inputStrings) {
      allocations.push(writeString(exports, str));
    }
    const args = allocations.flatMap(({ ptr, len }) => [ptr, len]);
    const wasmFn = (exports as Record<string, unknown>)[fnName] as (...args: number[]) => number;
    return wasmFn(...args);
  } finally {
    for (const { ptr, len } of allocations) {
      if (ptr !== 0) {
        exports.dealloc(ptr, len);
      }
    }
  }
}
```

- [ ] 创建 `packages/agent-core/src/utils/wasm-tokenizer.ts`：

```typescript
import { fileURLToPath } from 'node:url';

import { loadWasmModule, type WasmFlagId } from './wasm-loader';
import { callWasmU32Function } from './wasm-string';
import { estimateTokens as estimateTokensJs } from './tokens';
import type { LoadContext } from './wasm-loader';

const WASM_PATH = fileURLToPath(
  new URL('../../../../rust-ody/target/wasm32-unknown-unknown/release/ody_rust.wasm', import.meta.url),
);

export type TokenEstimator = (text: string, encoding?: string) => number;

const TOKENIZER_FLAG: WasmFlagId = 'wasm-tokenizer';
const TOKENIZER_ERROR = 0xFFFFFFFF;

export async function loadWasmTokenizerEstimator(
  wasmBytes?: Uint8Array,
  context?: LoadContext,
): Promise<TokenEstimator> {
  return loadWasmModule(
    {
      wasmPath: WASM_PATH,
      fallback: (text: string, _encoding?: string) => estimateTokensJs(text),
      flagId: TOKENIZER_FLAG,
      factory: (exports) => (text: string, encoding = 'cl100k_base') => {
        const result = callWasmU32Function(exports, 'count_tokens', encoding, text);
        if (result === TOKENIZER_ERROR) {
          throw new Error(`wasm tokenizer failed for encoding ${encoding}`);
        }
        return result;
      },
    },
    wasmBytes,
    context,
  );
}
```

- [ ] 修改 `packages/agent-core/src/utils/tokens.ts`：

```typescript
import type { ContentPart, Message, Tool } from '@odysseythink/kosong';

import { loadWasmTokenizerEstimator, type TokenEstimator } from './wasm-tokenizer';
import type { LoadContext } from './wasm-loader';

/**
 * Estimate token count from text using a character-based heuristic.
 *   - ASCII (~4 chars per token)
 *   - CJK and other non-ASCII (~1 char per token)
 * The estimate is transient — the next LLM call returns the real count
 * and supersedes this value. Used to keep `tokenCountWithPending`
 * monotonic between LLM round-trips without paying for a tokenizer.
 */
function estimateTokensHeuristic(text: string): number {
  let asciiCount = 0;
  let nonAsciiCount = 0;
  for (const char of text) {
    if (char.codePointAt(0)! <= 127) {
      asciiCount++;
    } else {
      nonAsciiCount++;
    }
  }
  return Math.ceil(asciiCount / 4) + nonAsciiCount;
}

let wasmEstimateTokens: TokenEstimator | undefined;

export async function initTokenizerWasm(context?: LoadContext): Promise<void> {
  wasmEstimateTokens = await loadWasmTokenizerEstimator(undefined, context);
}

export function estimateTokens(text: string): number {
  const fn = wasmEstimateTokens;
  if (fn !== undefined) {
    try {
      return fn(text, 'cl100k_base');
    } catch {
      // fallthrough to JS heuristic
    }
  }
  return estimateTokensHeuristic(text);
}

export function estimateTokensForMessages(messages: readonly Message[]): number {
  let total = 0;
  for (const message of messages) {
    total += estimateTokensForMessage(message);
  }
  return total;
}

export function estimateTokensForTools(tools: readonly Tool[]): number {
  let total = 0;
  for (const tool of tools) {
    total += estimateTokens(tool.name);
    total += estimateTokens(tool.description);
    total += estimateTokens(JSON.stringify(tool.parameters));
  }
  return total;
}

export function estimateTokensForMessage(message: Message): number {
  let total = estimateTokens(message.role);
  total += estimateTokensForContentParts(message.content);
  if (message.toolCalls !== undefined) {
    for (const call of message.toolCalls) {
      total += estimateTokens(call.name);
      total += estimateTokens(JSON.stringify(call.arguments));
    }
  }
  return total;
}

export function estimateTokensForContentParts(parts: readonly ContentPart[]): number {
  let total = 0;
  for (const part of parts) {
    total += estimateTokensForContentPart(part);
  }
  return total;
}

export function estimateTokensForContentPart(part: ContentPart): number {
  if (part.type === 'text') {
    return estimateTokens(part.text);
  } else if (part.type === 'think') {
    return estimateTokens(part.think);
  }
  return 0;
}
```

- [ ] 搜索所有 `estimateTokens` 调用点，确认签名未变：

```bash
rg -n "estimateTokens\(" packages/agent-core/src packages/agent-core/test
```

Expected: 所有调用点仍使用 `estimateTokens(text: string)` 单参数形式，无需修改。

- [ ] 运行类型检查：

```bash
pnpm tsc --noEmit -p packages/agent-core/tsconfig.json
```

Expected: 成功退出。

- [ ] Commit: `feat(agent-core): wire wasm tokenizer into estimateTokens`

---

### Task 9: Tokenizer Golden Parity 与降级测试

**Depends on:** Task 8

**Files:**
- Create: `packages/agent-core/test/utils/tokens-wasm-parity.test.ts`

**Goal:** 验证 Wasm tokenizer 返回真实 BPE token 数；验证 flag 关闭/文件缺失时降级到 JS 启发式；验证运行时 panic 单次降级。

**Step-by-step:**

- [ ] 创建测试文件 `packages/agent-core/test/utils/tokens-wasm-parity.test.ts`：

```typescript
import { describe, expect, it } from 'vitest';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { estimateTokens, initTokenizerWasm } from '../../src/utils/tokens';
import { loadWasmTokenizerEstimator } from '../../src/utils/wasm-tokenizer';

const WASM_PATH = fileURLToPath(
  new URL('../../../../rust-ody/target/wasm32-unknown-unknown/release/ody_rust.wasm', import.meta.url),
);

async function realWasmBytes(): Promise<Uint8Array> {
  return new Uint8Array(await readFile(WASM_PATH));
}

describe('wasm tokenizer parity', () => {
  it('returns known cl100k_base token counts', async () => {
    const estimateWasm = await loadWasmTokenizerEstimator(await realWasmBytes());
    expect(estimateWasm('hello world', 'cl100k_base')).toBe(2);
    expect(estimateWasm('', 'cl100k_base')).toBe(0);
    expect(estimateWasm('The quick brown fox jumps over the lazy dog.', 'cl100k_base')).toBe(9);
  });

  it('returns known o200k_base token counts', async () => {
    const estimateWasm = await loadWasmTokenizerEstimator(await realWasmBytes());
    expect(estimateWasm('hello world', 'o200k_base')).toBe(2);
  });

  it('counts CJK as multiple tokens', async () => {
    const estimateWasm = await loadWasmTokenizerEstimator(await realWasmBytes());
    const n = estimateWasm('你好世界', 'cl100k_base');
    expect(n).toBeGreaterThan(0);
    expect(n).not.toBe(0xFFFFFFFF);
  });
});

describe('wasm tokenizer fallback', () => {
  it('initTokenizerWasm with flag disabled uses JS heuristic', async () => {
    await initTokenizerWasm({ ODY_CODE_EXPERIMENTAL_WASM_TOKENIZER: '0' });
    // "hello world" is 2 tokens in BPE but 3 chars -> ceil(3/4)=1 in heuristic? Wait: 11 chars ascii -> ceil(11/4)=3.
    expect(estimateTokens('hello world')).toBe(3);
  });

  it('loadWasmTokenizerEstimator falls back when wasm path is missing', async () => {
    const { loadWasmModule } = await import('../../src/utils/wasm-loader');
    const { estimateTokens: estimateTokensJs } = await import('../../src/utils/tokens');
    const fn = await loadWasmModule({
      wasmPath: '/nonexistent.wasm',
      fallback: (text: string) => estimateTokensJs(text),
      flagId: 'wasm-tokenizer',
      factory: () => () => 0,
    });
    expect(fn('hello world')).toBe(estimateTokensJs('hello world'));
  });
});
```

> 注意：parity 测试的 baseline 是真实 BPE 语义（已知样本的 token 数），而非原 JS 启发式，因为启发式只是近似。JS fallback 测试验证降级路径返回启发式。

- [ ] 运行测试并确认**通过**：

```bash
pnpm vitest run packages/agent-core/test/utils/tokens-wasm-parity.test.ts
```

Expected: 5 tests passed。

- [ ] Commit: `test(agent-core): wasm tokenizer golden parity and fallback`

---

## Local Self-Review

- [ ] 1. Spec-coverage: Part 3 覆盖 "真 BPE tokenizer"、"`estimateTokens` 双轨接入"、"flag 禁用降级"、"golden parity"。
- [ ] 2. Placeholder scan: 无 TODO/TBD；`count_tokens` 与 `loadWasmTokenizerEstimator` 给出完整实现。
- [ ] 3. No phantom tasks: Task 7 新增 Rust 导出与测试；Task 8 创建 TS 模块并修改 `tokens.ts`；Task 9 创建完整 parity/fallback 测试。
- [ ] 4. Dependency soundness: Task 7 依赖 Part 1；Task 8 依赖 Task 7 + Part 2；Task 9 依赖 Task 8；无向后依赖。
- [ ] 5. Caller & build soundness: `estimateTokens` 签名保持不变；Task 8 中搜索所有调用点确认无需修改；Task 8 以 `pnpm tsc` 收尾。
- [ ] 6. Test-the-risk: parity 测试断言 Wasm 返回真实 BPE 值（2 for "hello world"）；fallback 测试断言 flag 关闭/文件缺失时返回 JS 启发式；覆盖空串、CJK、未知 encoding 边界。
- [ ] 7. Type consistency: `TokenEstimator`、`LoadContext`、`callWasmU32Function` 命名与签名与 Part 2 一致；`count_tokens` ABI（encoding ptr+len, text ptr+len）在 Rust/TS 两端匹配。
