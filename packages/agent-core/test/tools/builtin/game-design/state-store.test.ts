import { describe, it, expect, vi } from 'vitest';
import { join } from 'pathe';
import {
  FileSystemGameDesignStateStore,
  NoopGameDesignStateStore,
} from '../../../../src/office-hours/state';

describe('GameDesignStateStore', () => {
  const mockKaos = () => ({
    mkdir: vi.fn().mockResolvedValue(undefined),
    writeText: vi.fn().mockResolvedValue(undefined),
    readText: vi.fn().mockRejectedValue({ code: 'ENOENT' }),
  });

  describe('FileSystemGameDesignStateStore', () => {
    it('stores to .ody-code/game-design/ in the project directory', async () => {
      const kaos = mockKaos();
      const store = new FileSystemGameDesignStateStore(kaos as any, '/fake/project');
      await store.appendProfile({
        date: '2026-06-18',
        mode: 'startup',
        projectSlug: 'test-game',
        pillars: 'Explore, Build, Survive',
        audience: 'Casual',
        platform: 'Mobile',
        genre: 'Adventure',
        signals: [],
        designDoc: 'game-design.md',
      });
      expect(kaos.mkdir).toHaveBeenCalledWith(
        join('/fake/project', '.ody-code', 'game-design'),
        { parents: true, existOk: true },
      );
      expect(kaos.writeText).toHaveBeenCalledWith(
        expect.stringContaining('.ody-code/game-design/builder-profile.jsonl'),
        expect.stringContaining('"projectSlug":"test-game"'),
        { mode: 'a' },
      );
    });

    it('searchLearnings returns most recent entries first within limit', async () => {
      const entries = [
        { ts: '2026-01-01', skill: 'game-design', type: 'operational' as const, key: 'k1', insight: 'a', confidence: 0.5, source: 'observed' as const },
        { ts: '2026-01-02', skill: 'game-design', type: 'eureka' as const, key: 'k2', insight: 'b', confidence: 0.8, source: 'observed' as const },
        { ts: '2026-01-03', skill: 'game-design', type: 'operational' as const, key: 'k3', insight: 'c', confidence: 0.6, source: 'observed' as const },
      ];
      const kaos = {
        ...mockKaos(),
        readText: vi.fn().mockResolvedValue(entries.map(e => JSON.stringify(e)).join('\n')),
      };
      const store = new FileSystemGameDesignStateStore(kaos as any, '/fake/project');
      const result = await store.searchLearnings({ limit: 2 });
      expect(result).toHaveLength(2);
      expect(result[0]!.key).toBe('k3');
      expect(result[1]!.key).toBe('k2');
    });

    it('searchLearnings filters by branch when provided', async () => {
      const entries = [
        { ts: '2026-01-01', skill: 'game-design', type: 'operational' as const, key: 'k1', insight: 'a', confidence: 0.5, source: 'observed' as const, branch: 'main' },
        { ts: '2026-01-02', skill: 'game-design', type: 'eureka' as const, key: 'k2', insight: 'b', confidence: 0.8, source: 'observed' as const, branch: 'feature/x' },
        { ts: '2026-01-03', skill: 'game-design', type: 'operational' as const, key: 'k3', insight: 'c', confidence: 0.6, source: 'observed' as const, branch: 'main' },
      ];
      const kaos = {
        ...mockKaos(),
        readText: vi.fn().mockResolvedValue(entries.map(e => JSON.stringify(e)).join('\n')),
      };
      const store = new FileSystemGameDesignStateStore(kaos as any, '/fake/project');
      const result = await store.searchLearnings({ limit: 10, branch: 'main' });
      expect(result).toHaveLength(2);
      expect(result.every(e => e.branch === 'main')).toBe(true);
    });
  });

  describe('NoopGameDesignStateStore', () => {
    it('all methods are no-ops returning empty/zero values', async () => {
      const store = new NoopGameDesignStateStore();
      await expect(store.appendProfile({} as any)).resolves.toBeUndefined();
      await expect(store.readProfile()).resolves.toEqual([]);
      await expect(store.searchLearnings({ limit: 5 })).resolves.toEqual([]);
      const summary = await store.getSessionSummary();
      expect(summary).toEqual({ sessionCount: 0, tier: 'introduction' });
    });
  });
});
