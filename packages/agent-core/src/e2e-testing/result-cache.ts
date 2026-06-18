import { createHash } from 'node:crypto';
import { join } from 'pathe';
import type { Kaos } from '@odysseythink/kaos';
import type { ResolvedE2EConfig } from './config';
import type { TestFile, E2EExecutionResult } from './types';

interface CacheEntry {
  createdAt: string;
  key: string;
  result: E2EExecutionResult;
}

interface CacheStats {
  hits: number;
  misses: number;
}

/**
 * Compute a deterministic cache key from the set of changed files and
 * generated test files. Normalises paths (backslash → forward slash),
 * sorts everything, and produces a 64-char hex SHA-256 digest.
 */
export function computeCacheKey(
  changedFiles: string[],
  testFiles: TestFile[],
): string {
  // Normalise and deduplicate changed file paths
  const normalizedChanged = [
    ...new Set(changedFiles.map(f => f.replace(/\\/g, '/'))),
  ].sort();

  // Hash generated test contents: relativePath + NUL + content, sorted
  const contentParts = testFiles
    .map(f => f.relativePath.replace(/\\/g, '/') + '\0' + f.content)
    .sort();
  const contentHash = createHash('sha256')
    .update(contentParts.join('\n'))
    .digest('hex');

  // Combine and produce final key
  const payload = normalizedChanged.join('\n') + '\n' + contentHash;
  return createHash('sha256').update(payload).digest('hex');
}

function isExpired(createdAt: string, ttlDays: number): boolean {
  const ageMs = Date.now() - new Date(createdAt).getTime();
  return ageMs > ttlDays * 24 * 60 * 60 * 1000;
}

export class E2ETestResultCache {
  private stats: CacheStats = { hits: 0, misses: 0 };

  constructor(
    private readonly kaos: Kaos,
    private readonly config: ResolvedE2EConfig,
  ) {}

  async get(key: string): Promise<E2EExecutionResult | null> {
    if (!this.config.cacheEnabled) {
      this.stats.misses += 1;
      return null;
    }

    const path = join(this.config.cacheDir, key + '.json');
    try {
      const { existsSync, readFileSync } = await import('node:fs');
      if (!existsSync(path)) {
        this.stats.misses += 1;
        return null;
      }

      const text = readFileSync(path, 'utf-8');
      const entry = JSON.parse(text) as CacheEntry;

      if (isExpired(entry.createdAt, this.config.cacheTtlDays)) {
        const { unlinkSync } = await import('node:fs');
        try { unlinkSync(path); } catch { /* ignore */ }
        this.stats.misses += 1;
        return null;
      }

      this.stats.hits += 1;
      return entry.result;
    } catch {
      this.stats.misses += 1;
      return null;
    }
  }

  async set(key: string, result: E2EExecutionResult): Promise<void> {
    if (!this.config.cacheEnabled) return;

    const cacheDir = this.config.cacheDir;
    try {
      const { mkdirSync, writeFileSync } = await import('node:fs');
      mkdirSync(cacheDir, { recursive: true });

      const entry: CacheEntry = {
        createdAt: new Date().toISOString(),
        key,
        result,
      };

      const path = join(cacheDir, key + '.json');
      writeFileSync(path, JSON.stringify(entry, null, 2));
    } catch {
      // Cache write is best-effort; do not fail the E2E run
      return;
    }

    await this.prune();
  }

  async prune(): Promise<void> {
    const cacheDir = this.config.cacheDir;
    const { existsSync, readFileSync, readdirSync, statSync, unlinkSync } = await import('node:fs');

    try {
      if (!existsSync(cacheDir)) return;

      const entries: Array<{ path: string; mtimeMs: number }> = [];
      const filenames = readdirSync(cacheDir);

      for (const filename of filenames) {
        if (!filename.endsWith('.json')) continue;
        const filePath = join(cacheDir, filename);
        try {
          // Read createdAt from the JSON content itself for accurate TTL check
          const text = readFileSync(filePath, 'utf-8');
          const entry = JSON.parse(text) as CacheEntry;
          if (isExpired(entry.createdAt, this.config.cacheTtlDays)) {
            try { unlinkSync(filePath); } catch { /* ignore */ }
          } else {
            const stat = statSync(filePath);
            entries.push({ path: filePath, mtimeMs: stat.mtimeMs });
          }
        } catch {
          // File disappeared or is unreadable — skip
        }
      }

      // Enforce max-entries: keep only the N most recent
      if (entries.length > this.config.cacheMaxEntries) {
        entries.sort((a, b) => a.mtimeMs - b.mtimeMs);
        const toDelete = entries.slice(
          0,
          entries.length - this.config.cacheMaxEntries,
        );
        for (const entry of toDelete) {
          try { unlinkSync(entry.path); } catch { /* ignore */ }
        }
      }
    } catch {
      // Prune is best-effort
    }
  }

  resetStats(): void {
    this.stats = { hits: 0, misses: 0 };
  }
}
