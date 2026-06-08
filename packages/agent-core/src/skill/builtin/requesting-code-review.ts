import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import REQUESTING_CODE_REVIEW_BODY from './requesting-code-review.md';

const PSEUDO_PATH = 'builtin://requesting-code-review';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/requesting-code-review.md',
  skillDirName: 'requesting-code-review',
  source: 'builtin',
  text: REQUESTING_CODE_REVIEW_BODY,
});

export const REQUESTING_CODE_REVIEW_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};
