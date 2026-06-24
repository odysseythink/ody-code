import { describe, expect, it, afterAll, beforeAll } from 'vitest';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { RecursiveImpactAnalyzer } from '../src/recursive-impact-analyzer';

const tempRoots: string[] = [];

function makeProject(files: Record<string, string>): string {
  const root = mkdtempSync(join(tmpdir(), 'ria-e2e-'));
  tempRoots.push(root);
  for (const [rel, content] of Object.entries(files)) {
    const abs = join(root, rel);
    mkdirSync(join(abs, '..'), { recursive: true });
    writeFileSync(abs, content);
  }
  return root;
}

afterAll(() => {
  for (const root of tempRoots) rmSync(root, { recursive: true, force: true });
});

const analyzer = new RecursiveImpactAnalyzer();

describe('RecursiveImpactAnalyzer — TypeScript', () => {
  it('BFS: changed c.ts returns c, b, a for maxDepth=3', () => {
    const root = makeProject({
      'a.ts': `import { x } from './b';`,
      'b.ts': `import { y } from './c';`,
      'c.ts': `export const z = 1;`,
    });
    const result = analyzer.analyze(['c.ts'], root, 'typescript', { maxDepth: 3 });
    expect(new Set(result)).toEqual(new Set([
      join(root, 'c.ts'),
      join(root, 'b.ts'),
      join(root, 'a.ts'),
    ]));
  });

  it('BFS: maxDepth=1 only returns c and its direct dependents', () => {
    const root = makeProject({
      'a.ts': `import { x } from './b';`,
      'b.ts': `import { y } from './c';`,
      'c.ts': `export const z = 1;`,
    });
    const result = analyzer.analyze(['c.ts'], root, 'typescript', { maxDepth: 1 });
    expect(new Set(result)).toEqual(new Set([
      join(root, 'c.ts'),
      join(root, 'b.ts'),
    ]));
  });

  it('resolves import to index.ts', () => {
    const root = makeProject({
      'src/a.ts': `import { x } from './dir';`,
      'src/dir/index.ts': `import { y } from '../../b';`,
      'b.ts': `export const y = 1;`,
    });
    const result = analyzer.analyze(['b.ts'], root, 'typescript', { maxDepth: 3 });
    expect(new Set(result)).toEqual(new Set([
      join(root, 'b.ts'),
      join(root, 'src/dir/index.ts'),
      join(root, 'src/a.ts'),
    ]));
  });

  it('third-party import resolves to null (ignored)', () => {
    const root = makeProject({
      'a.ts': `import lodash from 'lodash';`,
    });
    const result = analyzer.analyze(['a.ts'], root, 'typescript');
    expect(result).toHaveLength(1);
    expect(result[0]!).toBe(join(root, 'a.ts'));
  });

  it('handles cyclic dependencies without infinite loop', () => {
    const root = makeProject({
      'a.ts': `import { x } from './b';`,
      'b.ts': `import { y } from './a';`,
    });
    const result = analyzer.analyze(['a.ts'], root, 'typescript');
    expect(result).toHaveLength(2);
  });
});

describe('RecursiveImpactAnalyzer — Python', () => {
  it('walks transitive Python imports', () => {
    const root = makeProject({
      'a.py': `from . import b`,
      'b.py': `from . import c`,
      'c.py': `x = 1`,
    });
    const result = analyzer.analyze(['c.py'], root, 'python', { maxDepth: 3 });
    expect(new Set(result)).toEqual(new Set([
      join(root, 'c.py'),
      join(root, 'b.py'),
      join(root, 'a.py'),
    ]));
  });

  it('resolves absolute import foo.bar', () => {
    const root = makeProject({
      'main.py': `import foo.bar`,
      'foo/__init__.py': '',
      'foo/bar.py': `x = 1`,
    });
    const result = analyzer.analyze(['foo/bar.py'], root, 'python');
    expect(new Set(result)).toEqual(new Set([
      join(root, 'foo/bar.py'),
      join(root, 'main.py'),
    ]));
  });
});

describe('RecursiveImpactAnalyzer — Go', () => {
  it('walks transitive Go imports within module', () => {
    const root = makeProject({
      'go.mod': 'module example.com/demo\n\ngo 1.22\n',
      'a.go': `package a\nimport "example.com/demo/b"`,
      'b.go': `package b\nimport "example.com/demo/c"`,
      'c.go': `package c`,
    });
    const result = analyzer.analyze(['c.go'], root, 'go');
    expect(new Set(result)).toEqual(new Set([
      join(root, 'c.go'),
      join(root, 'b.go'),
      join(root, 'a.go'),
    ]));
  });

  it('stdlib import is ignored', () => {
    const root = makeProject({
      'go.mod': 'module example.com/demo\n\ngo 1.22\n',
      'a.go': `package a\nimport "fmt"`,
    });
    const result = analyzer.analyze(['a.go'], root, 'go');
    expect(result).toHaveLength(1);
  });
});

describe('RecursiveImpactAnalyzer — exclusions', () => {
  it('excludes node_modules from scanning', () => {
    const root = makeProject({
      'src/a.ts': `import { x } from './b';`,
      'src/b.ts': `export const x = 1;`,
      'node_modules/pkg/index.ts': `import { z } from '../../src/b';`,
    });
    const result = analyzer.analyze(['src/b.ts'], root, 'typescript');
    expect(new Set(result)).toEqual(new Set([
      join(root, 'src/b.ts'),
      join(root, 'src/a.ts'),
    ]));
  });
});
