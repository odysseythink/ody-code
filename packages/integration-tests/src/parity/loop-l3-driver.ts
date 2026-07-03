import { randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import type {
  ContentPart,
  FinishReason,
  Message,
  TokenUsage,
  Tool,
  ToolCall,
} from '@odysseythink/kosong';

import { createLoopEventDispatcher } from '../../../agent-core/src/loop/events';
import { runTurn } from '../../../agent-core/src/loop/run-turn';
import { ToolAccesses } from '../../../agent-core/src/loop/tool-access';
import type {
  ExecutableTool,
  ExecutableToolContext,
  ExecutableToolResult,
  LoopHooks,
  RunnableToolExecution,
  ToolExecution,
  TurnResult,
} from '../../../agent-core/src/loop/types';
import type { LLM, LLMChatParams, LLMChatResponse } from '../../../agent-core/src/loop/llm';
import type {
  LoopEvent,
  LoopLiveOnlyEvent,
  LoopRecordedEvent,
} from '../../../agent-core/src/loop/events';

interface Fixture {
  readonly name?: string | undefined;
  readonly turnId: string;
  readonly maxSteps?: number | undefined;
  readonly maxRetryAttempts?: number | undefined;
  readonly messages: Message[];
  readonly responses: FixtureResponse[];
  readonly tools: FixtureToolDef[];
}

interface FixtureResponse {
  readonly toolCalls: ToolCall[];
  readonly finishReason?: string | undefined;
  readonly rawFinishReason?: string | undefined;
  readonly usage: TokenUsage;
}

interface FixtureToolDef extends Tool {
  readonly result: FixtureToolResult;
  readonly accesses?: import('../../../agent-core/src/loop/tool-access').ToolAccesses | undefined;
  readonly stopBatchAfterThis?: boolean | undefined;
}

interface FixtureToolResult {
  readonly output: string | ContentPart[];
  readonly isError?: boolean | undefined;
  readonly stopTurn?: boolean | undefined;
  readonly message?: string | undefined;
}

export interface LoopL3Snapshot {
  readonly turnResult: TurnResult;
  readonly recordedEvents: unknown[];
  readonly liveEvents: unknown[];
}

class FixtureLlm implements LLM {
  readonly systemPrompt = 'fixture';
  readonly modelName = 'mock';
  private index = 0;

  constructor(private readonly responses: FixtureResponse[]) {}

  async chat(_params: LLMChatParams): Promise<LLMChatResponse> {
    const response = this.responses[this.index] ?? this.responses[this.responses.length - 1];
    this.index += 1;
    if (response === undefined) {
      return {
        toolCalls: [],
        usage: { inputOther: 0, output: 0, inputCacheRead: 0, inputCacheCreation: 0 },
      };
    }
    return {
      toolCalls: response.toolCalls,
      providerFinishReason: response.finishReason as FinishReason | undefined,
      rawFinishReason: response.rawFinishReason,
      usage: response.usage,
    };
  }
}

class FixtureTool implements ExecutableTool {
  readonly name: string;
  readonly description: string;
  readonly parameters: Record<string, unknown>;

  constructor(private readonly def: FixtureToolDef) {
    this.name = def.name;
    this.description = def.description;
    this.parameters = def.parameters;
  }

  resolveExecution(_input: unknown): ToolExecution {
    const result: ExecutableToolResult =
      this.def.result.isError === true
        ? {
            output: this.def.result.output,
            isError: true,
            stopTurn: this.def.result.stopTurn,
            message: this.def.result.message,
          }
        : {
            output: this.def.result.output,
            stopTurn: this.def.result.stopTurn,
            message: this.def.result.message,
          };

    const execution: RunnableToolExecution = {
      isError: false,
      accesses: this.def.accesses ?? ToolAccesses.all(),
      approvalRule: '',
      execute: async (_ctx: ExecutableToolContext): Promise<ExecutableToolResult> => result,
      stopBatchAfterThis: this.def.stopBatchAfterThis,
    };
    return execution;
  }
}

const LIVE_ONLY_EVENT_TYPES = new Set<LoopEvent['type']>([
  'turn.interrupted',
  'step.retrying',
  'text.delta',
  'thinking.delta',
  'tool.call.delta',
  'tool.progress',
]);

function isLiveOnlyEvent(event: LoopEvent): event is LoopLiveOnlyEvent {
  return LIVE_ONLY_EVENT_TYPES.has(event.type);
}

export async function runLoopL3Fixture(fixturePath: string): Promise<LoopL3Snapshot> {
  const raw = await readFile(fixturePath, 'utf8');
  const fixture: Fixture = JSON.parse(raw);

  const llm: LLM = new FixtureLlm(fixture.responses);
  const tools: ExecutableTool[] = fixture.tools.map((def) => new FixtureTool(def));

  const recorded: LoopRecordedEvent[] = [];
  const live: LoopLiveOnlyEvent[] = [];

  const dispatchEvent = createLoopEventDispatcher({
    appendTranscriptRecord: async (record) => {
      recorded.push(record);
    },
    emitLiveEvent: (event) => {
      if (isLiveOnlyEvent(event)) {
        live.push(event);
      }
    },
  });

  const turnResult = await runTurn({
    turnId: fixture.turnId,
    signal: new AbortController().signal,
    llm,
    buildMessages: () => fixture.messages,
    dispatchEvent,
    tools,
    hooks: undefined,
    maxSteps: fixture.maxSteps,
    maxRetryAttempts: fixture.maxRetryAttempts,
    recordStepUsage: undefined,
  });

  return {
    turnResult,
    recordedEvents: recorded.map((event) => JSON.parse(JSON.stringify(event)) as unknown),
    liveEvents: live.map((event) => JSON.parse(JSON.stringify(event)) as unknown),
  };
}
