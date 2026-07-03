import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'pathe';
import type { AgentRecord } from '../../src/agent/records/types';

const fixturesDir = join(process.cwd(), '..', '..', 'rust-ody', 'crates', 'agent-rs', 'fixtures');
const rustFixturePath = join(fixturesDir, 'rust_records.jsonl');
const tsFixturePath = join(fixturesDir, 'ts_records.jsonl');

function loadFixture(path: string): AgentRecord[] {
  const text = readFileSync(path, 'utf-8');
  return text
    .split('\n')
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as AgentRecord);
}

describe('Rust-generated fixtures', () => {
  it('can be parsed by TypeScript', () => {
    const records = loadFixture(rustFixturePath);
    expect(records).toHaveLength(7);
    expect(records[0]!.type).toBe('metadata');
    expect(records[1]!.type).toBe('turn.prompt');
    expect(records[2]!.type).toBe('context.append_message');
    expect(records[3]!.type).toBe('context.append_loop_event');
    expect(records[4]!.type).toBe('permission.set_mode');
    expect(records[5]!.type).toBe('usage.record');
    expect(records[6]!.type).toBe('goal.create');
  });

  it('preserves tool call and message shapes', () => {
    const records = loadFixture(rustFixturePath);
    const prompt = records[1]!;
    expect(prompt.type).toBe('turn.prompt');
    if (prompt.type !== 'turn.prompt') return;
    expect(prompt.input[0]).toEqual({ type: 'text', text: 'Hello from Rust' });
    expect(prompt.input[1]).toEqual({
      type: 'image_url',
      imageUrl: {
        url: 'data:image/png;base64,iVBORw0KGgoAAAA==',
        id: 'img_1',
      },
    });

    const append = records[2]!;
    expect(append.type).toBe('context.append_message');
    if (append.type !== 'context.append_message') return;
    expect(append.message.role).toBe('assistant');
    expect(append.message.toolCalls[0]!.name).toBe('read');
    expect(append.message.toolCalls[0]!.arguments).toBe('{"path":"README.md"}');
  });

  it('preserves usage and goal shapes', () => {
    const records = loadFixture(rustFixturePath);
    const usage = records[5]!;
    expect(usage.type).toBe('usage.record');
    if (usage.type !== 'usage.record') return;
    expect(usage.model).toBe('kimi-k2');
    expect(usage.usage.inputCacheRead).toBe(1);

    const goal = records[6]!;
    expect(goal.type).toBe('goal.create');
    if (goal.type !== 'goal.create') return;
    expect(goal.goalId).toBe('g1');
    expect(goal.budgetLimits.tokenBudget).toBe(1000000);
  });
});

describe('TypeScript-generated fixtures', () => {
  it('can be parsed by TypeScript', () => {
    const records = loadFixture(tsFixturePath);
    expect(records).toHaveLength(8);
    expect(records[0]!.type).toBe('metadata');
    expect(records[1]!.type).toBe('turn.prompt');
    expect(records[2]!.type).toBe('turn.steer');
    expect(records[3]!.type).toBe('context.append_message');
    expect(records[4]!.type).toBe('context.append_loop_event');
    expect(records[5]!.type).toBe('permission.set_mode');
    expect(records[6]!.type).toBe('usage.record');
    expect(records[7]!.type).toBe('goal.create');
  });

  it('preserves tool call and message shapes', () => {
    const records = loadFixture(tsFixturePath);
    const prompt = records[1]!;
    expect(prompt.type).toBe('turn.prompt');
    if (prompt.type !== 'turn.prompt') return;
    expect(prompt.input[0]).toEqual({ type: 'text', text: 'Hello from TypeScript' });

    const append = records[3]!;
    expect(append.type).toBe('context.append_message');
    if (append.type !== 'context.append_message') return;
    expect(append.message.role).toBe('assistant');
    expect(append.message.toolCalls[0]!.name).toBe('read');
  });

  it('preserves usage and goal shapes', () => {
    const records = loadFixture(tsFixturePath);
    const usage = records[6]!;
    expect(usage.type).toBe('usage.record');
    if (usage.type !== 'usage.record') return;
    expect(usage.model).toBe('kimi-k2');
    expect(usage.usage.inputCacheRead).toBe(1);

    const goal = records[7]!;
    expect(goal.type).toBe('goal.create');
    if (goal.type !== 'goal.create') return;
    expect(goal.goalId).toBe('g2');
    expect(goal.budgetLimits.tokenBudget).toBe(1000000);
  });
});
