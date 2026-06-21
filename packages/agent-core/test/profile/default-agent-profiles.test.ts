import { describe, expect, it } from 'vitest';

import { DEFAULT_AGENT_PROFILES, loadAgentProfilesFromSources } from '../../src/profile';

const promptContext = {
  osEnv: {
    osKind: 'macOS',
    osArch: 'arm64',
    osVersion: '0',
    shellName: 'bash',
    shellPath: '/bin/bash',
  },
  cwd: '/workspace',
  now: '2026-05-09T00:00:00.000Z',
} as const;

describe('default agent profiles', () => {
  it('loads the bundled default system prompt from embedded sources', () => {
    const prompt = DEFAULT_AGENT_PROFILES['agent']?.systemPrompt(promptContext);

    expect(prompt).toContain('You are Ody Code CLI');
    expect(prompt).toContain('Available skills');
    expect(prompt).toContain('/workspace');
  });

  it('lists the goal tools on the agent profile but not on subagent profiles', () => {
    const agentTools = DEFAULT_AGENT_PROFILES['agent']?.tools ?? [];
    expect(agentTools).toEqual(expect.arrayContaining(['CreateGoal', 'GetGoal']));
    for (const name of ['coder', 'explore', 'plan']) {
      const tools = DEFAULT_AGENT_PROFILES[name]?.tools ?? [];
      expect(tools).not.toContain('CreateGoal');
      expect(tools).not.toContain('GetGoal');
    }
  });

  it('exposes the design-mode tools on the agent profile so design mode is usable', () => {
    // Design mode is toggled from the UI (shift+tab / /design / --design), so the
    // model only ever sees ExitDesignMode / ShowDesignMockup if the active profile
    // enables them. Regression guard for the "design mode gets stuck" bug.
    const agentTools = DEFAULT_AGENT_PROFILES['agent']?.tools ?? [];
    expect(agentTools).toEqual(
      expect.arrayContaining(['EnterDesignMode', 'ExitDesignMode', 'ShowDesignMockup']),
    );
  });

  it('instructs the model to test externally-facing interfaces through their interface', () => {
    // Reliability lever for "changed an HTTP endpoint → must add a handler-level
    // test". Project-agnostic; lives in the always-in-context system prompt.
    const prompt = DEFAULT_AGENT_PROFILES['agent']?.systemPrompt(promptContext) ?? '';
    expect(prompt).toContain('externally-facing interface');
    expect(prompt).toMatch(/HTTP endpoint\/handler/);
    // Authenticated interfaces: cover both the authorized and rejected paths.
    expect(prompt).toMatch(/authentication/i);
    expect(prompt).toMatch(/401\/403/);
  });

  it('exposes the E2E + test-review tools on the implementation profiles', () => {
    // These tools are registered as builtins, but the model only ever sees them
    // if the active profile enables them. They were silently absent from every
    // profile, so the auto-injected "Generate and run E2E tests" task could never
    // be executed. Regression guard for that dead-tool bug.
    for (const name of ['agent', 'coder']) {
      const tools = DEFAULT_AGENT_PROFILES[name]?.tools ?? [];
      expect(tools).toEqual(expect.arrayContaining(['RunE2ETests', 'ReviewTests']));
    }
  });

  it('exposes RequestCodeReview on the implementation profiles', () => {
    for (const name of ['agent', 'coder']) {
      const tools = DEFAULT_AGENT_PROFILES[name]?.tools ?? [];
      expect(tools).toContain('RequestCodeReview');
    }
  });

  it('defines a read-only reviewer subagent profile (no write tools)', () => {
    const reviewer = DEFAULT_AGENT_PROFILES['reviewer'];
    expect(reviewer).toBeDefined();
    const tools = reviewer?.tools ?? [];
    expect(tools).toEqual(expect.arrayContaining(['Read', 'Grep', 'Glob']));
    expect(tools).not.toContain('Write');
    expect(tools).not.toContain('Edit');
    // The agent profile must declare it as a spawnable subagent.
    expect(DEFAULT_AGENT_PROFILES['agent']?.subagents?.['reviewer']).toBeDefined();
  });

  it('fails loudly when an embedded system prompt source is missing', () => {
    expect(() =>
      loadAgentProfilesFromSources(['profile/default/agent.yaml'], {
        'profile/default/agent.yaml': 'name: agent\nsystemPromptPath: ./missing.md\n',
      }),
    ).toThrow(/Embedded agent profile source missing: profile\/default\/missing\.md/);
  });
});
