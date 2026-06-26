import { Command, Option } from 'commander';

import { CLI_COMMAND_NAME } from '#/constant/app';

import type { CLIOptions } from './options';
import { registerExportCommand } from './sub/export';
import { registerProviderCommand } from './sub/provider';
import { registerRequestCodeReviewCommand } from './sub/request-code-review';
import { registerServeCommand } from './sub/serve';

export type MainCommandHandler = (opts: CLIOptions) => void;
export type PluginNodeRunnerHandler = (entry: string, args: readonly string[]) => void;
export type UpgradeCommandHandler = () => void | Promise<void>;

export function createProgram(
  version: string,
  onMain: MainCommandHandler,
  onPluginNodeRunner: PluginNodeRunnerHandler = () => {},
  onUpgrade: UpgradeCommandHandler = () => {},
): Command {
  const program = new Command(CLI_COMMAND_NAME)
    .description('The Starting Point for Next-Gen Agents')
    .version(version, '-V, --version')
    .allowUnknownOption(false)
    .configureHelp({ helpWidth: 100 })
    .helpOption('-h, --help', 'Show help.')
    .addHelpText(
      'after',
      '\nDocumentation:        https://moonshotai.github.io/kimi-code/\n'
    );

  program
    .addOption(
      new Option(
        '-S, --session [id]',
        'Resume a session. With ID: resume that session. Without ID: interactively pick.',
      ).argParser((val: string | boolean) => (val === true ? '' : (val as string))),
    )
    .addOption(
      new Option('-r, --resume [id]')
        .hideHelp()
        .argParser((val: string | boolean) => (val === true ? '' : (val as string))),
    )
    .option('-C, --continue', 'Continue the previous session for the working directory.', false)
    .option('-y, --yolo', 'Automatically approve all actions.', false)
    .option('--auto', 'Start in auto permission mode.', false)
    .addOption(
      new Option(
        '-m, --model <model>',
        'LLM model alias to use for this invocation. Defaults to default_model in config.toml.',
      ),
    )
    .addOption(
      new Option(
        '-p, --prompt <prompt>',
        'Run one prompt non-interactively and print the response.',
      ),
    )
    .addOption(
      new Option(
        '--output-format <format>',
        'Output format for prompt mode. Defaults to text.',
      ).choices(['text', 'stream-json']),
    )
    .addOption(
      new Option(
        '--skills-dir <dir>',
        'Load skills from this directory instead of auto-discovered user and project directories. Can be repeated.',
      )
        .argParser((value: string, previous: string[] | undefined) => [...(previous ?? []), value])
        .default([]),
    )
    .addOption(new Option('--yes').hideHelp().default(false))
    .addOption(new Option('--auto-approve').hideHelp().default(false))
    .option('--session-mode <mode>', 'Start in session mode: normal, plan, design, office-hours, or game-design.', 'normal')
    .addOption(
      new Option(
        '--office-hours',
        'Start Ody Code in YC Office Hours mode. Exits after the design doc is written.',
      ).conflicts(['prompt', 'session', 'continue', 'sessionMode', 'yolo', 'auto']),
    )
    .addOption(
      new Option(
        '--game-design',
        'Start Ody Code in Game Design mode. Guided game design workflow based on the 100 Principles of Game Design. Exits after the design doc is written.',
      ).conflicts(['prompt', 'session', 'continue', 'sessionMode', 'yolo', 'auto', 'officeHours']),
    )
    .addOption(
      new Option(
        '-L, --login <provider-type>',
        'Interactive login for a supported LLM provider (deepseek, openai, anthropic, kimi, openai_responses).',
      ),
    )
    .addOption(
      new Option(
        '-O, --logout <provider-type>',
        'Interactive logout for providers of the given type.',
      ),
    )
    .addOption(new Option('--host <mode>', 'Run core in-process (inproc) or in external Rust host (rust).').choices(['inproc', 'rust']).default('inproc'))
    .option('--host-stdio', 'Launch Rust host in stdio mode.', false)
    .addOption(new Option('--host-socket <path>', 'Launch Rust host listening on a Unix socket.'))
    .addOption(new Option('--host-tcp <host:port>', 'Launch Rust host listening on TCP.'))
    .addOption(new Option('--host-binary <path>', 'Path to the Rust host executable (defaults to ody-host on PATH).'))
    .option('--smoke-test', 'Non-interactive smoke test: create a session and exit.', false);

  registerExportCommand(program);
  registerProviderCommand(program);
  registerRequestCodeReviewCommand(program);
  registerServeCommand(program, version);
  program
    .command('upgrade')
    .description('Upgrade Ody Code to the latest version.')
    .action(async () => {
      await onUpgrade();
    });

  program
    .command('__plugin_run_node', { hidden: true })
    .argument('<entry>')
    .argument('[args...]')
    .allowUnknownOption(true)
    .action((entry: string, args: string[]) => {
      onPluginNodeRunner(entry, args);
    });

  program.action(() => {
    const raw = program.opts<Record<string, unknown>>();

    const rawSession = raw['session'] ?? raw['resume'];
    const sessionValue = rawSession === true ? '' : (rawSession as string | undefined);
    const yoloValue = raw['yolo'] === true || raw['yes'] === true || raw['autoApprove'] === true;
    const autoValue = raw['auto'] === true;

    const opts: CLIOptions = {
      session: sessionValue,
      continue: raw['continue'] as boolean,
      yolo: yoloValue,
      auto: autoValue,
      sessionMode: (raw['sessionMode'] as CLIOptions['sessionMode']) ?? 'normal',
      officeHours: (raw['officeHours'] as boolean) ?? false,
      gameDesign: (raw['gameDesign'] as boolean) ?? false,
      model: raw['model'] as string | undefined,
      outputFormat: raw['outputFormat'] as CLIOptions['outputFormat'],
      prompt: raw['prompt'] as string | undefined,
      skillsDirs: raw['skillsDir'] as string[],
      loginProvider: raw['login'] as string | undefined,
      logoutProvider: raw['logout'] as string | undefined,
      host: (raw['host'] as CLIOptions['host']) ?? 'inproc',
      hostStdio: (raw['hostStdio'] as boolean) ?? false,
      hostSocket: raw['hostSocket'] as string | undefined,
      hostTcp: raw['hostTcp'] as string | undefined,
      hostBinary: raw['hostBinary'] as string | undefined,
      smokeTest: (raw['smokeTest'] as boolean) ?? false,
    };

    onMain(opts);
  });

  return program;
}
