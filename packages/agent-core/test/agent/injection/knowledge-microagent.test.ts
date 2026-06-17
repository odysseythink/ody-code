import { describe, expect, it } from 'vitest';

import type { ContextMessage } from '../../../src/agent/context';
import {
  extractLatestUserText,
  matchKnowledgeMicroagents,
  triggerMatches,
} from '../../../src/agent/injection/knowledge-microagent';
import type { SkillDefinition } from '../../../src/skill';

function microagent(
  name: string,
  triggers: readonly string[],
  content = '# Test\n\nSome body text.',
): SkillDefinition {
  return {
    name,
    description: `Knowledge: ${name}`,
    path: `/test/${name}.md`,
    dir: '/test',
    content,
    metadata: { type: 'knowledge', triggers },
    source: 'project',
  };
}

function userMessage(text: string): ContextMessage {
  return {
    role: 'user',
    content: [{ type: 'text', text }],
    toolCalls: [],
    origin: { kind: 'user' },
  };
}

function injectionMessage(variant: string): ContextMessage {
  return {
    role: 'user',
    content: [{ type: 'text', text: `<system-reminder>\nsome injection\n</system-reminder>` }],
    toolCalls: [],
    origin: { kind: 'injection', variant },
  };
}

function compactionSummaryMessage(): ContextMessage {
  return {
    role: 'user',
    content: [{ type: 'text', text: 'previous summary' }],
    toolCalls: [],
    origin: { kind: 'compaction_summary' },
  };
}

describe('triggerMatches', () => {
  it('M1: ASCII trigger matches standalone word', () => {
    expect(triggerMatches('add a component', 'component')).toBe(true);
  });

  it('M2a: ASCII trigger does not match as prefix', () => {
    expect(triggerMatches('add components', 'component')).toBe(false);
  });

  it('M2b: ASCII trigger does not match as suffix', () => {
    expect(triggerMatches('use the pager', 'page')).toBe(false);
  });

  it('M8: case-insensitive matching', () => {
    expect(triggerMatches('Add a COMPONENT', 'component')).toBe(true);
    expect(triggerMatches('add a component', 'Component')).toBe(true);
  });

  it('M3: Chinese trigger matches', () => {
    expect(triggerMatches('添加一个组件', '组件')).toBe(true);
  });

  it('M4: Chinese trigger does not match overlapping phrase', () => {
    expect(triggerMatches('添加一个组合件', '组件')).toBe(false);
  });

  // Must-survive inputs for word-boundary regex:
  // "component-based" contains "component" with hyphen as word boundary → matches
  it('word boundary: hyphen-separated word matches', () => {
    expect(triggerMatches('use component-based design', 'component')).toBe(true);
  });

  it('word boundary: "page" does not match "homepage"', () => {
    expect(triggerMatches('visit the homepage', 'page')).toBe(false);
  });
});

describe('extractLatestUserText', () => {
  it('returns text of latest user message', () => {
    const history: ContextMessage[] = [
      userMessage('first message'),
      { role: 'assistant', content: [{ type: 'text', text: 'ok' }], toolCalls: [] },
      userMessage('second message'),
    ];
    expect(extractLatestUserText(history)).toBe('second message');
  });

  it('skips injection origins', () => {
    const history: ContextMessage[] = [
      userMessage('real user message'),
      injectionMessage('knowledge_microagent'),
    ];
    expect(extractLatestUserText(history)).toBe('real user message');
  });

  it('skips compaction summary origins', () => {
    const history: ContextMessage[] = [
      userMessage('real user message'),
      compactionSummaryMessage(),
    ];
    expect(extractLatestUserText(history)).toBe('real user message');
  });

  it('returns undefined when no user message with real origin', () => {
    expect(extractLatestUserText([])).toBeUndefined();
    expect(extractLatestUserText([injectionMessage('x')])).toBeUndefined();
  });

  it('handles user messages with no origin', () => {
    const history: ContextMessage[] = [
      {
        role: 'user',
        content: [{ type: 'text', text: 'plain user message' }],
        toolCalls: [],
      },
    ];
    expect(extractLatestUserText(history)).toBe('plain user message');
  });
});

describe('matchKnowledgeMicroagents', () => {
  const agentA = microagent('agent-a', ['component', 'page']);
  const agentB = microagent('agent-b', ['database', 'sql']);
  const agentC = microagent('agent-c', ['组件']);
  const emptyBody = microagent('empty-agent', ['trigger'], '');

  it('M1: matches standalone word in message text', () => {
    const result = matchKnowledgeMicroagents({
      messageText: 'add a component to the page',
      microagents: [agentA, agentB],
      alreadyInjected: new Set(),
    });
    expect(result).toHaveLength(1);
    expect(result[0]!.skill.name).toBe('agent-a');
    expect(result[0]!.trigger).toBe('component');
  });

  it('M5: skips already-injected microagent', () => {
    const result = matchKnowledgeMicroagents({
      messageText: 'add a component',
      microagents: [agentA],
      alreadyInjected: new Set(['agent-a']),
    });
    expect(result).toHaveLength(0);
  });

  it('M6: multiple triggers, records first matching one', () => {
    const result = matchKnowledgeMicroagents({
      messageText: 'add a new page',
      microagents: [agentA],
      alreadyInjected: new Set(),
    });
    expect(result).toHaveLength(1);
    expect(result[0]!.trigger).toBe('page');
  });

  it('M7: multiple microagents can match', () => {
    const result = matchKnowledgeMicroagents({
      messageText: 'add a component and query the database',
      microagents: [agentA, agentB],
      alreadyInjected: new Set(),
    });
    expect(result).toHaveLength(2);
    const names = result.map((r) => r.skill.name).sort();
    expect(names).toEqual(['agent-a', 'agent-b']);
  });

  it('M3: Chinese trigger matches via substring', () => {
    const result = matchKnowledgeMicroagents({
      messageText: '添加一个组件到页面',
      microagents: [agentC],
      alreadyInjected: new Set(),
    });
    expect(result).toHaveLength(1);
    expect(result[0]!.skill.name).toBe('agent-c');
  });

  it('returns empty array when no triggers match', () => {
    const result = matchKnowledgeMicroagents({
      messageText: 'add a button',
      microagents: [agentA, agentB],
      alreadyInjected: new Set(),
    });
    expect(result).toHaveLength(0);
  });

  it('returns empty when microagents list is empty', () => {
    const result = matchKnowledgeMicroagents({
      messageText: 'add a component',
      microagents: [],
      alreadyInjected: new Set(),
    });
    expect(result).toHaveLength(0);
  });
});
