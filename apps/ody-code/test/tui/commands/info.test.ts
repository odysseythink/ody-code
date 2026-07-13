import { describe, expect, it } from 'vitest';
import type { HooksInfo } from '@odysseythink/ody-code-sdk';

import { buildHooksReportLines } from '#tui/commands/info';
import { findBuiltInSlashCommand } from '#tui/commands/registry';
import type { ColorPalette } from '#tui/theme/colors';

const colors = {
  primary: '#ffffff',
  text: '#aaaaaa',
  textDim: '#666666',
  success: '#00ff00',
  warning: '#ffff00',
  error: '#ff0000',
} as unknown as ColorPalette;

function stripAnsi(text: string): string {
  return text.replace(/\x1b\[[0-9;]*m/g, '');
}

describe('/hooks command', () => {
  it('is registered as a builtin slash command', () => {
    const command = findBuiltInSlashCommand('hooks');
    expect(command).toBeDefined();
    expect(command?.name).toBe('hooks');
  });

  it('renders profile, summary, executions and counts', () => {
    const info: HooksInfo = {
      profile: 'strict',
      disabled: ['stop-format-typecheck'],
      summary: { Stop: 1, PostToolUse: 2 },
      executions: [
        {
          ts: 1_700_000_000_000,
          event: 'Stop',
          hookId: 'stop-format-typecheck',
          kind: 'builtin',
          action: 'allow',
          durationMs: 120,
        },
        {
          ts: 1_700_000_001_000,
          event: 'PostToolUse',
          hookId: 'edit-accumulator',
          kind: 'builtin',
          action: 'skipped-profile',
          durationMs: 0,
          reason: 'profile=minimal',
        },
      ],
      counts: {
        allow: 1,
        block: 0,
        error: 0,
        timeout: 0,
        'skipped-profile': 1,
        dropped: 0,
      },
    };

    const lines = buildHooksReportLines({ colors, info });
    const text = lines.map(stripAnsi).join('\n');
    expect(text).toContain('strict');
    expect(text).toContain('stop-format-typecheck');
    expect(text).toContain('PostToolUse');
    expect(text).toContain('skipped-profile');
    expect(text).toContain('allow: 1');
  });
});
