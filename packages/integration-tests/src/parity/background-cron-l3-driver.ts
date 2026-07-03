import { readFile } from 'node:fs/promises';
import { writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import type { ContentPart, FinishReason, ToolCall } from '@odysseythink/kosong';

import { testAgent } from '../../../agent-core/test/agent/harness/agent';
import type { TestAgentContext } from '../../../agent-core/test/agent/harness/agent';
import { recordingTelemetry, type TelemetryRecord } from '../../../agent-core/test/fixtures/telemetry';
import type { BackgroundCronAction, BackgroundCronFixture, BackgroundCronSnapshot, FixtureOrigin } from './background-cron-fixture';

function originToAgentOrigin(origin: FixtureOrigin): { kind: string; name?: string; event?: string; blocked?: boolean } {
  return origin as { kind: string; name?: string; event?: string; blocked?: boolean };
}

function toToolCall(raw: unknown): ToolCall {
  const r = raw as { id: string; name: string; arguments: string };
  return {
    type: 'function',
    id: r.id,
    name: r.name,
    arguments: r.arguments,
  };
}

function buildParts(response: { toolCalls: readonly ToolCall[] }): (ContentPart | ToolCall)[] {
  const parts: (ContentPart | ToolCall)[] = [];
  if (response.toolCalls.length > 0) {
    parts.push(...response.toolCalls.map(toToolCall));
  }
  return parts;
}

export async function runBackgroundCronL3Fixture(fixturePath: string): Promise<BackgroundCronSnapshot> {
  const raw = await readFile(fixturePath, 'utf8');
  const fixture: BackgroundCronFixture = JSON.parse(raw);

  const clockFile = join(tmpdir(), `ody-cron-clock-${Date.now()}.txt`);
  writeFileSync(clockFile, '0', 'utf8');

  const previousManualTick = process.env['ODY_CRON_MANUAL_TICK'];
  const previousClock = process.env['ODY_CRON_CLOCK'];
  process.env['ODY_CRON_MANUAL_TICK'] = '1';
  process.env['ODY_CRON_CLOCK'] = `file:${clockFile}`;

  const telemetryRecords: TelemetryRecord[] = [];
  const ctx: TestAgentContext = testAgent({ telemetry: recordingTelemetry(telemetryRecords) });

  try {
    ctx.configure({ tools: [] });
    for (const response of fixture.responses) {
      ctx.mockNextProviderResponse({
        parts: buildParts(response),
        finishReason: response.finishReason as FinishReason | undefined,
        rawFinishReason: response.rawFinishReason,
      });
    }

    const turns: BackgroundCronSnapshot['turns'][number][] = [];
    let lastCronId: string | undefined;
    let lastBackgroundId: string | undefined;

    for (const action of fixture.actions) {
      await executeAction(ctx, action, {
        clockFile,
        turns,
        setLastCronId: (id) => { lastCronId = id; },
        getLastCronId: () => lastCronId,
        setLastBackgroundId: (id) => { lastBackgroundId = id; },
        getLastBackgroundId: () => lastBackgroundId,
      });
    }

    const events = ctx.allEvents
      .filter((e) => e.type === '[rpc]')
      .map((e) => ({ type: e.event, ...(e.args as Record<string, unknown>) }));

    const records = ctx.records.map((r) => JSON.parse(JSON.stringify(r)) as unknown);

    const contextInputs = ctx.agent.context
      .data()
      .history.filter((m) => m.role === 'user')
      .map((m) => ({
        text: m.content
          .filter((p): p is Extract<typeof p, { type: 'text' }> => p.type === 'text')
          .map((p) => p.text)
          .join(''),
        originKind: (m.origin as { kind?: string })?.kind ?? 'unknown',
      }));

    const cronTasks = ctx.agent.cron?.store.list().map((t) => ({
      id: t.id,
      cron: t.cron,
      prompt: t.prompt,
      recurring: t.recurring !== false,
      createdAt: t.createdAt,
      lastFiredAt: t.lastFiredAt,
    })) ?? [];

    const backgroundTasks = ctx.agent.background.list(false).map((info) => ({
      taskId: info.taskId,
      kind: info.kind,
      description: info.description,
      status: info.status,
      startedAt: info.startedAt,
      endedAt: info.endedAt ?? undefined,
      stopReason: info.stopReason,
    }));

    return {
      name: fixture.name,
      turns,
      events,
      records,
      contextInputs,
      cronTasks,
      backgroundTasks,
      telemetry: telemetryRecords.map((t) => ({ event: t.event, properties: t.properties })),
    };
  } finally {
    await ctx.agent.cron?.stop();
    process.env['ODY_CRON_MANUAL_TICK'] = previousManualTick;
    process.env['ODY_CRON_CLOCK'] = previousClock;
  }
}

interface ActionContext {
  readonly clockFile: string;
  turns: BackgroundCronSnapshot['turns'][number][];
  setLastCronId(id: string): void;
  getLastCronId(): string | undefined;
  setLastBackgroundId(id: string): void;
  getLastBackgroundId(): string | undefined;
}

async function executeAction(
  ctx: TestAgentContext,
  action: BackgroundCronAction,
  ac: ActionContext,
): Promise<void> {
  switch (action.op) {
    case 'prompt': {
      await ctx.rpc.prompt({ input: action.input as ContentPart[] });
      break;
    }
    case 'steer': {
      await ctx.rpc.steer({ input: action.input as ContentPart[] });
      break;
    }
    case 'cancel': {
      await ctx.rpc.cancel({ turnId: action.turnId });
      break;
    }
    case 'wait': {
      await ctx.untilTurnEnd();
      const ended = [...ctx.allEvents].reverse().find(
        (e) => e.type === '[rpc]' && e.event === 'turn.ended',
      );
      if (ended) {
        ac.turns.push({
          turnId: (ended.args as { turnId: number }).turnId,
          reason: (ended.args as { reason: string }).reason,
          error: (ended.args as { error?: unknown }).error,
        });
      }
      break;
    }
    case 'advance_clock_to': {
      writeFileSync(ac.clockFile, String(action.epoch_ms), 'utf8');
      break;
    }
    case 'cron_add': {
      const task = ctx.agent.cron!.addTask({
        cron: action.cron,
        prompt: action.prompt,
        recurring: action.recurring,
      });
      ac.setLastCronId(task.id);
      break;
    }
    case 'cron_remove_last': {
      const id = ac.getLastCronId();
      if (id !== undefined) {
        ctx.agent.cron!.removeTasks([id]);
        ac.setLastCronId(undefined as unknown as string);
      }
      break;
    }
    case 'cron_tick': {
      ctx.agent.cron!.tick();
      break;
    }
    case 'background_run_process': {
      const proc = await ctx.agent.kaos.exec(...action.args);
      // Use dynamic import for ProcessBackgroundTask to avoid build issues
      const { ProcessBackgroundTask } = await import('../../../agent-core/src/agent/background/process-task');
      const task = new ProcessBackgroundTask(proc, action.args.join(' '), action.description);
      const id = ctx.agent.background.registerTask(task);
      ac.setLastBackgroundId(id);
      break;
    }
    case 'background_wait_last': {
      const id = ac.getLastBackgroundId();
      if (id !== undefined) {
        await ctx.agent.background.wait(id, action.timeout_ms);
      }
      break;
    }
    case 'background_stop_last': {
      const id = ac.getLastBackgroundId();
      if (id !== undefined) {
        await ctx.agent.background.stop(id, action.reason);
      }
      break;
    }
  }
}
