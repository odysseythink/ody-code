# Part 3 — Recursive Impact Analysis

## Scope

### In Scope

- Build a language-agnostic recursive dependency traversal that, given a set of changed files, returns the transitive closure of files that depend on them (direct or indirect dependents). [C:USER]
- Implement lightweight, regex/line-based import parsers for TypeScript/Vitest, Go, Python, and Node.js. [C:USER]
- Resolve relative imports to absolute file paths within the project. [C:INFERRED]
- Cap traversal depth and respect exclusion patterns. [C:USER]
- Provide an `analyzeImpact` adapter so generators can opt into recursive analysis while keeping their own priority/strategy logic. [C:INFERRED]

### Out of Scope

- Full AST parsing or import graph construction via language-native tools. [C:DEFERRED]
- Resolution of third-party / external package dependencies. [C:DEFERRED]
- Cross-language dependency edges (e.g., TypeScript importing a Go binary). [C:DEFERRED]
- Persistent on-disk import graph index or incremental updates. [C:DEFERRED]
- Replacing the existing static `TOOL_IMPACT_MAP` for ody-code self-testing. [C:INFERRED]

---

## Interfaces & Types

```typescript
type SupportedLanguage = 'typescript' | 'go' | 'python' | 'nodejs';

interface RecursiveImpactOptions {
  /** Maximum dependency hops to follow. */
  maxDepth?: number; // default 3
  /** Glob/regex patterns for files/directories to ignore while scanning. */
  excludePatterns?: string[]; // default ['node_modules', 'vendor', '.git', 'dist', 'build']
}

interface ImportEdge {
  from: string; // absolute path of the file containing the import
  to: string;   // absolute path of the resolved imported file, or empty if unresolved
}

interface ImportGraph {
  /** forward[from] = files that `from` imports */
  forward: Map<string, string[]>;
  /** reverse[to] = files that import `to` */
  reverse: Map<string, string[]>;
}

class RecursiveImpactAnalyzer {
  /**
   * Return the transitive set of files (including the originally changed files)
   * that are reachable by following reverse dependency edges up to `maxDepth` hops.
   */
  analyze(
    changedFiles: string[],
    projectRoot: string,
    language: SupportedLanguage,
    options?: RecursiveImpactOptions,
  ): string[];
}

interface LanguageParser {
  extensions: string[];
  extractImports(content: string, filePath: string): string[]; // raw import specifiers
  resolveImport(specifier: string, fromFile: string, projectRoot: string): string | null;
}
```

---

## Algorithms

### C1. analyze(changedFiles, projectRoot, language, options)

```
function analyze(changedFiles, projectRoot, language, options): string[]
  opts = normalizeOptions(options)
  parser = getParser(language)

  // 1. Collect candidate source files
  files = collectSourceFiles(projectRoot, parser.extensions, opts.excludePatterns)

  // 2. Build forward + reverse graph
  graph = buildGraph(files, parser, projectRoot)

  // 3. BFS over reverse edges starting from changed files
  affected = new Set<string>()
  queue = changedFiles.map(resolveAbsolute)
  depth = 0

  while queue.length > 0 and depth < opts.maxDepth
    nextQueue = []
    for file in queue
      absFile = resolveAbsolute(file)
      if affected.has(absFile) continue
      affected.add(absFile)

      dependents = graph.reverse.get(absFile) ?? []
      for dependent in dependents
        if !affected.has(dependent)
          nextQueue.push(dependent)

    queue = nextQueue
    depth += 1

  return [...affected]
```

### C2. collectSourceFiles(projectRoot, extensions, excludePatterns)

```
function collectSourceFiles(projectRoot, extensions, excludePatterns): string[]
  results = []
  stack = [projectRoot]
  while stack.length > 0
    dir = stack.pop()
    entries = readdir(dir)
    for entry in entries
      fullPath = join(dir, entry.name)
      if matchesAny(entry.name, excludePatterns) continue
      if entry.isDirectory()
        stack.push(fullPath)
      else if extensions.includes(extname(entry.name))
        results.push(normalize(fullPath))
  return results
```

> Default excludes: `node_modules`, `vendor`, `.git`, `dist`, `build`, `coverage`, `*.d.ts`. [C:INFERRED]

### C3. buildGraph(files, parser, projectRoot)

```
function buildGraph(files, parser, projectRoot): ImportGraph
  forward = new Map<string, string[]>()
  reverse = new Map<string, string[]>()

  for file in files
    content = readFile(file)
    imports = parser.extractImports(content, file)
    resolved = []
    for specifier in imports
      target = parser.resolveImport(specifier, file, projectRoot)
      if target !== null and files.includes(target)
        resolved.push(target)
        if !reverse.has(target) reverse.set(target, [])
        reverse.get(target).push(file)
    forward.set(file, resolved)

  return { forward, reverse }
```

### C4. TypeScript / Node.js Parser

```
class TypeScriptNodeParser implements LanguageParser {
  extensions = ['.ts', '.tsx', '.js', '.jsx', '.mjs', '.cjs']

  extractImports(content, filePath): string[]
    imports = []
    // ES module imports: import { x } from './foo'; import './side-effect'
    for match in content.matchAll(/import\s+(?:[^'"]+\s+from\s+)?['"]([^'"]+)['"]/g)
      imports.push(match[1])
    // CommonJS require
    for match in content.matchAll(/require\s*\(\s*['"]([^'"]+)['"]\s*\)/g)
      imports.push(match[1])
    // Dynamic import
    for match in content.matchAll(/import\s*\(\s*['"]([^'"]+)['"]\s*\)/g)
      imports.push(match[1])
    return imports

  resolveImport(specifier, fromFile, projectRoot): string | null
    if specifier starts with '.'
      return resolveRelativeModule(specifier, fromFile)
    return null // third-party packages ignored
}
```

```
function resolveRelativeModule(specifier, fromFile): string | null
  base = join(dirname(fromFile), specifier)
  candidates = [
    base,
    base + '.ts', base + '.tsx', base + '.js', base + '.jsx',
    base + '.mjs', base + '.cjs',
    join(base, 'index.ts'), join(base, 'index.tsx'),
    join(base, 'index.js'), join(base, 'index.jsx'),
    join(base, 'index.mjs'), join(base, 'index.cjs'),
  ]
  for candidate in candidates
    if exists(candidate) return normalize(candidate)
  return null
```

> Verified with `node -e`: the ES import regex captures `import { x } from './foo'`, `import './side-effect'`, and ignores unrelated strings. The require/dynamic regexes capture their respective forms. [C:INFERRED]

### C5. Go Parser

```
class GoParser implements LanguageParser {
  extensions = ['.go']

  extractImports(content, filePath): string[]
    imports = []
    // Single-line imports (plain or aliased)
    for match in content.matchAll(/import\s+(?:\w+\s+)?["']([^"']+)["']/g)
      imports.push(match[1])
    // Block imports
    for match in content.matchAll(/import\s*\(([\s\S]*?)\)/g)
      for inner in match[1].matchAll(/["']([^"']+)["']/g)
        imports.push(inner[1])
    return imports

  resolveImport(specifier, fromFile, projectRoot): string | null
    moduleName = readGoModuleName(projectRoot) // from go.mod
    if moduleName !== null and specifier startsWith(moduleName + '/')
      relative = specifier.slice(moduleName.length + 1)
      target = join(projectRoot, relative)
      return resolveGoPackageDir(target)
    return null // stdlib / third-party ignored
}
```

```
function readGoModuleName(projectRoot): string | null
  goMod = join(projectRoot, 'go.mod')
  if !exists(goMod) return null
  for line in readFile(goMod).split('\n')
    m = line.match(/^module\s+(\S+)/)
    if m return m[1]
  return null

function resolveGoPackageDir(targetPath): string | null
  // A Go import path maps to a directory; return the directory if it contains .go files.
  if isDirectory(targetPath) and hasGoFiles(targetPath)
    return normalize(targetPath)
  // Also allow the directory of the target if the specifier ended with a file-less path.
  return null
```

> Verified with `node -e`: Go import regexes capture single-line and block imports including aliased forms. [C:INFERRED]

### C6. Python Parser

```
class PythonParser implements LanguageParser {
  extensions = ['.py']

  extractImports(content, filePath): string[]
    imports = []
    for line in content.split('\n')
      // import os, foo.bar
      m = line.match(/^\s*import\s+([a-zA-Z0-9_.]+)/)
      if m
        imports.push(m[1])
        continue
      // from foo import bar, from . import bar, from ..foo import bar
      m = line.match(/^\s*from\s+(\.?[a-zA-Z0-9_.]*)\s+import/)
      if m
        imports.push(m[1])
    return imports

  resolveImport(specifier, fromFile, projectRoot): string | null
    if specifier === '' or specifier === '.'
      return dirname(fromFile)

    if specifier startsWith('.')
      // relative import: .foo → dirname + foo, ..foo → parent + foo
      return resolvePythonRelative(specifier, fromFile)

    // absolute import: foo.bar → projectRoot/foo/bar.py or projectRoot/foo/bar/__init__.py
    return resolvePythonAbsolute(specifier, projectRoot)
}
```

```
function resolvePythonAbsolute(specifier, projectRoot): string | null
  parts = specifier.split('.')
  // Try package/module file first, then package/__init__.py
  modulePath = join(projectRoot, ...parts) + '.py'
  if exists(modulePath) return normalize(modulePath)
  packageInit = join(projectRoot, ...parts, '__init__.py')
  if exists(packageInit) return normalize(dirname(packageInit))
  return null

function resolvePythonRelative(specifier, fromFile): string | null
  dots = countLeadingDots(specifier)
  dir = dirname(fromFile)
  for i in 1..dots
    dir = dirname(dir)
  rest = specifier.slice(dots).replace(/\./g, '/')
  if rest === '' return normalize(dir)
  modulePath = join(dir, rest) + '.py'
  if exists(modulePath) return normalize(modulePath)
  packageInit = join(dir, rest, '__init__.py')
  if exists(packageInit) return normalize(dirname(packageInit))
  return null
```

> Verified with `node -e`: Python regex captures `import os`, `import foo.bar`, `from baz import qux`, `from . import local`, and `from ..parent import thing`. [C:INFERRED]

### C7. Generator Adapter

Each generator can call the analyzer to augment its own impact model:

```
function analyzeImpact(changedFiles, config): ImpactAnalysisResult
  if config.recursiveAnalysisEnabled
    affectedFiles = RecursiveImpactAnalyzer.analyze(
      changedFiles,
      config.projectRoot,
      this.language,
      { maxDepth: config.maxRecursiveDepth ?? 3 },
    )
    changedFiles = affectedFiles

  // existing directory-based priority logic (mirrors Go/Python/Node generators)
  return directoryBasedImpact(changedFiles, config)
```

---

## Call-Site Integration

### 1. New analyzer module

**File**: `packages/agent-core/src/e2e-testing/recursive-impact-analyzer.ts` (new)  
**Exports**: `RecursiveImpactAnalyzer` class and `LanguageParser` interface.

### 2. Opt-in from generators

**Files**:
- `packages/agent-core/src/e2e-testing/generator.ts` (TypeScript/Vitest)
- `packages/agent-core/src/e2e-testing/generators/go.ts`
- `packages/agent-core/src/e2e-testing/generators/python-pytest.ts`
- `packages/agent-core/src/e2e-testing/generators/nodejs-jest.ts`

Each generator's `analyzeImpact` begins with:

```typescript
const filesToAnalyze = this.config.recursiveAnalysisEnabled
  ? RecursiveImpactAnalyzer.analyze(changedFiles, this.projectRoot, this.language)
  : changedFiles;
```

Then applies its existing package/directory-based priority logic to `filesToAnalyze`. [C:INFERRED]

### 3. Config schema additions

**File**: `packages/agent-core/src/config/schema.ts`  
**Around line**: 283–292

Already specified in Part 1; recursive analysis fields:

```typescript
recursiveAnalysisEnabled: z.boolean().default(true),
maxRecursiveDepth: z.number().int().min(1).default(3),
```

> `maxRecursiveDepth` is added in addition to the fields from Part 1. [C:USER]

---

## Error Handling & Degradation

| Error Class | Immediate Handling | Degradation Path | Recovery |
|-------------|-------------------|------------------|----------|
| Unreadable source file | Skip file, log warning | Graph omits edges from/to that file | Fix file permissions / encoding |
| Import resolution fails (third-party package) | Edge ignored | Analysis stops at project boundary | Future work: package graph parsing |
| Cyclic dependencies | BFS tracks `affected` set, avoids re-processing | Terminates naturally | N/A |
| `maxDepth` reached | BFS stops | Some transitive dependents omitted | Increase `maxRecursiveDepth` |
| `recursiveAnalysisEnabled=false` | Analyzer not called | Generators fall back to changed-files-only impact | User toggles config |
| `go.mod` missing in Go project | `readGoModuleName` returns null | Only relative imports resolved; module imports ignored | Add go.mod detection earlier |

---

## Test Plan

### Unit Tests — `packages/agent-core/test/e2e-testing/recursive-impact-analyzer.test.ts`

1. **Graph building**
   - Given three TS files `a.ts → b.ts → c.ts`, `buildGraph` produces reverse edge `c → [b]`, `b → [a]`.

2. **TypeScript relative resolution**
   - `import { x } from './foo'` in `src/a.ts` resolves to `src/foo.ts`.
   - `import { x } from '../bar'` resolves to `bar.ts`.
   - `import { x } from './dir'` resolves to `dir/index.ts`.
   - Third-party import `'lodash'` resolves to `null`.

3. **Go resolution**
   - With `module example.com/proj`, import `example.com/proj/pkg/util` resolves to `<root>/pkg/util`.
   - Stdlib import `'fmt'` resolves to `null`.

4. **Python resolution**
   - `import foo.bar` resolves to `<root>/foo/bar.py` or `<root>/foo/bar/__init__.py`.
   - `from . import baz` resolves to sibling `baz.py`.
   - `from ..parent import thing` resolves to grandparent package.

5. **BFS traversal**
   - Changed `[c.ts]` returns `[c.ts, b.ts, a.ts]` for maxDepth=3.
   - Changed `[c.ts]` with maxDepth=1 returns `[c.ts, b.ts]`.
   - Cyclic graph `a → b → a` terminates and returns both files.

6. **Exclude patterns**
   - Files under `node_modules/` are not scanned even if they match extensions.

### Integration Tests

- In a temporary TS monorepo package, change `util.ts` and verify the analyzer also returns `api.ts` which imports `service.ts` which imports `util.ts`.

### Done Criteria

```bash
pnpm test packages/agent-core/test/e2e-testing/recursive-impact-analyzer.test.ts
pnpm exec tsc --noEmit -p packages/agent-core/tsconfig.json
```

---

## Local Assumptions

| # | Assumption | Confidence | Impact if Wrong |
|---|------------|------------|-----------------|
| L1 | Relative imports cover the majority of transitive impact cases within a project. | Medium | Third-party-driven impact not detected; acceptable for Phase 2. |
| L2 | Default maxDepth of 3 is enough to surface critical dependents without excessive noise. | Medium | Can be tuned via config. |
| L3 | File extensions in `resolveRelativeModule` cover common project layouts. | High | Add more variants if needed. |
| L4 | Python package resolution by `__init__.py` matches the project layout. | Medium | Falls back to module file path. |
