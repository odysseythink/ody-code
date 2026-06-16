import { join } from 'pathe';
import type { Kaos } from '@odysseythink/kaos';

export interface BuilderProfileEntry {
  readonly date: string;
  readonly mode: 'startup' | 'builder';
  readonly projectSlug: string;
  readonly signalCount: number;
  readonly signals: readonly string[];
  readonly designDoc: string;
  readonly assignment: string;
  readonly resourcesShown: readonly string[];
  readonly topics: readonly string[];
}

export interface OfficeHoursAnalyticsEvent {
  readonly ts: string;
  readonly skill: 'office-hours';
  readonly event: 'started' | 'completed' | 'resources_shown' | 'phase_changed';
  readonly branch: string;
  readonly session: string;
  readonly duration_s?: number;
  readonly outcome?: 'success' | 'abort' | 'unknown';
  readonly count?: number;
  readonly categories?: string;
}

export interface LearningEntry {
  readonly ts: string;
  readonly skill: 'office-hours';
  readonly type: 'operational' | 'eureka';
  readonly key: string;
  readonly insight: string;
  readonly confidence: number;
  readonly source: 'observed';
  readonly branch?: string;
}

export type Tier = 'introduction' | 'welcome_back' | 'regular' | 'inner_circle';

export interface Resource {
  readonly url: string;
  readonly category: string;
}

export interface OfficeHoursStateStore {
  appendProfile(entry: BuilderProfileEntry): Promise<void>;
  readProfile(): Promise<readonly BuilderProfileEntry[]>;
  appendAnalytics(event: OfficeHoursAnalyticsEvent): Promise<void>;
  appendLearning(entry: LearningEntry): Promise<void>;
  searchLearnings(options: {
    limit: number;
    crossProject?: boolean;
  }): Promise<readonly LearningEntry[]>;
  getSessionSummary(): Promise<{ sessionCount: number; tier: Tier }>;
}

export class FileSystemOfficeHoursStateStore implements OfficeHoursStateStore {
  private readonly baseDir: string;

  constructor(
    private readonly kaos: Kaos,
    homeDir: string,
  ) {
    this.baseDir = join(homeDir, 'office-hours');
  }

  private profilePath(): string {
    return join(this.baseDir, 'builder-profile.jsonl');
  }

  private analyticsPath(): string {
    return join(this.baseDir, 'analytics.jsonl');
  }

  private learningsPath(): string {
    return join(this.baseDir, 'learnings.jsonl');
  }

  private async ensureDir(): Promise<void> {
    await this.kaos.mkdir(this.baseDir, { parents: true, existOk: true });
  }

  async appendProfile(entry: BuilderProfileEntry): Promise<void> {
    await this.ensureDir();
    await this.kaos.writeText(this.profilePath(), JSON.stringify(entry) + '\n', {
      mode: 'a',
    });
  }

  async readProfile(): Promise<readonly BuilderProfileEntry[]> {
    try {
      const text = await this.kaos.readText(this.profilePath());
      return text
        .split('\n')
        .filter((line) => line.trim().length > 0)
        .map((line) => JSON.parse(line) as BuilderProfileEntry);
    } catch {
      return [];
    }
  }

  async appendAnalytics(event: OfficeHoursAnalyticsEvent): Promise<void> {
    await this.ensureDir();
    await this.kaos.writeText(this.analyticsPath(), JSON.stringify(event) + '\n', {
      mode: 'a',
    });
  }

  async appendLearning(entry: LearningEntry): Promise<void> {
    await this.ensureDir();
    await this.kaos.writeText(this.learningsPath(), JSON.stringify(entry) + '\n', {
      mode: 'a',
    });
  }

  async searchLearnings(options: {
    limit: number;
    crossProject?: boolean;
  }): Promise<readonly LearningEntry[]> {
    try {
      const text = await this.kaos.readText(this.learningsPath());
      const entries = text
        .split('\n')
        .filter((line) => line.trim().length > 0)
        .map((line) => JSON.parse(line) as LearningEntry);
      return entries.slice(-options.limit).reverse();
    } catch {
      return [];
    }
  }

  async getSessionSummary(): Promise<{ sessionCount: number; tier: Tier }> {
    const entries = await this.readProfile();
    return computeTier(entries);
  }
}

export class NoopOfficeHoursStateStore implements OfficeHoursStateStore {
  async appendProfile(_entry: BuilderProfileEntry): Promise<void> {}
  async readProfile(): Promise<readonly BuilderProfileEntry[]> {
    return [];
  }
  async appendAnalytics(_event: OfficeHoursAnalyticsEvent): Promise<void> {}
  async appendLearning(_entry: LearningEntry): Promise<void> {}
  async searchLearnings(_options: {
    limit: number;
    crossProject?: boolean;
  }): Promise<readonly LearningEntry[]> {
    return [];
  }
  async getSessionSummary(): Promise<{ sessionCount: number; tier: Tier }> {
    return { sessionCount: 0, tier: 'introduction' };
  }
}

export function computeTier(
  entries: readonly BuilderProfileEntry[],
): { tier: Tier; sessionCount: number } {
  const sessionCount = entries.length;
  if (sessionCount === 0) return { tier: 'introduction', sessionCount };
  if (sessionCount <= 3) return { tier: 'welcome_back', sessionCount };
  if (sessionCount <= 7) return { tier: 'regular', sessionCount };
  return { tier: 'inner_circle', sessionCount };
}

export function selectResources(
  profileEntries: readonly BuilderProfileEntry[],
  candidates: Resource[],
): Resource[] {
  const shown = new Set(profileEntries.flatMap((e) => e.resourcesShown));
  if (profileEntries.reduce((sum, e) => sum + e.resourcesShown.length, 0) >= 34) return [];
  const available = candidates.filter((r) => !shown.has(r.url));
  // Mix categories: never 3 of same type. Pick up to 3 with category diversity.
  const result: Resource[] = [];
  const usedCategories = new Set<string>();
  for (const r of available) {
    if (result.length >= 3) break;
    if (usedCategories.has(r.category) && usedCategories.size < available.length) continue;
    result.push(r);
    usedCategories.add(r.category);
  }
  return result;
}
