import { describe, expect, it } from 'vitest';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { LocalKaos } from '@odysseythink/kaos';
import { detectChangedFiles } from '#/e2e-testing/git-status';

function git(cwd: string, ...args: string[]): void {
  execFileSync('git', args, { cwd, stdio: 'pipe' });
}

function initRepo(): string {
  const root = mkdtempSync(join(tmpdir(), 'changed-files-'));
  git(root, 'init', '-q');
  git(root, 'config', 'user.email', 'test@example.com');
  git(root, 'config', 'user.name', 'test');
  git(root, 'config', 'commit.gpgsign', 'false');
  writeFileSync(join(root, 'base.txt'), 'base\n');
  git(root, 'add', '.');
  git(root, 'commit', '-qm', 'base');
  git(root, 'branch', '-M', 'main');
  return root;
}

describe('detectChangedFiles (real git repo)', () => {
  it('unions committed-since-base and uncommitted changes', async () => {
    const root = initRepo();
    try {
      // Work committed on a feature branch — invisible to `git status` but
      // captured by diffing against the merge-base with main.
      git(root, 'checkout', '-q', '-b', 'feature');
      mkdirSync(join(root, 'internal/api'), { recursive: true });
      writeFileSync(join(root, 'internal/api/handler.go'), 'package api\n');
      git(root, 'add', '.');
      git(root, 'commit', '-qm', 'add handler');
      // Plus an uncommitted change.
      writeFileSync(join(root, 'internal/api/router.go'), 'package api\n');

      const kaos = await LocalKaos.create();
      const changed = await detectChangedFiles(kaos, root);

      expect(changed).toContain('internal/api/handler.go'); // committed since base
      expect(changed).toContain('internal/api/router.go'); // uncommitted
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('falls back to uncommitted-only when no baseline predates HEAD', async () => {
    const root = initRepo();
    try {
      // Still on main, no remote → merge-base(HEAD, main) === HEAD → no diff.
      writeFileSync(join(root, 'b.go'), 'package b\n');
      const kaos = await LocalKaos.create();
      const changed = await detectChangedFiles(kaos, root);
      expect(changed).toEqual(['b.go']);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('returns empty for a non-git directory without throwing', async () => {
    const root = mkdtempSync(join(tmpdir(), 'not-git-'));
    try {
      const kaos = await LocalKaos.create();
      expect(await detectChangedFiles(kaos, root)).toEqual([]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
