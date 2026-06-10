import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import EXECUTING_PLANS_BODY from './executing-plans.md';

const PSEUDO_PATH = 'builtin://executing-plans';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/executing-plans.md',
  skillDirName: 'executing-plans',
  source: 'builtin',
  text: EXECUTING_PLANS_BODY,
});

export const EXECUTING_PLANS_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
    hiddenInModes: ['plan', 'design'],
  },
};
