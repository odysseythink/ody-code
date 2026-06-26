# Part 2 — Node.js/Jest E2E Generator

## Scope

### In Scope

- Detect Node.js projects via `package.json`. [C:USER]
- Classify projects into `express`, `nestjs`, `nextjs`, or `generic`. [C:USER]
- Find a plausible application entry point for the detected framework. [C:INFERRED]
- Generate `jest` black-box E2E tests that spawn the running app/script and assert on external behavior. [C:USER]
- Run generated tests via `jest` and parse `--json` output into `TestSuiteResult[]`. [C:USER]
- Register the generator in `registry.ts` before `PythonPytestGenerator` and `GoGenerator`. [C:INFERRED]

### Out of Scope

- `vitest` projects (handled by existing `TypeScriptVitestGenerator`). [C:INFERRED]
- Frontend React component tests for Next.js (only API routes / server-side black-box). [C:DEFERRED]
- `npm`/`yarn`/`pnpm` workspace detection inside the generator. [C:DEFERRED]
- TypeScript compilation orchestration (assumes target project can run `jest`). [C:DEFERRED]

---

## Interfaces & Types

```typescript
type NodejsKind = 'express' | 'nestjs' | 'nextjs' | 'generic';

interface NodejsDetection {
  kind: NodejsKind;
  framework: string; // 'express' | 'nestjs' | 'nextjs' | 'generic'
  /**
   * - express: path to file exporting the Express app, e.g. "src/app.js"
   * - nestjs: path to compiled entry or src/main.ts, e.g. "dist/main.js"
   * - nextjs: project root (Next.js convention), e.g. "."
   * - generic: path to runnable script, e.g. "index.js" or ""
   */
  entry: string;
  /**
   * Preferred package manager invocation prefix.
   * Determined by lockfile presence: 'pnpm', 'yarn', 'npm'.
   */
  packageManager: 'pnpm' | 'yarn' | 'npm';
}

class NodejsJestGenerator {
  readonly id = 'nodejs-jest';

  detectProjectStructure(root: string): Promise<ProjectStructure | null>;
  analyzeImpact(changedFiles: string[], config: ResolvedE2EConfig): ImpactAnalysisResult;
  generateTestsForFeature(feature: Feature, _outputDir: string): Promise<TestFile[]>;
  resolveGeneratedTestDir(config: ResolvedE2EConfig): string;
  runTests(absoluteTestPaths: string[], ctx: RunContext): Promise<TestSuiteResult[]>;
}

// Jest --json output shape (subset)
interface JestJsonOutput {
  testResults?: Array<{
    name: string; // absolute path
    status: 'passed' | 'failed';
    message?: string;
    assertionResults?: Array<{
      title: string;
      status: 'passed' | 'failed' | 'pending';
      failureMessages?: string[];
      duration?: number;
    }>;
  }>;
}
```

---

## Algorithms

### B1. detectProjectStructure(root)

```
function detectProjectStructure(root: string): Promise<ProjectStructure | null>
  if package.json does not exist at root
    return null

  pkg = readJson(join(root, 'package.json'))
  deps = merge(pkg.dependencies ?? {}, pkg.devDependencies ?? {})

  // Must have jest configured or installed to use this generator
  if !('jest' in deps or existsJestConfig(root))
    return null

  detection = classify(root, deps)
  if detection.kind === 'generic' and no .js/.ts/.mjs/.cjs files under root
    return null

  return {
    language: 'nodejs',
    framework: detection.framework,
    testTool: 'jest',
    root,
  }
```

### B2. existsJestConfig(root)

```
function existsJestConfig(root): boolean
  filenames = [
    'jest.config.js', 'jest.config.ts', 'jest.config.mjs',
    'jest.config.cjs', 'jest.config.json',
  ]
  for name in filenames
    if exists(join(root, name)) return true
  // Also accept "jest" key inside package.json
  return pkg.jest !== undefined
```

### B3. classify(root, deps)

```
function classify(root, deps): NodejsDetection
  pm = detectPackageManager(root) // see B4

  if deps contains 'next'
    return { kind: 'nextjs', framework: 'nextjs', entry: '.', packageManager: pm }
  if deps contains '@nestjs/core' or '@nestjs/common'
    return { kind: 'nestjs', framework: 'nestjs', entry: findNestJsEntry(root), packageManager: pm }
  if deps contains 'express'
    return { kind: 'express', framework: 'express', entry: findExpressEntry(root), packageManager: pm }

  return { kind: 'generic', framework: 'generic', entry: findGenericEntry(root), packageManager: pm }
```

### B4. detectPackageManager(root)

```
function detectPackageManager(root): 'pnpm' | 'yarn' | 'npm'
  if exists(join(root, 'pnpm-lock.yaml')) return 'pnpm'
  if exists(join(root, 'yarn.lock')) return 'yarn'
  if exists(join(root, 'package-lock.json')) return 'npm'
  return 'npm' // default
```

### B5. findFrameworkEntry helpers

```
function findExpressEntry(root): string
  files = listSourceFiles(root, 300)
  for file in files
    content = readFile(file)
    if content matches /(?:const|let|var)\s+\w+\s*=\s*express\s*\(/ OR /app\.listen\s*\(/
      return relativePath(root, file)
  return "src/app.js" // fallback placeholder

function findNestJsEntry(root): string
  // Prefer compiled output if present; otherwise source entry.
  if exists(join(root, 'dist/main.js')) return 'dist/main.js'
  if exists(join(root, 'dist/main.ts')) return 'dist/main.ts'
  if exists(join(root, 'src/main.ts')) return 'src/main.ts'
  if exists(join(root, 'src/main.js')) return 'src/main.js'
  return "src/main.ts" // fallback placeholder

function findGenericEntry(root): string
  files = listSourceFiles(root, 300)
  candidates = files.filter(f => isTopLevel(f) && !isTestFile(f) && basename(f).match(/^(index|main)\./))
  if candidates.length >= 1 return relativePath(root, candidates[0])
  return ""
```

### B6. generateTestsForFeature(feature, _outputDir)

```
function generateTestsForFeature(feature, _outputDir): Promise<TestFile[]>
  pkg = readJson(join(feature.projectRoot, 'package.json'))
  deps = merge(pkg.dependencies ?? {}, pkg.devDependencies ?? {})
  detection = classify(feature.projectRoot, deps)
  ident = camelIdent(feature.toolId)
  relativePath = `__tests__/${ident}.e2e.test.js`
  content = renderTemplate(detection.kind, ident, feature, detection.entry, detection.packageManager)
  return [{ relativePath, content }]
```

### B7. renderTemplate(kind, ...)

```
function renderTemplate(kind, ident, feature, entry, packageManager)
  switch kind
    case 'express': return renderExpressTemplate(ident, feature.projectRoot, entry, packageManager)
    case 'nestjs':  return renderNestJsTemplate(ident, feature.projectRoot, entry, packageManager)
    case 'nextjs':  return renderNextJsTemplate(ident, feature.projectRoot, packageManager)
    default:        return renderGenericTemplate(ident, feature.projectRoot, entry, packageManager)
```

### B8. runTests(absoluteTestPaths, ctx)

```
function runTests(absoluteTestPaths, ctx): Promise<TestSuiteResult[]>
  if absoluteTestPaths.length === 0 return []

  { kaos, config, projectRoot, signal } = ctx
  outputFile = join(generatedTestDir, `jest-report-${timestamp()}.json`)

  args = [
    '--json', '--outputFile=' + outputFile,
    '--testTimeout=' + config.testTimeout,
    '--runInBand', // sequential inside jest; executor handles cross-file concurrency
    ...absoluteTestPaths,
  ]

  pm = detectPackageManager(projectRoot)
  proc = await kaos.exec(pm, 'exec', 'jest', ...args)
  attachAbort(signal, proc)
  await proc.wait()

  if exists(outputFile)
    json = parseJson(readFile(outputFile))
    return parseJestJson(json)

  // Fallback: jest produced no JSON (very unusual)
  stderr = readStderr(proc)
  return [{
    file: absoluteTestPaths[0],
    status: 'failed',
    duration: 0,
    tests: [{
      name: 'jest failed to produce JSON report',
      status: 'failed',
      failureMessages: [stderr.slice(0, 2000)],
    }],
  }]
```

### B9. parseJestJson(output)

```
function parseJestJson(output: JestJsonOutput): TestSuiteResult[]
  suites = []
  for result in (output.testResults ?? [])
    suiteStatus = result.status === 'passed' ? 'passed' : 'failed'
    tests = []
    for assertion in (result.assertionResults ?? [])
      status = assertion.status === 'passed' ? 'passed'
             : assertion.status === 'pending' ? 'skipped'
             : 'failed'
      if status === 'failed' suiteStatus = 'failed'
      tests.push({
        name: assertion.title,
        status,
        failureMessages: assertion.failureMessages ?? [],
      })

    // If jest reported a suite-level failure with no assertions, surface it.
    if tests.length === 0 and result.message
      tests.push({
        name: 'suite setup',
        status: 'failed',
        failureMessages: [result.message.slice(0, 2000)],
      })
      suiteStatus = 'failed'

    suites.push({
      file: result.name,
      status: suiteStatus,
      duration: sum(assertion.duration for assertion in result.assertionResults ?? []),
      tests,
    })
  return suites
```

### B10. resolveGeneratedTestDir(config)

```
function resolveGeneratedTestDir(config): string
  return config.generatedTestDir
```

### B11. analyzeImpact(changedFiles, config)

```
function analyzeImpact(changedFiles, config): ImpactAnalysisResult
  packages = new Set<string>()
  for file in changedFiles
    normalized = file.replace(/\\/g, '/')
    if !isSourceFile(normalized) or isTestFile(normalized)
      continue
    slash = normalized.lastIndexOf('/')
    pkg = slash === -1 ? '.' : normalized.slice(0, slash)
    packages.add(pkg)

  affected = []
  for pkg in packages
    priority = config.criticalTools.includes(pkg) ? 'critical' : 'important'
    if config.strategy === 'critical-only' and priority !== 'critical'
      continue
    affected.push({ toolId: pkg, priority })

  if affected.length === 0 and config.strategy === 'always'
    affected.push({ toolId: 'general', priority: 'nice-to-have' })

  return { affectedTools: affected }
```

where `isSourceFile` matches `.js`, `.jsx`, `.ts`, `.tsx`, `.mjs`, `.cjs`. [C:INFERRED]

---

## Generated Test Templates (Illustrative)

### Express Template

- Uses a free TCP port.
- Imports the Express app from `<entry>` and calls `app.listen(port)`.
- Alternatively spawns `node <entry>` if the app is not exportable.
- Waits for readiness.
- Calls `http://127.0.0.1:<port>/` and asserts status 200.
- Closes the server in `afterAll`.
- Contains TODO comments for path/assertion customization.

### NestJS Template

- If `entry` points to a compiled `dist/main.js`, spawns `node <entry>`.
- If `entry` points to `src/main.ts`, uses `ts-node` if available, otherwise documents TODO.
- Waits for readiness, asserts on HTTP response, tears down process.

### Next.js Template

- Spawns `<packageManager> next dev --port <port>`.
- Calls an API route (default `/api/hello` or `/api/health`) and asserts status 200.
- Kills the dev server in `afterAll`.

### Generic Template

- If `entry` exists, spawns `node <entry>` and asserts `exitCode === 0` or stdout contains expected string.
- If no entry found, placeholder test with TODO.

---

## Call-Site Integration

### 1. Register the generator

**File**: `packages/agent-core/src/e2e-testing/registry.ts`  
**Around line**: 20–24

```typescript
import { NodejsJestGenerator } from './generators/nodejs-jest';
import { PythonPytestGenerator } from './generators/python-pytest';

export const registry = new E2EGeneratorRegistry();
registry.register(new TypeScriptVitestGenerator());
registry.register(new NodejsJestGenerator());
registry.register(new PythonPytestGenerator());
registry.register(new GoGenerator());
```

> Node/Jest is registered before Python and Go because it competes with TypeScript/Vitest for `package.json` projects. [C:INFERRED]

### 2. Config schema additions

See Part 1, call-site 3. The same fields serve Part 2.

---

## Error Handling & Degradation

| Error Class | Immediate Handling | Degradation Path | Recovery |
|-------------|-------------------|------------------|----------|
| `package.json` missing | `detectProjectStructure` returns `null` | E2E skipped | N/A |
| `jest` not installed/configured | Returns `null` | Falls through to `TypeScriptVitestGenerator` or no generator | User installs jest |
| Framework detected but entry missing | Fallback placeholder entry | Generated test passes with TODO | User edits entry / template |
| `ts-node` required but missing (NestJS source) | Generated test fails with clear message | User installs ts-node or builds first | User updates build step |
| Server fails to start | Test failure with stderr | Surfaces via `failurePolicy` | User fixes app startup |
| Orphan server process | `afterAll`/`finally` sends `SIGTERM`, then `SIGKILL` | OS/CI timeout reaps it | Template includes cleanup |
| Jest JSON report missing | Fallback to single-suite failed result | Coarse result, no per-test detail | Re-run with `--json` debug |

---

## Test Plan

### Unit Tests — `packages/agent-core/test/e2e-testing/nodejs-jest-generator.test.ts`

1. **Detection**
   - Returns `null` when no `package.json` exists.
   - Returns `null` when `package.json` exists but no jest config/deps.
   - Detects Express project with jest.
   - Detects NestJS project.
   - Detects Next.js project.
   - Detects generic Node project with jest config key.

2. **Classification / Entry**
   - Express entry resolves to file containing `const app = express()`.
   - NestJS entry prefers `dist/main.js` over `src/main.ts`.
   - Next.js entry is `"."`.
   - Package manager resolves from lockfile (`pnpm-lock.yaml`, `yarn.lock`, `package-lock.json`).

3. **Template Generation**
   - Express template imports/spawns the app and calls an HTTP endpoint.
   - NestJS template references the entry module.
   - Next.js template uses `<pm> next dev --port <port>`.
   - Generic template spawns `node <entry>` when entry exists.
   - All templates include `AUTO-GENERATED` marker and TODO.

4. **Run / Parse**
   - `runTests` invokes `<pm> exec jest --json --outputFile=...`.
   - `parseJestJson` maps a passing assertion to `status: 'passed'`.
   - `parseJestJson` maps a failing assertion to `status: 'failed'` with `failureMessages`.
   - Pending assertion maps to `status: 'skipped'`.
   - Empty test path list returns `[]`.

5. **Impact Analysis**
   - Changed `.ts` / `.js` files map to their directory.
   - Test files (`*.test.js`, `*.spec.ts`) are excluded.
   - `strategy='critical-only'` filters non-critical packages.

### End-to-End Test

- Create a temporary Express project with `app.js`, `package.json`, and jest installed.
- Run `E2ETestExecutor.execute(...)`.
- Assert `result.passed >= 1`, `result.failed === 0`.
- Assert JSON report written.

### Done Criteria

```bash
pnpm test packages/agent-core/test/e2e-testing/nodejs-jest-generator.test.ts
pnpm test packages/agent-core/test/e2e-testing/executor.test.ts
pnpm test packages/agent-core/test/e2e-testing/integration.test.ts
pnpm exec tsc --noEmit -p packages/agent-core/tsconfig.json
```

---

## Local Assumptions

| # | Assumption | Confidence | Impact if Wrong |
|---|------------|------------|-----------------|
| L1 | Node.js projects use the package manager whose lockfile is present (`pnpm`/`yarn`/`npm`). | High | Wrong CLI prefix could fail; fallback to `npm`. |
| L2 | Jest projects already have a runnable jest setup (config or dependency). | High | Detection returns null; no harm. |
| L3 | Express/NestJS/Next.js apps can be launched in dev mode on a random port without extra environment. | Medium | Generated tests need manual TODO adjustment. |
| L4 | `--runInBand` is acceptable for generated E2E tests to avoid port collisions inside one jest run. | High | Can be changed to parallel if projects support it. |
