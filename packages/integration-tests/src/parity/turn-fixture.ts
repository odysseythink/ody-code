import { z } from 'zod';

export const ContentPartSchema = z.union([
  z.object({ type: z.literal('text'), text: z.string() }),
  z.object({
    type: z.literal('image_url'),
    imageUrl: z.object({ id: z.string().optional(), url: z.string().optional() }).passthrough(),
  }),
]);

export type FixtureContentPart = z.infer<typeof ContentPartSchema>;

export const PromptOriginSchema = z.union([
  z.object({ kind: z.literal('user') }),
  z.object({ kind: z.literal('system_trigger'), name: z.string() }),
  z.object({ kind: z.literal('hook_result'), event: z.string(), blocked: z.boolean().optional() }),
  z.object({ kind: z.string(), name: z.string().optional() }).passthrough(),
]);

export type FixturePromptOrigin = z.infer<typeof PromptOriginSchema>;

export const FixtureActionSchema = z.union([
  z.object({ op: z.literal('prompt'), input: z.array(ContentPartSchema), origin: PromptOriginSchema }),
  z.object({ op: z.literal('steer'), input: z.array(ContentPartSchema), origin: PromptOriginSchema }),
  z.object({ op: z.literal('cancel'), turnId: z.number().optional(), reason: z.string().optional() }),
  z.object({ op: z.literal('wait') }),
]);

export type FixtureAction = z.infer<typeof FixtureActionSchema>;

export const FixtureResponseSchema = z.object({
  toolCalls: z.array(z.any()).default([]),
  finishReason: z.string().optional(),
  rawFinishReason: z.string().optional(),
  usage: z.object({
    inputOther: z.number().default(0),
    output: z.number().default(0),
    inputCacheRead: z.number().default(0),
    inputCacheCreation: z.number().default(0),
  }),
});

export type FixtureResponse = z.infer<typeof FixtureResponseSchema>;

export const FixtureToolResultSchema = z.object({
  output: z.union([z.string(), z.array(ContentPartSchema)]),
  isError: z.boolean().optional(),
  stopTurn: z.boolean().optional(),
  message: z.string().optional(),
});

export const FixtureToolDefSchema = z.object({
  name: z.string(),
  description: z.string(),
  parameters: z.record(z.string(), z.any()),
  result: FixtureToolResultSchema,
});

export type FixtureToolDef = z.infer<typeof FixtureToolDefSchema>;

export const TurnFixtureSchema = z.object({
  name: z.string(),
  initialGoal: z.object({
    status: z.enum(['active', 'paused', 'blocked', 'complete']),
    budget: z.object({
      tokenBudget: z.number().optional(),
      turnBudget: z.number().optional(),
      wallClockBudgetMs: z.number().optional(),
    }).default({}),
  }).optional(),
  loopControl: z.object({
    maxSteps: z.number().optional(),
    maxRetryAttempts: z.number().optional(),
  }).optional(),
  actions: z.array(FixtureActionSchema),
  responses: z.array(FixtureResponseSchema),
  tools: z.array(FixtureToolDefSchema).default([]),
});

export type TurnFixture = z.infer<typeof TurnFixtureSchema>;

export interface TurnL3Snapshot {
  readonly name: string;
  readonly turns: Array<{
    readonly turnId: number;
    readonly reason: string;
    readonly error?: unknown;
    readonly stopReason?: string;
  }>;
  readonly events: unknown[];
  readonly records: unknown[];
  readonly contextInputs: Array<{ text: string; originKind: string }>;
  readonly telemetry: Array<{ event: string; properties: unknown }>;
  readonly goalState?: { status: string; turnsUsed: number; tokensUsed: number };
}

export function parseTurnFixture(raw: string): TurnFixture {
  return TurnFixtureSchema.parse(JSON.parse(raw));
}
