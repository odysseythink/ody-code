import { describe, expect, it } from 'vitest';
import { SkillRegistry } from '#/skill/registry';
import { registerBuiltinSkills } from '#/skill/builtin/index';

describe('debt-ledger skill', () => {
  it('is registered as a builtin skill', () => {
    const registry = new SkillRegistry();
    registerBuiltinSkills(registry);

    const skill = registry.getSkill('debt-ledger');
    expect(skill).toBeDefined();
    expect(skill!.name).toBe('debt-ledger');
    expect(skill!.metadata.type).toBe('inline');
    expect(skill!.description).toContain('ody:');
  });

  it('is an invocable skill', () => {
    const registry = new SkillRegistry();
    registerBuiltinSkills(registry);

    const invocable = registry.listInvocableSkills();
    expect(invocable.some((s) => s.name === 'debt-ledger')).toBe(true);
  });
});
