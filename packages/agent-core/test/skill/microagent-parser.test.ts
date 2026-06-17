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
