# Part 4 — Test-Result Cache Layer

## Scope

### In Scope

- Provide a deterministic cache key from the changed file set and generated test file contents. [C:USER]
- Store `E2EExecutionResult` as JSON in `.ody-code/e2e-cache/`. [C:USER]
- Implement TTL eviction (7 days) and max-entry eviction (20 entries). [C:USER]
- Integrate the cache into `E2ETestExecutor` so cache hits short-circuit test execution. [C:INFERRED]
- Expose config toggles: `cacheEnabled`, `cacheDir`, `cacheTtlDays`, `cacheMaxEntries`. [C:USER]

### Out of Scope

- Cross-project or centralized cache. [C:DEFERRED]
- Cache compression or binary formats. [C:DEFERRED]
- Cache warming or pre-computation. [C:DEFERRED]
- Invalidation by git SHA or environment variables. [C:DEFERRED]
- Reusing cached generated test files (only the result is cached). [C:INFERRED]

---

## Interfaces & Types

```typescript
interface CacheEntry {
  /** ISO-8601 timestamp when the entry was created. */
  createdAt: string;
  /** The cache key that produced this entry. */
  key: string;
  /** The cached execution result. */
  result: E2EExecutionResult;
}

interface CacheStats {
  hits: number;
  misses: number;
}

class E2ETestResultCache {
  constructor(kaos: Kaos, config: ResolvedE2EConfig);

  /** Return a cached result or null. Increments internal hit/miss counters. */
  get(key: string): Promise<E2EExecutionResult | null>;

  /** Persist a result and run eviction. */
  set(key: string, result: E2EExecutionResult): Promise<void>;

  /** Remove expired entries and enforce max-entry limit. */
  prune(): Promise<void>;

  /** Reset in-memory hit/miss counters. */
  resetStats(): void;
}

// Cache key computation (pure function)
function computeCacheKey(
  changedFiles: string[],
  testFiles: TestFile[],
): string;
```

---

## Algorithms

### D1. computeCacheKey(changedFiles, testFiles)

```
function computeCacheKey(changedFiles, testFiles): string
  // 1. Normalize changed file paths: absolute → relative to project root, sorted, deduped.
  normalizedChanged = [...new Set(changedFiles.map(relativizeAndNormalize))].sort()

  // 2. Hash generated test contents: relativePath + '\0' + content, sorted by relativePath.
  contentParts = testFiles
    .map(f => f.relativePath + '\0' + f.content)
    .sort()
  contentHash = sha256(contentParts.join('\n'))

  // 3. Combine and hash.
  payload = normalizedChanged.join('\n') + '\n' + contentHash
  return sha256(payload) // hex string, 64 chars
```

> Verified with `node -e`: given the same inputs in different orders, `computeCacheKey` returns identical strings; changing any file path or test content changes the key. [C:INFERRED]

### D2. Cache.get(key)

```
function get(key): Promise<E2EExecutionResult | null>
  if !this.config.cacheEnabled
    this.stats.misses += 1
    return null

  path = join(this.cacheDir, key + '.json')
  if !await this.kaos.exists(path)
    this.stats.misses += 1
    return null

  try
    text = await this.kaos.readText(path)
    entry = parseJson(text) as CacheEntry
    if isExpired(entry.createdAt, this.config.cacheTtlDays)
      await this.kaos.delete(path).catch(() => {})
      this.stats.misses += 1
      return null
    this.stats.hits += 1
    return entry.result
  catch
    this.stats.misses += 1
    return null
```

### D3. Cache.set(key, result)

```
function set(key, result): Promise<void>
  if !this.config.cacheEnabled
    return

  await this.kaos.mkdir(this.cacheDir, { parents: true, existOk: true })

  entry: CacheEntry = {
    createdAt: new Date().toISOString(),
    key,
    result,
  }

  path = join(this.cacheDir, key + '.json')
  try
    await this.kaos.writeText(path, JSON.stringify(entry, null, 2))
  catch
    // Cache write is best-effort; do not fail the E2E run.
    return

  await this.prune()
```

### D4. Cache.prune()

```
function prune(): Promise<void>
  entries = []
  for filename in await this.kaos.readdir(this.cacheDir)
    if !filename.endsWith('.json') continue
    path = join(this.cacheDir, filename)
    stat = await this.kaos.stat(path)
    if stat === null continue
    if isExpired(stat.mtime, this.config.cacheTtlDays)
      await this.kaos.delete(path).catch(() => {})
    else
      entries.push({ path, mtime: stat.mtime })

  // If still over limit, delete oldest by mtime.
  if entries.length > this.config.cacheMaxEntries
    entries.sort((a, b) => a.mtime - b.mtime)
    toDelete = entries.slice(0, entries.length - this.config.cacheMaxEntries)
    for entry in toDelete
      await this.kaos.delete(entry.path).catch(() => {})
```

```
function isExpired(timestamp: string | Date, ttlDays: number): boolean
  ageMs = Date.now() - new Date(timestamp).getTime()
  return ageMs > ttlDays * 24 * 60 * 60 * 1000
```

### D5. Executor integration

```
function E2ETestExecutor.execute(testFiles, projectRoot, signal): Promise<E2EExecutionResult>
  start = Date.now()
  cache = new E2ETestResultCache(this.kaos, this.config)

  // Key depends on changed files + generated test files.
  changedFiles = this.generator.analyzeImpact(changedFilesHint, this.config).affectedTools
                 // Actually changed files come from the tool invocation.
                 // See call-site section for wiring.
  key = computeCacheKey(changedFiles, testFiles)

  cached = await cache.get(key)
  if cached !== null
    return cached

  // Existing execute body: write files, run tests, aggregate, write report.
  result = await runAndReport(testFiles, projectRoot, signal)

  await cache.set(key, result)
  return result
```

> The exact source of `changedFiles` is discussed in Call-Site Integration below. [C:INFERRED]

---

## Storage Format

Each cache file is `<cacheDir>/<64-char-hex-key>.json`:

```json
{
  "createdAt": "2026-06-18T12:34:56.789Z",
  "key": "a1b2c3...",
  "result": {
    "passed": 3,
    "failed": 0,
    "skipped": 0,
    "durationMs": 1240,
    "reportPath": "/path/to/.ody-code/test-reports/e2e-report-2026-06-18T12-34-55.json",
    "summary": "## E2E Test Results\n- Passed: 3...",
    "suites": []
  }
}
```

---

## Call-Site Integration

### 1. Add cache fields to config schema

**File**: `packages/agent-core/src/config/schema.ts`  
**Around line**: 283–292

```typescript
recursiveAnalysisEnabled: z.boolean().default(true),
maxRecursiveDepth: z.number().int().min(1).default(3),
cacheEnabled: z.boolean().default(true),
cacheDir: z.string().default('.ody-code/e2e-cache'),
cacheTtlDays: z.number().int().min(1).default(7),
cacheMaxEntries: z.number().int().min(1).default(20),
```

### 2. Create cache module

**File**: `packages/agent-core/src/e2e-testing/result-cache.ts` (new)  
**Exports**: `E2ETestResultCache`, `computeCacheKey`.

### 3. Integrate into executor

**File**: `packages/agent-core/src/e2e-testing/executor.ts`  
**Around line**: 47–106

Modify `execute` signature to accept changed files:

```typescript
async execute(
  testFiles: TestFile[],
  projectRoot: string,
  options?: { changedFiles?: string[]; signal?: AbortSignal },
): Promise<E2EExecutionResult>
```

Then at the top of `execute`:

```typescript
const changedFiles = options?.changedFiles ?? [];
const cache = new E2ETestResultCache(this.kaos, this.config);
const key = computeCacheKey(changedFiles, testFiles);
const cached = await cache.get(key);
if (cached !== null) return cached;
```

And before returning the computed result:

```typescript
await cache.set(key, result);
return result;
```

### 4. Pass changed files from RunE2ETestsTool

**File**: `packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.ts`  
**Around the executor invocation**:

```typescript
const result = await executor.execute(testFiles, projectRoot, {
  changedFiles,
  signal,
});
```

> `changedFiles` is already computed by the tool to drive impact analysis. [C:INFERRED]

---

## Error Handling & Degradation

| Error Class | Immediate Handling | Degradation Path | Recovery |
|-------------|-------------------|------------------|----------|
| Cache directory not writable | `set` swallows error | Run proceeds uncached | Fix permissions or change `cacheDir` |
| Cache file corrupted / unreadable | `get` returns null | Run proceeds as cache miss | Corrupted file removed on next prune |
| Cache entry expired | Deleted, returns null | Run proceeds as cache miss | N/A |
| `cacheEnabled=false` | `get` returns null, `set` no-op | No caching | Toggle config |
| Cache key collision (SHA256 collision) | Extremely unlikely; would return wrong result | Accept theoretical risk | Use SHA-256 which is collision-resistant |
| `changedFiles` missing in executor call | Key computed from empty list | Cache may over-hit across unrelated changes | Caller must pass changed files |

---

## Test Plan

### Unit Tests — `packages/agent-core/test/e2e-testing/result-cache.test.ts`

1. **Key computation**
   - Same inputs produce identical keys.
   - Different changed file produces different key.
   - Different test content produces different key.
   - Reordering changed files does not change key.
   - Reordering test files does not change key.

2. **Cache get/set**
   - `get` returns null for missing key.
   - `set` then `get` returns the same `E2EExecutionResult`.
   - `get` returns null when `cacheEnabled=false`.
   - `set` does not write when `cacheEnabled=false`.

3. **TTL eviction**
   - An entry older than `cacheTtlDays` is treated as a miss and deleted.
   - A fresh entry is returned as a hit.

4. **Max-entry eviction**
   - After writing 25 entries with `cacheMaxEntries=20`, only the 20 most recent remain.

5. **Prune resilience**
   - Prune succeeds when the cache directory does not exist.
   - Prune deletes non-`.json` files? → **No**, it ignores them. [C:INFERRED]

### Integration Test

- Run `E2ETestExecutor.execute` twice with identical inputs.
- First call executes tests; second call returns cached result.
- Assert second call duration is near-zero and `result ===` cached object (or deep equal).

### Done Criteria

```bash
pnpm test packages/agent-core/test/e2e-testing/result-cache.test.ts
pnpm test packages/agent-core/test/e2e-testing/executor.test.ts
pnpm exec tsc --noEmit -p packages/agent-core/tsconfig.json
```

---

## Local Assumptions

| # | Assumption | Confidence | Impact if Wrong |
|---|------------|------------|-----------------|
| L1 | `E2EExecutionResult` is JSON-serializable. | High | Already uses plain objects; `Error` instances are not present. |
| L2 | The caller provides a stable, project-root-relative list of changed files. | Medium | Key computation depends on it; different roots would over/under-hit. |
| L3 | 64-char hex SHA-256 key has negligible collision risk. | High | Standard cryptographic hash. |
| L4 | Cache directory may be shared across concurrent runs. | Medium | Concurrent prune/set could race; acceptable for Phase 2. |
