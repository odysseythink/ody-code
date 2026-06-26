import type { SkillRegistry } from '../registry';
import { DISPATCHING_PARALLEL_AGENTS_SKILL } from './dispatching-parallel-agents';
import { EXECUTING_PLANS_SKILL } from './executing-plans';
import { FINISHING_A_DEVELOPMENT_BRANCH_SKILL } from './finishing-a-development-branch';
import { IDEA_EVALUATOR_SKILL } from './idea-evaluator';
import { IDEA_GENERATOR_SKILL } from './idea-generator';
import { MCP_CONFIG_SKILL } from '@odysseythink/mcp-host';
import { RECEIVING_CODE_REVIEW_SKILL } from './receiving-code-review';
import { REQUESTING_CODE_REVIEW_SKILL } from './requesting-code-review';
import { SIMPLICITY_FIRST_SKILL } from './simplicity-first';
import { SUBAGENT_DRIVEN_DEVELOPMENT_SKILL } from './subagent-driven-development';
import { SYNC_CHANGELOG_SKILL } from './sync-changelog';
import { SYSTEMATIC_DEBUGGING_SKILL } from './systematic-debugging';
import { TEST_DRIVEN_DEVELOPMENT_SKILL } from './test-driven-development';
import { USING_GIT_WORKTREES_SKILL } from './using-git-worktrees';
import { VERIFICATION_BEFORE_COMPLETION_SKILL } from './verification-before-completion';
import { DEBT_LEDGER_SKILL } from './debt-ledger';
import { ROADMAP_ARCHITECT_SKILL } from './roadmap-architect';
import { registerGameDesignSkills } from './game-design-skills';

export function registerBuiltinSkills(registry: SkillRegistry): void {
  registry.registerBuiltinSkill(DISPATCHING_PARALLEL_AGENTS_SKILL);
  registry.registerBuiltinSkill(EXECUTING_PLANS_SKILL);
  registry.registerBuiltinSkill(FINISHING_A_DEVELOPMENT_BRANCH_SKILL);
  registry.registerBuiltinSkill(IDEA_EVALUATOR_SKILL);
  registry.registerBuiltinSkill(IDEA_GENERATOR_SKILL);
  registry.registerBuiltinSkill(MCP_CONFIG_SKILL);
  registry.registerBuiltinSkill(RECEIVING_CODE_REVIEW_SKILL);
  registry.registerBuiltinSkill(REQUESTING_CODE_REVIEW_SKILL);
  registry.registerBuiltinSkill(SIMPLICITY_FIRST_SKILL);
  registry.registerBuiltinSkill(SUBAGENT_DRIVEN_DEVELOPMENT_SKILL);
  registry.registerBuiltinSkill(SYNC_CHANGELOG_SKILL);
  registry.registerBuiltinSkill(SYSTEMATIC_DEBUGGING_SKILL);
  registry.registerBuiltinSkill(TEST_DRIVEN_DEVELOPMENT_SKILL);
  registry.registerBuiltinSkill(USING_GIT_WORKTREES_SKILL);
  registry.registerBuiltinSkill(VERIFICATION_BEFORE_COMPLETION_SKILL);
  registry.registerBuiltinSkill(DEBT_LEDGER_SKILL);
  registry.registerBuiltinSkill(ROADMAP_ARCHITECT_SKILL);
  registerGameDesignSkills(registry);
}

export {
  DISPATCHING_PARALLEL_AGENTS_SKILL,
  EXECUTING_PLANS_SKILL,
  FINISHING_A_DEVELOPMENT_BRANCH_SKILL,
  IDEA_EVALUATOR_SKILL,
  IDEA_GENERATOR_SKILL,
  MCP_CONFIG_SKILL,
  RECEIVING_CODE_REVIEW_SKILL,
  REQUESTING_CODE_REVIEW_SKILL,
  SIMPLICITY_FIRST_SKILL,
  SUBAGENT_DRIVEN_DEVELOPMENT_SKILL,
  SYNC_CHANGELOG_SKILL,
  SYSTEMATIC_DEBUGGING_SKILL,
  TEST_DRIVEN_DEVELOPMENT_SKILL,
  USING_GIT_WORKTREES_SKILL,
  VERIFICATION_BEFORE_COMPLETION_SKILL,
  DEBT_LEDGER_SKILL,
  ROADMAP_ARCHITECT_SKILL,
};
