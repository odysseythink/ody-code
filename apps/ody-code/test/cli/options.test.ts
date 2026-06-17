import { CommanderError } from 'commander';
import { describe, expect, it } from 'vitest';

import { createProgram } from '#/cli/commands';
import type { CLIOptions } from '#/cli/options';
import { OptionConflictError, validateOptions } from '#/cli/options';

function parse(argv: string[]): CLIOptions {
  let captured: CLIOptions | undefined;

  const program = createProgram(
    '0.1.0-test',
    (opts) => {
      captured = opts;
    },
    () => {},
  );

  program.exitOverride();
  program.configureOutput({
    writeOut: () => {},
    writeErr: () => {},
  });

  program.parse(['node', 'ody', ...argv]);

  if (captured === undefined) {
    throw new Error('Main action handler was not called');
  }
  return captured;
}

describe('CLI options parsing', () => {
  describe('defaults', () => {
    it('returns defaults when no arguments are given', () => {
      const opts = parse([]);
      expect(opts.yolo).toBe(false);
      expect(opts.sessionMode).toBe('normal');
      expect(opts.continue).toBe(false);
      expect(opts.session).toBeUndefined();
      expect(opts.model).toBeUndefined();
      expect(opts.outputFormat).toBeUndefined();
      expect(opts.prompt).toBeUndefined();
      expect(opts.skillsDirs).toEqual([]);
    });
  });

  describe('--version', () => {
    it('prints the version string and exits', () => {
      let output = '';
      const program = createProgram('1.2.3', () => {}, () => {});
      program.exitOverride();
      program.configureOutput({
        writeOut: (s) => {
          output += s;
        },
      });

      expect(() => program.parse(['node', 'ody', '--version'])).toThrow();
      expect(output).toContain('1.2.3');
    });

    it('supports -V as a short alias', () => {
      let output = '';
      const program = createProgram('4.5.6', () => {}, () => {});
      program.exitOverride();
      program.configureOutput({
        writeOut: (s) => {
          output += s;
        },
      });

      expect(() => program.parse(['node', 'ody', '-V'])).toThrow();
      expect(output).toContain('4.5.6');
    });
  });

  describe('hidden plugin node runner', () => {
    it('routes __plugin_run_node without calling the main action', () => {
      const pluginRunnerCalls: Array<{ entry: string; args: readonly string[] }> = [];
      const program = createProgram(
        '0.0.0',
        () => {
          throw new Error('main action should not run');
        },
        (entry, args) => {
          pluginRunnerCalls.push({ entry, args });
        },
      );
      program.exitOverride();
      program.configureOutput({
        writeOut: () => {},
        writeErr: () => {},
      });

      program.parse([
        'node',
        'ody',
        '__plugin_run_node',
        '/plugin/tool.mjs',
        '--',
        'query',
        '--flag',
      ]);

      expect(pluginRunnerCalls).toEqual([
        { entry: '/plugin/tool.mjs', args: ['query', '--flag'] },
      ]);
    });
  });

  describe('--yolo family', () => {
    it('--yolo sets yolo to true', () => {
      expect(parse(['--yolo']).yolo).toBe(true);
    });

    it('-y sets yolo to true', () => {
      expect(parse(['-y']).yolo).toBe(true);
    });

    it('--yes sets yolo to true (hidden alias)', () => {
      expect(parse(['--yes']).yolo).toBe(true);
    });

    it('--auto-approve sets yolo to true (hidden alias)', () => {
      expect(parse(['--auto-approve']).yolo).toBe(true);
    });
  });

  describe('--session / --resume / --continue', () => {
    it('-S sets session', () => {
      expect(parse(['-S', 'sess-123']).session).toBe('sess-123');
    });

    it('-r is an alias for --session', () => {
      expect(parse(['-r', 'sess-456']).session).toBe('sess-456');
    });

    it('--resume is an alias for --session', () => {
      expect(parse(['--resume', 'sess-789']).session).toBe('sess-789');
    });

    it('bare -S (no id) yields empty string — triggers the picker', () => {
      expect(parse(['-S']).session).toBe('');
    });

    it('-C sets continue', () => {
      expect(parse(['-C']).continue).toBe(true);
    });

    it('--continue and --session combined raises a conflict', () => {
      const opts = parse(['--continue', '--session', 'abc123']);
      expect(() => validateOptions(opts)).toThrow(OptionConflictError);
      expect(() => validateOptions(opts)).toThrow('Cannot combine --continue, --session.');
    });
  });

  describe('--session-mode', () => {
    it('defaults to normal', () => {
      expect(parse([]).sessionMode).toBe('normal');
    });
    it('accepts plan', () => {
      expect(parse(['--session-mode', 'plan']).sessionMode).toBe('plan');
    });
    it('accepts design', () => {
      expect(parse(['--session-mode', 'design']).sessionMode).toBe('design');
    });
    it('rejects invalid mode in validateOptions', () => {
      const opts = parse(['--session-mode', 'invalid']);
      expect(() => validateOptions(opts)).toThrow(OptionConflictError);
    });
  });

  describe('--model / -m', () => {
    it('parses -m as a model override', () => {
      expect(parse(['-m', 'kimi-code/k2']).model).toBe('kimi-code/k2');
    });

    it('parses --model=value as a model override', () => {
      expect(parse(['--model=kimi-code/k2.5']).model).toBe('kimi-code/k2.5');
    });

    it('rejects empty model values', () => {
      const opts = parse(['--model', '   ']);
      expect(() => validateOptions(opts)).toThrow(OptionConflictError);
      expect(() => validateOptions(opts)).toThrow('Model cannot be empty.');
    });
  });

  describe('--prompt / -p', () => {
    it('parses -p as prompt mode', () => {
      const opts = parse(['-p', 'explain this repo']);
      expect(opts.prompt).toBe('explain this repo');
      expect(validateOptions(opts).uiMode).toBe('print');
    });

    it('parses --prompt=value as prompt mode', () => {
      const opts = parse(['--prompt=explain this repo']);
      expect(opts.prompt).toBe('explain this repo');
      expect(validateOptions(opts).uiMode).toBe('print');
    });

    it('rejects empty prompt values before reaching the SDK', () => {
      const opts = parse(['-p', '   ']);
      expect(() => validateOptions(opts)).toThrow(OptionConflictError);
      expect(() => validateOptions(opts)).toThrow('Prompt cannot be empty.');
    });

    it('allows prompt mode with --continue', () => {
      const opts = parse(['-p', 'continue here', '--continue']);
      expect(opts.continue).toBe(true);
      expect(validateOptions(opts).uiMode).toBe('print');
    });

    it('allows prompt mode with a concrete session id', () => {
      const opts = parse(['-p', 'resume here', '--session', 'ses_123']);
      expect(opts.session).toBe('ses_123');
      expect(validateOptions(opts).uiMode).toBe('print');
    });

    it('rejects prompt mode with bare --session picker', () => {
      const opts = parse(['-p', 'resume here', '--session']);
      expect(() => validateOptions(opts)).toThrow(OptionConflictError);
      expect(() => validateOptions(opts)).toThrow('Cannot use --session without an id in prompt mode.');
    });

    it('rejects prompt mode with --yolo because prompt mode always uses auto permission', () => {
      const opts = parse(['-p', 'run this', '--yolo']);
      expect(() => validateOptions(opts)).toThrow(OptionConflictError);
      expect(() => validateOptions(opts)).toThrow('Cannot combine --prompt with --yolo.');
    });

    it('rejects prompt mode with --session-mode', () => {
      const opts = parse(['-p', 'run this', '--session-mode', 'plan']);
      expect(() => validateOptions(opts)).toThrow(OptionConflictError);
      expect(() => validateOptions(opts)).toThrow('Cannot combine --prompt with --session-mode.');
    });

    it('parses --output-format=stream-json in prompt mode', () => {
      const opts = parse(['-p', 'run this', '--output-format=stream-json']);
      expect(opts.outputFormat).toBe('stream-json');
      expect(validateOptions(opts).uiMode).toBe('print');
    });

    it('parses --output-format text in prompt mode', () => {
      const opts = parse(['-p', 'run this', '--output-format', 'text']);
      expect(opts.outputFormat).toBe('text');
    });

    it('rejects --output-format outside prompt mode', () => {
      const opts = parse(['--output-format=stream-json']);
      expect(() => validateOptions(opts)).toThrow(OptionConflictError);
      expect(() => validateOptions(opts)).toThrow(
        'Output format is only supported in prompt mode.',
      );
    });
  });

  describe('--skills-dir', () => {
    it('collects repeated skill directories', () => {
      expect(parse(['--skills-dir', '/one', '--skills-dir=/two']).skillsDirs).toEqual([
        '/one',
        '/two',
      ]);
    });
  });

  describe('sub-commands', () => {
    it('routes upgrade without calling the main action', () => {
      let upgradeCalls = 0;
      const program = createProgram(
        '0.0.0',
        () => {
          throw new Error('main action should not run');
        },
        () => {},
        () => {
          upgradeCalls += 1;
        },
      );
      program.exitOverride();
      program.configureOutput({
        writeOut: () => {},
        writeErr: () => {},
      });

      program.parse(['node', 'ody', 'upgrade']);

      expect(upgradeCalls).toBe(1);
    });

    it('registers the visible sub-commands', () => {
      const program = createProgram('0.0.0', () => {}, () => {});
      const commandNames: string[] = program.commands
        .filter((command) => !command.name().startsWith('__'))
        .map((command) => command.name());
      expect(commandNames).toEqual(['export', 'provider', 'request-code-review', 'upgrade']);
    });
  });

  describe('--office-hours', () => {
    it('defaults officeHours to false', () => {
      expect(parse([]).officeHours).toBe(false);
    });

    it('--office-hours sets officeHours to true', () => {
      expect(parse(['--office-hours']).officeHours).toBe(true);
    });

    it('--office-hours forces uiMode to shell', () => {
      const opts = parse(['--office-hours']);
      expect(validateOptions(opts).uiMode).toBe('shell');
    });

    it('rejects --office-hours combined with --prompt', () => {
      expect(() => parse(['--office-hours', '--prompt', 'x'])).toThrow(CommanderError);
    });

    it('rejects --office-hours combined with --session', () => {
      expect(() => parse(['--office-hours', '--session', 'abc'])).toThrow(CommanderError);
    });

    it('rejects --office-hours combined with --continue', () => {
      expect(() => parse(['--office-hours', '--continue'])).toThrow(CommanderError);
    });

    it('rejects --office-hours combined with --session-mode', () => {
      expect(() => parse(['--office-hours', '--session-mode', 'plan'])).toThrow(CommanderError);
    });

    it('rejects --office-hours combined with --yolo', () => {
      expect(() => parse(['--office-hours', '--yolo'])).toThrow(CommanderError);
    });

    it('rejects --office-hours combined with --auto', () => {
      expect(() => parse(['--office-hours', '--auto'])).toThrow(CommanderError);
    });
  });

  describe('rejected flags', () => {
    it('any removed flag is unknown to Commander', () => {
      for (const arg of [
        '--verbose',
        '--debug',
        '--work-dir=/',
        '--config=x',
        '--thinking',
        '--print',
        '--wire',
        '--agent=default',
        '--add-dir=/',
        '--raw-model',
        '--config-file=x',
        '--quiet',
        '--final-message-only',
        '--input-format=text',
        '--agent-file=x',
        '--mcp-config={}',
        '--mcp-config-file=/',
      ]) {
        expect(() => parse([arg])).toThrow();
      }
    });
  });
});
