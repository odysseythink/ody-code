import { z } from 'zod';

export const CompactionL1FixtureSchema = z.object({
  name: z.string(),
  history: z.array(z.object({
    role: z.string(),
    name: z.string().nullable().optional(),
    content: z.array(z.union([
      z.object({ type: z.literal('text'), text: z.string() }),
      z.object({ type: z.literal('think'), think: z.string() }).passthrough(),
    ])),
    toolCalls: z.array(z.any()).default([]),
    toolCallId: z.string().nullable().optional(),
    origin: z.any().nullable().optional(),
    isError: z.boolean().nullable().optional(),
  })),
  strategy: z.object({ max_size: z.number() }),
  begin: z.object({
    source: z.string(),
    instruction: z.string().nullable().optional(),
  }),
  generate_one_off_result: z.object({
    text: z.string(),
    finishReason: z.string().optional(),
    usage: z.object({
      inputOther: z.number().default(0),
      output: z.number().default(0),
      inputCacheRead: z.number().default(0),
      inputCacheCreation: z.number().default(0),
    }),
  }),
});

export type CompactionL1Fixture = z.infer<typeof CompactionL1FixtureSchema>;
