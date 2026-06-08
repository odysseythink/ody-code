import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import FINISHING_A_DEVELOPMENT_BRANCH_BODY from './finishing-a-development-branch.md';

const PSEUDO_PATH = 'builtin://finishing-a-development-branch';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/finishing-a-development-branch.md',
  skillDirName: 'finishing-a-development-branch',
  source: 'builtin',
  text: FINISHING_A_DEVELOPMENT_BRANCH_BODY,
});

export const FINISHING_A_DEVELOPMENT_BRANCH_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};
