import { join, dirname, extname } from 'pathe';

type SupportedLanguage = 'typescript' | 'go' | 'python' | 'nodejs';

interface RecursiveImpactOptions {
  maxDepth?: number;
  excludePatterns?: string[];
}

interface LanguageParser {
  extensions: string[];
  extractImports(content: string): string[];
  resolveImport(
    specifier: string,
    fromFile: string,
    projectRoot: string,
    existsSync: (p: string) => boolean,
  ): string | null;
}

// ------ TypeScript / Node.js Parser ------

const tsParser: LanguageParser = {
  extensions: ['.ts', '.tsx', '.js', '.jsx', '.mjs', '.cjs'],
  extractImports(content: string): string[] {
    const imports: string[] = [];
    // ES module: import { x } from './foo'; import './side-effect'
    for (const m of content.matchAll(/import\s+(?:[^'"]+\s+from\s+)?['"]([^'"]+)['"]/g)) {
      if (m[1]) imports.push(m[1]);
    }
    // CommonJS: require('./foo')
    for (const m of content.matchAll(/require\s*\(\s*['"]([^'"]+)['"]\s*\)/g)) {
      if (m[1]) imports.push(m[1]);
    }
    // Dynamic: import('./foo')
    for (const m of content.matchAll(/import\s*\(\s*['"]([^'"]+)['"]\s*\)/g)) {
      if (m[1]) imports.push(m[1]);
    }
    return imports;
  },
  resolveImport(
    specifier: string,
    fromFile: string,
    _projectRoot: string,
    existsSync: (p: string) => boolean,
  ): string | null {
    if (!specifier.startsWith('.')) return null; // third-party
    return resolveRelativeModule(specifier, fromFile, tsParser.extensions, existsSync);
  },
};

function resolveRelativeModule(
  specifier: string,
  fromFile: string,
  extensions: string[],
  existsSync: (p: string) => boolean,
): string | null {
  const base = join(dirname(fromFile), specifier);
  const { statSync } = require('node:fs') as typeof import('node:fs');

  // Check exact file with each extension first
  for (const ext of extensions) {
    if (existsSync(base + ext)) return normalize(base + ext);
  }

  // Check if base itself is a file (no extension needed, e.g. .ts already included)
  if (existsSync(base)) {
    try {
      if (statSync(base).isFile()) return normalize(base);
    } catch { /* ignore */ }
    // It's a directory — try index files
    for (const ext of extensions) {
      const indexPath = join(base, 'index' + ext);
      if (existsSync(indexPath)) return normalize(indexPath);
    }
  }

  return null;
}

// ------ Go Parser ------

const goParser: LanguageParser = {
  extensions: ['.go'],
  extractImports(content: string): string[] {
    const imports: string[] = [];
    // Single-line: import "fmt" or import alias "fmt"
    for (const m of content.matchAll(/import\s+(?:\w+\s+)?["']([^"']+)["']/g)) {
      if (m[1]) imports.push(m[1]);
    }
    // Block: import ( ... )
    for (const m of content.matchAll(/import\s*\(([\s\S]*?)\)/g)) {
      if (m[1]) {
        for (const inner of m[1].matchAll(/["']([^"']+)["']/g)) {
          if (inner[1]) imports.push(inner[1]);
        }
      }
    }
    return imports;
  },
  resolveImport(
    specifier: string,
    _fromFile: string,
    projectRoot: string,
    existsSync: (p: string) => boolean,
  ): string | null {
    const moduleName = readGoModuleName(projectRoot, existsSync);
    if (moduleName !== null && specifier.startsWith(moduleName + '/')) {
      const relative = specifier.slice(moduleName.length + 1);
      const target = join(projectRoot, relative);
      if (existsSync(target)) {
        try {
          const { readdirSync } = require('node:fs') as typeof import('node:fs');
          const entries = readdirSync(target, { withFileTypes: true });
          if (entries.some(e => e.isFile() && e.name.endsWith('.go'))) {
            return normalize(target);
          }
        } catch { /* ignore */ }
      }
      for (const ext of goParser.extensions) {
        if (existsSync(target + ext)) return normalize(target + ext);
      }
      return null;
    }
    return null; // stdlib or third-party
  },
};

function readGoModuleName(
  projectRoot: string,
  existsSync: (p: string) => boolean,
): string | null {
  const goMod = join(projectRoot, 'go.mod');
  if (!existsSync(goMod)) return null;
  try {
    const { readFileSync } = require('node:fs') as typeof import('node:fs');
    const content = readFileSync(goMod, 'utf-8');
    for (const line of content.split('\n')) {
      const m = line.match(/^module\s+(\S+)/);
      if (m) return m[1]!;
    }
  } catch { /* ignore */ }
  return null;
}

// ------ Python Parser ------

const pyParser: LanguageParser = {
  extensions: ['.py'],
  extractImports(content: string): string[] {
    const imports: string[] = [];
    for (const line of content.split('\n')) {
      // import os, import foo.bar
      const importMatch = line.match(/^\s*import\s+([a-zA-Z0-9_.]+)/);
      if (importMatch) {
        if (importMatch[1]) imports.push(importMatch[1]);
        continue;
      }
      // from X import Y, from . import Y, from ..foo import Y
      // For relative imports (starting with .), produce specifier like `.Y`
      // so the resolver can find it as a sibling module.
      const fromMatch = line.match(/^\s*from\s+(\.?[a-zA-Z0-9_.]*)\s+import\s+([a-zA-Z0-9_*]+)/);
      if (fromMatch) {
        let specifier = fromMatch[1]!;
        const importedName = fromMatch[2]!;
        // For `from . import b`, produce `.b` (relative module path + name)
        if (specifier === '.' && importedName !== '*') {
          imports.push('.' + importedName);
        } else if (specifier.startsWith('.')) {
          // `from ..foo import bar` → keep specifier as `..foo`
          imports.push(specifier);
        } else {
          // `from os import path` → just push `os` (absolute)
          imports.push(specifier);
        }
      }
    }
    return imports;
  },
  resolveImport(
    specifier: string,
    fromFile: string,
    projectRoot: string,
    existsSync: (p: string) => boolean,
  ): string | null {
    if (specifier === '' || specifier === '.') {
      return normalize(dirname(fromFile));
    }

    if (specifier.startsWith('.')) {
      return resolvePythonRelative(specifier, fromFile, existsSync);
    }

    return resolvePythonAbsolute(specifier, projectRoot, existsSync);
  },
};

function resolvePythonAbsolute(
  specifier: string,
  projectRoot: string,
  existsSync: (p: string) => boolean,
): string | null {
  const parts = specifier.split('.');
  const modulePath = join(projectRoot, ...parts) + '.py';
  if (existsSync(modulePath)) return normalize(modulePath);
  const packageInit = join(projectRoot, ...parts, '__init__.py');
  if (existsSync(packageInit)) return normalize(dirname(packageInit));
  return null;
}

function resolvePythonRelative(
  specifier: string,
  fromFile: string,
  existsSync: (p: string) => boolean,
): string | null {
  let dots = 0;
  while (dots < specifier.length && specifier[dots] === '.') dots++;
  let dir = dirname(fromFile);
  for (let i = 1; i < dots; i++) dir = dirname(dir);
  const rest = specifier.slice(dots).replace(/\./g, '/');
  if (rest === '') return normalize(dir);
  const modulePath = join(dir, rest) + '.py';
  if (existsSync(modulePath)) return normalize(modulePath);
  const packageInit = join(dir, rest, '__init__.py');
  if (existsSync(packageInit)) return normalize(dirname(packageInit));
  return null;
}

// ------ Analyzer ------

function normalize(p: string): string {
  return p.replace(/\\/g, '/');
}

export class RecursiveImpactAnalyzer {
  private parsers: Record<SupportedLanguage, LanguageParser> = {
    typescript: tsParser,
    nodejs: tsParser, // same parser as TypeScript
    go: goParser,
    python: pyParser,
  };

  analyze(
    changedFiles: string[],
    projectRoot: string,
    language: SupportedLanguage,
    options?: RecursiveImpactOptions,
  ): string[] {
    const maxDepth = options?.maxDepth ?? 3;
    const excludePatterns = options?.excludePatterns ?? [
      'node_modules', 'vendor', '.git', 'dist', 'build', 'coverage',
    ];

    const parser = this.parsers[language];
    if (!parser) return changedFiles.map(f => join(projectRoot, f));

    const { existsSync, readFileSync, readdirSync } = require('node:fs') as typeof import('node:fs');

    // 1. Collect source files
    const files = collectSourceFiles(
      projectRoot, parser.extensions, excludePatterns, existsSync, readdirSync,
    );

    // 2. Build reverse dependency graph
    const reverse = buildReverseGraph(
      files, parser, projectRoot, existsSync, readFileSync,
    );

    // 3. BFS over reverse edges
    const affected = new Set<string>();
    const resolvedChanged = changedFiles.map(f => {
      const abs = join(projectRoot, f.replace(/\\/g, '/'));
      return abs;
    });

    // First add all changed files
    for (const f of resolvedChanged) {
      if (files.has(f)) affected.add(f);
    }

    let frontier = resolvedChanged.filter(f => files.has(f));
    for (let depth = 0; depth < maxDepth && frontier.length > 0; depth++) {
      const next: string[] = [];
      for (const file of frontier) {
        const dependents = reverse.get(file) ?? [];
        for (const dependent of dependents) {
          if (!affected.has(dependent)) {
            affected.add(dependent);
            next.push(dependent);
          }
        }
      }
      frontier = next;
    }

    return [...affected].sort();
  }
}

function collectSourceFiles(
  root: string,
  extensions: string[],
  excludePatterns: string[],
  existsSync: typeof import('node:fs').existsSync,
  readdirSync: typeof import('node:fs').readdirSync,
): Set<string> {
  const results = new Set<string>();
  const stack = [root];
  const extSet = new Set(extensions);

  while (stack.length > 0) {
    const dir = stack.pop()!;
    let entries: import('node:fs').Dirent[];
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      const fullPath = normalize(join(dir, entry.name));
      const name = entry.name;
      if (entry.isDirectory()) {
        if (name.startsWith('.') || name.startsWith('_') || excludePatterns.includes(name)) continue;
        stack.push(fullPath);
      } else if (extSet.has(extname(name)) && !name.endsWith('.d.ts')) {
        results.add(fullPath);
      }
    }
  }
  return results;
}

function buildReverseGraph(
  files: Set<string>,
  parser: LanguageParser,
  projectRoot: string,
  existsSync: typeof import('node:fs').existsSync,
  readFileSync: typeof import('node:fs').readFileSync,
): Map<string, string[]> {
  const reverse = new Map<string, string[]>();

  for (const file of files) {
    let content: string;
    try {
      content = readFileSync(file, 'utf-8');
    } catch {
      continue;
    }
    const specifiers = parser.extractImports(content);
    for (const spec of specifiers) {
      const target = parser.resolveImport(spec, file, projectRoot, existsSync);
      if (target !== null && files.has(target)) {
        let deps = reverse.get(target);
        if (!deps) {
          deps = [];
          reverse.set(target, deps);
        }
        deps.push(file);
      }
    }
  }

  return reverse;
}
