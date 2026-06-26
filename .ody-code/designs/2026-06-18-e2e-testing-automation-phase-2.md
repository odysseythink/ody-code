# E2E Testing Automation — Phase 2 Design

**Date**: 2026-06-18  
**Audit Level**: Deep  
**Status**: Index — parts pending  
**Related Roadmap**: `.ody-code/roadmaps/e2e-testing-automation-roadmap.md`  

---

## Scope

### In Scope

1. **Python/Pytest E2E Generator** [C:USER]
   - Auto-detect Python projects via `pyproject.toml`, `requirements.txt`, or `setup.py`.
   - Recognize frameworks: FastAPI, Flask, Django, and generic Python.
   - Generate `pytest` black-box E2E tests that spawn the app/server as a subprocess and assert on HTTP/CLI behavior.
   - Parse `pytest` JSON output into the normalized `TestSuiteResult[]` model.

2. **Node.js/Jest E2E Generator** [C:USER]
   - Auto-detect Node.js projects via `package.json`.
   - Recognize frameworks: Express, NestJS, Next.js API routes, and generic Node.js.
   - Generate `jest` black-box E2E tests.
   - Parse `jest` JSON output into the normalized `TestSuiteResult[]` model.

3. **Recursive Impact Analysis** [C:USER]
   - Extend impact analysis to walk `import`/`require`/`include` dependencies recursively for TS/Vitest, Go, Python, and Node.js.
   - Identify transitive tools/files affected by a change.
   - Keep the existing static `TOOL_IMPACT_MAP` as a fast path / fallback for ody-code self-testing.

4. **Test-Result Caching Layer** [C:USER]
   - Cache `E2EExecutionResult` keyed by `(changed file set hash + generated test content hash)`.
   - Store cache in `.ody-code/e2e-cache/`.
   - TTL = 7 days, max 20 entries [C:USER].
   - Short-circuit the executor when a matching cache hit exists.

### Out of Scope (Deferred)

| Item | Reason |
|------|--------|
| ML-driven risk scoring / mutation testing | Phase 3 roadmap item; requires research infrastructure. [C:DEFERRED] |
| Contract testing | Phase 3 roadmap item; distinct subsystem. [C:DEFERRED] |
| Distributed test execution | Future scalability work, not needed for current targets. [C:DEFERRED] |
| Framework-specific template tuning beyond listed stacks | Keep Phase 2 bounded; additional frameworks (Django REST, FastAPI advanced patterns, etc.) can extend templates later. [C:DEFERRED] |
| Re-implementing existing TS/Vitest or Go generators | They already exist and satisfy Phase 1; this design only touches them where necessary for integration. [C:INFERRED] |

---

## Resolved Decisions

| # | Dimension | Decision | Source |
|---|-----------|----------|--------|
| 1 | Scope | Implement all 4 deferred Phase 2 deliverables. | [C:USER] |
| 2 | Python frameworks | Generic + FastAPI + Flask + Django. | [C:USER] |
| 3 | Node frameworks | Generic + Express + NestJS + Next.js API routes. | [C:USER] |
| 4 | Recursive analysis languages | TS/Vitest + Go + Python + Node. | [C:USER] |
| 5 | Cache key | Hash of changed-file set + generated-test-content hash. | [C:USER] |
| 6 | Cache location | `.ody-code/e2e-cache/` inside project workspace. | [C:USER] |
| 7 | Cache TTL | 7 days, max 20 entries. | [C:USER] |
| 8 | Detection failure | Skip E2E task silently; execution failures follow `config.failurePolicy`. | [C:USER] |
| 9 | Feature flags | No new experimental flag; reuse `[e2e] enabled` and add config fields. | [C:USER] |
| 10 | Impact analysis parsing | Regex/line-based import matching first (80% coverage). | [C:USER] |
| 11 | Architecture | Incrementally extend existing framework (Approach A). | [C:USER] |

---

## Architecture & Data Flow

### High-Level Flow

```
User request
  ↓
WritingPlan / Plan Enricher
  ├─ git status → changed files
  ├─ registry.detectAndGet(projectRoot) → E2ETestGenerator
  ├─ generator.analyzeImpact(changedFiles) → affected tools
  └─ append "Generate & run E2E tests" task to plan
  ↓
User approves plan
  ↓
ExecutingPlan / RunE2ETestsTool
  ├─ load config → ResolvedE2EConfig
  ├─ registry.detectAndGet(projectRoot) → generator
  ├─ generator.analyzeImpact(changedFiles) → features
  ├─ for each feature: generator.generateTestsForFeature(feature, outputDir) → TestFile[]
  ├─ E2EExecutor.execute(testFiles, projectRoot)
  │   ├─ compute cache key
  │   ├─ cache hit? → return cached E2EExecutionResult
  │   ├─ write TestFile[] to generatedTestDir
  │   ├─ generator.runTests(absolutePaths, ctx) → TestSuiteResult[]
  │   ├─ aggregate counts
  │   ├─ write JSON report
  │   └─ store result in cache
  └─ return markdown summary
```

### New / Modified Components

| Component | Location | Responsibility |
|-----------|----------|----------------|
| `PythonPytestGenerator` | `packages/agent-core/src/e2e-testing/generators/python-pytest.ts` | Detect Python projects, classify framework, generate pytest templates, run `pytest`, parse JSON output. |
| `NodejsJestGenerator` | `packages/agent-core/src/e2e-testing/generators/nodejs-jest.ts` | Detect Node.js projects, classify framework, generate jest templates, run `jest`, parse JSON output. |
| `RecursiveImpactAnalyzer` | `packages/agent-core/src/e2e-testing/recursive-impact-analyzer.ts` | Walk dependency edges recursively per language, return affected files/tools. |
| `E2ETestResultCache` | `packages/agent-core/src/e2e-testing/result-cache.ts` | Compute cache keys, read/write `E2EExecutionResult`, enforce TTL and max-entry limits. |
| `E2ETestExecutor` | `packages/agent-core/src/e2e-testing/executor.ts` (modified) | Insert cache lookup/store around existing write/run/report flow. |
| `E2EConfigSchema` | `packages/agent-core/src/config/schema.ts` (modified) | Add `cacheEnabled`, `cacheDir`, `cacheTtlDays`, `cacheMaxEntries`, `recursiveAnalysisEnabled`. |
| `E2EGeneratorRegistry` | `packages/agent-core/src/e2e-testing/registry.ts` (modified) | Register new generators; ensure detection order resolves Jest vs Vitest correctly. |
| `E2EPlanEnricher` | `packages/agent-core/src/e2e-testing/plan-enricher.ts` (modified) | Extend fallback file-path regex to include `.py` files. |

### Data-Flow Arrows

1. `RunE2ETestsTool.derivePackageRoot(changedFiles)` → `projectRoot: string`
2. `registry.detectAndGet(projectRoot)` → `generator: E2ETestGenerator`
3. `generator.detectProjectStructure(root)` → `ProjectStructure | null`
4. `generator.analyzeImpact(changedFiles, config)` → `ImpactAnalysisResult`
5. `RecursiveImpactAnalyzer.analyze(changedFiles, projectRoot, language)` → `string[]` (transitively affected files)
6. `generator.generateTestsForFeature(feature, outputDir)` → `TestFile[]`
7. `E2ETestResultCache.get(key)` → `E2EExecutionResult | null`
8. `E2ETestExecutor.execute(testFiles, projectRoot)` → `E2EExecutionResult`
9. `E2ETestResultCache.set(key, result)` → `void`
10. `RunE2ETestsTool` → markdown summary returned to plan executor

---

## Prior Art

Phase 2 is an internal extension of the existing E2E framework rather than a port of an external system. The reference implementation inside the repo is the Go generator (`packages/agent-core/src/e2e-testing/generators/go.ts`), which establishes the pattern:

- Detect project structure by inspecting manifest files (`go.mod`).
- Classify into a small set of kinds (`http-server`, `cli`, `generic`).
- Render language-specific templates with `replaceAll` placeholders.
- Run tests via subprocess and parse structured output (`go test -json`).
- Normalize into the shared `TestSuiteResult` model.

Existing open-source tools (e.g., Jest `--json`, pytest `pytest-json-report`) provide the output formats we consume, but the orchestration, registry, and normalization layers are repo-specific and are reused rather than replaced. [C:INFERRED]

---

## Reuse Analysis

| File / Module | What It Provides | Reuse Verdict |
|---------------|------------------|---------------|
| `packages/agent-core/src/e2e-testing/types.ts` | `E2ETestGenerator`, `ProjectStructure`, `Feature`, `TestFile`, `TestSuiteResult`, `E2EExecutionResult`, `RunContext`. | **Use as-is.** The interface already pushes `analyzeImpact`, `generateTestsForFeature`, `resolveGeneratedTestDir`, and `runTests` into generators. [C:UPSTREAM-ish / existing code] |
| `packages/agent-core/src/e2e-testing/registry.ts` | `E2EGeneratorRegistry` with `register` / `detectAndGet`. | **Adapt.** Register new generators; adjust detection order so Node/Jest wins over TS/Vitest when Jest is configured. |
| `packages/agent-core/src/e2e-testing/executor.ts` | `E2ETestExecutor`: writes files, delegates run, aggregates, writes report. | **Adapt.** Insert cache lookup before run and cache store after run. |
| `packages/agent-core/src/e2e-testing/generators/go.ts` | Pattern for detection, classification, templating, subprocess run, JSON parsing. | **Reference pattern.** Python/Node generators mirror this structure but implement their own templates and parsers. |
| `packages/agent-core/src/e2e-testing/config.ts` + `config/schema.ts` | `[e2e]` config schema and resolver. | **Adapt.** Add cache/recursive-analysis fields to schema. |
| `packages/agent-core/src/e2e-testing/plan-enricher.ts` | Appends E2E task to plan; parses changed files. | **Adapt.** Extend file regex to include `.py`. |
| `packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.ts` | `RunE2ETestsTool` orchestrates the whole flow. | **Use as-is.** No changes needed for Phase 2 features. |
| `packages/agent-core/src/e2e-testing/impact-analyzer.ts` + `impact-map.ts` | Static file→tool mapping for ody-code. | **Keep as fallback.** Recursive analyzer replaces it for user projects; ody-code self-testing keeps static map. |

**Greenfield components**: `PythonPytestGenerator`, `NodejsJestGenerator`, `RecursiveImpactAnalyzer`, `E2ETestResultCache` — no existing module solves these specific problems. [C:INFERRED]

---

## Cross-Cutting Interfaces (Index-Only Contracts)

```typescript
// Cache configuration added to ResolvedE2EConfig
interface E2ECacheConfig {
  cacheEnabled: boolean;       // default true
  cacheDir: string;            // default '.ody-code/e2e-cache'
  cacheTtlDays: number;        // default 7
  cacheMaxEntries: number;     // default 20
}

// Recursive analysis configuration
interface E2ERecursiveAnalysisConfig {
  recursiveAnalysisEnabled: boolean; // default true
  maxDepth: number;                  // default 3
}

// Public API of the cache layer
interface E2ETestResultCache {
  get(key: CacheKey): Promise<E2EExecutionResult | null>;
  set(key: CacheKey, result: E2EExecutionResult): Promise<void>;
  prune(): Promise<void>; // enforce TTL + max entries
}

type CacheKey = string; // hex(SHA256(sorted changed files + content hash))

// Public API of the recursive analyzer
interface RecursiveImpactAnalyzer {
  analyze(
    changedFiles: string[],
    projectRoot: string,
    language: 'typescript' | 'go' | 'python' | 'nodejs',
    options?: { maxDepth?: number; excludePatterns?: string[] },
  ): string[];
}
```

---

## Data Models

This section summarizes the new cross-cutting data structures. Per-language and per-component details live in the part files.

| Type / Interface | Purpose | Defined In |
|------------------|---------|------------|
| `PythonDetection` `{ kind, framework, entry }` | Internal classification result for Python projects. | Part 1 |
| `NodejsDetection` `{ kind, framework, entry, packageManager }` | Internal classification result for Node.js projects. | Part 2 |
| `ImportGraph` `{ forward, reverse }` | Bidirectional file→dependency maps for recursive impact analysis. | Part 3 |
| `LanguageParser` `{ extensions, extractImports, resolveImport }` | Pluggable parser contract for TS/Go/Python/Node. | Part 3 |
| `CacheEntry` `{ createdAt, key, result }` | On-disk JSON schema for cached `E2EExecutionResult`. | Part 4 |
| `E2ETestResultCache` `{ get, set, prune }` | Cache orchestrator used by the executor. | Part 4 |
| `E2EConfigSchema` additions | `cacheEnabled`, `cacheDir`, `cacheTtlDays`, `cacheMaxEntries`, `recursiveAnalysisEnabled`, `maxRecursiveDepth`. | Parts 1, 3, 4 |

---

## Algorithms

The following high-level algorithms orchestrate the four parts. Concrete pseudocode for each step is in the corresponding part file.

### AG1. Multi-Language E2E Generation Flow

```
RunE2ETestsTool(projectRoot, changedFiles)
  config = E2EConfigResolver.resolve(odyConfig)
  generator = registry.detectAndGet(projectRoot)
  affectedFiles = config.recursiveAnalysisEnabled
    ? RecursiveImpactAnalyzer.analyze(changedFiles, projectRoot, generator.language)
    : changedFiles
  impact = generator.analyzeImpact(affectedFiles, config)
  testFiles = []
  for feature in impact.affectedTools
    testFiles.push(...await generator.generateTestsForFeature(feature, outputDir))
  result = await E2ETestExecutor.execute(testFiles, projectRoot, { changedFiles, signal })
  return result.summary
```

### AG2. Registry Detection Order

```
registry.register(TypeScriptVitestGenerator()) // ody-code self-testing
registry.register(NodejsJestGenerator())       // package.json + jest
registry.register(PythonPytestGenerator())     // pyproject.toml / requirements.txt
registry.register(GoGenerator())               // go.mod
```

### AG3. Recursive Impact Traversal

```
for each source file in project
  imports = parser.extractImports(content)
  for spec in imports
    target = parser.resolveImport(spec, file, projectRoot)
    if target is in-project
      reverse[target].add(file)

affected = changedFiles
frontier = changedFiles
for depth in 0..maxDepth-1
  next = []
  for file in frontier
    for dependent in reverse[file]
      if dependent not in affected
        affected.add(dependent)
        next.push(dependent)
  frontier = next
return affected
```

### AG4. Cache Short-Circuit

```
key = computeCacheKey(changedFiles, testFiles)
cached = await cache.get(key)
if cached !== null
  return cached
result = await runTestsAndWriteReport(testFiles)
await cache.set(key, result)
return result
```

---

## Error Handling

Cross-cutting error scenarios and their handling:

| Scenario | Handling | Degradation |
|----------|----------|-------------|
| No generator matches project | `E2ENoMatchingGeneratorError` thrown; `RunE2ETestsTool` skips E2E task | Plan continues without E2E |
| Generator detects framework but entry missing | Generated test uses fallback placeholder + TODO | Test passes; user customizes |
| Recursive analysis over-reads dependencies | `maxDepth=3` + `strategy=critical-only` cap | May miss some transitive impact |
| Cache directory unwritable | Cache errors swallowed | Execution still works, no speedup |
| Subprocess leaves orphan server | Generated templates use fixture teardown + `SIGKILL` fallback | CI timeout as backstop |
| Cache hit returns stale result due to env change | Documented limitation; `cacheEnabled=false` escape hatch | User can disable cache |

Per-generator and per-algorithm error tables are in Parts 1–4.

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if Wrong | How to Verify |
|---|------------|------------|-----------------|---------------|
| 1 | `pytest` and `jest` are installed in the target project when their respective generator is selected. | Medium | Generated tests fail to run; fallback to `failurePolicy=warn/ignore` still works. | Detection step should also verify CLI availability; design includes graceful degradation. |
| 2 | A simple regex/line parser for imports is sufficient for recursive impact analysis in Phase 2. | Medium | False positives/negatives in affected-tool detection; can be upgraded to AST parsing later. | Unit tests with real-world import patterns; measure false-positive rate in dog-fooding. |
| 3 | `.ody-code/e2e-cache/` is writable and gitignored in user projects. | Medium | Cache writes fail silently; E2E still works but no speedup. | Check `.gitignore` template; executor should swallow cache I/O errors. |
| 4 | Node/Jest and TS/Vitest projects can be disambiguated by checking `devDependencies` for `jest` vs `vitest`. | High | Wrong generator selected for mixed projects. | Verify detection order in registry and add tests. |
| 5 | Python projects use a single interpreter (`python3` or `python`) available in PATH. | Medium | Subprocess spawn fails if project expects a venv. | Generated test can try `python3` first, then `python`; design defers venv auto-detection. |
| 6 | The existing `E2ETestGenerator` interface does not need new methods for Phase 2. | High | Would require broader refactor. | Already verified in code: interface supports detection, impact, generation, run. |
| 7 | Cache invalidation by content hash is safe: any code change that affects behavior also changes at least one changed file or generated test. | Medium | Could reuse stale result if behavior depends on external state (env vars, DB). | Document limitation; cache key can be extended with env snapshot in future. |

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|------------|--------|------------|
| 1 | Detection picks wrong generator (e.g., Jest project misidentified as Vitest). | Medium | Wrong test framework / no tests generated. | Order registry by specificity; add `testTool` priority; unit-test detection fixtures. |
| 2 | Generated Python/Node templates are too generic to catch real bugs. | Medium | Low test value, user ignores failures. | Templates include concrete black-box assertions; provide TODO comments for customization. |
| 3 | Recursive impact analysis produces noisy/overbroad affected sets. | Medium | Too many E2E tests run, execution time exceeds 15s target. | Cap `maxDepth` at 3; respect `strategy=critical-only`; allow exclusion patterns. |
| 4 | Cache hides real regressions due to incomplete key (e.g., env changes). | Low | False confidence, bugs slip through. | Cache key includes changed files + generated content; document env-var limitation; `cacheEnabled=false` escape hatch. |
| 5 | New generators break existing TS/Vitest or Go flows. | Low | Regression in Phase 1 capability. | Keep existing generators unchanged except registry registration; run full existing test suite. |
| 6 | Subprocess execution of user apps leaves orphan servers. | Medium | Resource leaks in CI/local runs. | Generated tests use `pytest` fixtures / Jest `afterAll` to kill spawned processes; set timeouts. |

---

## Parts

| # | File | Scope | Status |
|---|------|-------|--------|
| 1 | `2026-06-18-e2e-testing-automation-phase-2/python-pytest.md` | Python/Pytest generator: detection, classification, templates, run, parse, tests. | done |
| 2 | `2026-06-18-e2e-testing-automation-phase-2/nodejs-jest.md` | Node.js/Jest generator: detection, classification, templates, run, parse, tests. | done |
| 3 | `2026-06-18-e2e-testing-automation-phase-2/recursive-impact-analysis.md` | Recursive dependency traversal: algorithms, language-specific parsers, integration. | done |
| 4 | `2026-06-18-e2e-testing-automation-phase-2/test-result-cache.md` | Cache layer: key computation, storage format, TTL, executor integration, tests. | done |

---

## Self-Review

### Expensive Decisions — Adversarial Verification

#### D1. Cache key determinism (`computeCacheKey`)

| # | Input | Expected Output | Verified |
|---|-------|-----------------|----------|
| 1 | `changed=['src/a.py','src/b.py']`, one test file | key K1 | Yes (`node -e`) |
| 2 | Same files reordered `['src/b.py','src/a.py']` | key K1 (same) | Yes |
| 3 | Backslash paths `['src\\a.py','src\\b.py']` | key K1 (same) | Yes |
| 4 | Different content in test file | key ≠ K1 | Yes |

Finding: key is stable across ordering and path-separator differences, and sensitive to content changes. No fix needed.

#### D2. Python dependency-name extraction regex

| # | Input Line | Expected Match | Verified |
|---|------------|----------------|----------|
| 1 | `fastapi = "^0.100"` | `fastapi` | Yes |
| 2 | `  fastapi = "^0.100"` (leading spaces) | `fastapi` | Yes (fixed regex to `^\s*`) |
| 3 | `# fastapi` | no match | Yes |
| 4 | `flask==2.0` | `flask` | Yes |

Finding: original regex missed leading whitespace; fixed to `/^\s*[\"']?([a-zA-Z0-9_-]+)/`. Verified with `node -e`.

#### D3. Recursive import parsers

| Parser | Input | Expected Imports | Verified |
|--------|-------|------------------|----------|
| TS/Node | `import { x } from './foo'; require('./bar'); import('./baz')` | `./foo`, `./bar`, `./baz` | Yes |
| Go | `import "fmt"; import ("os"; mylib "example.com/foo")` | `fmt`, `os`, `example.com/foo` | Yes |
| Python | `import os; import foo.bar; from . import baz` | `os`, `foo.bar`, `.` | Yes |

Finding: all three regex-based parsers capture relative/project imports; third-party imports are intentionally left unresolved. No fix needed.

### Four-Lens Sweep

- **Security**: Checked all regexes for false positives/negatives and PII leakage. Cache path is project-local (`.ody-code/e2e-cache/`). Generated test templates include TODO placeholders, not secrets. No PII flows into cache keys. Risk register notes orphan-process mitigation.
- **Test**: Every behavior in the test plans has must-pass and must-reject cases (e.g., cache hit vs miss, detection match vs no-match, parser match vs comment exclusion). Fixed the Python regex after a must-survive case (leading whitespace) was initially rejected.
- **Ops**: Cache TTL + max-entry limits prevent unbounded growth. Recursive analysis caps `maxDepth` at 3 and respects exclude patterns. Executor chunking stays generator-local to avoid concurrency surprises. Identifiers (cache keys) are 64-char SHA-256, collision-resistant.
- **Integration**: Verified existing hooks exist: `E2ETestGenerator` interface, `E2EGeneratorRegistry`, `E2ETestExecutor`, `E2EConfigSchema`, `E2EPlanEnricher`, and `RunE2ETestsTool` all exist in the paths cited in the parts. Design lands at the Phase 2 location inferred from the Phase 1 design doc pattern (`.ody-code/designs/2026-06-18-e2e-testing-automation-phase-2.md`).
- **Scope**: Still one coherent Phase 2 design with four well-bounded parts; no need for further decomposition.

---

## User Final Approval

Status: **Approved** — all [C:INFERRED] assumptions accepted after Deep audit gate sign-off; design ready for implementation planning.
