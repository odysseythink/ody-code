import { describe, expect, it } from 'vitest';
import { mkdtempSync, writeFileSync, mkdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { buildAuditDigest } from '../../src/code-review/simplicity';

describe('buildAuditDigest', () => {
  it('discovers source files in a directory', () => {
    const dir = mkdtempSync(join(tmpdir(), 'audit-test-'));
    try {
      writeFileSync(join(dir, 'index.ts'), 'export const x = 1;');
      writeFileSync(join(dir, 'helper.js'), 'function f() {}');
      mkdirSync(join(dir, 'sub'));
      writeFileSync(join(dir, 'sub', 'nested.ts'), 'const y = 2;');
      mkdirSync(join(dir, 'node_modules'));
      writeFileSync(join(dir, 'node_modules', 'ignored.ts'), 'ignored');

      const digest = buildAuditDigest(dir);
      const names = digest.files.map((f) => f.replace(/\\/g, '/'));
      expect(names).toContain('index.ts');
      expect(names).toContain('helper.js');
      expect(names).toContain('sub/nested.ts');
      expect(names).not.toContain('node_modules/ignored.ts');
      expect(digest.fileCount).toBe(3);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('caps at MAX_AUDIT_FILES', () => {
    const dir = mkdtempSync(join(tmpdir(), 'audit-cap-'));
    try {
      for (let i = 0; i < 250; i += 1) {
        writeFileSync(join(dir, `file${i}.ts`), '// test');
      }
      const digest = buildAuditDigest(dir);
      expect(digest.fileCount).toBeLessThanOrEqual(200);
      expect(digest.files.length).toBeLessThanOrEqual(200);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('excludes dot-directories', () => {
    const dir = mkdtempSync(join(tmpdir(), 'audit-dot-'));
    try {
      mkdirSync(join(dir, '.hidden'));
      writeFileSync(join(dir, '.hidden', 'secret.ts'), '// hidden');
      writeFileSync(join(dir, 'visible.ts'), '// visible');

      const digest = buildAuditDigest(dir);
      const names = digest.files.map((f) => f.replace(/\\/g, '/'));
      expect(names).not.toContain('.hidden/secret.ts');
      expect(names).toContain('visible.ts');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('extracts package.json dependencies', () => {
    const dir = mkdtempSync(join(tmpdir(), 'audit-deps-'));
    try {
      writeFileSync(join(dir, 'package.json'), JSON.stringify({
        dependencies: { lodash: '^4.0.0' },
        devDependencies: { vitest: '^3.0.0' },
      }));
      writeFileSync(join(dir, 'index.ts'), 'export {}');

      const digest = buildAuditDigest(dir);
      expect(digest.dependencies).toContain('lodash');
      expect(digest.dependencies).toContain('vitest');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('handles missing package.json gracefully', () => {
    const dir = mkdtempSync(join(tmpdir(), 'audit-nopkg-'));
    try {
      writeFileSync(join(dir, 'index.ts'), 'export {}');
      const digest = buildAuditDigest(dir);
      expect(digest.dependencies).toEqual([]);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('respects AbortSignal', () => {
    const dir = mkdtempSync(join(tmpdir(), 'audit-abort-'));
    try {
      for (let i = 0; i < 100; i += 1) {
        writeFileSync(join(dir, `f${i}.ts`), '// test');
      }
      const ctrl = new AbortController();
      ctrl.abort();
      const digest = buildAuditDigest(dir, ctrl.signal);
      expect(digest.fileCount).toBe(0);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
