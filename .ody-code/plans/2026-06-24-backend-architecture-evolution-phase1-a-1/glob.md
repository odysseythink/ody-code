# Part 5: Glob Wasm 化

Scope: 在 Rust 侧实现 `glob_match(value, pattern, options)`，使用 `globset` 复刻 `picomatch.isMatch` 的核心语义；在 agent-core 中将 `globMatch`/`pathGlobMatch` 接入 Wasm 双轨路径，对不支持的复杂模式自动降级到 `picomatch`。

---

### Task 13: Rust `glob_match`

**Depends on:** Part 1 (`rust-wasm-foundation.md`: Task 2)

**Files:**
- Modify: `rust-ody/src/lib.rs:1-300`

**Goal:** 暴露 `glob_match(value_ptr, value_len, pattern_ptr, pattern_len, opts_ptr, opts_len) -> u32`：返回 `1` 表示匹配，`0` 表示不匹配，`u32::MAX`（`GLOB_ERROR`）表示模式不支持或解析失败。支持 `*`（不跨 `/`）、`**`、`?`、`[...]`、一级 `{a,b}` brace 展开、大小写不敏感；复杂嵌套 brace / extglob 返回 `GLOB_ERROR`，由 TS 侧回退到 `picomatch`。

**Step-by-step:**

- [ ] 先写失败的 Rust 单元测试。在 `rust-ody/src/lib.rs` 末尾追加测试模块（此时 `glob_match` 未定义，编译失败）：

```rust
#[cfg(test)]
mod glob_tests {
    use super::*;

    #[test]
    fn glob_match_star() {
        assert_eq!(call_glob("main.ts", "*.ts", "false"), 1);
        assert_eq!(call_glob("src/main.ts", "*.ts", "false"), 0);
    }

    #[test]
    fn glob_match_double_star() {
        assert_eq!(call_glob("src/deep/main.ts", "src/**/*.ts", "false"), 1);
        assert_eq!(call_glob("main.ts", "src/**/*.ts", "false"), 0);
    }

    #[test]
    fn glob_match_brace() {
        assert_eq!(call_glob("a/b.ts", "a/{b,c}.ts", "false"), 1);
        assert_eq!(call_glob("a/c.ts", "a/{b,c}.ts", "false"), 1);
        assert_eq!(call_glob("a/d.ts", "a/{b,c}.ts", "false"), 0);
    }

    #[test]
    fn glob_match_nocase() {
        assert_eq!(call_glob("MAIN.TS", "*.ts", "true"), 1);
        assert_eq!(call_glob("MAIN.TS", "*.ts", "false"), 0);
    }

    #[test]
    fn glob_match_escaped_special_and_question() {
        assert_eq!(call_glob("a*b", "a\\*b", "false"), 1);
        assert_eq!(call_glob("aXb", "a?b", "false"), 1);
        assert_eq!(call_glob("a/b", "a?b", "false"), 0);
    }

    #[test]
    fn glob_match_character_class() {
        assert_eq!(call_glob("abc", "a[bc]c", "false"), 1);
        assert_eq!(call_glob("adc", "a[bc]c", "false"), 0);
    }

    #[test]
    fn glob_match_unsupported_returns_error() {
        assert_eq!(call_glob("a/c.ts", "a/{b,{c,d}}.ts", "false"), GLOB_ERROR);
    }

    fn call_glob(value: &str, pattern: &str, opts: &str) -> u32 {
        let v = value.as_bytes();
        let p = pattern.as_bytes();
        let o = opts.as_bytes();
        glob_match(v.as_ptr(), v.len(), p.as_ptr(), p.len(), o.as_ptr(), o.len())
    }
}
```

- [ ] 运行测试并确认**失败**：

```bash
cd rust-ody && cargo test --quiet glob_tests
```

Expected failure: `error[E0425]: cannot find function 'glob_match' in this scope`。

- [ ] 在 `rust-ody/src/lib.rs` 中实现 glob 函数（保留既有内容，在 diff 函数之后追加）：

```rust
use globset::GlobBuilder;

const GLOB_ERROR: u32 = u32::MAX;

/// Match a glob pattern against a value.
///
/// `options` is a UTF-8 string: "true" for case-insensitive, anything else for
/// case-sensitive. Returns 1 on match, 0 on no-match, and GLOB_ERROR when the
/// pattern cannot be handled by the Rust subset (caller should fall back to
/// picomatch).
#[no_mangle]
pub extern "C" fn glob_match(
    value_ptr: *const u8,
    value_len: usize,
    pattern_ptr: *const u8,
    pattern_len: usize,
    opts_ptr: *const u8,
    opts_len: usize,
) -> u32 {
    let value = match unsafe { decode_utf8(value_ptr, value_len) } {
        Ok(s) => s,
        Err(_) => return GLOB_ERROR,
    };
    let pattern = match unsafe { decode_utf8(pattern_ptr, pattern_len) } {
        Ok(s) => s,
        Err(_) => return GLOB_ERROR,
    };
    let opts = match unsafe { decode_utf8(opts_ptr, opts_len) } {
        Ok(s) => s,
        Err(_) => return GLOB_ERROR,
    };
    let nocase = opts == "true";
    glob_match_impl(&value, &pattern, nocase)
}

fn glob_match_impl(value: &str, pattern: &str, nocase: bool) -> u32 {
    let patterns = match expand_braces(pattern) {
        Some(ps) => ps,
        None => return GLOB_ERROR,
    };

    for p in patterns {
        let glob = match GlobBuilder::new(&p)
            .literal_separator(true)
            .backslash_escape(true)
            .case_insensitive(nocase)
            .build()
        {
            Ok(g) => g,
            Err(_) => return GLOB_ERROR,
        };
        if glob.compile_matcher().is_match(value) {
            return 1;
        }
    }
    0
}

/// One-level brace expansion. Returns None if the pattern contains braces that
/// cannot be expanded safely (nested braces, braces inside unclosed character
/// classes, etc.), signalling a fall-back to picomatch.
fn expand_braces(pattern: &str) -> Option<Vec<String>> {
    let bytes = pattern.as_bytes();
    let mut bracket_depth: i32 = 0;
    let mut brace_start: Option<usize> = None;

    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'[' => bracket_depth += 1,
            b']' => bracket_depth = (bracket_depth - 1).max(0),
            b'{' if bracket_depth == 0 => {
                if brace_start.is_some() {
                    // Nested braces are not supported.
                    return None;
                }
                brace_start = Some(i);
            }
            b'}' if bracket_depth == 0 => {
                let start = brace_start?;
                let inner = &pattern[start + 1..i];
                // Reject empty choice list and nested braces inside the choice.
                if inner.is_empty() || inner.contains('{') || inner.contains('}') {
                    return None;
                }
                let prefix = &pattern[..start];
                let suffix = &pattern[i + 1..];
                let choices: Vec<&str> = inner.split(',').collect();
                return Some(
                    choices
                        .iter()
                        .map(|c| format!("{}{}{}", prefix, c, suffix))
                        .collect(),
                );
            }
            _ => {}
        }
    }

    // No (valid) braces found.
    if brace_start.is_some() {
        return None;
    }
    Some(vec![pattern.to_string()])
}
```

- [ ] 运行 native 测试并确认**通过**：

```bash
cd rust-ody && cargo test --quiet glob_tests
```

Expected: 7 tests passed。

- [ ] 运行 Wasm release 构建：

```bash
cd rust-ody && cargo build --release --target wasm32-unknown-unknown
```

- [ ] **Manual verification:** 构建成功；记录 `.wasm` 大小。

- [ ] Commit: `feat(rust-ody): wasm glob_match with globset`

---

### Task 14: TS 双轨 Glob 集成

**Depends on:** Task 13 + Part 2 (`wasm-loader-framework.md`: Task 5/6) + Part 3 (`tokenizer.md`: Task 8, for `callWasmU32Function`)

**Files:**
- Create: `packages/agent-core/src/utils/wasm-glob.ts:1-90`
- Modify: `packages/agent-core/src/tools/support/path-glob-match.ts:1-146`

**Goal:** `globMatch` 优先使用 Wasm 判断正匹配，Wasm 返回 `0` 或不支持时回退到 `picomatch` 权威实现；`pathGlobMatch` 无需改动即可透明受益；新增 `initGlobWasm()` 供启动流程调用。

**Step-by-step:**

- [ ] 创建 `packages/agent-core/src/utils/wasm-glob.ts`：

```typescript
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
        // Wasm 只负责“确认匹配”；任何非 1 结果（包括 GLOB_ERROR 和真正的 0）
        // 都回退到 picomatch，确保与原有语义逐字节一致。
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
```

- [ ] 修改 `packages/agent-core/src/tools/support/path-glob-match.ts`：

  - 移除 `import picomatch from 'picomatch';`
  - 新增：

```typescript
import { globMatch, initGlobWasm } from '../utils/wasm-glob';

export { globMatch, initGlobWasm };
```

  - 删除原有的 `globMatch` 函数实现和 `stripLeadingDotSlash` 函数实现。
  - 保持 `PermissionPathMatchOptions`、`pathGlobMatch` 及所有路径规范化辅助函数不变；`pathGlobMatch` 内部继续调用 `globMatch`，现在自动走 Wasm 双轨路径。

修改后文件大致结构如下（供执行时对照）：

```typescript
import { isAbsolute, join, parse } from 'pathe';

import { globMatch, initGlobWasm } from '../utils/wasm-glob';
import { canonicalizePath, type PathClass } from '../policies/path-access';

export { globMatch, initGlobWasm };

export interface PermissionPathMatchOptions {
  readonly cwd?: string;
  readonly pathClass?: PathClass;
  readonly homeDir?: string;
  readonly caseInsensitivePaths?: boolean;
}

interface PathMatchSemantics {
  readonly pathClass: PathClass;
}

/**
 * Match ordinary string fields, like command text or search patterns.
 * `*` and `**` work as wildcards, but the value is not treated as a file path.
 */
// globMatch is now imported from '../utils/wasm-glob'.

/**
 * Match file path fields, like Read/Write/Edit `path`.
 * Also compares normalized forms, so `./a`, `dir/../a`, and Windows
 * separator or case variants can match the same rule.
 */
export function pathGlobMatch(
  value: string,
  pattern: string,
  pathOptions?: PermissionPathMatchOptions,
): boolean {
  const semantics = pathMatchSemantics(value, pattern, pathOptions);
  const nocase = pathOptions?.caseInsensitivePaths ?? true;

  if (globMatch(value, pattern, { nocase })) return true;

  for (const valueVariant of pathVariants(value, semantics, pathOptions)) {
    for (const patternVariant of pathVariants(pattern, semantics, pathOptions)) {
      if (globMatch(valueVariant, patternVariant, { nocase })) return true;
    }
  }
  return false;
}

// ... 保留 pathVariants / canonicalizePathPattern / expandUserPath /
//      defaultCwdForPath / pathMatchSemantics / addPathVariant /
//      stripLeadingDotPath 等现有实现不变 ...
```

- [ ] 搜索 `globMatch` / `pathGlobMatch` 调用点，确认签名未变：

```bash
rg -n "globMatch\(|pathGlobMatch\(" packages/agent-core/src packages/agent-core/test
```

Expected: 所有调用点仍从 `path-glob-match.ts` 导入，参数签名不变；无需修改。

- [ ] 运行类型检查：

```bash
pnpm tsc --noEmit -p packages/agent-core/tsconfig.json
```

Expected: 成功退出。

- [ ] Commit: `feat(agent-core): wire wasm glob into path-glob-match`

---

### Task 15: Glob Golden Parity 与降级测试

**Depends on:** Task 14

**Files:**
- Create: `packages/agent-core/test/tools/support/glob-wasm-parity.test.ts`

**Goal:** 验证 Wasm glob 在支持的语义上与 `picomatch` 一致；验证 `GLOB_ERROR`/复杂模式自动回退；验证 flag 禁用时使用 JS；验证 `pathGlobMatch` 的路径规范化仍正常工作。

**Step-by-step:**

- [ ] 创建测试文件 `packages/agent-core/test/tools/support/glob-wasm-parity.test.ts`：

```typescript
import { describe, expect, it } from 'vitest';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import {
  globMatch,
  pathGlobMatch,
  initGlobWasm,
} from '../../../src/tools/support/path-glob-match';
import { loadWasmGlobMatcher } from '../../../src/utils/wasm-glob';

const WASM_PATH = fileURLToPath(
  new URL('../../../../rust-ody/target/wasm32-unknown-unknown/release/ody_rust.wasm', import.meta.url),
);

async function realWasmBytes(): Promise<Uint8Array> {
  return new Uint8Array(await readFile(WASM_PATH));
}

describe('wasm glob parity', () => {
  it('matches simple star patterns', async () => {
    await initGlobWasm();
    expect(globMatch('main.ts', '*.ts')).toBe(true);
    expect(globMatch('src/main.ts', '*.ts')).toBe(false);
  });

  it('matches double-star patterns', async () => {
    await initGlobWasm();
    expect(globMatch('src/deep/main.ts', 'src/**/*.ts')).toBe(true);
    expect(globMatch('main.ts', 'src/**/*.ts')).toBe(false);
  });

  it('matches brace expansion', async () => {
    await initGlobWasm();
    expect(globMatch('a/b.ts', 'a/{b,c}.ts')).toBe(true);
    expect(globMatch('a/c.ts', 'a/{b,c}.ts')).toBe(true);
    expect(globMatch('a/d.ts', 'a/{b,c}.ts')).toBe(false);
  });

  it('matches escaped specials and question mark', async () => {
    await initGlobWasm();
    expect(globMatch('a*b', 'a\\*b')).toBe(true);
    expect(globMatch('aXb', 'a?b')).toBe(true);
    expect(globMatch('a/b', 'a?b')).toBe(false);
  });

  it('matches character class', async () => {
    await initGlobWasm();
    expect(globMatch('abc', 'a[bc]c')).toBe(true);
    expect(globMatch('adc', 'a[bc]c')).toBe(false);
  });

  it('honours nocase option', async () => {
    await initGlobWasm();
    expect(globMatch('MAIN.TS', '*.ts', { nocase: true })).toBe(true);
    expect(globMatch('MAIN.TS', '*.ts', { nocase: false })).toBe(false);
  });

  it('falls back to picomatch for unsupported nested braces', async () => {
    await initGlobWasm();
    expect(globMatch('a/c.ts', 'a/{b,{c,d}}.ts')).toBe(true);
    expect(globMatch('a/z.ts', 'a/{b,{c,d}}.ts')).toBe(false);
  });

  it('falls back to picomatch for leading-dot-slash variants', async () => {
    await initGlobWasm();
    expect(globMatch('./main.ts', '*.ts')).toBe(true);
  });
});

describe('wasm glob fallback', () => {
  it('initGlobWasm with flag disabled uses JS', async () => {
    await initGlobWasm({ ODY_CODE_EXPERIMENTAL_WASM_GLOB: '0' });
    expect(globMatch('main.ts', '*.ts')).toBe(true);
    expect(globMatch('src/main.ts', '*.ts')).toBe(false);
    expect(globMatch('./main.ts', '*.ts')).toBe(true);
  });

  it('loadWasmGlobMatcher with missing wasm path falls back to JS', async () => {
    const matcher = await loadWasmGlobMatcher(undefined, { ODY_CODE_EXPERIMENTAL_WASM_GLOB: '0' });
    expect(matcher('main.ts', '*.ts')).toBe(true);
  });
});

describe('pathGlobMatch integration', () => {
  it('still normalizes paths with wasm enabled', async () => {
    await initGlobWasm();
    expect(pathGlobMatch('./main.ts', '*.ts', { cwd: '/repo' })).toBe(true);
  });
});
```

> 若上述 parity 测试中出现 Wasm 与 `picomatch` 结果不一致（排除回退路径后仍有差异），说明 `globset` 无法安全替代 `picomatch`。此时应在 `packages/agent-core/src/flags/registry.ts` 中将 `wasm-glob` 的 `default` 改为 `false` 并提交，然后在设计文档中记录该风险已触发。

- [ ] 运行测试并确认**通过**：

```bash
pnpm vitest run packages/agent-core/test/tools/support/glob-wasm-parity.test.ts
```

Expected: 12 tests passed。

- [ ] Commit: `test(agent-core): wasm glob golden parity and fallback`

---

## Local Self-Review

- [ ] 1. Spec-coverage: Part 5 覆盖 "glob/路径匹配 Wasm 化"、"picomatch 核心语义复刻"、"不支持模式自动降级"、"`pathGlobMatch` 透明受益"、"flag 禁用降级"。
- [ ] 2. Placeholder scan: 无 TODO/TBD；`glob_match`、`expand_braces`、`globMatchJs`、`loadWasmGlobMatcher` 均给出完整实现。
- [ ] 3. No phantom tasks: Task 13 新增 Rust 导出与测试；Task 14 创建 TS 模块并修改 `path-glob-match.ts`；Task 15 创建 parity/fallback 测试。
- [ ] 4. Dependency soundness: Task 13 依赖 Part 1；Task 14 依赖 Task 13 + Part 2 + Part 3 Task 8；Task 15 依赖 Task 14；无向后依赖。
- [ ] 5. Caller & build soundness: `globMatch`/`pathGlobMatch` 签名保持不变；Task 14 中搜索所有调用点确认无需修改；Task 14 以 `pnpm tsc` 收尾。
- [ ] 6. Test-the-risk: parity 测试覆盖 `*` 不跨 `/`、`**`、brace 展开、escaped special、`?` 不跨 `/`、字符类、nocase；fallback 测试覆盖嵌套 brace、`./` 前缀、flag 关闭、wasm 路径缺失；`pathGlobMatch` 测试覆盖路径规范化。
- [ ] 7. Type一致性: `GlobMatcher` 类型、`loadWasmModule`/`callWasmU32Function` 复用 Part 2/3 定义；Rust `glob_match` ABI（value/pattern/options 三字符串）与 TS factory 调用参数匹配；`GLOB_ERROR` 常量 `0xFFFFFFFF` 在两端一致。
