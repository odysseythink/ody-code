# Phase 1-A 详细设计：Wasm 高确定性计算热点

> **对应总路线图**: `.ody-code/roadmaps/backend-architecture-evolution-roadmap.md` 的 "1-A｜Wasm 高确定性热点(策略 W,低风险)"。
> **Document Type**: Design · **Status**: DRAFT (awaiting approval) · **Audit Level**: Deep

---

## Scope In/Out

### In Scope [C:USER]

- **真 BPE 分词器**:在 `rust-ody/` 中新增基于 `tiktoken-rs` 的 Wasm 导出，支持可配置 encoding（`cl100k_base`、`o200k_base` 等），rank 数据编译进 Wasm 二进制；TS 侧提供与原 `estimateTokens` 同签名的双轨加载器。
- **diff/patch 纯计算**:把 `code-review/diff.ts` 中"spawn git 取 diff"与"diff 处理"解耦；Rust/Wasm 侧同时提供：
  - `compute_unified_diff(old_text, new_text)` 纯文本 diff；
  - `format_git_diff(raw_diff)` 对 git 原始输出做解析、清洗、分块。
- **glob/路径匹配**:用 Rust crate（`globset` 或 `glob-match`）在 Wasm 中复刻 `picomatch` 语义，替换 `tools/support/path-glob-match.ts` 与 `rule-match.ts` 的底层匹配。
- **统一双轨加载框架**:每个模块默认尝试 Wasm，加载失败或运行时异常时静默降级到原 TS 实现；可通过 `flags.enabled()` / env 全局禁用 Wasm。
- **Golden parity 测试**:每个 Wasm 化函数必须与原 TS 实现逐值一致。
- **G1-A 收益基准报告**:命令行输出 + 落盘 Markdown 报告；合成样本用于回归测试，真实采样用于收益报告。

### Out of Scope [C:USER]

- MessagePort / Socket transport（Phase 1-B）。
- `agent-core` 拆包（Phase 1-C）。
- 其他计算模块（session 序列化、compaction 预算等）的 Wasm 化（留待 G1-A 通过后再评估）。
- Native Rust (napi-rs) 模块（Phase 2-E）。
- 真 BPE 以外的 tokenizer（如 sentencepiece、wordpiece）。

---

## Prior Art

- **npm `tiktoken`**:已把 BPE tokenizer 编成 Wasm 并在 Node/Browser 分发，证明技术路径可行；它把 rank 数据放在独立 JSON 文件中，需要 bundler/SEA 额外处理。本设计选择把 rank 数据嵌入 Wasm 以简化 SEA 交付 [C:UPSTREAM]。
- **Rust `similar`**:提供纯 Rust diff 算法（Myers / LCS / Patience），无 I/O，天然适合 Wasm；生态中已有 diff.rs 等项目在浏览器跑 [C:UPSTREAM]。
- **Rust `globset` / `glob-match` / `fast-glob`**:都是无 I/O 的 glob 匹配库；`globset` 语义最接近 picomatch，但需验证 `wasm32-unknown-unknown` 兼容性 [C:UPSTREAM]。
- **本仓库 PoC**: `rust-ody/ts/wasm-tokens.ts` 与 `bench.ts` 已验证 raw-ABI Wasm 加载、内存分配/释放、JS-vs-Wasm parity 方法论 [C:UPSTREAM]。

---

## Reuse Analysis

| # | 候选文件 | 可复用内容 | 使用方式 |
|---|---|---|---|
| 1 | `rust-ody/build.sh:1-25` / `rust-ody/Cargo.toml:1-17` | Rust → Wasm 构建脚本与 release profile | 扩展：新增导出函数与依赖 crate [C:UPSTREAM] |
| 2 | `rust-ody/src/lib.rs:1-91` | `alloc`/`dealloc` 内存管理 ABI 与 `estimate_tokens` 示例 | 扩展：保留 ABI，新增 `count_tokens`、`compute_diff`、`glob_match` 等导出 [C:UPSTREAM] |
| 3 | `rust-ody/ts/wasm-tokens.ts:1-52` | Wasm 字节加载、实例化、内存读写、降级 fallback 模式 | 扩展为通用 `loadWasmModule<T>(path, fallback)` 工厂 [C:UPSTREAM] |
| 4 | `rust-ody/ts/bench.ts:1-77` | warmup + `process.hrtime.bigint` 多尺寸 benchmark 方法论 | 复用：新增分词器/diff/glob 三套 benchmark 与报告生成 [C:UPSTREAM] |
| 5 | `packages/agent-core/src/utils/tokens.ts:1-69` | `estimateTokens*` 系列函数签名与调用点 | 改造：保留 JS fallback，新增异步 Wasm 初始化入口 [C:USER] |
| 6 | `packages/agent-core/src/code-review/diff.ts:1-100` | `fetchDiff`、`buildDiffSource`、`parsePrNumber` | 改造：保留 git/gh 取 diff，新增 `formatGitDiff` 与可选的 `computeTextDiff` [C:USER] |
| 7 | `packages/agent-core/src/tools/support/path-glob-match.ts:1-146` / `rule-match.ts:1-41` | `globMatch`、`pathGlobMatch`、规则匹配接口 | 改造：保留 JS fallback，内部可切换 Wasm matcher [C:USER] |
| 8 | `packages/agent-core/src/flags/registry.ts:1-41` | 实验 flag 注册表 | 扩展：新增 `wasm-tokenizer`、`wasm-diff`、`wasm-glob` flag [C:UPSTREAM] |
| 9 | `packages/agent-core/test/rpc/create-rpc.test.ts` / `agent/harness/snapshots.ts` | parity 测试与快照归一化 | 复用：为每个 Wasm 模块写 golden parity 测试 [C:INFERRED] |

> 结论：无现成通用 Wasm 热点的加载/降级框架，属 greenfield 框架设计；各算法均有成熟 Rust crate 可复用，TS 侧大量复用现有函数签名与调用点。

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| A1 | `tiktoken-rs`、`similar`、`globset` 均能编译到 `wasm32-unknown-unknown` 且不依赖 I/O | Medium | 若任一 crate 不支持 Wasm，对应模块需换 crate 或取消 Wasm 化 | 在 `rust-ody/Cargo.toml` 引入依赖后执行 `cargo build --target wasm32-unknown-unknown` [C:INFERRED] |
| A2 | 把 rank 数据用 `include_str!`/`include_bytes!` 编译进 Wasm 后，二进制体积仍在可接受范围（<2MB） | Medium | 体积过大将影响 SEA 打包与首次加载 | 实测 release build 后的 `.wasm` 大小 [C:INFERRED] |
| A3 | Rust glob crate（`globset`/`glob-match`）能够逐例复刻 `picomatch` 对 `*`、`**`、`?`、`[]`、`{}`、转义、case-insensitive 的处理 | Medium | 语义漂移会导致权限规则意外通过/拒绝 | 用现有 `path-glob-match.ts` 的测试集做 parity 测试 [C:INFERRED] |
| A4 | 当前 `estimateTokens` 的调用方都能接受异步初始化（或已有同步 fallback） | Medium | 若某调用方在 Wasm 初始化完成前调用会拿到 JS fallback，行为一致但无法验证 Wasm 路径 | 审计所有 `estimateTokens` 调用点，确认它们在初始化后使用 [C:INFERRED] |
| A5 | 真实会话/代码采样可用于基准报告而不泄露敏感信息（经匿名化或仅使用开源样本） | Medium | 可能泄露用户代码/消息 | 基准脚本提供 `--samples-dir` 参数，CI 使用固定开源数据集 [C:INFERRED] |
| A6 | CI 环境已安装 Rust toolchain 并支持 `wasm32-unknown-unknown` | Medium | CI 无法构建 Wasm，导致测试/SEA 失败 | 检查现有 CI 配置或在新 PR 中验证 `rust-ody/build.sh` [C:INFERRED] |

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | `tiktoken-rs` 编译 Wasm 后体积过大（>2MB） | 中 | SEA 打包与加载变慢 | 先用 `wasm-opt` 压缩；仍过大则改为运行时加载 rank JSON [C:USER] |
| R2 | `globset` 与 `picomatch` 语义存在不可调和差异 | 中 | 权限规则误匹配 | 用完整现有测试集做 parity；失败则放弃 glob Wasm 化 [C:USER] |
| R3 | Wasm 边界税在小输入下抵消甚至超过计算收益 | 高 | G1-A 门被触发，只保留 tokenizer | 复用现有 bench 方法论测量多尺寸；严格按 <2% 总延迟门控 [C:USER] |
| R4 | CI 缺少 Rust toolchain 或 wasm target | 中 | 构建失败 | 在 `flake.nix`/CI 中增加 `rustup target add wasm32-unknown-unknown` [C:INFERRED] |
| R5 | 多 encoding 懒加载导致首次调用延迟抖动 | 低 | 用户体验出现偶发卡顿 | 初始化时预加载默认 encoding；其他 encoding 按需 [C:INFERRED] |
| R6 | Wasm 内存增长导致大输入失败 | 低 | 分词/diff 大文本时 panic | ABI 设计预留 realloc 或返回错误码；parity 测试含大 payload [C:INFERRED] |

---

## Parts

本设计为单一相干主题（在 `rust-ody` 中扩展 Wasm 计算热点并接入 `agent-core`），无需拆分。


---

## Architecture & Data Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  TS Layer (agent-core)                                                       │
│                                                                              │
│  estimateTokens(text) ──┐                                                    │
│  globMatch(...) ────────┼──► WasmDualTrackLoader                             │
│  pathGlobMatch(...) ────┤       │                                            │
│  formatGitDiff(...) ────┤       ▼                                            │
│  computeTextDiff(...) ──┘  flags.enabled('wasm-tokenizer')?                  │
│                            flags.enabled('wasm-diff')?                       │
│                            flags.enabled('wasm-glob')?                       │
│                                   │                                          │
│              ┌────────────────────┴────────────────────┐                     │
│              ▼                                         ▼                     │
│       Wasm path (default)                       JS fallback                  │
│              │                                         │                     │
│              ▼                                         ▼                     │
│   loadWasmModule(path) ──► WebAssembly.instantiate    original TS function   │
│              │                                         │                     │
│              ▼                                         ▼                     │
│   call exported fn (alloc/write/call/read/dealloc)   return result           │
│              │                                         │                     │
│              └────────────────────┬────────────────────┘                     │
│                                   ▼                                          │
│                            return result to caller                           │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  Rust Layer (rust-ody/src/lib.rs)                                            │
│                                                                              │
│  #[no_mangle] extern "C" fn alloc(len) -> *mut u8                           │
│  #[no_mangle] extern "C" fn dealloc(ptr, len)                               │
│  #[no_mangle] extern "C" fn count_tokens(encoding_ptr, text_ptr) -> u32     │
│  #[no_mangle] extern "C" fn compute_diff(old_ptr, new_ptr) -> *mut u8       │
│  #[no_mangle] extern "C" fn glob_match(pattern_ptr, value_ptr) -> u32       │
│                                                                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                          │
│  │ tiktoken-rs │  │   similar   │  │  globset    │                          │
│  │   (BPE)     │  │  (diff/patch)│  │  (glob)     │                          │
│  └─────────────┘  └─────────────┘  └─────────────┘                          │
│                                                                              │
│  Rank data embedded via include_str!/include_bytes!                         │
└─────────────────────────────────────────────────────────────────────────────┘
```

关键控制点:

1. **TS 侧不直接实例化 Wasm**; 统一通过 `WasmDualTrackLoader` 工厂，按 feature flag 决定走 Wasm 还是 JS。
2. **Wasm 路径失败时静默降级**: `loadWasmModule` 在实例化失败、导出缺失、运行时 panic 时返回 fallback 函数。
3. **Rust 侧保持 raw ABI**: 延续 PoC 的 `alloc`/`dealloc` 模式，避免引入 `wasm-bindgen` 额外依赖与体积。
4. **每个导出函数自己管理输入/输出生命周期**: 调用方负责 `alloc` 写入输入、调用函数、读取输出、调用 `dealloc`。

---

## Data Models

### Core Types

```typescript
// 通用 Wasm 模块导出接口（以 tokenizer 为例，diff/glob 同构）
// [C:INFERRED]
interface WasmTokenizerExports {
  readonly memory: WebAssembly.Memory;
  alloc(len: number): number;
  dealloc(ptr: number, len: number): void;
  count_tokens(encoding_ptr: number, text_ptr: number, text_len: number): number;
}

interface WasmDiffExports {
  readonly memory: WebAssembly.Memory;
  alloc(len: number): number;
  dealloc(ptr: number, len: number): void;
  // 返回 diff 字符串指针，调用方读取后 dealloc
  compute_diff(old_ptr: number, old_len: number, new_ptr: number, new_len: number): number;
  format_git_diff(raw_ptr: number, raw_len: number): number;
}

interface WasmGlobExports {
  readonly memory: WebAssembly.Memory;
  alloc(len: number): number;
  dealloc(ptr: number, len: number): void;
  // 返回 0/1；pathGlobMatch 选项通过额外参数或序列化传递
  glob_match(pattern_ptr: number, pattern_len: number, value_ptr: number, value_len: number, options: number): number;
}

// 统一双轨加载器返回的函数签名保持与原 TS 一致
// [C:USER]
type TokenEstimator = (text: string, encoding?: string) => number;
type DiffFormatter = (rawDiff: string) => string;
type TextDiffer = (oldText: string, newText: string) => string;
type GlobMatcher = (value: string, pattern: string, options?: { nocase?: boolean }) => boolean;
```

### Internal Shapes

```typescript
// 统一加载器配置
// [C:INFERRED]
interface WasmModuleConfig<T> {
  readonly wasmPath: string;
  readonly fallback: T;
  readonly flagId: 'wasm-tokenizer' | 'wasm-diff' | 'wasm-glob';
}

// 基准报告结构
// [C:USER]
interface WasmBenchmarkReport {
  readonly timestamp: string;
  readonly commit?: string;
  readonly results: readonly {
    readonly module: 'tokenizer' | 'diff' | 'glob';
    readonly scenario: string;
    readonly inputSize: number;
    readonly jsNsPerCall: number;
    readonly wasmNsPerCall: number;
    readonly speedup: number;
    readonly correct: boolean;
  }[];
  readonly g1aVerdict: 'GO' | 'NO-GO';
  readonly note: string;
}
```

---

## Algorithms

### ALG-1: `loadWasmModule<T>(config) -> Promise<T>`

统一双轨加载器，负责按 flag 决定走 Wasm 还是 JS，并在 Wasm 失败时降级。

```
function loadWasmModule<T>(config: WasmModuleConfig<T>): Promise<T> {
  if (!flags.enabled(config.flagId)) {
    log('wasm disabled by flag', config.flagId);
    return Promise.resolve(config.fallback);
  }

  return tryLoadWasm<T>(config.wasmPath)
    .then((wasmFn) => {
      log('wasm loaded', config.flagId);
      return wrapWithFallback(wasmFn, config.fallback, config.flagId);
    })
    .catch((error) => {
      log('wasm load failed, using fallback', config.flagId, error);
      return config.fallback;
    });
}
```

关键不变量:
- 永远返回与 `fallback` 同签名的可调用对象。
- Wasm 失败时**不抛错**，避免调用方感知。
- flag 关闭时直接返回 fallback，不做任何 Wasm I/O。

### ALG-2: `callWasmStringFunction(exports, fnName, ...inputStrings) -> string`

通用 raw-ABI 字符串调用封装：把多个 UTF-8 输入串写入 Wasm 内存，调用导出函数，读取返回的字符串指针，最后释放所有内存。

```
function callWasmStringFunction(
  exports: WasmExports,
  fnName: string,
  ...inputStrings: string[]
): string {
  const encoder = new TextEncoder();
  const allocations: Array<{ ptr: number; len: number }> = [];

  for (const str of inputStrings) {
    const bytes = encoder.encode(str);
    const ptr = exports.alloc(bytes.length);
    if (ptr === 0 && bytes.length > 0) throw new Error('wasm alloc failed');
    new Uint8Array(exports.memory.buffer, ptr, bytes.length).set(bytes);
    allocations.push({ ptr, len: bytes.length });
  }

  const outPtr = exports[fnName](...allocations.flatMap(({ ptr, len }) => [ptr, len]));

  try {
    if (outPtr === 0) return '';
    // 读取直到 NUL（输出约定：C 风格字符串）
    const view = new Uint8Array(exports.memory.buffer);
    let end = outPtr;
    while (view[end] !== 0) end += 1;
    const bytes = view.subarray(outPtr, end);
    return new TextDecoder().decode(bytes);
  } finally {
    for (const { ptr, len } of allocations) {
      if (ptr !== 0) exports.dealloc(ptr, len);
    }
    if (outPtr !== 0) exports.dealloc(outPtr, 1); // 输出串由 Rust 侧以 NUL 结尾分配
  }
}
```

边界:
- 输入为空串时 `alloc(0)` 返回 0 指针，不写入。
- 输出约定：Rust 侧所有返回字符串的函数必须分配一个以 `\0` 结尾的 buffer。

### ALG-3: `count_tokens(encoding, text) -> number` (Rust/Wasm)

```
function count_tokens(encoding_ptr, encoding_len, text_ptr, text_len) -> u32 {
  encoding = decode_utf8(encoding_ptr, encoding_len);
  text = decode_utf8(text_ptr, text_len);

  bpe = lazy_load_bpe(encoding); // static cache, first call initializes
  tokens = bpe.encode(text);
  return tokens.len() as u32;
}
```

关键不变量:
- encoding 名到 BPE rank 数据的映射在 Wasm 内部通过 `include_str!` 静态嵌入。
- 首次调用某 encoding 时初始化 `CoreBPE`；后续调用复用缓存。
- 返回 `u32::MAX` 表示错误（如未知 encoding），TS 侧 fallback 到 JS heuristic。

### ALG-4: `compute_diff(old_text, new_text) -> string` (Rust/Wasm)

```
function compute_diff(old_ptr, old_len, new_ptr, new_len) -> *mut u8 {
  old_text = decode_utf8(old_ptr, old_len);
  new_text = decode_utf8(new_ptr, new_len);

  diff = similar::TextDiff::from_lines(old_text, new_text);
  unified = diff.unified_diff()
    .context_radius(3)
    .header(&old_label, &new_label)
    .to_string();

  return alloc_cstring(unified);
}
```

### ALG-5: `format_git_diff(raw_diff) -> string` (Rust/Wasm)

```
function format_git_diff(raw_ptr, raw_len) -> *mut u8 {
  raw = decode_utf8(raw_ptr, raw_len);

  // 1. 解析统一 diff header 与 hunks
  patches = parse_unified_diff(raw);

  // 2. 清洗：合并相邻 hunk、去掉空 hunk、标准化路径前缀
  cleaned = normalize_patches(patches);

  // 3. 重新序列化为统一 diff 字符串
  output = render_unified_diff(cleaned);

  return alloc_cstring(output);
}
```

> 注：Phase 1-A 中 `format_git_diff` 的清洗逻辑保持最小；仅做解析与重新序列化，确保输出与 git 原始输出逐字节一致。

### ALG-6: `glob_match(pattern, value, options) -> u32` (Rust/Wasm)

```
function glob_match(pattern_ptr, pattern_len, value_ptr, value_len, options) -> u32 {
  pattern = decode_utf8(pattern_ptr, pattern_len);
  value = decode_utf8(value_ptr, value_len);
  nocase = (options & 1) != 0;

  builder = GlobBuilder::new(pattern)
    .literal_separator(true)
    .case_insensitive(nocase);
  glob = builder.build();

  return if glob.is_match(value) { 1 } else { 0 };
}
```

关键不变量:
- `literal_separator(true)` 使 `*` 不匹配 `/`，与 `picomatch` 默认行为一致。
- 返回值用 `u32` 而非 `bool`，避免 ABI 中的 bool 歧义。


---

## Call-Site Integration

### 修改点 1: `packages/agent-core/src/flags/registry.ts:13-38`

新增三个实验 flag，默认开启 Wasm 路径。

```typescript
// 新增 [C:USER]
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
```

### 修改点 2: 新增 `rust-ody/ts/wasm-loader.ts`

通用双轨加载器工厂，被三个具体模块复用。

```typescript
// 伪代码接口 [C:INFERRED]
export interface WasmModuleConfig<T> {
  readonly wasmPath: string;
  readonly fallback: T;
  readonly flagId: 'wasm-tokenizer' | 'wasm-diff' | 'wasm-glob';
}

export async function loadWasmModule<T>(config: WasmModuleConfig<T>): Promise<T>;
```

### 修改点 3: `packages/agent-core/src/utils/tokens.ts:11-22`

保留原 `estimateTokens` 作为 JS fallback；新增异步初始化入口。

```typescript
// 改造前
export function estimateTokens(text: string): number { ... }

// 改造后 [C:USER]
let wasmEstimateTokens: ((text: string, encoding?: string) => number) | undefined;

export async function initTokenizerWasm(): Promise<void> {
  wasmEstimateTokens = await loadWasmTokenizerEstimator();
}

export function estimateTokens(text: string): number {
  const fn = wasmEstimateTokens;
  if (fn !== undefined) {
    try {
      return fn(text, 'cl100k_base');
    } catch {
      // fallthrough to JS
    }
  }
  // 原 JS heuristic
  ...
}
```

调用点（如 `packages/agent-core/src/agent/index.ts:393-395`）无需改动；启动流程在 Agent 初始化时调用 `initTokenizerWasm()` [C:INFERRED]。

### 修改点 4: `packages/agent-core/src/code-review/diff.ts:36-70`

保留 `fetchDiff` 的 git/gh 调用；新增/导出纯 diff 与格式化函数。

```typescript
// 新增 [C:USER]
export async function computeTextDiff(oldText: string, newText: string): Promise<string>;
export async function formatGitDiff(rawDiff: string): Promise<string>;

// fetchDiff 内部可在返回前调用 formatGitDiff [C:INFERRED]
export async function fetchDiff(
  source: CodeReviewDiffSource,
  cwd: string,
  _signal?: AbortSignal,
  opts?: { readonly env?: Record<string, string | undefined> },
): Promise<string> {
  const raw = await runGitDiffOrGh(...);
  return formatGitDiff(raw); // 默认走 Wasm 格式化
}
```

### 修改点 5: `packages/agent-core/src/tools/support/path-glob-match.ts:22-56`

保留 `globMatch`/`pathGlobMatch` 签名；内部可切换 Wasm matcher。

```typescript
// 改造后 [C:USER]
let wasmGlobMatch: ((value: string, pattern: string, options?: { nocase?: boolean }) => boolean) | undefined;

export async function initGlobWasm(): Promise<void> {
  wasmGlobMatch = await loadWasmGlobMatcher();
}

export function globMatch(value: string, pattern: string, options?: { nocase?: boolean }): boolean {
  if (wasmGlobMatch !== undefined) {
    try {
      return wasmGlobMatch(value, pattern, options);
    } catch {
      // fallthrough
    }
  }
  // 原 picomatch 路径
  ...
}
```

### 修改点 6: 新增 `rust-ody/ts/bench-phase1a.ts`

扩展现有 bench 脚本，输出 Markdown 报告。

```typescript
// 新增 [C:USER]
async function main() {
  const report = await runAllBenchmarks({ synthetic: true, realSamplesDir: process.argv[2] });
  console.log(formatConsole(report));
  await writeFile('.ody-code/reports/wasm-phase1a-benchmark.md', formatMarkdown(report));
}
```

---

## Error Handling

| Error Class | Immediate Handling | Degradation Path | Recovery Condition |
|---|---|---|---|
| Wasm 模块加载失败（文件缺失、编译错误、target 不支持） | `loadWasmModule` catch 并返回 JS fallback | 调用方无感知，使用原 TS 实现 | 修复构建/环境后重启进程 |
| Wasm 运行时 panic/异常 | `wrapWithFallback` 捕获异常并切到 fallback | 单次调用降级为 JS；后续调用仍尝试 Wasm | 修复 Rust 代码后重编 |
| 未知 encoding 名 | Rust 返回 `u32::MAX`，TS 侧 fallback 到 JS heuristic | 该次调用使用启发式估算 | 添加对应 encoding 数据 |
| Wasm 内存分配失败 | 抛错，被 `wrapWithFallback` 捕获后走 JS | 该次调用降级 | 输入过大时改用 JS |
| `formatGitDiff` 解析失败 | 返回原始字符串，避免丢失数据 | 不清洗，原样交给下游 | 修复解析器 |
| Glob 语义不匹配（parity 测试发现） | 测试失败，CI 阻断 | 禁用 `wasm-glob` flag，回退 picomatch | 修复 Rust matcher 或放弃该项 |

---

## Test Plan

### 现有测试必须继续通过

- `pnpm vitest run packages/agent-core`
- `pnpm tsc --noEmit -p packages/agent-core/tsconfig.json`

### 新增测试 1: Wasm 模块 golden parity

文件: `packages/agent-core/test/utils/tokens-wasm-parity.test.ts`

断言:
```typescript
const estimateWasm = await loadWasmTokenizerEstimator();
for (const sample of [/* 合成 + 边界样本 */]) {
  expect(estimateWasm(sample, 'cl100k_base')).toBe(estimateTokensJs(sample));
}
```

文件: `packages/agent-core/test/code-review/diff-wasm-parity.test.ts`

断言:
```typescript
const diffWasm = await loadWasmDiffModule();
expect(diffWasm.computeTextDiff('a\nb', 'a\nc\nb')).toBe(computeTextDiffJs('a\nb', 'a\nc\nb'));
expect(diffWasm.formatGitDiff(rawGitDiff)).toBe(formatGitDiffJs(rawGitDiff));
```

文件: `packages/agent-core/test/tools/support/glob-wasm-parity.test.ts`

断言:
```typescript
const globWasm = await loadWasmGlobMatcher();
for (const { value, pattern, options, expected } of EXISTING_GLOB_TEST_CASES) {
  expect(globWasm(value, pattern, options)).toBe(expected);
}
```

### 新增测试 2: 双轨降级

断言:
```typescript
// 传一个不存在的 wasm 路径
const fn = await loadWasmModule({ wasmPath: '/nonexistent.wasm', fallback: jsFn, flagId: 'wasm-tokenizer' });
expect(fn('hello')).toBe(jsFn('hello'));
```

### 新增测试 3: Feature flag 禁用

断言:
```typescript
process.env.ODY_CODE_EXPERIMENTAL_WASM_TOKENIZER = '0';
const fn = await loadWasmTokenizerEstimator();
// 应直接返回 JS fallback，不做 Wasm 加载
```

### 新增测试 4: G1-A 收益基准

运行:
```bash
pnpm tsx rust-ody/ts/bench-phase1a.ts --samples-dir ./test-samples
```

断言:
- 所有 parity 测试通过（`correct: true`）。
- 报告文件生成于 `.ody-code/reports/wasm-phase1a-benchmark.md`。
- `g1aVerdict` 字段由脚本根据阈值自动判定。

### Done Criteria

```bash
# 1. Rust native 单元测试
cd rust-ody && cargo test

# 2. Wasm 构建
./rust-ody/build.sh

# 3. TypeScript 类型检查
pnpm tsc --noEmit -p packages/agent-core/tsconfig.json

# 4. agent-core 测试（含新增 parity）
pnpm vitest run packages/agent-core

# 5. G1-A 收益基准（合成样本）
pnpm tsx rust-ody/ts/bench-phase1a.ts

# 6. 真实样本基准（CI 使用固定数据集）
pnpm tsx rust-ody/ts/bench-phase1a.ts --samples-dir ./ci-benchmark-samples
```


---

## Self-Review

### 1-3 个最昂贵的决策 + adversarial 输入

**决策 1: `glob_match` 使用 `literal_separator(true)` 复刻 picomatch `*` 不跨路径分隔符**

| # | 输入 | 预期输出 |
|---|---|---|
| 1.1 | pattern=`"a/*"`, value=`"a/b"` | match=true |
| 1.2 | pattern=`"a/*"`, value=`"a/b/c"` | match=false |
| 1.3 | pattern=`"a/**"`, value=`"a/b/c"` | match=true |

> 验证方式：在 parity 测试中固定这三组；若 Rust `globset` 结果不同，则调整 builder 配置或放弃 glob Wasm 化。

**决策 2: Wasm 运行时异常静默降级到 JS fallback**

| # | 输入 | 预期输出 |
|---|---|---|
| 2.1 | Wasm 加载成功且调用正常 | 返回 Wasm 结果 |
| 2.2 | Wasm 加载成功但某次调用抛错 | 返回 JS fallback 结果，不抛错 |
| 2.3 | Wasm 加载失败 | 返回 JS fallback 结果，不抛错 |

**决策 3: `count_tokens` 对未知 encoding 返回 `u32::MAX`**

| # | 输入 | 预期输出 |
|---|---|---|
| 3.1 | encoding=`"cl100k_base"`, text=`"hello"` | 正整数 token 数 |
| 3.2 | encoding=`"unknown_encoding"`, text=`"hello"` | `u32::MAX` → TS fallback 到 heuristic |
| 3.3 | encoding=`""`, text=`"hello"` | `u32::MAX` → TS fallback |

### 四镜扫描

- **Security**:检查了 Wasm 模块只处理本地文本/路径，不访问网络、文件系统或 secrets。风险点在于基准报告可能包含用户代码/路径样本；已在设计中规定报告使用本地 `.ody-code/reports/` 目录，真实样本需匿名化或由 CI 使用固定开源数据集。无输入过滤器/正则需要验证 false positive/negative。
- **Test**:每个 Wasm 化函数都有 golden parity 测试（must-pass）和 fallback 测试（must-reject Wasm 路径）。G1-A 收益报告同时给出 `speedup` 与 `correct` 字段，确保性能结论建立在正确性之上。未发现断言与依赖常量矛盾。
- **Ops**:Wasm 模块在进程内单例加载一次，后续调用只有 raw-ABI 内存拷贝开销；flag 检查为常数时间。`alloc`/`dealloc` 配对使用，pariy 测试可检测内存泄漏。多 encoding 懒加载可能导致首次调用延迟，已设计默认 encoding 预加载。
- **Integration**:已验证 `packages/agent-core/src/flags/registry.ts` 存在且结构符合新增 flag 要求；`rust-ody/ts/wasm-tokens.ts` 与 `bench.ts` 存在；`packages/agent-core/src/utils/tokens.ts`、`code-review/diff.ts`、`tools/support/path-glob-match.ts` 存在且调用点明确。设计落在用户指定的 `rust-ody/` 与 `packages/agent-core/src/` 路径，未静默改目标。
- **Scope**:本设计仍围绕单一主题——把三个高确定性计算热点 Wasm 化并接入 agent-core，共享同一加载/降级/基准框架，未膨胀为多个独立项目。

### 内联修正

- 明确 `formatGitDiff` 的清洗逻辑在 Phase 1-A 保持最小，只做解析与重新序列化，避免引入行为漂移。
- 在 ALG-2 中约定 Rust 侧返回字符串必须分配以 `\0` 结尾的 buffer，统一 TS 读取方式。
- 在 Call-Site Integration 中说明 `estimateTokens` 调用方无需改动，启动流程负责初始化 Wasm。

---

## User Final Approval

- [ ] 设计文件已读并理解
- [ ] Scope In/Out 接受
- [ ] Architecture & Data Flow 接受
- [ ] Data Models / Interfaces 接受
- [ ] Algorithms 接受
- [ ] Call-Site Integration 接受
- [ ] Error Handling 接受
- [ ] Test Plan / Done Criteria 接受
- [ ] Risk Register 接受
- [ ] Assumptions & Unverified Items 已逐项确认（见下节审计门）
