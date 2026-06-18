import { describe, it, expect, vi } from 'vitest';

// Mock the OdyTUI class before importing
vi.mock('../../src/tui/ody-tui', () => ({
  OdyTUI: vi.fn().mockImplementation(() => ({
    start: vi.fn().mockResolvedValue(undefined),
    onExit: undefined as any,
  })),
}));

vi.mock('@odysseythink/ody-telemetry', () => ({
  track: vi.fn(),
}));

describe('runGameDesign', () => {
  it('is a function', async () => {
    const { runGameDesign } = await import('../../src/cli/run-game-design');
    expect(typeof runGameDesign).toBe('function');
  });
});
