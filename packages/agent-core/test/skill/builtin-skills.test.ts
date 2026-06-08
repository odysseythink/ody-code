import { describe, expect, it } from 'vitest';
import { SkillRegistry } from '../../src/skill/registry';
import {
  DISPATCHING_PARALLEL_AGENTS_SKILL,
  EXECUTING_PLANS_SKILL,
  FINISHING_A_DEVELOPMENT_BRANCH_SKILL,
  MCP_CONFIG_SKILL,
  RECEIVING_CODE_REVIEW_SKILL,
  REQUESTING_CODE_REVIEW_SKILL,
  SUBAGENT_DRIVEN_DEVELOPMENT_SKILL,
  SYNC_CHANGELOG_SKILL,
  SYSTEMATIC_DEBUGGING_SKILL,
  TEST_DRIVEN_DEVELOPMENT_SKILL,
  USING_GIT_WORKTREES_SKILL,
  VERIFICATION_BEFORE_COMPLETION_SKILL,
} from '../../src/skill/builtin';

const BUILTIN_SKILLS = [
  { skill: DISPATCHING_PARALLEL_AGENTS_SKILL, name: 'dispatching-parallel-agents' },
  { skill: EXECUTING_PLANS_SKILL, name: 'executing-plans' },
  { skill: FINISHING_A_DEVELOPMENT_BRANCH_SKILL, name: 'finishing-a-development-branch' },
  { skill: MCP_CONFIG_SKILL, name: 'mcp-config' },
  { skill: RECEIVING_CODE_REVIEW_SKILL, name: 'receiving-code-review' },
  { skill: REQUESTING_CODE_REVIEW_SKILL, name: 'requesting-code-review' },
  { skill: SUBAGENT_DRIVEN_DEVELOPMENT_SKILL, name: 'subagent-driven-development' },
  { skill: SYNC_CHANGELOG_SKILL, name: 'sync-changelog' },
  { skill: SYSTEMATIC_DEBUGGING_SKILL, name: 'systematic-debugging' },
  { skill: TEST_DRIVEN_DEVELOPMENT_SKILL, name: 'test-driven-development' },
  { skill: USING_GIT_WORKTREES_SKILL, name: 'using-git-worktrees' },
  { skill: VERIFICATION_BEFORE_COMPLETION_SKILL, name: 'verification-before-completion' },
];

describe('built-in skills', () => {
  it('has exactly 12 built-in skills', () => {
    expect(BUILTIN_SKILLS).toHaveLength(12);
  });

  it.each(BUILTIN_SKILLS)('skill "$name" has correct metadata', ({ skill, name }) => {
    expect(skill.name).toBe(name);
    expect(skill.source).toBe('builtin');
    expect(skill.path).toBe(`builtin://${name}`);
    expect(skill.dir).toBe(`builtin://${name}`);
    expect(skill.content.length).toBeGreaterThan(0);
    expect(skill.description.length).toBeGreaterThan(0);
  });

  it('all skills are sorted alphabetically in listSkills', () => {
    const registry = new SkillRegistry();
    for (const { skill } of BUILTIN_SKILLS) {
      registry.registerBuiltinSkill(skill);
    }
    const listed = registry.listSkills();
    const names = listed.map((s) => s.name);
    const sorted = [...names].sort((a, b) => a.localeCompare(b));
    expect(names).toEqual(sorted);
  });
});
