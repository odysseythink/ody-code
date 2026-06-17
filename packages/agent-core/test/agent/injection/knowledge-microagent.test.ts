import { beforeAll, afterAll, describe, expect, it, vi } from 'vitest';

import type { ContextMessage } from '../../../src/agent/context';
import {
  extractLatestUserText,
  matchKnowledgeMicroagents,
  triggerMatches,
} from '../../../src/agent/injection/knowledge-microagent';
import type { SkillDefinition, SkillRegistry } from '../../../src/skill';
import type { Agent } from '../../../src/agent';

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

// ===== Injector tests =====

import {
  KnowledgeMicroagentInjector,
} from '../../../src/agent/injection/knowledge-microagent';

// ── Agent stub helpers ─────────────────────────────────────────────

interface MicroagentAgentStub {
  history: ContextMessage[];
  enabledFlags: Set<string>;
  sessionActive: boolean;
  microagents: SkillDefinition[] | null;
  telemetryCalls: Array<{ event: string; properties: Record<string, unknown> }>;
}

function microagentAgent(stub: MicroagentAgentStub): Agent {
  const fakeRegistry = {
    listKnowledgeMicroagents: () => stub.microagents ?? [],
  } as unknown as SkillRegistry;

  return {
    type: 'main',
    context: {
      get history() {
        return stub.history;
      },
      appendSystemReminder: (content: string, origin: ContextMessage['origin']) => {
        stub.history.push({
          role: 'user',
          content: [{ type: 'text', text: `<system-reminder>\n${content}\n</system-reminder>` }],
          toolCalls: [],
          origin,
        });
      },
    } as unknown as Agent['context'],
    sessionMode: {
      get isActive() {
        return stub.sessionActive;
      },
      kind: 'plan' as const,
    } as Agent['sessionMode'],
    skills: {
      registry: fakeRegistry,
    } as Agent['skills'],
    telemetry: {
      track: (event: string, properties: Record<string, unknown>) => {
        stub.telemetryCalls.push({ event, properties });
      },
    } as unknown as Agent['telemetry'],
    log: {
      warn: () => {},
      error: () => {},
      info: () => {},
      debug: () => {},
    } as unknown as Agent['log'],
  } as unknown as Agent;
}

function reminderText(history: readonly ContextMessage[]): string | undefined {
  const message = history.findLast(
    (entry) =>
      entry.origin?.kind === 'injection' &&
      entry.origin.variant === 'knowledge_microagent',
  );
  return message?.content
    .map((part) => (part.type === 'text' ? part.text : ''))
    .join('');
}

describe('KnowledgeMicroagentInjector', () => {
  beforeAll(() => {
    vi.stubEnv('ODY_CODE_EXPERIMENTAL_REPO_KNOWLEDGE', '1');
  });

  afterAll(() => {
    vi.unstubAllEnvs();
  });

  const reuse = microagent('reuse', ['component'], '# Reuse conventions\n\nAlways use existing code.');

  it('I1: injects on first matching user message', async () => {
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [reuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const text = reminderText(history);
    expect(text).toBeDefined();
    expect(text).toContain('repo-specific conventions');
    expect(text).toContain('Reuse conventions');
    expect(text).toContain('## reuse');
  });

  it('I2: does not re-inject same microagent on next turn', async () => {
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [reuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();
    const countAfterFirst = history.length;
    expect(countAfterFirst).toBeGreaterThan(1);

    await injector.inject();
    expect(history.length).toBe(countAfterFirst);
  });

  it('I3: clears injected set on context clear', async () => {
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [reuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();
    expect(history.length).toBeGreaterThan(1);

    injector.onContextClear();
    history.push(userMessage('add a component'));

    const countBeforeReInject = history.length;
    await injector.inject();
    expect(history.length).toBe(countBeforeReInject + 1);
  });

  it('I4: clears injected set on compaction', async () => {
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [reuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();
    expect(history.length).toBeGreaterThan(1);

    injector.onContextCompacted(5);
    history.push(userMessage('add a component'));

    const countBeforeReInject = history.length;
    await injector.inject();
    expect(history.length).toBe(countBeforeReInject + 1);
  });

  it('I5: skips empty bodies', async () => {
    const emptyReuse = microagent('empty-reuse', ['component'], '');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [emptyReuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();
    expect(reminderText(history)).toBeUndefined();

    // Verify: after microagent gets content, it should match on next turn
    const filledReuse = microagent('empty-reuse', ['component'], '# Now has content');
    (agent as unknown as { skills: { registry: { listKnowledgeMicroagents: () => SkillDefinition[] } } }).skills = {
      registry: { listKnowledgeMicroagents: () => [filledReuse] },
    };
    history.push(userMessage('add a component'));
    await injector.inject();
    expect(reminderText(history)).toBeDefined();
  });

  it('I6: only runs in normal mode', async () => {
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: true,
      microagents: [reuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();
    expect(reminderText(history)).toBeUndefined();
  });

  it('I7: no-op when flag disabled', async () => {
    vi.stubEnv('ODY_CODE_EXPERIMENTAL_REPO_KNOWLEDGE', '0');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(),
      sessionActive: false,
      microagents: [reuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();
    expect(reminderText(history)).toBeUndefined();
    vi.stubEnv('ODY_CODE_EXPERIMENTAL_REPO_KNOWLEDGE', '1');
  });

  it('I8: emits telemetry on injection', async () => {
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [reuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    expect(telemetryCalls).toHaveLength(1);
    expect(telemetryCalls[0]!.event).toBe('microagent_injected');
    expect(telemetryCalls[0]!.properties).toMatchObject({
      skill_name: 'reuse',
      trigger: 'component',
      skill_source: 'project',
    });
  });

  it('N1: no-op when skills not loaded (agent.skills is null)', async () => {
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: null,
      telemetryCalls,
    });
    (agent as unknown as { skills: null }).skills = null;
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();
    expect(reminderText(history)).toBeUndefined();
  });

  it('N2: no-op when latest user text is empty or whitespace-only', async () => {
    const history: ContextMessage[] = [userMessage('   ')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [reuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();
    expect(reminderText(history)).toBeUndefined();
  });
});
