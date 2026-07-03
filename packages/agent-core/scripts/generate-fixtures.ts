import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import type { AgentRecord } from '../src/agent/records/types';

const here = dirname(fileURLToPath(import.meta.url));
const fixturesDir = join(here, '..', '..', '..', 'rust-ody', 'crates', 'agent-rs', 'fixtures');
mkdirSync(fixturesDir, { recursive: true });

const records: AgentRecord[] = [
  {
    type: 'metadata',
    time: 1700000000000,
    protocol_version: '1.3',
    created_at: 1700000000000,
    app_version: '0.0.0',
    resumed: false,
  },
  {
    type: 'turn.prompt',
    time: 1700000000001,
    input: [
      { type: 'text', text: 'Hello from TypeScript' },
      {
        type: 'image_url',
        imageUrl: {
          url: 'data:image/png;base64,iVBORw0KGgoAAAA==',
          id: 'img_1',
        },
      },
    ],
    origin: { kind: 'user' },
  },
  {
    type: 'turn.steer',
    time: 1700000000002,
    input: [{ type: 'text', text: 'Keep it short' }],
    origin: { kind: 'user' },
  },
  {
    type: 'context.append_message',
    time: 1700000000003,
    message: {
      role: 'assistant',
      content: [{ type: 'text', text: 'Acknowledged' }],
      toolCalls: [
        {
          type: 'function',
          id: 'call_1',
          name: 'read',
          arguments: '{"path":"README.md"}',
        },
      ],
      origin: { kind: 'user' },
    },
  },
  {
    type: 'context.append_loop_event',
    time: 1700000000004,
    event: {
      type: 'tool.result',
      parentUuid: 'p1',
      toolCallId: 'call_1',
      result: {
        output: [{ type: 'text', text: 'file contents' }],
      },
    },
  },
  {
    type: 'permission.set_mode',
    time: 1700000000005,
    mode: 'yolo',
  },
  {
    type: 'usage.record',
    time: 1700000000006,
    model: 'kimi-k2',
    usage: {
      inputOther: 12,
      output: 5,
      inputCacheRead: 1,
      inputCacheCreation: 0,
    },
    usageScope: 'turn',
  },
  {
    type: 'goal.create',
    time: 1700000000007,
    goalId: 'g2',
    objective: 'finish the fixture',
    status: 'active',
    actor: 'user',
    budgetLimits: {
      tokenBudget: 1000000,
      turnBudget: 100,
    },
  },
];

const lines = records.map((r) => JSON.stringify(r)).join('\n') + '\n';
const outPath = join(fixturesDir, 'ts_records.jsonl');
writeFileSync(outPath, lines);
console.log(`Wrote ${outPath}`);
