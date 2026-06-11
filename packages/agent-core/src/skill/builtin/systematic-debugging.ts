import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import SYSTEMATIC_DEBUGGING_BODY from './systematic-debugging.md';

const PSEUDO_PATH = 'builtin://systematic-debugging';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/systematic-debugging.md',
  skillDirName: 'systematic-debugging',
  source: 'builtin',
  text: SYSTEMATIC_DEBUGGING_BODY,
});

export const SYSTEMATIC_DEBUGGING_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
    hiddenInModes: ['plan', 'design'],
  },
};
