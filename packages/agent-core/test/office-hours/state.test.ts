import { describe, expect, it, vi } from 'vitest';
import { join } from 'pathe';
import {
  FileSystemOfficeHoursStateStore,
  NoopOfficeHoursStateStore,
  computeTier,
  selectResources,
} from '@odysseythink/agent-core-shared';
import type { BuilderProfileEntry } from '@odysseythink/agent-core-shared';

function mockKaos() {
  const files: Record<string, string> = {};
  return {
    name: 'mock',
    osEnv: {} as any,
    pathClass: vi.fn().mockReturnValue('posix' as const),
    normpath: vi.fn((p: string) => p),
    gethome: vi.fn().mockReturnValue('/fake/home'),
    getcwd: vi.fn().mockReturnValue('/fake/cwd'),
    chdir: vi.fn(),
    withCwd: vi.fn(),
    stat: vi.fn(),
    iterdir: vi.fn(),
    glob: vi.fn(),
    readBytes: vi.fn(),
    readText: vi.fn(async (p: string) => {
      if (p in files) return files[p];
      throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
    }),
    readLines: vi.fn(),
    writeBytes: vi.fn(),
    writeText: vi.fn(async (p: string, data: string, opts?: { mode?: 'w' | 'a' }) => {
      if (opts?.mode === 'a') {
        files[p] = (files[p] ?? '') + data;
      } else {
        files[p] = data;
      }
      return data.length;
    }),
    mkdir: vi.fn().mockResolvedValue(undefined),
    exec: vi.fn(),
    execWithEnv: vi.fn(),
  };
}

describe('FileSystemOfficeHoursStateStore', () => {
  it('appendProfile and readProfile round-trip', async () => {
    const kaos = mockKaos();
    const store = new FileSystemOfficeHoursStateStore(kaos as any, '/fake/home/.ody-code');
    const entry: BuilderProfileEntry = {
      date: '2026-06-16T12:00:00.000Z',
      mode: 'startup',
      projectSlug: 'my-project',
      signalCount: 3,
      signals: ['named_users', 'demand_transacted', 'agency'],
      designDoc: '/proj/.ody-code/office-hours/2026-06-16-test.md',
      assignment: 'Build the MVP',
      resourcesShown: ['https://example.com/resource1'],
      topics: ['saas', 'b2b'],
    };
    await store.appendProfile(entry);
    const entries = await store.readProfile();
    expect(entries).toHaveLength(1);
    expect(entries[0]!.projectSlug).toBe('my-project');
    expect(entries[0]!.signalCount).toBe(3);
  });

  it('appendAnalytics writes to analytics.jsonl', async () => {
    const kaos = mockKaos();
    const store = new FileSystemOfficeHoursStateStore(kaos as any, '/fake/home/.ody-code');
    await store.appendAnalytics({
      ts: '2026-06-16T12:00:00.000Z',
      skill: 'office-hours',
      event: 'started',
      branch: 'main',
      session: 's1',
    });
    const writeCalls = vi.mocked(kaos.writeText).mock.calls.filter(
      (c: unknown[]) => typeof c[0] === 'string' && (c[0] as string).includes('analytics'),
    );
    expect(writeCalls.length).toBeGreaterThan(0);
  });

  it('appendLearning and searchLearnings', async () => {
    const kaos = mockKaos();
    const store = new FileSystemOfficeHoursStateStore(kaos as any, '/fake/home/.ody-code');
    await store.appendLearning({
      ts: '2026-06-16T12:00:00.000Z',
      skill: 'office-hours',
      type: 'eureka',
      key: 'pricing-model',
      insight: 'Freemium works better than trial for this segment',
      confidence: 0.8,
      source: 'observed',
    });
    const results = await store.searchLearnings({ limit: 5 });
    expect(results).toHaveLength(1);
    expect(results[0]!.key).toBe('pricing-model');
  });
});

describe('computeTier', () => {
  it('0 entries → introduction', () => {
    expect(computeTier([]).tier).toBe('introduction');
  });
  it('3 entries → welcome_back', () => {
    expect(computeTier([{}, {}, {}] as unknown as BuilderProfileEntry[]).tier).toBe('welcome_back');
  });
  it('8 entries → inner_circle', () => {
    expect(computeTier(Array(8).fill({}) as unknown as BuilderProfileEntry[]).tier).toBe('inner_circle');
  });
});

describe('selectResources', () => {
  const candidates = [
    { url: 'https://a.com', category: 'essay' },
    { url: 'https://b.com', category: 'video' },
    { url: 'https://c.com', category: 'essay' },
    { url: 'https://d.com', category: 'book' },
  ];
  it('filters already-shown resources', () => {
    const shown = [{ resourcesShown: ['https://a.com'] }] as unknown as BuilderProfileEntry[];
    const result = selectResources(shown, candidates);
    expect(result.find((r) => r.url === 'https://a.com')).toBeUndefined();
  });
  it('returns empty when 34+ already shown', () => {
    const shown = [
      { resourcesShown: Array(34).fill('https://x.com') },
    ] as unknown as BuilderProfileEntry[];
    expect(selectResources(shown, candidates)).toHaveLength(0);
  });
});

describe('NoopOfficeHoursStateStore', () => {
  it('does not throw on any method', async () => {
    const store = new NoopOfficeHoursStateStore();
    await expect(store.appendProfile({} as BuilderProfileEntry)).resolves.toBeUndefined();
    await expect(store.readProfile()).resolves.toEqual([]);
  });
});
