import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../../src/agent';
import {
  buildCriticPrompt,
  DesignReviewer,
  escalatedSeverities,
  parseAuditLevel,
  parseFindings,
} from '../../../src/agent/plan/design-reviewer';

function makeAgent(
  overrides: {
    rawGenerate?: ReturnType<typeof vi.fn>;
    modelProvider?: unknown;
  } = {},
): { agent: Agent; rawGenerate: ReturnType<typeof vi.fn>; track: ReturnType<typeof vi.fn> } {
  const rawGenerate =
    overrides.rawGenerate ??
    vi.fn().mockResolvedValue({
      message: {
        content: [
          {
            type: 'text',
            text: JSON.stringify({
              findings: [{ severity: 'high', title: 'Substring filter', detail: 'rejects auth-refactor' }],
            }),
          },
        ],
      },
    });
  const track = vi.fn();
  const agent = {
    rawGenerate,
    log: { warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() },
    telemetry: { track },
    modelProvider:
      overrides.modelProvider !== undefined
        ? overrides.modelProvider
        : {
            resolveProviderConfig: vi.fn(() => ({
              providerName: 'deepseek_1',
              provider: { type: 'deepseek', model: 'reviewer', apiKey: 'k' },
              modelCapabilities: {},
            })),
            resolveAuth: vi.fn(() => undefined),
          },
  } as unknown as Agent;
  return { agent, rawGenerate, track };
}

const reviewerAlias = 'ody-code/kimi-for-coding';

describe('parseAuditLevel', () => {
  it('reads the recorded audit level in all three flavours', () => {
    expect(parseAuditLevel('## Audit Level\n\n**Deep** [C:USER] — confirm everything')).toBe('Deep');
    expect(parseAuditLevel('## Audit Level\n**Basic**')).toBe('Basic');
    expect(parseAuditLevel('## Audit Level\n**Standard** — review each')).toBe('Standard');
  });

  it('is case-insensitive on the heading and value', () => {
    expect(parseAuditLevel('## audit level\n**deep**')).toBe('Deep');
  });

  it('falls back to Standard when no audit level is recorded', () => {
    expect(parseAuditLevel('# Some design\n\nNo audit section here.')).toBe('Standard');
  });
});

describe('escalatedSeverities', () => {
  it('escalates monotonically with the audit level', () => {
    expect(escalatedSeverities('Basic')).toEqual(['high']);
    expect(escalatedSeverities('Standard')).toEqual(['high', 'med']);
    expect(escalatedSeverities('Deep')).toEqual(['high', 'med', 'low']);
  });
});

describe('buildCriticPrompt', () => {
  it('demands strict JSON, concrete triggers, and the security lens', () => {
    const prompt = buildCriticPrompt();
    expect(prompt).toContain('STRICT JSON');
    expect(prompt).toContain('false positives');
    expect(prompt).toContain('CONCRETE input that breaks it');
  });
});

describe('parseFindings', () => {
  it('parses a clean JSON object', () => {
    const findings = parseFindings('{"findings":[{"severity":"med","title":"t","detail":"d"}]}');
    expect(findings).toEqual([{ severity: 'med', title: 't', detail: 'd' }]);
  });

  it('strips ```json fences', () => {
    const findings = parseFindings('```json\n{"findings":[{"severity":"low","title":"x","detail":"y"}]}\n```');
    expect(findings).toHaveLength(1);
    expect(findings?.[0]?.severity).toBe('low');
  });

  it('salvages a JSON object wrapped in prose', () => {
    const findings = parseFindings('Here is my review: {"findings":[{"severity":"high","title":"a","detail":"b"}]} done');
    expect(findings).toHaveLength(1);
  });

  it('returns an empty array when there are no findings', () => {
    expect(parseFindings('{"findings":[]}')).toEqual([]);
  });

  it('skips entries with an invalid severity', () => {
    const findings = parseFindings(
      '{"findings":[{"severity":"nope","title":"a","detail":"b"},{"severity":"high","title":"c","detail":"d"}]}',
    );
    expect(findings).toEqual([{ severity: 'high', title: 'c', detail: 'd' }]);
  });

  it('keeps optional location and suggestedFix when present', () => {
    const findings = parseFindings(
      '{"findings":[{"severity":"high","title":"t","detail":"d","location":"L12","suggestedFix":"use whole-word match"}]}',
    );
    expect(findings?.[0]).toMatchObject({ location: 'L12', suggestedFix: 'use whole-word match' });
  });

  it('returns null on unparseable output', () => {
    expect(parseFindings('not json at all')).toBeNull();
    expect(parseFindings('')).toBeNull();
    expect(parseFindings('{"notFindings": 1}')).toBeNull();
  });
});

describe('DesignReviewer', () => {
  it('runs the critique and returns parsed findings with the file audit level', async () => {
    const { agent, track } = makeAgent();
    const reviewer = new DesignReviewer(agent, { reviewerAlias });
    const result = await reviewer.review('## Audit Level\n**Deep**\n\nsome design');

    expect(result.ok).toBe(true);
    expect(result.auditLevel).toBe('Deep');
    expect(result.findings).toHaveLength(1);
    expect(result.findings[0]?.severity).toBe('high');
    expect(track).toHaveBeenCalledWith(
      'design_review_completed',
      expect.objectContaining({ auditLevel: 'Deep' }),
    );
  });

  it('resolves the reviewer model auth and threads it through to rawGenerate', async () => {
    const withAuth = vi.fn((req: (auth: unknown) => unknown) => req({ apiKey: 'reviewer-token' }));
    const { agent, rawGenerate } = makeAgent({
      modelProvider: {
        resolveProviderConfig: vi.fn(() => ({
          providerName: 'managed',
          provider: { type: 'kimi', model: 'kimi-for-coding', apiKey: '' },
          modelCapabilities: {},
        })),
        resolveAuth: vi.fn(() => withAuth),
      },
    });
    const reviewer = new DesignReviewer(agent, { reviewerAlias });
    await reviewer.review('design');

    expect(withAuth).toHaveBeenCalledTimes(1);
    const options = rawGenerate.mock.calls[0]?.[5];
    expect(options).toMatchObject({ auth: { apiKey: 'reviewer-token' } });
  });

  it('degrades when no model provider is available', async () => {
    const { agent, track } = makeAgent({ modelProvider: undefined });
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (agent as any).modelProvider = undefined;
    const result = await reviewer(agent).review('design');

    expect(result.ok).toBe(false);
    expect(result.note).toContain('No model provider');
    expect(track).toHaveBeenCalledWith('design_review_failed', { reason: 'no_model_provider' });
  });

  it('degrades when the reviewer alias cannot be resolved', async () => {
    const { agent, track } = makeAgent({
      modelProvider: {
        resolveProviderConfig: vi.fn(() => {
          throw new Error('Model "x" is not configured');
        }),
        resolveAuth: vi.fn(() => undefined),
      },
    });
    const result = await reviewer(agent).review('design');

    expect(result.ok).toBe(false);
    expect(result.note).toContain('unavailable');
    expect(track).toHaveBeenCalledWith('design_review_failed', { reason: 'alias_unresolved' });
  });

  it('degrades when generation throws (e.g. timeout)', async () => {
    const { agent } = makeAgent({ rawGenerate: vi.fn().mockRejectedValue(new Error('aborted')) });
    const result = await reviewer(agent).review('design');
    expect(result.ok).toBe(false);
    expect(result.note).toContain('Review generation failed');
  });

  it('degrades when the reviewer output is unparseable', async () => {
    const { agent, track } = makeAgent({
      rawGenerate: vi.fn().mockResolvedValue({ message: { content: [{ type: 'text', text: 'sorry, no JSON' }] } }),
    });
    const result = await reviewer(agent).review('design');
    expect(result.ok).toBe(false);
    expect(result.note).toContain('could not be parsed');
    expect(track).toHaveBeenCalledWith('design_review_failed', { reason: 'unparseable' });
  });
});

function reviewer(agent: Agent): DesignReviewer {
  return new DesignReviewer(agent, { reviewerAlias });
}
