import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../../src/agent';
import {
  buildCriticPrompt,
  AdvancedSessionReviewer,
  escalatedSeverities,
  parseAuditLevel,
  parseFindings,
  parseMutationProbes,
  shouldEscalate,
} from '../../../src/agent/session-mode/reviewer';

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
      'modelProvider' in overrides
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

  // Regression guard for the adversarial stance-cut. The reviewer must be primed
  // as an attacker whose win condition is breaking the design — this fights the
  // confirmation bias a neutral "review" persona inherits. Fails loudly if the
  // stance is softened back to evaluator; does NOT prove a behavioral lift (that
  // needs an eval harness feeding designs with known defects).
  it('primes an adversary stance with a win/lose framing', () => {
    const prompt = buildCriticPrompt();
    expect(prompt).toContain('ADVERSARY');
    expect(prompt).toContain('BREAK it');
    expect(prompt).toContain('LOSING answer');
    // A finding without a concrete trigger loses the round (kills vibe-only output).
    expect(prompt).toContain('does not count');
  });

  it('includes the self-falsification gate to kill self-contradictory findings', () => {
    const prompt = buildCriticPrompt();
    // Gate: mentally execute trigger → if output is correct, discard the finding.
    expect(prompt).toContain('the output you describe is actually correct');
    expect(prompt).toContain('discard the finding');
  });

  it('includes the confidence schema and rubric', () => {
    const prompt = buildCriticPrompt();
    expect(prompt).toContain('"confidence"');
    expect(prompt).toContain('speculative');
    expect(prompt).toContain('certain');
    expect(prompt).toContain('likely');
  });
});

describe("buildCriticPrompt('plan')", () => {
  it('also includes the self-falsification gate and confidence schema', () => {
    const prompt = buildCriticPrompt('plan');
    expect(prompt).toContain('the output you describe is actually correct');
    expect(prompt).toContain('"confidence"');
    expect(prompt).toContain('speculative');
  });
  it('shares the adversary stance and JSON envelope with the design prompt', () => {
    const prompt = buildCriticPrompt('plan');
    expect(prompt).toContain('ADVERSARY');
    expect(prompt).toContain('STRICT JSON');
    expect(prompt).toContain('does not count');
    // It attacks an EXECUTION PLAN, not a design document.
    expect(prompt).toContain('EXECUTION PLAN');
  });

  it('targets execution-plan failure modes, not design lenses', () => {
    const prompt = buildCriticPrompt('plan');
    expect(prompt).toContain('Depends on:');
    expect(prompt).toContain('--allow-empty');
    expect(prompt).toContain('EVERY caller');
    expect(prompt).toContain('must-survive');
    expect(prompt).toContain('spec-coverage');
    expect(prompt).toContain('GAP');
    // The design-only PII/secrets lens is not the focus of a plan review.
    expect(prompt).not.toContain('PII leaking');
  });
});

describe("buildCriticPrompt('tests')", () => {
  it('shares the adversary stance, self-falsification gate, and JSON envelope', () => {
    const prompt = buildCriticPrompt('tests');
    expect(prompt).toContain('ADVERSARY');
    expect(prompt).toContain('STRICT JSON');
    expect(prompt).toContain('does not count');
    expect(prompt).toContain('the output you describe is actually correct');
    expect(prompt).toContain('"confidence"');
    // It attacks the TEST SUITE, not a design or plan.
    expect(prompt).toContain('TEST SUITE');
    expect(prompt).not.toContain('EXECUTION PLAN');
    expect(prompt).not.toContain('PII leaking');
  });

  it('targets test-code failure modes (tautology, mock theatre, happy-path-only, weak assertions)', () => {
    const prompt = buildCriticPrompt('tests');
    expect(prompt).toContain('Tautology');
    expect(prompt).toContain('mock');
    expect(prompt).toContain('must-pass AND a must-reject');
    expect(prompt).toContain('Weak assertions');
  });

  it('requests executable mutation probes and extends the JSON envelope with them', () => {
    const prompt = buildCriticPrompt('tests');
    expect(prompt).toContain('Mutation probes');
    expect(prompt).toContain('cannot run code');
    expect(prompt).toContain('"mutationProbes"');
    expect(prompt).toContain('expectedCatch');
  });

  it('does NOT leak mutation-probe instructions into design or plan prompts', () => {
    expect(buildCriticPrompt('design')).not.toContain('mutationProbes');
    expect(buildCriticPrompt('design')).not.toContain('Mutation probes');
    expect(buildCriticPrompt('plan')).not.toContain('mutationProbes');
  });
});

describe('parseMutationProbes', () => {
  it('parses a well-formed probe array', () => {
    const probes = parseMutationProbes(
      '{"findings":[],"mutationProbes":[{"location":"src/a.ts:42","mutation":"negate the < check","expectedCatch":"rejects empty input"}]}',
    );
    expect(probes).toEqual([
      { location: 'src/a.ts:42', mutation: 'negate the < check', expectedCatch: 'rejects empty input' },
    ]);
  });

  it('drops probes with no mutation and tolerates coarse location/expectedCatch', () => {
    const probes = parseMutationProbes(
      '{"mutationProbes":[{"location":"x","expectedCatch":"y"},{"mutation":"change 3 to 4"}]}',
    );
    expect(probes).toEqual([{ location: '', mutation: 'change 3 to 4', expectedCatch: '' }]);
  });

  it('returns [] when the array is absent, empty, or the output is unparseable', () => {
    expect(parseMutationProbes('{"findings":[]}')).toEqual([]);
    expect(parseMutationProbes('{"mutationProbes":[]}')).toEqual([]);
    expect(parseMutationProbes('not json')).toEqual([]);
    expect(parseMutationProbes('')).toEqual([]);
  });

  it('strips ```json fences before parsing', () => {
    const probes = parseMutationProbes(
      '```json\n{"mutationProbes":[{"location":"l","mutation":"m","expectedCatch":"e"}]}\n```',
    );
    expect(probes).toHaveLength(1);
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

  it('does not strip ``` fences that appear inside JSON detail fields', () => {
    // Regression guard: the detail field may contain markdown code fences.
    // stripCodeFences must only match fences that wrap the ENTIRE output.
    const raw =
      '{"findings":[{"severity":"high","title":"t","detail":"`extractFirstHeading(\'```\\n# not a heading\\n```\\n# Real\')`"}]}';
    const findings = parseFindings(raw);
    expect(findings).toHaveLength(1);
    expect(findings?.[0]?.severity).toBe('high');
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

  it('parses a valid confidence field', () => {
    const findings = parseFindings(
      '{"findings":[{"severity":"high","confidence":"speculative","title":"t","detail":"d"}]}',
    );
    expect(findings?.[0]?.confidence).toBe('speculative');
  });

  it('parses certain and likely confidence values', () => {
    const certain = parseFindings(
      '{"findings":[{"severity":"med","confidence":"certain","title":"t","detail":"d"}]}',
    );
    expect(certain?.[0]?.confidence).toBe('certain');
    const likely = parseFindings(
      '{"findings":[{"severity":"low","confidence":"likely","title":"t","detail":"d"}]}',
    );
    expect(likely?.[0]?.confidence).toBe('likely');
  });

  it('omits confidence when the field is missing or invalid', () => {
    const missing = parseFindings('{"findings":[{"severity":"high","title":"t","detail":"d"}]}');
    expect(missing?.[0]).not.toHaveProperty('confidence');

    const invalid = parseFindings(
      '{"findings":[{"severity":"high","confidence":"unknown","title":"t","detail":"d"}]}',
    );
    expect(invalid?.[0]).not.toHaveProperty('confidence');
  });
});

describe('shouldEscalate', () => {
  it('escalates high/certain at Standard level', () => {
    expect(shouldEscalate('high', 'certain', 'Standard')).toBe(true);
  });

  it('never escalates speculative findings regardless of severity or level', () => {
    expect(shouldEscalate('high', 'speculative', 'Deep')).toBe(false);
    expect(shouldEscalate('med', 'speculative', 'Standard')).toBe(false);
    expect(shouldEscalate('low', 'speculative', 'Deep')).toBe(false);
  });

  it('treats undefined confidence as non-speculative (no regression)', () => {
    // A reviewer that omits the field should not have its findings silently suppressed.
    expect(shouldEscalate('high', undefined, 'Standard')).toBe(true);
    expect(shouldEscalate('med', undefined, 'Standard')).toBe(true);
    expect(shouldEscalate('low', undefined, 'Standard')).toBe(false); // low not in Standard
  });

  it('respects audit level thresholds', () => {
    expect(shouldEscalate('low', 'certain', 'Basic')).toBe(false);
    expect(shouldEscalate('low', 'certain', 'Standard')).toBe(false);
    expect(shouldEscalate('low', 'certain', 'Deep')).toBe(true);
    expect(shouldEscalate('med', 'certain', 'Basic')).toBe(false);
    expect(shouldEscalate('med', 'certain', 'Standard')).toBe(true);
  });
});

function reviewer(agent: Agent): AdvancedSessionReviewer {
  return new AdvancedSessionReviewer(agent, { reviewerAlias });
}

describe('AdvancedSessionReviewer', () => {
  it('runs the critique and returns parsed findings with the file audit level', async () => {
    const { agent, track } = makeAgent();
    const reviewer = new AdvancedSessionReviewer(agent, { reviewerAlias });
    const result = await reviewer.review('## Audit Level\n**Deep**\n\nsome design');

    expect(result.ok).toBe(true);
    expect(result.auditLevel).toBe('Deep');
    expect(result.findings).toHaveLength(1);
    expect(result.findings[0]?.severity).toBe('high');
    expect(track).toHaveBeenCalledWith(
      'advanced_session_review_completed',
      expect.objectContaining({ auditLevel: 'Deep' }),
    );
  });

  it('routes the critic prompt by document kind (default is design)', async () => {
    // Default (no kind): the system prompt is the design attack surface.
    const design = makeAgent();
    await new AdvancedSessionReviewer(design.agent, { reviewerAlias }).review('a design');
    expect(design.rawGenerate.mock.calls[0]?.[1]).toContain('DESIGN DOCUMENT');
    expect(design.rawGenerate.mock.calls[0]?.[1]).not.toContain('EXECUTION PLAN');

    // kind:'plan' selects the execution-plan attack surface instead.
    const plan = makeAgent();
    await new AdvancedSessionReviewer(plan.agent, { reviewerAlias, kind: 'plan' }).review('a plan');
    expect(plan.rawGenerate.mock.calls[0]?.[1]).toContain('EXECUTION PLAN');
    expect(plan.rawGenerate.mock.calls[0]?.[1]).toContain('Depends on:');
  });

  it("routes kind:'tests' to the test-code attack surface and attaches mutation probes", async () => {
    const tests = makeAgent({
      rawGenerate: vi.fn().mockResolvedValue({
        message: {
          content: [
            {
              type: 'text',
              text: JSON.stringify({
                findings: [{ severity: 'high', title: 'Tautological assert', detail: 'expect(x).toBe(x)' }],
                mutationProbes: [
                  { location: 'src/sum.ts:3', mutation: 'return a - b', expectedCatch: 'adds two numbers' },
                ],
              }),
            },
          ],
        },
      }),
    });
    const result = await new AdvancedSessionReviewer(tests.agent, { reviewerAlias, kind: 'tests' }).review(
      'changed test + impl',
    );
    expect(tests.rawGenerate.mock.calls[0]?.[1]).toContain('TEST SUITE');
    expect(tests.rawGenerate.mock.calls[0]?.[1]).toContain('Mutation probes');
    expect(result.ok).toBe(true);
    expect(result.findings).toHaveLength(1);
    expect(result.mutationProbes).toEqual([
      { location: 'src/sum.ts:3', mutation: 'return a - b', expectedCatch: 'adds two numbers' },
    ]);
  });

  it('does not attach mutation probes for non-test reviews', async () => {
    const { agent } = makeAgent({
      rawGenerate: vi.fn().mockResolvedValue({
        message: {
          content: [
            {
              type: 'text',
              text: JSON.stringify({
                findings: [],
                mutationProbes: [{ location: 'a', mutation: 'b', expectedCatch: 'c' }],
              }),
            },
          ],
        },
      }),
    });
    const result = await new AdvancedSessionReviewer(agent, { reviewerAlias, kind: 'design' }).review('a design');
    expect(result.mutationProbes).toBeUndefined();
  });

  it('disables extended thinking on the reviewer provider (latency / timeout guard)', async () => {
    // The critique is a single-shot JSON task; a long streamed reasoning trace is
    // pure latency and the usual cause of the review tripping its timeout (this is
    // exactly how GLM-5.1, which defaults thinking ON, blows past the deadline).
    // The provider handed to rawGenerate must have thinking turned off.
    const { agent, rawGenerate } = makeAgent({
      modelProvider: {
        resolveProviderConfig: vi.fn(() => ({
          providerName: 'glm_1',
          provider: { type: 'glm', model: 'glm-5.1', apiKey: 'k' },
          modelCapabilities: {},
        })),
        resolveAuth: vi.fn(() => undefined),
      },
    });
    await new AdvancedSessionReviewer(agent, { reviewerAlias }).review('a design');
    const provider = rawGenerate.mock.calls[0]?.[0] as { thinkingEffort?: unknown };
    expect(provider.thinkingEffort).toBe('off');
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
    const reviewer = new AdvancedSessionReviewer(agent, { reviewerAlias });
    await reviewer.review('design');

    expect(withAuth).toHaveBeenCalledTimes(1);
    const options = rawGenerate.mock.calls[0]?.[5];
    expect(options).toMatchObject({ auth: { apiKey: 'reviewer-token' } });
  });

  it('degrades when no model provider is available', async () => {
    const { agent, track } = makeAgent({ modelProvider: undefined });
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

