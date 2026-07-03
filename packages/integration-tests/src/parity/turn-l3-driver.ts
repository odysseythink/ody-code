import { randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import type { ContentPart, ToolCall } from '@odysseythink/kosong';

import { testAgent } from '../../../agent-core/test/agent/harness/agent';
import type { TestAgentContext } from '../../../agent-core/test/agent/harness/agent';
import {
  recordingTelemetry,
  type TelemetryRecord,
} from '../../../agent-core/test/fixtures/telemetry';
import {
  SessionGoalStore,
  type SessionGoalState,
  type GoalActor,
} from '../../../agent-core/src/session/goal';
import type {
  ExecutableTool,
  ExecutableToolResult,
  ToolExecution,
} from '../../../agent-core/src/loop';
import type { RunnableToolExecution } from '../../../agent-core/src/loop/types';
import {
  parseTurnFixture,
  type FixtureAction,
  type FixtureResponse,
  type FixtureToolDef,
  type TurnFixture,
  type TurnL3Snapshot,
} from './turn-fixture';

function toToolCall(raw: unknown): ToolCall {
  const r = raw as { id: string; name: string; arguments: string };
  return {
    type: 'function',
    id: r.id,
    name: r.name,
    arguments: r.arguments,
  };
}

function toContentParts(raw: unknown): ContentPart[] {
  return raw as ContentPart[];
}

function buildParts(response: FixtureResponse): (ContentPart | ToolCall)[] {
  const parts: (ContentPart | ToolCall)[] = [];
  if (response.toolCalls.length > 0) {
    parts.push(...response.toolCalls.map(toToolCall));
  }
  return parts;
}

function createFakeTool(def: FixtureToolDef): ExecutableTool {
  return {
    name: def.name,
    description: def.description,
    parameters: def.parameters,
    resolveExecution: async () => {
      const output = def.result.output as Extract<
        ExecutableToolResult,
        { output: unknown }
      >['output'];
      const result: ExecutableToolResult =
        def.result.isError === true ? { output, isError: true } : { output };
      const execution: RunnableToolExecution = {
        approvalRule: def.name,
        execute: async () => result,
      };
      return execution as ToolExecution;
    },
  } as ExecutableTool;
}

interface ToolManagerInternals {
  userTools: Map<string, ExecutableTool>;
  enabledTools: Set<string>;
}

function registerFakeTools(ctx: TestAgentContext, defs: FixtureToolDef[]): void {
  const tools = ctx.agent.tools as unknown as ToolManagerInternals;
  for (const def of defs) {
    tools.userTools.set(def.name, createFakeTool(def));
    tools.enabledTools.add(def.name);
  }
}

function buildInitialGoal(fixture: TurnFixture): SessionGoalState | undefined {
  if (!fixture.initialGoal) return undefined;
  const now = new Date().toISOString();
  return {
    goalId: randomUUID(),
    objective: 'fixture goal',
    status: fixture.initialGoal.status,
    createdAt: now,
    updatedAt: now,
    startedBy: 'user' as GoalActor,
    updatedBy: 'user' as GoalActor,
    turnsUsed: 0,
    tokensUsed: 0,
    wallClockMs: 0,
    budgetLimits: fixture.initialGoal.budget,
  };
}

export async function runTurnL3Fixture(fixturePath: string): Promise<TurnL3Snapshot> {
  const raw = await readFile(fixturePath, 'utf8');
  const fixture = parseTurnFixture(raw);

  const telemetryRecords: TelemetryRecord[] = [];
  let goalState: SessionGoalState | undefined = buildInitialGoal(fixture);

  const goalStore =
    goalState !== undefined
      ? new SessionGoalStore({
          readState: () => goalState,
          writeState: async (s) => {
            goalState = s as SessionGoalState | undefined;
          },
        })
      : undefined;

  const ctx: TestAgentContext = testAgent({
    initialConfig: {
      providers: {},
      loopControl: fixture.loopControl
        ? {
            maxStepsPerTurn: fixture.loopControl.maxSteps,
            maxRetriesPerStep: fixture.loopControl.maxRetryAttempts,
          }
        : undefined,
    },
    goals: goalStore,
    telemetry: recordingTelemetry(telemetryRecords),
  });

  const toolNames = fixture.tools.map((t) => t.name);
  ctx.configure({ tools: toolNames });
  registerFakeTools(ctx, fixture.tools);
  if (toolNames.includes('Bash') || toolNames.includes('UpdateGoal')) {
    await ctx.rpc.setPermission({ mode: 'yolo' });
  }

  for (const response of fixture.responses) {
    ctx.mockNextResponse(...buildParts(response));
  }

  const turns: TurnL3Snapshot['turns'] = [];
  let lastEventIndex = 0;

  for (const action of fixture.actions) {
    switch (action.op) {
      case 'prompt': {
        await ctx.rpc.prompt({ input: toContentParts(action.input) });
        break;
      }
      case 'steer': {
        await ctx.rpc.steer({ input: toContentParts(action.input) });
        break;
      }
      case 'cancel': {
        await ctx.rpc.cancel({ turnId: action.turnId });
        break;
      }
      case 'wait': {
        await ctx.untilTurnEnd();
        const slice = ctx.allEvents.slice(lastEventIndex);
        lastEventIndex = ctx.allEvents.length;
        const ended = slice.find(
          (e) => e.type === '[rpc]' && e.event === 'turn.ended',
        );
        if (ended) {
          turns.push({
            turnId: (ended.args as { turnId: number }).turnId,
            reason: (ended.args as { reason: string }).reason,
            error: (ended.args as { error?: unknown }).error,
          });
        }
        break;
      }
    }
  }

  const events = ctx.allEvents.filter((e) => e.type === '[rpc]').map((e) => ({
    type: e.event,
    ...(e.args as Record<string, unknown>),
  }));

  const records = ctx.records.map((r) => JSON.parse(JSON.stringify(r)) as unknown);

  const contextInputs = ctx.agent.context
    .data()
    .history.filter((m) => m.role === 'user')
    .map((m) => ({
      text: m.content
        .filter((p): p is Extract<typeof p, { type: 'text' }> => p.type === 'text')
        .map((p) => p.text)
        .join(''),
      originKind: m.origin?.kind ?? 'unknown',
    }));

  const goalSnapshot = ctx.agent.goals?.getGoal().goal;

  return {
    name: fixture.name,
    turns,
    events,
    records,
    contextInputs,
    telemetry: telemetryRecords.map((t) => ({ event: t.event, properties: t.properties })),
    goalState: goalSnapshot
      ? {
          status: goalSnapshot.status,
          turnsUsed: goalSnapshot.turnsUsed,
          tokensUsed: goalSnapshot.tokensUsed,
        }
      : undefined,
  };
}
