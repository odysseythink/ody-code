# Part 4: Diff Wasm 化

Scope: 在 Rust 侧实现 `compute_diff`（纯文本 unified diff）与 `format_git_diff`（git diff 最小清洗）；在 agent-core 中接入 `fetchDiff`；通过 golden parity 验证 Wasm 输出。

---

### Task 10: Rust `compute_diff` + `format_git_diff`

**Depends on:** Part 1 (`rust-wasm-foundation.md`: Task 2)

**Files:**
- Modify: `rust-ody/src/lib.rs:1-200`

**Goal:** 暴露两个字符串返回型 Wasm 导出：`compute_diff(old_text, new_text)` 使用 `similar` 生成 unified diff；`format_git_diff(raw_diff)` 解析并最小清洗 git 原始 diff（去空 hunk、去行尾空格、保证 trailing newline）。

**Step-by-step:**

- [ ] 先写失败的 Rust 单元测试。在 `rust-ody/src/lib.rs` 末尾追加测试模块（此时 `compute_diff`/`format_git_diff` 未定义，编译失败）：

```rust
#[cfg(test)]
mod diff_tests {
    use super::*;

    #[test]
    fn compute_diff_basic() {
        let out = call_compute_diff("a\nb", "a\nc\nb");
        assert!(out.contains("@@"));
        assert!(out.contains("+c"));
        assert!(out.contains("--- old"));
        assert!(out.contains("+++ new"));
    }

    #[test]
    fn compute_diff_empty_inputs() {
        assert_eq!(call_compute_diff("", ""), "");
    }

    #[test]
    fn format_git_diff_strips_trailing_whitespace() {
        let raw = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n ";
        let out = call_format_git_diff(raw);
        assert!(!out.ends_with(' '));
        assert!(out.contains("diff --git"));
    }

    #[test]
    fn format_git_diff_drops_empty_hunk() {
        // A hunk with only context lines and no +/- is considered empty.
        let raw = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n context\n context\n";
        let out = call_format_git_diff(raw);
        assert!(!out.contains("@@"));
    }

    fn call_compute_diff(old: &str, new: &str) -> String {
        let old_b = old.as_bytes();
        let new_b = new.as_bytes();
        let ptr = compute_diff(old_b.as_ptr(), old_b.len(), new_b.as_ptr(), new_b.len());
        read_cstring(ptr)
    }

    fn call_format_git_diff(raw: &str) -> String {
        let b = raw.as_bytes();
        let ptr = format_git_diff(b.as_ptr(), b.len());
        read_cstring(ptr)
    }

    fn read_cstring(ptr: *mut u8) -> String {
        if ptr.is_null() {
            return String::new();
        }
        unsafe {
            let view = std::slice::from_raw_parts(ptr, 1024);
            let mut len = 0;
            while view[len] != 0 {
                len += 1;
            }
            let s = String::from_utf8_lossy(&view[..len]).to_string();
            dealloc(ptr, len + 1);
            s
        }
    }
}
```

- [ ] 运行测试并确认**失败**：

```bash
cd rust-ody && cargo test --quiet diff_tests
```

Expected failure: `error[E0425]: cannot find function 'compute_diff' in this scope`。

- [ ] 在 `rust-ody/src/lib.rs` 中实现 diff 函数。保留既有内容，在 `count_tokens` 之后新增：

```rust
use similar::TextDiff;

/// Compute a unified diff between two UTF-8 texts.
/// Returns a NUL-terminated string pointer (caller must dealloc with decoded_len + 1).
#[no_mangle]
pub extern "C" fn compute_diff(
    old_ptr: *const u8,
    old_len: usize,
    new_ptr: *const u8,
    new_len: usize,
) -> *mut u8 {
    let old_text = match unsafe { decode_utf8(old_ptr, old_len) } {
        Ok(s) => s,
        Err(_) => return alloc_cstring(""),
    };
    let new_text = match unsafe { decode_utf8(new_ptr, new_len) } {
        Ok(s) => s,
        Err(_) => return alloc_cstring(""),
    };

    if old_text.is_empty() && new_text.is_empty() {
        return alloc_cstring("");
    }

    let diff = TextDiff::from_lines(&old_text, &new_text);
    let unified = diff
        .unified_diff()
        .context_radius(3)
        .header("old", "new")
        .to_string();
    alloc_cstring(&unified)
}

/// Minimal git-diff cleaner: strip trailing whitespace, drop empty hunks,
/// preserve trailing newline. On parse failure returns the raw input unchanged.
#[no_mangle]
pub extern "C" fn format_git_diff(raw_ptr: *const u8, raw_len: usize) -> *mut u8 {
    let raw = match unsafe { decode_utf8(raw_ptr, raw_len) } {
        Ok(s) => s,
        Err(_) => return alloc_cstring(""),
    };
    let formatted = format_git_diff_impl(&raw);
    alloc_cstring(&formatted)
}

fn format_git_diff_impl(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim_end();
        if line.starts_with("@@ ") {
            let header_idx = i;
            i += 1;
            let mut body: Vec<String> = Vec::new();
            let mut has_change = false;
            while i < lines.len() {
                let l = lines[i].trim_end();
                if l.starts_with("@@ ")
                    || l.starts_with("diff --git")
                    || l.starts_with("--- ")
                    || l.starts_with("+++ ")
                {
                    break;
                }
                if l.starts_with('+') || l.starts_with('-') {
                    has_change = true;
                }
                body.push(l.to_string());
                i += 1;
            }
            if has_change {
                out.push(lines[header_idx].trim_end().to_string());
                out.extend(body);
            }
        } else {
            out.push(line.to_string());
            i += 1;
        }
    }

    let mut result = out.join("\n");
    if raw.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }
    result
}
```

- [ ] 运行 native 测试并确认**通过**：

```bash
cd rust-ody && cargo test --quiet diff_tests
```

Expected: 4 tests passed。

- [ ] 运行 Wasm release 构建：

```bash
cd rust-ody && cargo build --release --target wasm32-unknown-unknown
```

- [ ] **Manual verification:** 构建成功；记录 `.wasm` 大小。

- [ ] Commit: `feat(rust-ody): wasm compute_diff and format_git_diff`

---

### Task 11: TS 双轨 Diff 集成

**Depends on:** Task 10 + Part 2 (`wasm-loader-framework.md`: Task 5/6)

**Files:**
- Create: `packages/agent-core/src/utils/wasm-diff.ts:1-120`
- Modify: `packages/agent-core/src/code-review/diff.ts:1-100`

**Goal:** `fetchDiff` 返回前调用 `formatGitDiff`；新增 `computeTextDiff` 与 `formatGitDiff` 双轨函数；`formatGitDiff` 的 JS fallback 为 identity，`computeTextDiff` 的 JS fallback 为简单行级 diff。

**Step-by-step:**

- [ ] 创建 `packages/agent-core/src/utils/wasm-diff.ts`：

```typescript
import { fileURLToPath } from 'node:url';

import { loadWasmModule, type WasmFlagId } from './wasm-loader';
import { callWasmStringFunction } from './wasm-string';
import type { LoadContext } from './wasm-loader';

const WASM_PATH = fileURLToPath(
  new URL('../../../../rust-ody/target/wasm32-unknown-unknown/release/ody_rust.wasm', import.meta.url),
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
        computeTextDiff: (oldText: string, newText: string) =>
          callWasmStringFunction(exports, 'compute_diff', oldText, newText),
        formatGitDiff: (rawDiff: string) => callWasmStringFunction(exports, 'format_git_diff', rawDiff),
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
    for (let j = 1; j <= n; j += 1) {
      dp[i][j] = a[i - 1] === b[j - 1] ? dp[i - 1][j - 1] + 1 : Math.max(dp[i - 1][j], dp[i][j - 1]);
    }
  }

  const result: Array<[number, number]> = [];
  let i = m;
  let j = n;
  while (i > 0 && j > 0) {
    if (a[i - 1] === b[j - 1]) {
      result.unshift([i - 1, j - 1]);
      i -= 1;
      j -= 1;
    } else if (dp[i - 1][j] >= dp[i][j - 1]) {
      i -= 1;
    } else {
      j -= 1;
    }
  }
  return result;
}
```

- [ ] 修改 `packages/agent-core/src/code-review/diff.ts`：

```typescript
import { spawn } from 'node:child_process';
import type { CodeReviewDiffSource } from './types';
import { formatGitDiff } from '../utils/wasm-diff';

const GH_PR_REGEX = /^https?:\/\/github\.com\/([^/]+)\/([^/]+)\/pull\/(\d+)$/i;

export function parsePrNumber(urlOrNumber: string): string {
  // ... existing implementation unchanged ...
  const trimmed = urlOrNumber.trim();
  if (/^\d+$/.test(trimmed)) return trimmed;
  const match = trimmed.match(GH_PR_REGEX);
  if (match === null) {
    throw new Error('PR URL must be a GitHub pull request URL (e.g. https://github.com/owner/repo/pull/42)');
  }
  const prNumber = match[3];
  if (prNumber === undefined) {
    throw new Error('Failed to extract PR number from URL');
  }
  return prNumber;
}

export function buildDiffSource(options: {
  readonly base?: string | undefined;
  readonly head?: string | undefined;
  readonly pr?: string | undefined;
}): CodeReviewDiffSource {
  // ... existing implementation unchanged ...
  if (options.pr !== undefined) {
    return { kind: 'pr', prUrlOrNumber: options.pr };
  }
  if (options.base !== undefined || options.head !== undefined) {
    const base = options.base ?? 'HEAD~1';
    const head = options.head ?? 'HEAD';
    return { kind: 'commits', base, head };
  }
  return { kind: 'working-tree' };
}

export async function fetchDiff(
  source: CodeReviewDiffSource,
  cwd: string,
  _signal?: AbortSignal,
  opts?: { readonly env?: Record<string, string | undefined> },
): Promise<string> {
  let raw: string;
  switch (source.kind) {
    case 'commits':
      raw = await runGitDiff(['diff', source.base, source.head], cwd, opts);
      break;
    case 'working-tree':
      raw = await runGitDiff(['diff'], cwd, opts);
      break;
    case 'pr':
      raw = await runGhPrDiff(parsePrNumber(source.prUrlOrNumber), cwd, opts);
      break;
  }
  return formatGitDiff(raw);
}

async function runGitDiff(
  args: string[],
  cwd: string,
  opts?: { readonly env?: Record<string, string | undefined> },
): Promise<string> {
  return runCommand('git', args, cwd, opts);
}

async function runGhPrDiff(
  prNumber: string,
  cwd: string,
  opts?: { readonly env?: Record<string, string | undefined> },
): Promise<string> {
  const diff = await runCommand('gh', ['pr', 'diff', prNumber], cwd, opts);
  if (diff.trim().length === 0) {
    throw new Error('PR diff is empty. Ensure gh CLI is authenticated (gh auth login) and the PR exists.');
  }
  return diff;
}

function runCommand(
  command: string,
  args: string[],
  cwd: string,
  opts?: { readonly env?: Record<string, string | undefined> },
): Promise<string> {
  return new Promise<string>((resolve, reject) => {
    const child = spawn(command, args, {
      cwd,
      env: opts?.env ?? process.env,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    const stdoutChunks: Buffer[] = [];
    const stderrChunks: Buffer[] = [];
    child.stdout.on('data', (chunk: Buffer) => stdoutChunks.push(chunk));
    child.stderr.on('data', (chunk: Buffer) => stderrChunks.push(chunk));
    child.on('error', (err) => {
      reject(new Error(`${command} failed to start: ${err.message}`));
    });
    child.on('close', (code) => {
      if (code !== 0) {
        const stderr = Buffer.concat(stderrChunks).toString('utf-8').trim();
        reject(new Error(`${command} ${args.join(' ')} exited with ${code}${stderr ? ': ' + stderr : ''}`));
      } else {
        resolve(Buffer.concat(stdoutChunks).toString('utf-8'));
      }
    });
  });
}
```

- [ ] 搜索 `fetchDiff` 调用点，确认签名未变：

```bash
rg -n "fetchDiff\(" packages/agent-core/src packages/agent-core/test
```

Expected: 所有调用点仍使用 `(source, cwd, signal?, opts?)` 四参数签名，无需修改。

- [ ] 运行类型检查：

```bash
pnpm tsc --noEmit -p packages/agent-core/tsconfig.json
```

Expected: 成功退出。

- [ ] Commit: `feat(agent-core): wire wasm diff into code-review/diff`

---

### Task 12: Diff Golden Parity 与降级测试

**Depends on:** Task 11

**Files:**
- Create: `packages/agent-core/test/code-review/diff-wasm-parity.test.ts`

**Goal:** 验证 `computeTextDiff` 与 `formatGitDiff` 的 Wasm 输出符合预期；验证 flag 禁用时降级到 JS fallback。

**Step-by-step:**

- [ ] 创建测试文件 `packages/agent-core/test/code-review/diff-wasm-parity.test.ts`：

```typescript
import { describe, expect, it } from 'vitest';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { loadWasmDiffModule, computeTextDiff, formatGitDiff, initDiffWasm } from '../../src/utils/wasm-diff';

const WASM_PATH = fileURLToPath(
  new URL('../../../../rust-ody/target/wasm32-unknown-unknown/release/ody_rust.wasm', import.meta.url),
);

async function realWasmBytes(): Promise<Uint8Array> {
  return new Uint8Array(await readFile(WASM_PATH));
}

describe('wasm diff parity', () => {
  it('computeTextDiff produces a unified diff', async () => {
    const diff = await loadWasmDiffModule(await realWasmBytes());
    const out = diff.computeTextDiff('a\nb', 'a\nc\nb');
    expect(out).toContain('@@');
    expect(out).toContain('+c');
    expect(out).toContain('--- old');
    expect(out).toContain('+++ new');
  });

  it('formatGitDiff strips trailing whitespace and preserves structure', async () => {
    const diff = await loadWasmDiffModule(await realWasmBytes());
    const raw = 'diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n ';
    const out = diff.formatGitDiff(raw);
    expect(out).not.toEndWith(' ');
    expect(out).toContain('diff --git');
    expect(out).toContain('-a');
    expect(out).toContain('+b');
  });

  it('formatGitDiff drops empty hunks', async () => {
    const diff = await loadWasmDiffModule(await realWasmBytes());
    const raw =
      'diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n context\n context\n';
    const out = diff.formatGitDiff(raw);
    expect(out).not.toContain('@@');
  });
});

describe('wasm diff fallback', () => {
  it('initDiffWasm with flag disabled uses JS fallback', async () => {
    await initDiffWasm({ ODY_CODE_EXPERIMENTAL_WASM_DIFF: '0' });
    const out = computeTextDiff('a\nb', 'a\nc\nb');
    expect(out).toContain('+c');
    // JS fallback header matches Wasm fallback header for easy comparison.
    expect(out).toContain('--- old');
  });

  it('formatGitDiff JS fallback is identity', async () => {
    await initDiffWasm({ ODY_CODE_EXPERIMENTAL_WASM_DIFF: '0' });
    const raw = 'diff --git a/f b/f\n-a\n+b\n';
    expect(formatGitDiff(raw)).toBe(raw);
  });
});
```

- [ ] 运行测试并确认**通过**：

```bash
pnpm vitest run packages/agent-core/test/code-review/diff-wasm-parity.test.ts
```

Expected: 5 tests passed。

> 若 `formatGitDiff` 的 parity 测试失败（Rust 解析器输出与预期不符），保留 JS fallback 为 identity，并在 `loadWasmDiffModule.factory.formatGitDiff` 中直接抛错以强制走 fallback；随后在 Task 11 的 commit 中记录此决定。

- [ ] Commit: `test(agent-core): wasm diff golden parity and fallback`

---

## Local Self-Review

- [ ] 1. Spec-coverage: Part 4 覆盖 "`compute_unified_diff`"、"`format_git_diff`"、"`fetchDiff` 返回前调用 format"、"flag 禁用降级"。
- [ ] 2. Placeholder scan: 无 TODO/TBD；`compute_diff`、`format_git_diff`、`computeTextDiffJs`、`longestCommonSubsequence` 均给出完整实现。
- [ ] 3. No phantom tasks: Task 10 新增 Rust 导出与测试；Task 11 创建 TS 模块并修改 `diff.ts`；Task 12 创建 parity/fallback 测试。
- [ ] 4. Dependency soundness: Task 10 依赖 Part 1；Task 11 依赖 Task 10 + Part 2；Task 12 依赖 Task 11；无向后依赖。
- [ ] 5. Caller & build soundness: `fetchDiff` 签名保持不变；Task 11 中搜索所有调用点确认无需修改；Task 11 以 `pnpm tsc` 收尾。
- [ ] 6. Test-the-risk: parity 测试验证 `computeTextDiff` 输出含 hunk header 与新增行；`formatGitDiff` 测试验证去行尾空格与去空 hunk；fallback 测试验证 flag 关闭时使用 JS。
- [ ] 7. Type一致性: `DiffModule` 接口、`callWasmStringFunction` 复用 Part 2；Rust `compute_diff`/`format_git_diff` ABI 与 TS factory 调用参数匹配。
