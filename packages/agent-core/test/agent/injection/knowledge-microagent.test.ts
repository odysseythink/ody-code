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
  source: SkillDefinition['source'] = 'project',
): SkillDefinition {
  return {
    name,
    description: `Knowledge: ${name}`,
    path: `/test/${name}.md`,
    dir: '/test',
    content,
    metadata: { type: 'knowledge', triggers },
    source,
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

// ── Budget & precedence helpers ─────────────────────────────────

import {
  sortBySourcePriority,
  resolveBudgetLimit,
  applyBudget,
} from '../../../src/agent/injection/knowledge-microagent';
import { estimateTokens } from '../../../src/utils/tokens';
import type { Agent as AgentType } from '../../../src/agent';

describe('sortBySourcePriority', () => {
  it('orders by source: project > user > extra > builtin', () => {
    const builtin = microagent('ba', ['t1']);
    const project = microagent('pa', ['t1']);
    const user = microagent('ua', ['t1']);
    const extra = microagent('ea', ['t1']);

    const skillDefs = [
      { ...builtin, source: 'builtin' as const },
      { ...project, source: 'project' as const },
      { ...user, source: 'user' as const },
      { ...extra, source: 'extra' as const },
    ] as SkillDefinition[];

    const matches = skillDefs.map((s) => ({ skill: s, trigger: 't1' }));
    const sorted = sortBySourcePriority(matches);
    const sources = sorted.map((m) => m.skill.source);
    expect(sources).toEqual(['project', 'user', 'extra', 'builtin']);
  });

  it('tie-breaks by name lexicographically within same source', () => {
    const beta = { ...microagent('beta', ['t1']), source: 'project' as const } as SkillDefinition;
    const alpha = { ...microagent('alpha', ['t1']), source: 'project' as const } as SkillDefinition;

    const matches = [
      { skill: beta, trigger: 't1' },
      { skill: alpha, trigger: 't1' },
    ];
    const sorted = sortBySourcePriority(matches);
    expect(sorted.map((m) => m.skill.name)).toEqual(['alpha', 'beta']);
  });

  it('returns empty for empty input', () => {
    expect(sortBySourcePriority([])).toEqual([]);
  });
});

describe('resolveBudgetLimit', () => {
  it('returns configured maxTokens', () => {
    const agent = {
      kimiConfig: { microagentBudget: { maxTokens: 500 } },
    } as unknown as AgentType;
    expect(resolveBudgetLimit(agent)).toBe(500);
  });

  it('returns default 1024 when microagentBudget is undefined', () => {
    const agent = { kimiConfig: undefined } as unknown as AgentType;
    expect(resolveBudgetLimit(agent)).toBe(1024);
  });

  it('returns default 1024 when maxTokens is undefined', () => {
    const agent = {
      kimiConfig: { microagentBudget: {} },
    } as unknown as AgentType;
    expect(resolveBudgetLimit(agent)).toBe(1024);
  });

  it('returns Infinity when maxTokens is 0', () => {
    const agent = {
      kimiConfig: { microagentBudget: { maxTokens: 0 } },
    } as unknown as AgentType;
    expect(resolveBudgetLimit(agent)).toBe(Infinity);
  });
});

describe('applyBudget', () => {
  const shortContent = '# Short\n\nOnly a few tokens.';           // ~10 tokens
  const longContent = '# Long\n\n' + 'x'.repeat(5000);           // ~1250 tokens

  const short = {
    ...microagent('short', ['t1'], shortContent),
    source: 'project' as const,
  } as SkillDefinition;
  const long = {
    ...microagent('long', ['t1'], longContent),
    source: 'project' as const,
  } as SkillDefinition;

  it('injects all when budget is unlimited (maxTokens=Infinity)', () => {
    const matches = [
      { skill: long, trigger: 't1' },
      { skill: short, trigger: 't1' },
    ];
    const result = applyBudget(matches, Infinity);
    expect(result.injected).toHaveLength(2);
    expect(result.skipped).toHaveLength(0);
    expect(result.total).toBe(Infinity);
    expect(result.used).toBeGreaterThan(0);
  });

  it('skips microagent when it would exceed budget', () => {
    const budget = estimateTokens(shortContent) + 1; // fits short + 1 extra
    const matches = [
      { skill: short, trigger: 't1' },
      { skill: long, trigger: 't1' },
    ];
    const result = applyBudget(matches, budget);
    expect(result.injected).toHaveLength(1);
    expect(result.injected[0]!.skill.name).toBe('short');
    expect(result.skipped).toHaveLength(1);
    expect(result.skipped[0]!.match.skill.name).toBe('long');
    expect(result.skipped[0]!.reason).toBe('budget_exceeded');
  });

  it('skips all when every body exceeds budget', () => {
    const matches = [{ skill: long, trigger: 't1' }];
    const result = applyBudget(matches, 10); // tiny budget
    expect(result.injected).toHaveLength(0);
    expect(result.skipped).toHaveLength(1);
    expect(result.used).toBe(0);
  });

  it('skips empty bodies silently (no budget consumed)', () => {
    const empty = {
      ...microagent('empty', ['t1'], ''),
      source: 'project' as const,
    } as SkillDefinition;
    const matches = [
      { skill: empty, trigger: 't1' },
      { skill: short, trigger: 't1' },
    ];
    const result = applyBudget(matches, 100);
    // empty body skipped without consuming budget
    expect(result.injected).toHaveLength(1);
    expect(result.injected[0]!.skill.name).toBe('short');
    expect(result.skipped).toHaveLength(0);
  });

  it('injects body that fits exactly at budget limit', () => {
    const exactContent = 'abcd'; // 1 ASCII token
    const exactBudget = estimateTokens(exactContent);
    const skill = {
      ...microagent('exact', ['t1'], exactContent),
      source: 'project' as const,
    } as SkillDefinition;
    const result = applyBudget([{ skill, trigger: 't1' }], exactBudget);
    expect(result.injected).toHaveLength(1);
    expect(result.skipped).toHaveLength(0);
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
  kimiConfig?: Record<string, unknown>;
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
    kimiConfig: stub.kimiConfig,
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

  // ── Precedence tests ─────────────────────────────────────────────

  it('P1: project wins over builtin under budget (only one fits)', async () => {
    const projectContent = '# Project conventions\n\n' + 'x'.repeat(3600); // ~907 tokens
    const builtinContent = '# Builtin conventions\n\nSome text.'; // ~7 tokens

    const projectAgent = microagent('project-agent', ['component'], projectContent, 'project');
    const builtinAgent = microagent('builtin-agent', ['component'], builtinContent, 'builtin');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [projectAgent, builtinAgent],
      telemetryCalls,
      kimiConfig: { microagentBudget: { maxTokens: 910 } }, // project fits, project+builtin doesn't
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const text = reminderText(history);
    expect(text).toBeDefined();
    expect(text).toContain('Project conventions');
    expect(text).toContain('## project-agent');
    expect(text).not.toContain('Builtin conventions');

    // Check skipped telemetry
    const skippedCalls = telemetryCalls.filter((c) => c.event === 'microagent_skipped');
    expect(skippedCalls).toHaveLength(1);
    expect(skippedCalls[0]!.properties['skill_name']).toBe('builtin-agent');
    expect(skippedCalls[0]!.properties['reason']).toBe('budget_exceeded');
  });

  it('P2: user wins over extra under budget', async () => {
    const userContent = '# User conventions\n\n' + 'x'.repeat(3600); // ~907 tokens
    const extraContent = '# Extra conventions\n\nSome text.'; // ~7 tokens

    const userAgent = microagent('user-agent', ['component'], userContent, 'user');
    const extraAgent = microagent('extra-agent', ['component'], extraContent, 'extra');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [userAgent, extraAgent],
      telemetryCalls,
      kimiConfig: { microagentBudget: { maxTokens: 910 } }, // user fits, user+extra doesn't
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const text = reminderText(history);
    expect(text).toBeDefined();
    expect(text).toContain('User conventions');
    expect(text).not.toContain('Extra conventions');
  });

  it('P3: same-source tie-breaker is name lexicographic', async () => {
    const beta = microagent('beta-agent', ['component'], '# Beta conventions\n\nContent.', 'project');
    const alpha = microagent('alpha-agent', ['component'], '# Alpha conventions\n\nContent.', 'project');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [beta, alpha],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const text = reminderText(history);
    expect(text).toBeDefined();
    const alphaIndex = text!.indexOf('## alpha-agent');
    const betaIndex = text!.indexOf('## beta-agent');
    expect(alphaIndex).toBeLessThan(betaIndex);
  });

  // ── Budget tests ──────────────────────────────────────────────────

  it('B1: default budget (1024) caps injection', async () => {
    // ~900 tokens → fits
    const projectContent = '# Project\n\n' + 'x'.repeat(3600);
    // ~300 tokens → would fit alone but together exceeds default 1024
    const userContent = '# User\n\n' + 'y'.repeat(1200);

    const projectAgent = microagent('proj', ['component'], projectContent, 'project');
    const userAgent = microagent('usr', ['component'], userContent, 'user');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [projectAgent, userAgent],
      telemetryCalls,
      // No kimiConfig → default 1024
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const text = reminderText(history);
    expect(text).toBeDefined();
    expect(text).toContain('## proj');
    // user may or may not fit depending on token estimate — check telemetry
    const injectedCalls = telemetryCalls.filter((c) => c.event === 'microagent_injected');
    expect(injectedCalls).toHaveLength(1);
    expect(injectedCalls[0]!.properties['skill_name']).toBe('proj');
    expect(injectedCalls[0]!.properties).toHaveProperty('budget_used');
    expect(injectedCalls[0]!.properties).toHaveProperty('budget_total');
  });

  it('B2: maxTokens=0 disables cap (unlimited)', async () => {
    const largeContent = '# Large\n\n' + 'x'.repeat(10000);
    const agentA = microagent('a', ['component'], largeContent, 'project');
    const agentB = microagent('b', ['component'], largeContent, 'project');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [agentA, agentB],
      telemetryCalls,
      kimiConfig: { microagentBudget: { maxTokens: 0 } },
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const text = reminderText(history);
    expect(text).toBeDefined();
    expect(text).toContain('## a');
    expect(text).toContain('## b');

    const skippedCalls = telemetryCalls.filter((c) => c.event === 'microagent_skipped');
    expect(skippedCalls).toHaveLength(0);

    const injectedCalls = telemetryCalls.filter((c) => c.event === 'microagent_injected');
    injectedCalls.forEach((c) => {
      expect(c.properties['budget_total']).toBe(0); // 0 when unlimited
    });
  });

  it('B3: custom maxTokens works', async () => {
    const smallContent = '# Small\n\nabc'; // ~2 tokens
    const mediumContent = '# Medium\n\n' + 'x'.repeat(100); // ~25 tokens
    const largeContent = '# Large\n\n' + 'x'.repeat(2000); // ~500 tokens

    const small = microagent('small', ['component'], smallContent, 'project');
    const medium = microagent('medium', ['component'], mediumContent, 'project');
    const large = microagent('large', ['component'], largeContent, 'project');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [small, medium, large],
      telemetryCalls,
      kimiConfig: { microagentBudget: { maxTokens: 50 } },
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const injectedCalls = telemetryCalls.filter((c) => c.event === 'microagent_injected');
    // small + medium should fit (~27 tokens), large (~500) should not
    const names = injectedCalls.map((c) => c.properties['skill_name'] as string);
    expect(names).toContain('small');
    expect(names).toContain('medium');
    expect(names).not.toContain('large');

    // budget_total should be 50
    injectedCalls.forEach((c) => {
      expect(c.properties['budget_total']).toBe(50);
    });
  });

  it('B4: single oversized body skipped, no reminder emitted', async () => {
    const hugeContent = '# Huge\n\n' + 'x'.repeat(10000); // ~2500 tokens
    const huge = microagent('huge-agent', ['component'], hugeContent, 'project');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [huge],
      telemetryCalls,
      kimiConfig: { microagentBudget: { maxTokens: 500 } },
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    // No bodies injected → no reminder
    const text = reminderText(history);
    expect(text).toBeUndefined();

    // But skipped telemetry is emitted
    const skippedCalls = telemetryCalls.filter((c) => c.event === 'microagent_skipped');
    expect(skippedCalls).toHaveLength(1);
    expect(skippedCalls[0]!.properties['skill_name']).toBe('huge-agent');
    expect(skippedCalls[0]!.properties['reason']).toBe('budget_exceeded');
  });

  it('B5: budget usage telemetry on microagent_injected', async () => {
    const content = '# Test\n\nSome body text.';
    const agentA = microagent('test-agent', ['component'], content, 'project');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [agentA],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const injectedCall = telemetryCalls.find((c) => c.event === 'microagent_injected');
    expect(injectedCall).toBeDefined();
    expect(injectedCall!.properties['budget_used']).toEqual(expect.any(Number));
    expect(injectedCall!.properties['budget_total']).toEqual(expect.any(Number));
  });

  it('B6: skipped telemetry has correct properties', async () => {
    const hugeContent = '# Huge\n\n' + 'x'.repeat(10000);
    const huge = microagent('huge', ['component'], hugeContent, 'project');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [huge],
      telemetryCalls,
      kimiConfig: { microagentBudget: { maxTokens: 100 } },
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const skippedCall = telemetryCalls.find((c) => c.event === 'microagent_skipped');
    expect(skippedCall).toBeDefined();
    expect(skippedCall!.properties).toMatchObject({
      skill_name: 'huge',
      trigger: 'component',
      skill_source: 'project',
      reason: 'budget_exceeded',
    });
    expect(skippedCall!.properties['budget_used']).toEqual(expect.any(Number));
    expect(skippedCall!.properties['budget_total']).toEqual(expect.any(Number));
  });

  it('B7: reminder includes omitted-note when microagent is skipped', async () => {
    // Two microagents: one fits, one doesn't → reminder has both + omitted note
    const shortContent = '# Short\n\nabc'; // ~2 tokens
    const longContent = '# Long\n\n' + 'x'.repeat(10000); // ~2500 tokens

    const short = microagent('short-keep', ['component'], shortContent, 'project');
    const long = microagent('long-skip', ['component'], longContent, 'user');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [short, long],
      telemetryCalls,
      kimiConfig: { microagentBudget: { maxTokens: 500 } },
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const text = reminderText(history);
    expect(text).toBeDefined();
    expect(text).toContain('## short-keep');
    expect(text).not.toContain('## long-skip');
    expect(text).toContain('omitted due to the microagent token budget');
    expect(text).toContain('long-skip');
  });

  it('B8: empty bodies still not counted toward budget', async () => {
    const empty = microagent('empty-keep', ['component'], '', 'project');
    const normal = microagent('normal-keep', ['component'], '# Normal\n\nContent.', 'project');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [empty, normal],
      telemetryCalls,
      kimiConfig: { microagentBudget: { maxTokens: 5 } }, // tight budget: normal (~5 tok) fits, empty doesn't consume
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const text = reminderText(history);
    expect(text).toBeDefined();
    // empty skipped without consuming budget; normal fits
    expect(text).not.toContain('## empty-keep');
    expect(text).toContain('## normal-keep');

    // No skipped telemetry for empty body
    const injectedCalls = telemetryCalls.filter((c) => c.event === 'microagent_injected');
    expect(injectedCalls).toHaveLength(1);
    expect(injectedCalls[0]!.properties['skill_name']).toBe('normal-keep');
  });
});
