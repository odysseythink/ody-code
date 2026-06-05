import chalk from 'chalk';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { FooterComponent } from '#/tui/components/chrome/footer';
import { setRainbowDance, type RainbowDanceController } from '#/tui/easter-eggs/dance';
import { darkColors } from '#/tui/theme/colors';
import type { AppState } from '#/tui/types';

const TRUECOLOR_PATTERN = /\[38;2;(\d+);(\d+);(\d+)m/g;

function truecolorCodes(text: string): Set<string> {
  const codes = new Set<string>();
  for (const match of text.matchAll(TRUECOLOR_PATTERN)) {
    codes.add(`${match[1]},${match[2]},${match[3]}`);
  }
  return codes;
}

// Dark dance colors the footer never uses outside of /dance.
const RAINBOW_CYAN = '91,192,190';
const RAINBOW_GREEN = '78,200,126';

function setDanceView(colored: boolean, phase: number): void {
  const dance: RainbowDanceController = {
    colored,
    phase,
    start: () => {},
    stop: () => {},
    dispose: () => {},
  };
  setRainbowDance(dance);
}

const appState: AppState = {
  version: '1.2.3',
  workDir: '/tmp/project',
  sessionId: 'ses-1',
  sessionTitle: null,
  model: 'kimi-k2',
  permissionMode: 'manual',
  thinking: false,
  contextUsage: 0,
  contextTokens: 0,
  maxContextTokens: 0,
  isCompacting: false,
  isReplaying: false,
  streamingPhase: 'idle',
  streamingStartTime: 0,
  sessionMode: 'normal',
  theme: 'dark',
  editorCommand: null,
  notifications: { enabled: true, condition: 'unfocused' },
  upgrade: { autoInstall: true },
  availableModels: {},
  availableProviders: {},
  mcpServersSummary: null,
};

describe('FooterComponent', () => {
  const previousChalkLevel = chalk.level;

  beforeEach(() => {
    chalk.level = 3;
  });

  afterEach(() => {
    chalk.level = previousChalkLevel;
    setRainbowDance(undefined);
  });

  it('paints the model name in rainbow while colored', () => {
    setDanceView(true, 0);
    const footer = new FooterComponent(appState, darkColors);

    const codes = truecolorCodes(footer.render(120).join('\n'));

    // "kimi-k2" spreads across the palette, pulling in colors the footer
    // never renders on its own.
    expect(codes.has(RAINBOW_CYAN)).toBe(true);
    expect(codes.has(RAINBOW_GREEN)).toBe(true);
  });

  it('renders the model name in its normal color when not dancing', () => {
    const footer = new FooterComponent(appState, darkColors);

    const codes = truecolorCodes(footer.render(120).join('\n'));

    expect(codes.has(RAINBOW_CYAN)).toBe(false);
    expect(codes.has(RAINBOW_GREEN)).toBe(false);
  });
});

function stripAnsi(text: string): string {
  // eslint-disable-next-line no-control-regex
  return text.replace(/\u001b\[[0-9;]*m/g, '');
}

const baseAppState: AppState = {
  version: '1.2.3',
  workDir: '/tmp/project',
  sessionId: 'ses-1',
  sessionTitle: null,
  model: 'kimi-k2',
  permissionMode: 'manual',
  thinking: false,
  contextUsage: 0.452,
  contextTokens: 12_300,
  maxContextTokens: 27_300,
  isCompacting: false,
  isReplaying: false,
  streamingPhase: 'idle',
  streamingStartTime: 0,
  sessionMode: 'normal',
  theme: 'dark',
  editorCommand: null,
  notifications: { enabled: true, condition: 'unfocused' },
  upgrade: { autoInstall: true },
  availableModels: {},
  availableProviders: {},
  mcpServersSummary: null,
};

describe('FooterComponent mode badge', () => {
  it('renders inverted plan badge without filename on Line 2', () => {
    const state = { ...baseAppState, sessionMode: 'plan', advancedSessionModeFilePath: 'plan.md' };
    const footer = new FooterComponent(state, darkColors);
    const lines = footer.render(120);
    const line2 = stripAnsi(lines[1]!);
    expect(line2).toContain('📝');
    expect(line2).toContain('plan');
    expect(line2).not.toContain('plan.md');
    expect(line2).not.toContain('·');
    expect(line2).toContain('【');
    expect(line2).toContain('】');
  });

  it('renders inverted design badge without filename on Line 2', () => {
    const state = { ...baseAppState, sessionMode: 'design', advancedSessionModeFilePath: 'design.md' };
    const footer = new FooterComponent(state, darkColors);
    const lines = footer.render(120);
    const line2 = stripAnsi(lines[1]!);
    expect(line2).toContain('✏️');
    expect(line2).toContain('design');
    expect(line2).not.toContain('design.md');
    expect(line2).not.toContain('·');
  });

  it('renders normal badge without filename when no advancedSessionModeFilePath is set', () => {
    const state = { ...baseAppState, sessionMode: 'normal', sessionMode: 'normal' };
    const footer = new FooterComponent(state, darkColors);
    const lines = footer.render(120);
    const line2 = stripAnsi(lines[1]!);
    expect(line2).toContain('⚒️');
    expect(line2).toContain('normal');
    expect(line2).not.toContain('·');
  });

  it('renders normal badge with filename when advancedSessionModeFilePath is set', () => {
    const state = { ...baseAppState, sessionMode: 'normal', sessionMode: 'normal', advancedSessionModeFilePath: 'normal.md' };
    const footer = new FooterComponent(state, darkColors);
    const lines = footer.render(120);
    const line2 = stripAnsi(lines[1]!);
    expect(line2).toContain('⚒️');
    expect(line2).toContain('normal');
    expect(line2).toContain('normal.md');
    expect(line2).toContain('·');
  });

  it('falls back to mode-only badge when advancedSessionModeFilePath is null', () => {
    const state = { ...baseAppState, sessionMode: 'plan', advancedSessionModeFilePath: null };
    const footer = new FooterComponent(state, darkColors);
    const lines = footer.render(120);
    const line2 = stripAnsi(lines[1]!);
    expect(line2).toContain('📝');
    expect(line2).toContain('plan');
    expect(line2).not.toContain('·');
  });

  it('truncates long filenames on the badge', () => {
    const longName = 'very-long-file-name-that-exceeds-the-available-space.md';
    const state = { ...baseAppState, sessionMode: 'normal', sessionMode: 'normal', advancedSessionModeFilePath: longName };
    const footer = new FooterComponent(state, darkColors);
    const lines = footer.render(120);
    const line2 = stripAnsi(lines[1]!);
    expect(line2).toContain('…');
    expect(line2).not.toContain(longName);
  });

  it('places transient hint on the right when space allows', () => {
    const state = { ...baseAppState, sessionMode: 'plan', advancedSessionModeFilePath: 'plan.md' };
    const footer = new FooterComponent(state, darkColors);
    footer.setTransientHint('Press Ctrl+C again');
    const lines = footer.render(120);
    const line2 = stripAnsi(lines[1]!);
    const hintIndex = line2.indexOf('Press Ctrl+C again');
    const contextIndex = line2.indexOf('context:');
    expect(hintIndex).toBeGreaterThan(-1);
    expect(contextIndex).toBeGreaterThan(-1);
    expect(hintIndex).toBeLessThan(contextIndex);
  });

  it('hides hint on narrow terminals while keeping badge and context', () => {
    const state = { ...baseAppState, sessionMode: 'plan', advancedSessionModeFilePath: 'plan.md' };
    const footer = new FooterComponent(state, darkColors);
    footer.setTransientHint('Press Ctrl+C again to exit');
    const lines = footer.render(40);
    const line2 = stripAnsi(lines[1]!);
    expect(line2).toContain('📝');
    expect(line2).toContain('context:');
    expect(line2).not.toContain('Press Ctrl+C');
  });

  it('does not render plain mode badge on Line 1', () => {
    const state = { ...baseAppState, sessionMode: 'plan', advancedSessionModeFilePath: 'plan.md' };
    const footer = new FooterComponent(state, darkColors);
    const lines = footer.render(120);
    const line1 = stripAnsi(lines[0]!);
    const line1Words = line1.split(/\s+/);
    expect(line1Words).not.toContain('plan');
    expect(line1Words).not.toContain('design');
    expect(line1Words).not.toContain('normal');
  });
});
