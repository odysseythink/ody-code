import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../../src/agent';
import { NormalModeTaskCheckpoint } from '../../../src/agent/compaction/normal-task-checkpoint';
import type { TodoItem } from '../../../src/tools/builtin/state/todo-list';

const SIGNAL = new AbortController().signal;

interface MockOptions {
  /** TodoList state returned by agent.tools.storeData()['todo']. */
  todos?: readonly TodoItem[];
  /** Whether sessionMode is active. */
  sessionModeActive?: boolean;
  /** Context ratio configuration (undefined = use default 0.5). */
  ratio?: number | undefined;
  /** Model's max context tokens. */
  maxContextTokens?: number;
  /** Current context usage. */
  tokenCountWithPending?: number;
  /** Whether data() or storeData() throws. */
  dataThrows?: boolean;
  /** E2E config override. */
  e2e?: { enabled: boolean };
  /** Test-review config override. */
  testReview?: { enabled: boolean };
}

function makeAgent(opts: MockOptions): {
  agent: Agent;
  compactCheckpoint: ReturnType<typeof vi.fn>;
  appendSystemReminder: ReturnType<typeof vi.fn>;
} {
  const compactCheckpoint = vi.fn(async () => {});
  const appendSystemReminder = vi.fn();
  const kimiConfig: Record<string, unknown> =
    opts.ratio === undefined ? {} : { loopControl: { normalTaskCompactionRatio: opts.ratio } };
  if (opts.e2e !== undefined) {
    kimiConfig['e2e'] = opts.e2e;
  }
  if (opts.testReview !== undefined) {
    kimiConfig['testReview'] = opts.testReview;
  }
  const agent = {
    kimiConfig,
    sessionMode: { isActive: opts.sessionModeActive ?? false },
    config: { modelCapabilities: { max_context_tokens: opts.maxContextTokens ?? 1000, max_output_tokens: 0 } },
    context: { tokenCountWithPending: opts.tokenCountWithPending ?? 0, appendSystemReminder },
    tools: {
      storeData: vi.fn(() =>
        opts.dataThrows ? { __error: 'read failed' } : { todo: opts.todos ?? [] },
      ),
    },
    fullCompaction: { compactCheckpoint },
  } as unknown as Agent;
  return { agent, compactCheckpoint, appendSystemReminder };
}

describe('NormalModeTaskCheckpoint', () => {
  it('never compacts on the first observation (only initializes)', async () => {
    // Over threshold with a `done` item, but no prior baseline.
    const { agent, compactCheckpoint } = makeAgent({
      todos: [{ title: 'Task 1', status: 'done' }],
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0.5,
    });
    await new NormalModeTaskCheckpoint(agent).beforeStep(SIGNAL);
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });

  it('compacts when a task is completed, context is over ratio, and work remains', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      todos: [
        { title: 'Task 1', status: 'done' },
        { title: 'Task 2', status: 'pending' },
      ],
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0.5,
    });
    const checkpoint = new NormalModeTaskCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL); // observe 1 done
    (agent.tools.storeData as ReturnType<typeof vi.fn>).mockReturnValue({
      todo: [
        { title: 'Task 1', status: 'done' },
        { title: 'Task 2', status: 'done' },
        { title: 'Task 3', status: 'pending' },
      ],
    });
    await checkpoint.beforeStep(SIGNAL); // 2 done now, work remains
    expect(compactCheckpoint).toHaveBeenCalledTimes(1);
  });

  it('does not compact when task is completed but no work remains', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      todos: [{ title: 'Task 1', status: 'done' }],
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0.5,
    });
    const checkpoint = new NormalModeTaskCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL);
    // Both tasks complete — no `pending` or `in_progress` remain.
    (agent.tools.storeData as ReturnType<typeof vi.fn>).mockReturnValue({
      todo: [
        { title: 'Task 1', status: 'done' },
        { title: 'Task 2', status: 'done' },
      ],
    });
    await checkpoint.beforeStep(SIGNAL);
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });

  it('does not compact when no new task completed (done count unchanged)', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      todos: [
        { title: 'Task 1', status: 'done' },
        { title: 'Task 2', status: 'in_progress' },
      ],
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0.5,
    });
    const checkpoint = new NormalModeTaskCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL);
    // done count stays at 1, still in progress on task 2
    (agent.tools.storeData as ReturnType<typeof vi.fn>).mockReturnValue({
      todo: [
        { title: 'Task 1', status: 'done' },
        { title: 'Task 2', status: 'in_progress' },
      ],
    });
    await checkpoint.beforeStep(SIGNAL);
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });

  it('does not compact when context is under the ratio', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      todos: [{ title: 'Task 1', status: 'done' }],
      maxContextTokens: 1000,
      tokenCountWithPending: 400, // under 0.5 * 1000
      ratio: 0.5,
    });
    const checkpoint = new NormalModeTaskCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL);
    (agent.tools.storeData as ReturnType<typeof vi.fn>).mockReturnValue({
      todo: [
        { title: 'Task 1', status: 'done' },
        { title: 'Task 2', status: 'done' },
        { title: 'Task 3', status: 'pending' },
      ],
    });
    await checkpoint.beforeStep(SIGNAL);
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });

  it('is disabled when the ratio is 0', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      todos: [{ title: 'Task 1', status: 'done' }],
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0,
    });
    const checkpoint = new NormalModeTaskCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL);
    (agent.tools.storeData as ReturnType<typeof vi.fn>).mockReturnValue({
      todo: [
        { title: 'Task 1', status: 'done' },
        { title: 'Task 2', status: 'done' },
        { title: 'Task 3', status: 'pending' },
      ],
    });
    await checkpoint.beforeStep(SIGNAL);
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });

  it('does nothing when sessionMode is active (plan/design mode)', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      todos: [{ title: 'Task 1', status: 'done' }],
      sessionModeActive: true,
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0.5,
    });
    const checkpoint = new NormalModeTaskCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL);
    (agent.tools.storeData as ReturnType<typeof vi.fn>).mockReturnValue({
      todo: [
        { title: 'Task 1', status: 'done' },
        { title: 'Task 2', status: 'done' },
        { title: 'Task 3', status: 'pending' },
      ],
    });
    await checkpoint.beforeStep(SIGNAL);
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });

  it('uses the 0.5 default ratio when unset', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      todos: [{ title: 'Task 1', status: 'done' }],
      maxContextTokens: 1000,
      tokenCountWithPending: 600, // over 0.5 default, under 0.85 global
      ratio: undefined,
    });
    const checkpoint = new NormalModeTaskCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL);
    (agent.tools.storeData as ReturnType<typeof vi.fn>).mockReturnValue({
      todo: [
        { title: 'Task 1', status: 'done' },
        { title: 'Task 2', status: 'done' },
        { title: 'Task 3', status: 'pending' },
      ],
    });
    await checkpoint.beforeStep(SIGNAL);
    expect(compactCheckpoint).toHaveBeenCalledTimes(1);
  });

  it('handles missing todo state gracefully (treats as empty list)', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0.5,
    });
    const checkpoint = new NormalModeTaskCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL);
    // Still empty
    await checkpoint.beforeStep(SIGNAL);
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });

  it('reset() clears the baseline so the next observation re-initializes', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      todos: [
        { title: 'Task 1', status: 'done' },
        { title: 'Task 2', status: 'pending' },
      ],
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0.5,
    });
    const checkpoint = new NormalModeTaskCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL); // observe 1 done
    checkpoint.reset();
    // Even though done count is now 1 (not > 0), reset() cleared the baseline,
    // so this is treated as a first observation and must not compact.
    await checkpoint.beforeStep(SIGNAL);
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });

  describe('E2E reminder hook', () => {
    it('injects reminder when an E2E todo is completed and E2E is enabled', async () => {
      const { agent, appendSystemReminder } = makeAgent({
        todos: [{ title: 'Run E2E tests', status: 'done' }],
        e2e: { enabled: true },
        testReview: { enabled: false }, // isolate: only the E2E reminder under test here
      });
      const checkpoint = new NormalModeTaskCheckpoint(agent);
      await checkpoint.beforeStep(SIGNAL); // observe 1 done
      expect(appendSystemReminder).not.toHaveBeenCalled();
      (agent.tools.storeData as ReturnType<typeof vi.fn>).mockReturnValue({
        todo: [
          { title: 'Run E2E tests', status: 'done' },
          { title: 'Task 2', status: 'done' },
        ],
      });
      await checkpoint.beforeStep(SIGNAL);
      expect(appendSystemReminder).toHaveBeenCalledTimes(1);
    });

    it('does not inject reminder when E2E is disabled', async () => {
      const { agent, appendSystemReminder } = makeAgent({
        todos: [{ title: 'Run E2E tests', status: 'done' }],
        e2e: { enabled: false },
        testReview: { enabled: false },
      });
      const checkpoint = new NormalModeTaskCheckpoint(agent);
      await checkpoint.beforeStep(SIGNAL);
      (agent.tools.storeData as ReturnType<typeof vi.fn>).mockReturnValue({
        todo: [
          { title: 'Run E2E tests', status: 'done' },
          { title: 'Task 2', status: 'done' },
        ],
      });
      await checkpoint.beforeStep(SIGNAL);
      expect(appendSystemReminder).not.toHaveBeenCalled();
    });

    it('does not inject reminder for generic "test" todos', async () => {
      const { agent, appendSystemReminder } = makeAgent({
        todos: [{ title: 'Write unit tests', status: 'done' }],
        e2e: { enabled: true },
        testReview: { enabled: false },
      });
      const checkpoint = new NormalModeTaskCheckpoint(agent);
      await checkpoint.beforeStep(SIGNAL);
      (agent.tools.storeData as ReturnType<typeof vi.fn>).mockReturnValue({
        todo: [
          { title: 'Write unit tests', status: 'done' },
          { title: 'Task 2', status: 'done' },
        ],
      });
      await checkpoint.beforeStep(SIGNAL);
      expect(appendSystemReminder).not.toHaveBeenCalled();
    });

    it('does not inject reminder when E2E config is absent', async () => {
      const { agent, appendSystemReminder } = makeAgent({
        todos: [{ title: 'Run E2E tests', status: 'done' }],
        testReview: { enabled: false },
      });
      const checkpoint = new NormalModeTaskCheckpoint(agent);
      await checkpoint.beforeStep(SIGNAL);
      (agent.tools.storeData as ReturnType<typeof vi.fn>).mockReturnValue({
        todo: [
          { title: 'Run E2E tests', status: 'done' },
          { title: 'Task 2', status: 'done' },
        ],
      });
      await checkpoint.beforeStep(SIGNAL);
      expect(appendSystemReminder).not.toHaveBeenCalled();
    });
  });

  describe('test-review reminder hook', () => {
    function reminderNames(mock: ReturnType<typeof vi.fn>): string[] {
      return mock.mock.calls.map((c) => (c[1] as { name?: string } | undefined)?.name ?? '');
    }

    it('injects a test_review reminder by default (no config) when a test-related task completes', async () => {
      // Default-on: testReview config absent → feature is enabled.
      const { agent, appendSystemReminder } = makeAgent({
        todos: [{ title: 'Write unit tests for parser', status: 'done' }],
      });
      const checkpoint = new NormalModeTaskCheckpoint(agent);
      await checkpoint.beforeStep(SIGNAL); // observe baseline
      expect(appendSystemReminder).not.toHaveBeenCalled();
      (agent.tools.storeData as ReturnType<typeof vi.fn>).mockReturnValue({
        todo: [
          { title: 'Write unit tests for parser', status: 'done' },
          { title: 'Add tests for validator', status: 'done' },
        ],
      });
      await checkpoint.beforeStep(SIGNAL);
      expect(reminderNames(appendSystemReminder)).toContain('test_review_reminder');
      expect(appendSystemReminder.mock.calls[0]?.[0]).toContain('ReviewTests');
    });

    it('does not inject when the feature is explicitly disabled', async () => {
      const { agent, appendSystemReminder } = makeAgent({
        todos: [{ title: 'Write unit tests', status: 'done' }],
        testReview: { enabled: false },
      });
      const checkpoint = new NormalModeTaskCheckpoint(agent);
      await checkpoint.beforeStep(SIGNAL);
      (agent.tools.storeData as ReturnType<typeof vi.fn>).mockReturnValue({
        todo: [
          { title: 'Write unit tests', status: 'done' },
          { title: 'Refactor parser', status: 'done' },
        ],
      });
      await checkpoint.beforeStep(SIGNAL);
      expect(appendSystemReminder).not.toHaveBeenCalled();
    });

    it('does not inject for tasks unrelated to tests', async () => {
      const { agent, appendSystemReminder } = makeAgent({
        todos: [{ title: 'Refactor the parser', status: 'done' }],
        testReview: { enabled: true },
      });
      const checkpoint = new NormalModeTaskCheckpoint(agent);
      await checkpoint.beforeStep(SIGNAL);
      (agent.tools.storeData as ReturnType<typeof vi.fn>).mockReturnValue({
        todo: [
          { title: 'Refactor the parser', status: 'done' },
          { title: 'Wire up the loop', status: 'done' },
        ],
      });
      await checkpoint.beforeStep(SIGNAL);
      expect(appendSystemReminder).not.toHaveBeenCalled();
    });
  });
});
