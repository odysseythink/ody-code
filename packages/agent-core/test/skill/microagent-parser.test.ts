import { describe, expect, it } from 'vitest';

import { parseSkillText } from '../../src/skill/parser';
import { isKnowledgeSkillType, isSupportedSkillType } from '../../src/skill/types';

// ---- Type helpers ----

describe('isKnowledgeSkillType', () => {
  it('returns true for "knowledge"', () => {
    expect(isKnowledgeSkillType('knowledge')).toBe(true);
  });

  it('returns false for undefined', () => {
    expect(isKnowledgeSkillType(undefined)).toBe(false);
  });

  it('returns false for "prompt"', () => {
    expect(isKnowledgeSkillType('prompt')).toBe(false);
  });

  it('returns false for "flow"', () => {
    expect(isKnowledgeSkillType('flow')).toBe(false);
  });
});

describe('isSupportedSkillType', () => {
  it('accepts knowledge alongside existing types', () => {
    expect(isSupportedSkillType('knowledge')).toBe(true);
    expect(isSupportedSkillType('prompt')).toBe(true);
    expect(isSupportedSkillType('flow')).toBe(true);
    expect(isSupportedSkillType(undefined)).toBe(true); // undefined → inline
  });

  it('rejects unknown types', () => {
    expect(isSupportedSkillType('garbage')).toBe(false);
  });
});

// ---- Parser: knowledge microagent validation ----

function skillText(lines: string[]): string {
  return lines.join('\n');
}

function parse(text: string): ReturnType<typeof parseSkillText> {
  return parseSkillText({
    text,
    skillMdPath: '/tmp/test.md',
    skillDirName: 'test',
    source: 'project',
  });
}

describe('parseSkillText with type: knowledge', () => {
  // P1: valid knowledge microagent
  it('parses a valid knowledge microagent with normalized triggers', () => {
    const skill = parse(skillText([
      '---',
      'type: knowledge',
      'triggers:',
      '  -  Page ',
      '  - PAGE',
      '  - component',
      '---',
      '这是知识内容。',
    ]));

    expect(skill.metadata.type).toBe('knowledge');
    expect(skill.metadata.triggers).toEqual(['component', 'page']);
    expect(skill.content).toBe('这是知识内容。');
  });

  // P2: triggers with mixed case, whitespace, and duplicates → normalized
  it('normalizes triggers: lowercased, trimmed, deduplicated, sorted', () => {
    const skill = parse(skillText([
      '---',
      'type: knowledge',
      'triggers:',
      '  -  Page ',
      '  - PAGE',
      '  - component',
      '---',
      'Body',
    ]));

    expect(skill.metadata.triggers).toEqual(['component', 'page']);
  });

  // P3: missing triggers rejected
  it('rejects knowledge microagent without triggers', () => {
    expect(() =>
      parse(skillText(['---', 'type: knowledge', '---', 'Body'])),
    ).toThrow(/triggers/);
  });

  // P4: empty trigger string rejected
  it('rejects knowledge microagent with an empty trigger string', () => {
    expect(() =>
      parse(skillText([
        '---',
        'type: knowledge',
        'triggers:',
        '  - ""',
        '---',
        'Body',
      ])),
    ).toThrow(/triggers/);
  });

  // P5: non-array triggers rejected
  it('rejects knowledge microagent with non-array triggers', () => {
    expect(() =>
      parse(skillText([
        '---',
        'type: knowledge',
        'triggers: not-an-array',
        '---',
        'Body',
      ])),
    ).toThrow(/triggers/);
  });
});


// ---- Discovery tests ----

import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'pathe';
import { afterEach } from 'vitest';

import { discoverSkills, resolveSkillRoots } from '../../src/skill';

const microagentTempDirs: string[] = [];

afterEach(async () => {
  for (const dir of microagentTempDirs.splice(0)) {
    await rm(dir, { recursive: true, force: true });
  }
});

async function makeMicroagentWorkspace(): Promise<{
  homeDir: string; repoDir: string; workDir: string;
}> {
  const tmp = await mkdtemp(path.join(tmpdir(), 'kimi-microagent-'));
  microagentTempDirs.push(tmp);
  const homeDir = path.join(tmp, 'home');
  const repoDir = path.join(tmp, 'repo');
  const workDir = path.join(repoDir, 'packages', 'app');
  await mkdir(path.join(repoDir, '.git'), { recursive: true });
  await mkdir(workDir, { recursive: true });
  return { homeDir, repoDir, workDir };
}

describe('microagent discovery', () => {
  // D1: .ody-code/microagents/ root discovered
  it('discovers .ody-code/microagents as a project skill root', async () => {
    const { homeDir, repoDir, workDir } = await makeMicroagentWorkspace();
    const microagentsDir = path.join(repoDir, '.ody-code', 'microagents');
    await mkdir(microagentsDir, { recursive: true });

    const roots = await resolveSkillRoots({
      paths: { userHomeDir: homeDir, workDir },
    });

    const microagentRoot = roots.find(
      (r) => r.path.endsWith('.ody-code/microagents') && r.source === 'project',
    );
    expect(microagentRoot).toBeDefined();
  });

  // D2: microagents loaded via discoverSkills
  it('loads flat .md microagents from .ody-code/microagents', async () => {
    const { homeDir, repoDir, workDir } = await makeMicroagentWorkspace();
    const microagentsDir = path.join(repoDir, '.ody-code', 'microagents');
    await mkdir(microagentsDir, { recursive: true });
    await writeFile(
      path.join(microagentsDir, 'reuse-conventions.md'),
      [
        '---',
        'type: knowledge',
        'triggers:',
        '  - reuse',
        '  - conventions',
        '---',
        'Prefer existing utilities.',
      ].join('\n'),
    );

    const roots = await resolveSkillRoots({
      paths: { userHomeDir: homeDir, workDir },
    });
    const skills = await discoverSkills({ roots });

    const reuse = skills.find((s) => s.name === 'reuse-conventions');
    expect(reuse).toBeDefined();
    expect(reuse?.metadata.type).toBe('knowledge');
    expect(reuse?.metadata.triggers).toEqual(['conventions', 'reuse']);
    expect(reuse?.content).toBe('Prefer existing utilities.');
  });

  // D3: invalid microagent skipped with warning
  it('skips invalid microagent and calls onWarning', async () => {
    const { homeDir, repoDir, workDir } = await makeMicroagentWorkspace();
    const microagentsDir = path.join(repoDir, '.ody-code', 'microagents');
    await mkdir(microagentsDir, { recursive: true });
    await writeFile(
      path.join(microagentsDir, 'bad-triggers.md'),
      [
        '---',
        'type: knowledge',
        'triggers: not-an-array',
        '---',
        'Body',
      ].join('\n'),
    );

    const roots = await resolveSkillRoots({
      paths: { userHomeDir: homeDir, workDir },
    });
    const warnings: string[] = [];
    const skills = await discoverSkills({
      roots,
      onWarning: (msg) => warnings.push(msg),
    });

    expect(skills.find((s) => s.name === 'bad-triggers')).toBeUndefined();
    expect(warnings.length).toBeGreaterThanOrEqual(1);
    expect(warnings.some((w) => w.includes('bad-triggers'))).toBe(true);
  });
});


// ---- Registry tests ----

import { SkillRegistry } from '../../src/skill/registry';

describe('SkillRegistry with knowledge microagents', () => {
  // R1: invocable skills exclude knowledge
  it('excludes knowledge microagents from listInvocableSkills', () => {
    const registry = new SkillRegistry();
    registry.register(makeKnowledgeSkill('reuse'));
    registry.register(makePromptSkill('greet'));

    const invocable = registry.listInvocableSkills();
    expect(invocable.map((s) => s.name)).toEqual(['greet']);
    expect(invocable.map((s) => s.name)).not.toContain('reuse');
  });

  // R2: listKnowledgeMicroagents returns only knowledge skills
  it('listKnowledgeMicroagents returns only knowledge-typed skills', () => {
    const registry = new SkillRegistry();
    registry.register(makeKnowledgeSkill('reuse'));
    registry.register(makeKnowledgeSkill('conventions'));
    registry.register(makePromptSkill('greet'));

    const microagents = registry.listKnowledgeMicroagents();
    expect(microagents.map((s) => s.name).toSorted()).toEqual([
      'conventions',
      'reuse',
    ]);
  });

  // R3: skill wins over same-name microagent
  it('skill wins over same-name microagent via first-wins registration', () => {
    const registry = new SkillRegistry();
    // Register skill first (simulating load order: skills before microagents)
    registry.register(makePromptSkill('collision'));
    registry.register(makeKnowledgeSkill('collision'));

    const skill = registry.getSkill('collision');
    expect(skill?.metadata.type).toBe('prompt');
  });
});

// ---- Activation guard test ----

import { ErrorCodes, OdyError } from '../../src/errors';
import { SkillManager } from '../../src/agent/skill';
import type { Agent } from '../../src/agent';

describe('SkillManager activation guard', () => {
  // A1: knowledge microagent cannot be activated
  it('throws SKILL_TYPE_UNSUPPORTED when activating a knowledge microagent', () => {
    const registry = new SkillRegistry();
    registry.register(makeKnowledgeSkill('reuse'));

    // Minimal Agent stub — SkillManager.activate only uses registry.getSkill
    // and checks isUserActivatableSkillType; it does not access other Agent fields
    // for this error path.
    const agent = { emitEvent: () => {}, telemetry: { track: () => {} } } as unknown as Agent;
    const manager = new SkillManager(agent, registry);

    expect(() =>
      manager.activate({ name: 'reuse' } as any),
    ).toThrow(OdyError);

    try {
      manager.activate({ name: 'reuse' } as any);
    } catch (error) {
      expect(error).toBeInstanceOf(OdyError);
      expect((error as OdyError).code).toBe(ErrorCodes.SKILL_TYPE_UNSUPPORTED);
    }
  });

  it('can still activate a regular prompt skill', () => {
    const registry = new SkillRegistry();
    registry.register(makePromptSkill('greet'));

    // The activate method will call agent.turn.prompt() after registry.renderSkillPrompt.
    // We don't need a full Agent — we just verify the guard does NOT throw for prompt.
    const agent = {
      emitEvent: () => {},
      telemetry: { track: () => {} },
      turn: { prompt: () => {} },
    } as unknown as Agent;
    const manager = new SkillManager(agent, registry);

    expect(() =>
      manager.activate({ name: 'greet' } as any),
    ).not.toThrow();
  });
});

// ---- Test helpers for registry/activation tests ----

import type { SkillDefinition } from '../../src/skill/types';

function makeKnowledgeSkill(name: string): SkillDefinition {
  return {
    name,
    description: `Knowledge about ${name}`,
    path: `/tmp/${name}.md`,
    dir: '/tmp',
    content: `Content for ${name}`,
    metadata: { type: 'knowledge', triggers: ['test'] },
    source: 'project',
  };
}

function makePromptSkill(name: string): SkillDefinition {
  return {
    name,
    description: `Skill: ${name}`,
    path: `/tmp/${name}.md`,
    dir: '/tmp',
    content: `Content for ${name}`,
    metadata: { type: 'prompt' },
    source: 'project',
  };
}
