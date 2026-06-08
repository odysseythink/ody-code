import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import USING_GIT_WORKTREES_BODY from './using-git-worktrees.md';

const PSEUDO_PATH = 'builtin://using-git-worktrees';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/using-git-worktrees.md',
  skillDirName: 'using-git-worktrees',
  source: 'builtin',
  text: USING_GIT_WORKTREES_BODY,
});

export const USING_GIT_WORKTREES_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};
