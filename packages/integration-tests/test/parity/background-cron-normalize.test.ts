import { describe, expect, it } from 'vitest';

import { normalizeBackgroundCronSnapshot } from '../../src/parity/normalize-background-cron';

describe('normalizeBackgroundCronSnapshot', () => {
  it('masks dynamic background ids and timestamps but keeps semantic fields', () => {
    const normalized = normalizeBackgroundCronSnapshot({
      backgroundTasks: [
        {
          taskId: 'bash-a1b2c3d4',
          kind: 'process',
          description: 'echo done',
          status: 'completed',
          startedAt: 1_700_000_000_000,
          endedAt: 1_700_000_001_000,
          stopReason: undefined,
        },
      ],
    });

    const tasks = (normalized as { backgroundTasks: Record<string, unknown>[] }).backgroundTasks;
    expect(tasks[0]!['taskId']).toBe('<bg-id>');
    expect(tasks[0]!['startedAt']).toBe('<timestamp>');
    expect(tasks[0]!['endedAt']).toBe('<timestamp>');
    expect(tasks[0]!['description']).toBe('echo done');
    expect(tasks[0]!['status']).toBe('completed');
    expect(tasks[0]!['kind']).toBe('process');
  });

  it('masks dynamic cron ids but keeps semantic fields', () => {
    const normalized = normalizeBackgroundCronSnapshot({
      cronTasks: [
        {
          id: 'deadbeef',
          cron: '* * * * *',
          prompt: 'ping',
          recurring: true,
          createdAt: 1_700_000_000_000,
          lastFiredAt: 1_700_000_060_000,
        },
      ],
    });

    const tasks = (normalized as { cronTasks: Record<string, unknown>[] }).cronTasks;
    expect(tasks[0]!['id']).toBe('<cron-id>');
    // createdAt / lastFiredAt are dropped from cron-task canonicalization
    // because Rust does not yet emit them.
    expect(tasks[0]!['createdAt']).toBeUndefined();
    expect(tasks[0]!['lastFiredAt']).toBeUndefined();
    expect(tasks[0]!['cron']).toBe('* * * * *');
    expect(tasks[0]!['prompt']).toBe('ping');
    expect(tasks[0]!['recurring']).toBe(true);
  });

  it('replaces cron/background injected XML text with a stable placeholder', () => {
    const normalized = normalizeBackgroundCronSnapshot({
      contextInputs: [
        { text: '<cron-fire cron="* * * * *"><prompt>ping</prompt></cron-fire>', originKind: 'cron_job' },
        { text: 'plain user message', originKind: 'user' },
      ],
    });

    const inputs = (normalized as { contextInputs: Record<string, unknown>[] }).contextInputs;
    expect(inputs[0]!['text']).toBe('<injected-xml>');
    expect(inputs[1]!['text']).toBe('plain user message');
  });

  it('does not mask must-survive values that happen to look like ids or timestamps', () => {
    const normalized = normalizeBackgroundCronSnapshot({
      meta: {
        // "ping" does not match 8-hex id regex, must survive
        prompt: 'ping',
        // description contains "bash" but is not a taskId field, must survive
        description: 'bash wrapper script',
        // status word must survive
        status: 'completed',
        // cron expression must survive
        schedule: '*/5 * * * *',
        // non-timestamp numeric keys must survive
        retryCount: 3,
      },
    });

    const meta = (normalized as { meta: Record<string, unknown> }).meta;
    expect(meta['prompt']).toBe('ping');
    expect(meta['description']).toBe('bash wrapper script');
    expect(meta['status']).toBe('completed');
    expect(meta['schedule']).toBe('*/5 * * * *');
    expect(meta['retryCount']).toBe(3);
  });
});
