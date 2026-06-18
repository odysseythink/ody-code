import { describe, it, expect } from 'vitest';
import { SkillRegistry } from '../../src/skill/registry';
import { registerBuiltinSkills } from '../../src/skill/builtin';

describe('game-design skills', () => {
  it('are registered with hiddenInModes excluding game-design mode', () => {
    const registry = new SkillRegistry();
    registerBuiltinSkills(registry);

    // game-design skills should be visible in game-design mode
    const gdSkills = registry.listInvocableSkills('game-design');
    const gdNames = gdSkills.map((s) => s.name);
    expect(gdNames).toContain('game-design/flow-state-design-framework');
    expect(gdNames).toContain('game-design/game-design-methodology');
    expect(gdNames).toContain('game-design/skill');

    // Game-design skills should NOT be visible in normal mode
    const normalSkills = registry.listInvocableSkills('normal');
    const normalNames = new Set(normalSkills.map((s) => s.name));
    for (const name of gdNames) {
      if (name.startsWith('game-design/')) {
        expect(normalNames.has(name)).toBe(false);
      }
    }

    // Game-design skills should NOT be visible in plan mode
    const planSkills = registry.listInvocableSkills('plan');
    const planNames = new Set(planSkills.map((s) => s.name));
    for (const name of gdNames) {
      if (name.startsWith('game-design/')) {
        expect(planNames.has(name)).toBe(false);
      }
    }

    // Game-design skills should NOT be visible in office-hours mode
    const ohSkills = registry.listInvocableSkills('office-hours');
    const ohNames = new Set(ohSkills.map((s) => s.name));
    for (const name of gdNames) {
      if (name.startsWith('game-design/')) {
        expect(ohNames.has(name)).toBe(false);
      }
    }
  });

  it('skill names use game-design/ namespace', () => {
    const registry = new SkillRegistry();
    registerBuiltinSkills(registry);
    const gdSkills = registry.listInvocableSkills('game-design').filter((s) =>
      s.name.startsWith('game-design/'),
    );
    expect(gdSkills.length).toBeGreaterThan(0);
    for (const skill of gdSkills) {
      expect(skill.name).toMatch(/^game-design\//);
    }
  });

  it('companion files are included in the parent skill content', () => {
    const registry = new SkillRegistry();
    registerBuiltinSkills(registry);
    const gdSkills = registry.listInvocableSkills('game-design');
    const charOpt = gdSkills.find((s) => s.name === 'game-design/character-optimization-design');
    expect(charOpt).toBeDefined();
    expect(charOpt!.content.length).toBeGreaterThan(200);
  });
});
