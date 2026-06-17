import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  normalizeName,
  normalizeTriggers,
  validateMicroagentInput,
  renderMicroagentFile,
  installStarterPackIfEmpty,
} from '@/tui/commands/microagent-helpers';

describe('normalizeName', () => {
  it('accepts lowercase alphanumeric with hyphens and underscores', () => {
    expect(normalizeName('reuse-conventions')).toBe('reuse-conventions');
    expect(normalizeName('my-agent_v2')).toBe('my-agent_v2');
  });

  it('lowercases input', () => {
    expect(normalizeName('MyAgent')).toBe('myagent');
  });

  it('rejects uppercase-only names (must-be-lowercase rule)', () => {
    expect(normalizeName('REUSE')).toBeUndefined();
  });

  it('rejects path separators', () => {
    expect(normalizeName('foo/bar')).toBeUndefined();
    expect(normalizeName('foo\\bar')).toBeUndefined();
  });

  it('rejects dots', () => {
    expect(normalizeName('foo.bar')).toBeUndefined();
    expect(normalizeName('foo..bar')).toBeUndefined();
  });

  it('rejects empty or whitespace-only', () => {
    expect(normalizeName('')).toBeUndefined();
    expect(normalizeName('   ')).toBeUndefined();
  });

  it('trims whitespace', () => {
    expect(normalizeName('  my-agent  ')).toBe('my-agent');
  });
});

describe('normalizeTriggers', () => {
  it('splits on comma, Chinese comma, and whitespace', () => {
    const result = normalizeTriggers('组件, page ，test');
    expect(result).toEqual(['page', 'test', '组件']);
  });

  it('deduplicates and sorts', () => {
    expect(normalizeTriggers('page, component, page')).toEqual(['component', 'page']);
  });

  it('lowercases ASCII but passes CJK through', () => {
    expect(normalizeTriggers('Component, 组件, COMPONENT')).toEqual(['component', '组件']);
  });

  it('rejects empty or whitespace-only', () => {
    expect(normalizeTriggers('')).toBeUndefined();
    expect(normalizeTriggers('   ')).toBeUndefined();
    expect(normalizeTriggers(' , ， ')).toBeUndefined();
  });

  it('trims each token', () => {
    expect(normalizeTriggers('  a  ,  b  ')).toEqual(['a', 'b']);
  });
});

describe('validateMicroagentInput', () => {
  it('returns ok for valid input', () => {
    const result = validateMicroagentInput('reuse-conventions', 'component, page', 'Reuse existing components');
    expect(result.ok).toBe(true);
    expect(result.input).toEqual({
      name: 'reuse-conventions',
      triggers: ['component', 'page'],
      description: 'Reuse existing components',
    });
  });

  it('rejects invalid name', () => {
    const result = validateMicroagentInput('Foo/bar', 'x', 'desc');
    expect(result.ok).toBe(false);
    expect(result.error?.field).toBe('name');
  });

  it('rejects empty triggers', () => {
    const result = validateMicroagentInput('x', '   ', 'desc');
    expect(result.ok).toBe(false);
    expect(result.error?.field).toBe('triggers');
  });

  it('rejects empty description', () => {
    const result = validateMicroagentInput('x', 'y', '');
    expect(result.ok).toBe(false);
    expect(result.error?.field).toBe('description');
  });

  it('rejects description over 200 chars', () => {
    const long = 'a'.repeat(201);
    const result = validateMicroagentInput('x', 'y', long);
    expect(result.ok).toBe(false);
    expect(result.error?.field).toBe('description');
  });

  it('accepts description exactly 200 chars', () => {
    const exact = 'a'.repeat(200);
    const result = validateMicroagentInput('x', 'y', exact);
    expect(result.ok).toBe(true);
  });
});

describe('renderMicroagentFile', () => {
  it('generates correct YAML frontmatter and body', () => {
    const content = renderMicroagentFile({
      name: 'reuse',
      triggers: ['component', 'page'],
      description: 'Reuse existing things',
    });
    expect(content).toContain('name: reuse');
    expect(content).toContain('type: knowledge');
    expect(content).toContain('triggers:');
    expect(content).toContain('  - component');
    expect(content).toContain('  - page');
    expect(content).toContain('description: Reuse existing things');
    expect(content).toContain('# reuse');
    expect(content).toContain('<!-- TODO: Add repo-specific conventions below. -->');
  });

  it('produces output parseable as valid frontmatter', () => {
    const content = renderMicroagentFile({
      name: 'test-agent',
      triggers: ['keyword'],
      description: 'A test agent',
    });
    // Frontmatter starts with --- and has a closing ---
    const lines = content.split('\n');
    expect(lines[0]).toBe('---');
    const closingIndex = lines.indexOf('---', 1);
    expect(closingIndex).toBeGreaterThan(0);
    // Body starts after closing ---
    expect(lines.slice(closingIndex + 1).join('\n').trim()).toContain('# test-agent');
  });
});

import { mkdir, readdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach } from 'vitest';
import type { SlashCommandHost } from '@/tui/commands/dispatch';
import { handleMicroagentCommand } from '@/tui/commands/microagent';
import { setExperimentalFlags } from '@/tui/commands/experimental-flags';
import { getColorPalette } from '@/tui/theme/colors';

describe('installStarterPackIfEmpty', () => {
  const testRoots: string[] = [];

  afterEach(async () => {
    for (const root of testRoots) {
      await import('node:fs/promises').then((fs) => fs.rm(root, { recursive: true, force: true }));
    }
    testRoots.length = 0;
  });

  async function tmpDir(): Promise<string> {
    const dir = join(tmpdir(), `ody-microagent-test-${Date.now()}-${Math.random().toString(36).slice(2)}`);
    await mkdir(dir, { recursive: true });
    testRoots.push(dir);
    return dir;
  }

  it('installs all 4 starter templates when directory is empty', async () => {
    const dir = await tmpDir();
    const installed = await installStarterPackIfEmpty(dir);
    expect(installed).toHaveLength(4);
    const names = installed.map((f) => f.fileName).sort();
    expect(names).toEqual([
      'documentation.md',
      'glossary.md',
      'reuse-conventions.md',
      'testing.md',
    ]);
    // Verify files actually exist
    const entries = await readdir(dir);
    expect(entries.sort()).toEqual(names);
  });

  it('skips installation when .md files already exist', async () => {
    const dir = await tmpDir();
    await writeFile(join(dir, 'user.md'), 'user content', 'utf-8');
    const installed = await installStarterPackIfEmpty(dir);
    expect(installed).toHaveLength(0);
    // user.md still exists
    const entries = await readdir(dir);
    expect(entries).toContain('user.md');
  });

  it('installs when directory has non-.md files only', async () => {
    const dir = await tmpDir();
    await writeFile(join(dir, 'notes.txt'), 'some notes', 'utf-8');
    const installed = await installStarterPackIfEmpty(dir);
    expect(installed).toHaveLength(4);
  });

  it('creates directory if it does not exist', async () => {
    const parent = await tmpDir();
    const dir = join(parent, 'nested', 'microagents');
    const installed = await installStarterPackIfEmpty(dir);
    expect(installed).toHaveLength(4);
  });
});

function makeHost(overrides: Partial<Record<keyof SlashCommandHost, unknown>> = {}) {
  const host = {
    state: {
      appState: {
        workDir: '/fake/project',
        model: 'test-model',
        permissionMode: 'auto',
        streamingPhase: 'idle',
        isCompacting: false,
      },
      ui: { requestRender: vi.fn(), setFocus: vi.fn() },
      theme: { colors: getColorPalette('dark') },
      editorContainer: { clear: vi.fn(), addChild: vi.fn() },
      editor: {},
    },
    session: undefined,
    harness: undefined,
    cancelInFlight: undefined,
    deferUserMessages: false,
    setAppState: vi.fn(),
    resetLivePane: vi.fn(),
    showError: vi.fn(),
    showStatus: vi.fn(),
    showNotice: vi.fn(),
    track: vi.fn(),
    mountEditorReplacement: vi.fn(),
    restoreEditor: vi.fn(),
    restoreInputText: vi.fn(),
    ...overrides,
  } as unknown as SlashCommandHost;
  return host;
}

describe('handleMicroagentCommand', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('shows error when feature flag is disabled', async () => {
    setExperimentalFlags({});
    const host = makeHost();
    await handleMicroagentCommand(host, '');
    expect(host.showError).toHaveBeenCalledWith(
      expect.stringContaining('repo-knowledge'),
    );
    expect(host.mountEditorReplacement).not.toHaveBeenCalled();
  });

  it('shows error when workDir is missing', async () => {
    setExperimentalFlags({ 'repo-knowledge': true });
    const host = makeHost();
    host.state.appState.workDir = '';
    await handleMicroagentCommand(host, '');
    expect(host.showError).toHaveBeenCalledWith(
      expect.stringContaining('workspace'),
    );
    expect(host.mountEditorReplacement).not.toHaveBeenCalled();
  });

  it('shows error when workDir is undefined', async () => {
    setExperimentalFlags({ 'repo-knowledge': true });
    const host = makeHost();
    (host.state.appState as unknown as Record<string, unknown>)['workDir'] = undefined;
    await handleMicroagentCommand(host, '');
    expect(host.showError).toHaveBeenCalledWith(
      expect.stringContaining('workspace'),
    );
    expect(host.mountEditorReplacement).not.toHaveBeenCalled();
  });

  it('calls mountEditorReplacement for the name dialog when flag is on and workDir is set', async () => {
    setExperimentalFlags({ 'repo-knowledge': true });
    const host = makeHost();
    // Fire the command; the wizard will hang on the name prompt awaiting user input
    const cmd = handleMicroagentCommand(host, '');
    await vi.waitFor(
      () => {
        expect(host.mountEditorReplacement).toHaveBeenCalled();
      },
      { timeout: 500 },
    );
    // cmd is still pending waiting for user input; that's fine for this test
  });
});
