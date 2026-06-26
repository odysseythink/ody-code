# Part 1 — Python/Pytest E2E Generator

## Scope

### In Scope

- Detect Python projects via `pyproject.toml`, `requirements.txt`, or `setup.py`. [C:USER]
- Classify projects into `fastapi`, `flask`, `django`, or `generic`. [C:USER]
- Find a plausible application entry point for the detected framework. [C:INFERRED]
- Generate `pytest` black-box E2E tests that spawn the running app/script and assert on external behavior. [C:USER]
- Run generated tests via `pytest` and parse results into `TestSuiteResult[]`. [C:USER]
- Register the generator in `registry.ts`. [C:INFERRED]

### Out of Scope

- Virtual-environment auto-detection / activation. [C:DEFERRED]
- Framework-specific authentication, database setup, or migrations. [C:DEFERRED]
- `pytest-xdist` parallel execution inside the generator. [C:DEFERRED]
- Non-HTTP Django behaviors (e.g., management commands). [C:DEFERRED]

---

## Interfaces & Types

```typescript
// Detection result internal to the generator
type PythonKind = 'fastapi' | 'flask' | 'django' | 'generic';

interface PythonDetection {
  kind: PythonKind;
  framework: string; // e.g. 'fastapi', 'flask', 'django', 'generic'
  /** Entry module / script for launching the app.
   *  - fastapi: module name exposing the `app` object, e.g. "main"
   *  - flask: module name exposing the `app` object, e.g. "app"
   *  - django: path to manage.py, e.g. "manage.py"
   *  - generic: path to entry script, e.g. "src/main.py" or ""
   */
  entry: string;
}

// Public generator class (implements E2ETestGenerator from ../types.ts)
class PythonPytestGenerator {
  readonly id = 'python-pytest';

  detectProjectStructure(root: string): Promise<ProjectStructure | null>;
  analyzeImpact(changedFiles: string[], config: ResolvedE2EConfig): ImpactAnalysisResult;
  generateTestsForFeature(feature: Feature, _outputDir: string): Promise<TestFile[]>;
  resolveGeneratedTestDir(config: ResolvedE2EConfig): string;
  runTests(absoluteTestPaths: string[], ctx: RunContext): Promise<TestSuiteResult[]>;
}

// Pytest JSON-report shape (subset we consume)
interface PytestJsonReport {
  summary?: {
    passed?: number;
    failed?: number;
    skipped?: number;
    duration?: number;
  };
  tests?: Array<{
    nodeid: string;
    outcome: 'passed' | 'failed' | 'skipped';
    setup?: { outcome?: string; longrepr?: string };
    call?: { outcome?: string; longrepr?: string };
    teardown?: { outcome?: string; longrepr?: string };
    duration?: number;
  }>;
}
```

---

## Algorithms

### A1. detectProjectStructure(root)

```
function detectProjectStructure(root: string): Promise<ProjectStructure | null>
  if none of (pyproject.toml, requirements.txt, setup.py) exist
    return null

  manifest = readManifest(root) // first available of pyproject.toml, requirements.txt, setup.py
  detection = classify(root, manifest)
  if detection.kind === 'generic' and no .py files under root
    return null

  return {
    language: 'python',
    framework: detection.framework,
    testTool: 'pytest',
    root,
  }
```

### A2. classify(root, manifest)

```
function classify(root, manifest): PythonDetection
  deps = extractDependencyNames(manifest)

  if deps contains 'fastapi'
    return { kind: 'fastapi', framework: 'fastapi', entry: findFastApiEntry(root) }
  if deps contains 'flask'
    return { kind: 'flask', framework: 'flask', entry: findFlaskEntry(root) }
  if deps contains 'django'
    return { kind: 'django', framework: 'django', entry: findDjangoEntry(root) }

  return { kind: 'generic', framework: 'generic', entry: findGenericEntry(root) }
```

Dependency extraction rules:

```
function extractDependencyNames(manifest): string[]
  names = []
  for each line in manifest
    // pyproject.toml [project]dependencies or [tool.poetry.dependencies]
    // example: "fastapi = \"^0.100\"" or "fastapi>=0.100"
    match = line.match(/^\s*[\"']?([a-zA-Z0-9_-]+)/)
    if match
      names.push(match[1].toLowerCase())
  return names
```

> Verified with `node -e`: the leading-name regex correctly captures `fastapi`, `"fastapi`, and `'fastapi` forms while ignoring version operators. [C:INFERRED]

### A3. findFrameworkEntry helpers

```
function findFastApiEntry(root): string
  files = listPythonFiles(root, 300)
  for file in files
    content = readFile(file)
    if content matches /app\s*=\s*FastAPI\s*\(/
      return moduleName(root, file) // e.g. "main" for root/main.py
  return "main" // fallback placeholder

function findFlaskEntry(root): string
  files = listPythonFiles(root, 300)
  for file in files
    content = readFile(file)
    if content matches /app\s*=\s*Flask\s*\(/
      return moduleName(root, file)
  return "app" // fallback placeholder

function findDjangoEntry(root): string
  if exists(root + "/manage.py") return "manage.py"
  return "" // fallback

function findGenericEntry(root): string
  // Prefer a single top-level .py file that is not __init__.py or a test file.
  files = listPythonFiles(root, 300)
  candidates = files.filter(f => isTopLevel(f) && !isTestFile(f) && !endsWith('__init__.py'))
  if candidates.length === 1 return candidates[0]
  return ""
```

### A4. generateTestsForFeature(feature, _outputDir)

```
function generateTestsForFeature(feature, _outputDir): Promise<TestFile[]>
  detection = classify(feature.projectRoot, readManifest(feature.projectRoot))
  ident = pythonIdent(feature.toolId)
  relativePath = `${ident}_e2e_test.py`
  content = renderTemplate(detection.kind, ident, feature, detection.entry)
  return [{ relativePath, content }]
```

Template selection:

```
function renderTemplate(kind, ident, feature, entry)
  switch kind
    case 'fastapi': return renderFastApiTemplate(ident, feature.projectRoot, entry)
    case 'flask':   return renderFlaskTemplate(ident, feature.projectRoot, entry)
    case 'django':  return renderDjangoTemplate(ident, feature.projectRoot, entry)
    default:        return renderGenericTemplate(ident, feature.projectRoot, entry)
```

### A5. runTests(absoluteTestPaths, ctx)

```
function runTests(absoluteTestPaths, ctx): Promise<TestSuiteResult[]>
  if absoluteTestPaths.length === 0 return []

  { kaos, config, projectRoot, signal } = ctx
  reportFile = join(generatedTestDir, `pytest-report-${timestamp()}.json`)

  // 1. Attempt structured JSON report
  args = [
    '-m', 'pytest',
    '--json-report', '--json-report-file=' + reportFile,
    '-q', '--tb=short',
    ...absoluteTestPaths,
  ]
  proc = await kaos.exec('python3', ...args) // fallback to 'python' if python3 missing
  attachAbort(signal, proc)
  await proc.wait()

  if exists(reportFile)
    json = parseJson(readFile(reportFile))
    return parsePytestJsonReport(json)

  // 2. Fallback: pytest-json-report unavailable, use exit code only
  return [{
    file: absoluteTestPaths[0],
    status: proc.exitCode === 0 ? 'passed' : 'failed',
    duration: 0,
    tests: [{
      name: 'pytest suite',
      status: proc.exitCode === 0 ? 'passed' : 'failed',
      failureMessages: proc.exitCode === 0 ? [] : [stderr.slice(0, 2000)],
    }],
  }]
```

### A6. parsePytestJsonReport(report)

```
function parsePytestJsonReport(report: PytestJsonReport): TestSuiteResult[]
  suiteMap = Map<string, TestSuiteResult>() // key = file path

  for test in (report.tests ?? [])
    file = test.nodeid.split('::')[0]
    suite = suiteMap.get(file) ?? { file, status: 'passed', duration: 0, tests: [] }
    outcome = test.outcome === 'passed' ? 'passed'
            : test.outcome === 'skipped' ? 'skipped'
            : 'failed'
    if outcome === 'failed' suite.status = 'failed'

    failureMessages = []
    for phase in [test.setup, test.call, test.teardown]
      if phase?.outcome === 'failed' and phase.longrepr
        failureMessages.push(phase.longrepr.slice(0, 2000))

    suite.tests.push({
      name: test.nodeid,
      status: outcome,
      failureMessages,
    })
    suite.duration += test.duration ?? 0
    suiteMap.set(file, suite)

  return [...suiteMap.values()]
```

### A7. resolveGeneratedTestDir(config)

```
function resolveGeneratedTestDir(config): string
  return config.generatedTestDir // default .ody-code/test-generated/e2e
```

### A8. analyzeImpact(changedFiles, config)

```
function analyzeImpact(changedFiles, config): ImpactAnalysisResult
  packages = new Set<string>()
  for file in changedFiles
    normalized = file.replace(/\\/g, '/')
    if !normalized.endsWith('.py') or normalized.endsWith('_test.py')
      continue
    // Python package = directory containing the file
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

> This mirrors the Go generator's package-directory impact model. [C:UPSTREAM-ish]

---

## Generated Test Templates (Illustrative)

The following are the **semantic contracts** of the generated Python files; exact string contents are left to the implementer, but they must satisfy these invariants.

### FastAPI Template

- Picks a free TCP port.
- Launches `python -m uvicorn <entry>:app --host 127.0.0.1 --port <port>` in a subprocess.
- Waits until `http://127.0.0.1:<port>/` responds (or times out).
- Calls the endpoint and asserts `status == 200`.
- Terminates the subprocess in a fixture teardown.
- Contains a TODO comment telling the user to adjust the path and assertions.

### Flask Template

- Picks a free TCP port.
- Sets `FLASK_APP=<entry>` and launches `flask run --host 127.0.0.1 --port <port>`.
- Waits for readiness, asserts on HTTP response, tears down process.

### Django Template

- Picks a free TCP port.
- Launches `python manage.py runserver 127.0.0.1:<port>`.
- Waits for readiness, asserts on HTTP response, tears down process.

### Generic Template

- If `entry` is a runnable script: `python <entry>` and assert `returncode == 0`.
- If no entry is found: placeholder test that logs a TODO and passes.

---

## Call-Site Integration

### 1. Register the generator

**File**: `packages/agent-core/src/e2e-testing/registry.ts`  
**Around line**: 1–24

```typescript
import { PythonPytestGenerator } from './generators/python-pytest';

export const registry = new E2EGeneratorRegistry();
registry.register(new TypeScriptVitestGenerator());
registry.register(new NodejsJestGenerator()); // added in Part 2
registry.register(new PythonPytestGenerator());
registry.register(new GoGenerator());
```

> Detection order: TypeScript/Vitest first for ody-code self-testing; Node/Jest next because it also looks at `package.json`; Python next; Go last. [C:INFERRED]

### 2. Extend plan-enricher file regex

**File**: `packages/agent-core/src/e2e-testing/plan-enricher.ts`  
**Line**: 58

Change:

```typescript
const regex = /(?:packages|apps)\/[a-zA-Z0-9\-_/.]+\.[jt]sx?/g;
```

to:

```typescript
const regex = /(?:packages|apps)\/[a-zA-Z0-9\-_/.]+\.(?:[jt]sx?|py)/g;
```

This allows the fallback path extractor to recognize Python files when `git status` is unavailable. [C:INFERRED]

### 3. Config schema additions

**File**: `packages/agent-core/src/config/schema.ts`  
**Around line**: 283–292

Add to `E2EConfigSchema`:

```typescript
recursiveAnalysisEnabled: z.boolean().default(true),
cacheEnabled: z.boolean().default(true),
cacheDir: z.string().default('.ody-code/e2e-cache'),
cacheTtlDays: z.number().int().min(1).default(7),
cacheMaxEntries: z.number().int().min(1).default(20),
```

> Exact field names shared with Parts 3 and 4. [C:USER]

---

## Error Handling & Degradation

| Error Class | Immediate Handling | Degradation Path | Recovery |
|-------------|-------------------|------------------|----------|
| No Python manifest found | `detectProjectStructure` returns `null` | E2E task skipped for this project | User adds `pyproject.toml` / `requirements.txt` |
| Framework detected but entry not found | Use fallback placeholder entry (e.g., `"main"`) | Generated test contains TODO and passes; user edits entry | User updates template / entry detection |
| `python3` not in PATH | Try `python`; if both missing, run fails | Return single failed suite with clear message | User installs Python |
| `pytest-json-report` plugin missing | Fallback to exit-code-only result | Coarse-grained pass/fail, no per-test detail | User installs plugin for richer reports |
| Spawned server fails to start | Test fails with subprocess stderr | Failure surfaces through `failurePolicy` | User fixes server startup |
| Subprocess orphan (server keeps running) | Fixture teardown sends `terminate()` then `kill()` after timeout | OS reaps process eventually; CI timeout as backstop | Generated test includes `try/finally` cleanup |
| `analyzeImpact` finds no affected packages | If `strategy === 'always'`, emit `general` placeholder; otherwise skip | No E2E tests generated | User adjusts `strategy` |

---

## Test Plan

### Unit Tests — `packages/agent-core/test/e2e-testing/python-pytest-generator.test.ts`

1. **Detection**
   - `detectProjectStructure` returns `null` when no Python manifest exists.
   - Detects FastAPI from `pyproject.toml` containing `dependencies = ["fastapi"]`.
   - Detects Flask from `requirements.txt` containing `flask`.
   - Detects Django from `setup.py` containing `"django"`.
   - Returns `null` for a directory with only `.txt` files and no `.py` files (generic guard).

2. **Classification / Entry**
   - FastAPI entry resolves to module name of file containing `app = FastAPI(...)`.
   - Flask entry resolves to module name of file containing `app = Flask(__name__)`.
   - Django entry resolves to `"manage.py"` when present.
   - Generic entry picks the only top-level non-test `.py` file.

3. **Template Generation**
   - Generated FastAPI file contains `uvicorn` subprocess launch.
   - Generated Flask file contains `flask run`.
   - Generated Django file contains `manage.py runserver`.
   - Generated generic file asserts on `subprocess.run` returncode when an entry exists.
   - All generated files include an `AUTO-GENERATED` marker and a TODO comment.

4. **Run / Parse**
   - `runTests` invokes `python3 -m pytest --json-report ...`.
   - `parsePytestJsonReport` maps a passing test to `status: 'passed'`.
   - `parsePytestJsonReport` maps a failing test to `status: 'failed'` with failure messages.
   - Fallback path (no JSON report) returns a single suite whose status matches exit code.
   - Empty test path list returns `[]`.

5. **Impact Analysis**
   - Changed `.py` files map to their package directory.
   - `_test.py` files are excluded.
   - `strategy='critical-only'` filters out non-critical packages.

### End-to-End Test

- Create a temporary FastAPI project with `main.py` and `pyproject.toml`.
- Run `E2ETestExecutor.execute(...)` against it.
- Assert `result.passed >= 1` and `result.failed === 0`.
- Assert a JSON report was written to `.ody-code/test-reports/`.

### Done Criteria

```bash
pnpm test packages/agent-core/test/e2e-testing/python-pytest-generator.test.ts
pnpm test packages/agent-core/test/e2e-testing/executor.test.ts
pnpm test packages/agent-core/test/e2e-testing/integration.test.ts
pnpm exec tsc --noEmit -p packages/agent-core/tsconfig.json
```

---

## Local Assumptions

| # | Assumption | Confidence | Impact if Wrong |
|---|------------|------------|-----------------|
| L1 | Python projects use either `python3` or `python` as the interpreter command. | Medium | Tests fail to spawn; fallback tries both. |
| L2 | FastAPI/Flask/Django apps expose a single app object and can be launched by the generated template without extra setup. | Medium | Generated tests may need manual TODO completion. |
| L3 | `pytest` is installed in the target environment when this generator is selected. | Medium | Run fails; fallback only gives coarse result. |
| L4 | Leading-name dependency parsing is sufficient for framework detection. | High | Detection is straightforward; can be extended later. |
