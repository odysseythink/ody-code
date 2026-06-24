import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { describe, expect, it, afterEach } from 'vitest';
import { resolveBuiltInRoot, BuiltInRootNotFoundError } from '#/built-in/resolve-root';

const tempDirs: string[] = [];

afterEach(() => {
  for (const dir of tempDirs.splice(0)) {
    rmSync(dir, { recursive: true, force: true });
  }
});

function makeTempDir(): string {
  const dir = mkdtempSync(join(tmpdir(), 'kimi-built-in-'));
  tempDirs.push(dir);
  return dir;
}

describe('resolveBuiltInRoot', () => {
  it('returns the first candidate containing package.json', () => {
    const dir = makeTempDir();
    const serverDir = join(dir, 'chrome-devtools');
    mkdirSync(serverDir, { recursive: true });
    writeFileSync(join(serverDir, 'package.json'), '{}');
    const result = resolveBuiltInRoot('chrome-devtools', [
      join(dir, 'does-not-exist'),
      serverDir,
    ]);
    expect(result).toBe(serverDir);
  });

  it('returns the first candidate containing index.js', () => {
    const dir = makeTempDir();
    const serverDir = join(dir, 'chrome-devtools');
    mkdirSync(serverDir, { recursive: true });
    writeFileSync(join(serverDir, 'index.js'), '');
    const result = resolveBuiltInRoot('chrome-devtools', [serverDir]);
    expect(result).toBe(serverDir);
  });

  it('throws BuiltInRootNotFoundError when no candidate matches', () => {
    expect(() =>
      resolveBuiltInRoot('nonexistent-server-xyz', [join(tmpdir(), 'nonexistent-123')]),
    ).toThrow(BuiltInRootNotFoundError);
    try {
      resolveBuiltInRoot('nonexistent-server-xyz', [join(tmpdir(), 'nonexistent-123')]);
    } catch (error) {
      expect(error).toBeInstanceOf(BuiltInRootNotFoundError);
      expect((error as BuiltInRootNotFoundError).serverName).toBe('nonexistent-server-xyz');
    }
  });

  it('prefers earlier candidate when both match', () => {
    const dir = makeTempDir();
    const first = join(dir, 'first', 'chrome-devtools');
    const second = join(dir, 'second', 'chrome-devtools');
    mkdirSync(first, { recursive: true });
    mkdirSync(second, { recursive: true });
    writeFileSync(join(first, 'package.json'), '{}');
    writeFileSync(join(second, 'package.json'), '{}');
    const result = resolveBuiltInRoot('chrome-devtools', [first, second]);
    expect(result).toBe(first);
  });
});
