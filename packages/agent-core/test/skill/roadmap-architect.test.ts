import { describe, expect, it } from 'vitest';
import { SkillRegistry } from '../../src/skill/registry';
import { ROADMAP_ARCHITECT_SKILL, registerBuiltinSkills } from '../../src/skill/builtin';

describe('roadmap-architect built-in skill', () => {
  it('parses with the expected built-in metadata', () => {
    expect(ROADMAP_ARCHITECT_SKILL.name).toBe('roadmap-architect');
    expect(ROADMAP_ARCHITECT_SKILL.source).toBe('builtin');
    expect(ROADMAP_ARCHITECT_SKILL.path).toBe('builtin://roadmap-architect');
    expect(ROADMAP_ARCHITECT_SKILL.dir).toBe('builtin://roadmap-architect');
    expect(ROADMAP_ARCHITECT_SKILL.description.length).toBeGreaterThan(0);
    expect(ROADMAP_ARCHITECT_SKILL.content.length).toBeGreaterThan(0);
  });

  it('is registered by registerBuiltinSkills and listed alphabetically', () => {
    const registry = new SkillRegistry();
    registerBuiltinSkills(registry);
    const names = registry.listSkills().map((s) => s.name);
    expect(names).toContain('roadmap-architect');
    const sorted = [...names].sort((a, b) => a.localeCompare(b));
    expect(names).toEqual(sorted);
  });
});
