import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../../src/agent';
import { SplitPlanCheckpoint } from '../../../src/agent/compaction/split-checkpoint';

const SIGNAL = new AbortController().signal;

interface MockOptions {
  /** Manifest content returned by sessionMode.data(); '' or no-table means no manifest. */
  content?: string;
  isActive?: boolean;
  ratio?: number | undefined;
  maxContextTokens?: number;
  tokenCountWithPending?: number;
  dataThrows?: boolean;
  /** The split-index file path; the baseline is keyed to it. */
  filePath?: string | null;
}

interface MockAgent {
  agent: Agent;
  compactCheckpoint: ReturnType<typeof vi.fn>;
  sessionMode: { isActive: boolean; sessionModeFilePath: string | null; data: ReturnType<typeof vi.fn> };
}

function makeAgent(opts: MockOptions): MockAgent {
  const compactCheckpoint = vi.fn(async () => {});
  const sessionMode = {
    isActive: opts.isActive ?? true,
    sessionModeFilePath: opts.filePath ?? null,
    data: opts.dataThrows
      ? vi.fn(async () => {
          throw new Error('read failed');
        })
      : vi.fn(async () => (opts.content === undefined ? null : { content: opts.content })),
  };
  const agent = {
    kimiConfig:
      opts.ratio === undefined ? {} : { loopControl: { splitPlanCompactionRatio: opts.ratio } },
    sessionMode,
    config: { modelCapabilities: { max_context_tokens: opts.maxContextTokens ?? 1000 } },
    context: { tokenCountWithPending: opts.tokenCountWithPending ?? 0 },
    fullCompaction: { compactCheckpoint },
  } as unknown as Agent;
  return { agent, compactCheckpoint, sessionMode };
}

const TWO_PART_FIRST_DONE = [
  '| # | File | Scope | Status |',
  '|---|---|---|---|',
  '| 1 | dir/core.md | models | done |',
  '| 2 | dir/api.md | endpoints | pending |',
].join('\n');

const TWO_PART_NONE_DONE = [
  '| 1 | dir/core.md | models | pending |',
  '| 2 | dir/api.md | endpoints | pending |',
].join('\n');

const TWO_PART_ALL_DONE = [
  '| 1 | dir/core.md | models | done |',
  '| 2 | dir/api.md | endpoints | done |',
].join('\n');

describe('SplitPlanCheckpoint', () => {
  it('never compacts on the first observation (only initializes)', async () => {
    // Over threshold AND a part is already done, but there is no prior state to
    // compare against — so the boundary is not yet "crossed".
    const { agent, compactCheckpoint } = makeAgent({
      content: TWO_PART_FIRST_DONE,
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0.5,
    });
    await new SplitPlanCheckpoint(agent).beforeStep(SIGNAL);
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });

  it('compacts when a part boundary is crossed and context is over the ratio', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      content: TWO_PART_NONE_DONE,
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0.5,
    });
    const checkpoint = new SplitPlanCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL); // observe done=0
    // Part 1 just finished → done grows to 1, one pending remains.
    (agent.sessionMode.data as ReturnType<typeof vi.fn>).mockResolvedValue({
      content: TWO_PART_FIRST_DONE,
    });
    await checkpoint.beforeStep(SIGNAL);
    expect(compactCheckpoint).toHaveBeenCalledTimes(1);
  });

  it('does not compact when no new part completed since last step', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      content: TWO_PART_FIRST_DONE,
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0.5,
    });
    const checkpoint = new SplitPlanCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL); // observe done=1
    await checkpoint.beforeStep(SIGNAL); // still done=1 — no crossing
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });

  it('does not compact at the final boundary (no pending parts remain)', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      content: TWO_PART_FIRST_DONE,
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0.5,
    });
    const checkpoint = new SplitPlanCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL); // done=1, pending=1
    // Last part finishes → all done, nothing pending.
    (agent.sessionMode.data as ReturnType<typeof vi.fn>).mockResolvedValue({
      content: TWO_PART_ALL_DONE,
    });
    await checkpoint.beforeStep(SIGNAL);
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });

  it('does not compact when context is under the ratio', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      content: TWO_PART_NONE_DONE,
      maxContextTokens: 1000,
      tokenCountWithPending: 400, // under 0.5 * 1000
      ratio: 0.5,
    });
    const checkpoint = new SplitPlanCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL);
    (agent.sessionMode.data as ReturnType<typeof vi.fn>).mockResolvedValue({
      content: TWO_PART_FIRST_DONE,
    });
    await checkpoint.beforeStep(SIGNAL);
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });

  it('is disabled when the ratio is 0', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      content: TWO_PART_NONE_DONE,
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0,
    });
    const checkpoint = new SplitPlanCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL);
    (agent.sessionMode.data as ReturnType<typeof vi.fn>).mockResolvedValue({
      content: TWO_PART_FIRST_DONE,
    });
    await checkpoint.beforeStep(SIGNAL);
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });

  it('does nothing when no session mode is active', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      content: TWO_PART_FIRST_DONE,
      isActive: false,
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0.5,
    });
    await new SplitPlanCheckpoint(agent).beforeStep(SIGNAL);
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });

  it('does nothing for a single-file plan with no manifest', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      content: '# A plan\n\njust prose, no parts table',
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0.5,
    });
    await new SplitPlanCheckpoint(agent).beforeStep(SIGNAL);
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });

  it('uses the 0.5 default ratio when unset', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      content: TWO_PART_NONE_DONE,
      maxContextTokens: 1000,
      tokenCountWithPending: 600, // over 0.5 default, under 0.85 global
      ratio: undefined,
    });
    const checkpoint = new SplitPlanCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL);
    (agent.sessionMode.data as ReturnType<typeof vi.fn>).mockResolvedValue({
      content: TWO_PART_FIRST_DONE,
    });
    await checkpoint.beforeStep(SIGNAL);
    expect(compactCheckpoint).toHaveBeenCalledTimes(1);
  });

  it('does not inherit a baseline across different split-index files', async () => {
    // Index A reaches done=0; the session then switches to a different index file B
    // whose manifest already shows done=2. That is B's first observation, not a
    // boundary crossing, so it must not compact despite 2 > 0.
    const { agent, compactCheckpoint, sessionMode } = makeAgent({
      content: TWO_PART_NONE_DONE,
      filePath: '/plans/a.md',
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0.5,
    });
    const checkpoint = new SplitPlanCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL); // index A, done=0
    sessionMode.sessionModeFilePath = '/plans/b.md';
    sessionMode.data.mockResolvedValue({ content: TWO_PART_FIRST_DONE });
    await checkpoint.beforeStep(SIGNAL); // index B, done=1 — first observation of B
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });

  it('reset() forgets prior state so the next observation re-initializes', async () => {
    const { agent, compactCheckpoint } = makeAgent({
      content: TWO_PART_NONE_DONE,
      maxContextTokens: 1000,
      tokenCountWithPending: 900,
      ratio: 0.5,
    });
    const checkpoint = new SplitPlanCheckpoint(agent);
    await checkpoint.beforeStep(SIGNAL); // observe done=0
    checkpoint.reset();
    // Even though done now jumps to 1, the reset cleared the baseline, so this is
    // treated as a first observation and must not compact.
    (agent.sessionMode.data as ReturnType<typeof vi.fn>).mockResolvedValue({
      content: TWO_PART_FIRST_DONE,
    });
    await checkpoint.beforeStep(SIGNAL);
    expect(compactCheckpoint).not.toHaveBeenCalled();
  });
});
