import { dump as dumpYaml } from 'js-yaml';
import { join } from 'pathe';
import type { Kaos } from '@odysseythink/kaos';

import type { ContextMessage } from '../../../agent/context/types';
import {
  formatDatePrefix,
  slugifyTitle,
  stripDatePrefix,
} from '../../../agent/session-mode/topic-generator';
import { ensureGitignore } from '../../../utils/gitignore';

export type IdeaReportType = 'generator' | 'evaluator';

export interface SaveIdeaReportInput {
  readonly title: string;
  readonly content: string;
  readonly type: IdeaReportType;
  readonly score?: number;
  readonly tags?: readonly string[];
}

export const IDEA_SKILL_NAMES: readonly string[] = ['idea-generator', 'idea-evaluator'];

export const SENSITIVE_TITLE_WORDS = [
  'key',
  'token',
  'password',
  'secret',
  'credential',
] as const;

export const MAX_SUFFIX = 1000;

export function validateIdeaReportInput(
  input: unknown,
): { ok: true; data: SaveIdeaReportInput } | { ok: false; error: string } {
  if (input === null || typeof input !== 'object') {
    return { ok: false, error: 'Input must be an object' };
  }
  const record = input as Record<string, unknown>;

  if (typeof record['title'] !== 'string' || (record['title'] as string).trim().length === 0) {
    return { ok: false, error: 'title is required and must be non-empty' };
  }
  const title = (record['title'] as string).trim();
  const lowerTitle = title.toLowerCase();
  if (SENSITIVE_TITLE_WORDS.some((word) => new RegExp(`\\b${word}\\b`).test(lowerTitle))) {
    return { ok: false, error: 'title contains sensitive words; provide a different title' };
  }

  if (typeof record['content'] !== 'string') {
    return { ok: false, error: 'content must be a string' };
  }

  if (record['type'] !== 'generator' && record['type'] !== 'evaluator') {
    return { ok: false, error: 'type must be "generator" or "evaluator"' };
  }

  if (record['score'] !== undefined) {
    if (
      typeof record['score'] !== 'number' ||
      !Number.isFinite(record['score']) ||
      record['score'] < 0 ||
      record['score'] > 10
    ) {
      return { ok: false, error: 'score must be a number between 0 and 10' };
    }
  }

  let tags: string[] | undefined;
  if (record['tags'] !== undefined) {
    if (!Array.isArray(record['tags'])) {
      return { ok: false, error: 'tags must be an array of strings' };
    }
    const seen = new Set<string>();
    tags = [];
    for (const raw of record['tags'] as unknown[]) {
      if (typeof raw !== 'string') continue;
      const tag = raw.trim();
      if (tag.length === 0 || seen.has(tag)) continue;
      seen.add(tag);
      tags.push(tag);
    }
  }

  return {
    ok: true,
    data: {
      title,
      content: record['content'] as string,
      type: record['type'] as IdeaReportType,
      score: record['score'] as number | undefined,
      tags,
    },
  };
}

export async function generateIdeaFilePath(
  ideasDir: string,
  title: string,
  now: Date,
  exists: (path: string) => Promise<boolean>,
): Promise<string> {
  let slug = slugifyTitle(title);
  slug = stripDatePrefix(slug);
  const baseStem = `${formatDatePrefix(now)}-${slug || 'untitled'}`;

  let stem = baseStem;
  for (let suffix = 1; suffix <= MAX_SUFFIX; suffix++) {
    const candidate = join(ideasDir, `${stem}.md`);
    if (!(await exists(candidate))) {
      return candidate;
    }
    stem = `${baseStem}-${suffix}`;
  }
  return join(ideasDir, `${baseStem}-${Date.now()}.md`);
}

export function buildIdeaReportBody(input: SaveIdeaReportInput, now: Date): string {
  const frontmatter: Record<string, unknown> = {
    title: input.title,
    type: input.type,
    date: now.toISOString(),
  };
  if (input.score !== undefined) {
    frontmatter['score'] = input.score;
  }
  if (input.tags !== undefined) {
    frontmatter['tags'] = input.tags;
  }
  const yaml = dumpYaml(frontmatter, { lineWidth: -1, noRefs: true }).trim();
  return `---\n${yaml}\n---\n\n${input.content.trim()}\n`;
}

export function isIdeaSkillActive(history: readonly ContextMessage[]): boolean {
  for (let index = history.length - 1; index >= 0; index--) {
    const message = history[index];
    if (message?.role !== 'user') continue;
    const origin = message.origin;
    if (origin?.kind !== 'skill_activation') continue;
    return IDEA_SKILL_NAMES.includes(origin.skillName);
  }
  return false;
}

export async function ensureIdeasDirectory(
  cwd: string,
  kaos: Pick<Kaos, 'mkdir' | 'readText' | 'writeText'>,
): Promise<string> {
  const ideasDir = join(cwd, '.ody-code', 'ideas');
  await kaos.mkdir(ideasDir, { parents: true, existOk: true });
  await ensureGitignore(cwd, kaos);
  return ideasDir;
}
