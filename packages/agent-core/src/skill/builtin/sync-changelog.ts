import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import SYNC_CHANGELOG_BODY from './sync-changelog.md';

const PSEUDO_PATH = 'builtin://sync-changelog';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/sync-changelog.md',
  skillDirName: 'sync-changelog',
  source: 'builtin',
  text: SYNC_CHANGELOG_BODY,
});

export const SYNC_CHANGELOG_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};
