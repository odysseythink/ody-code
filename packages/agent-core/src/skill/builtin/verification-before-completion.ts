import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import VERIFICATION_BEFORE_COMPLETION_BODY from './verification-before-completion.md';

const PSEUDO_PATH = 'builtin://verification-before-completion';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/verification-before-completion.md',
  skillDirName: 'verification-before-completion',
  source: 'builtin',
  text: VERIFICATION_BEFORE_COMPLETION_BODY,
});

export const VERIFICATION_BEFORE_COMPLETION_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
    hiddenInModes: ['plan', 'design'],
  },
};
