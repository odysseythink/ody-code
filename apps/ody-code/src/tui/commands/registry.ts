import type { AutocompleteItem } from '@earendil-works/pi-tui';

import { completeLeadingArg, type ArgCompletionSpec } from './complete-args';
import type { KimiSlashCommand, SessionMode, SlashCommandAvailability } from './types';

/** Commands hidden in office-hours mode: the mode is intentionally restricted
 *  to a single `/exit` command, so no mode toggles or general utilities appear. */
const OFFICE_HOURS_HIDDEN: readonly SessionMode[] = ['office-hours'];

/** Subcommands offered when autocompleting `/goal <…>`. */
const GOAL_ARG_COMPLETIONS: readonly ArgCompletionSpec[] = [
  { value: 'status', description: 'Show the current goal' },
  { value: 'pause', description: 'Pause the active goal' },
  { value: 'resume', description: 'Resume a paused goal' },
  { value: 'cancel', description: 'Cancel and remove the current goal' },
  { value: 'replace', description: 'Replace the current goal with a new objective' },
];

/** Argument autocompletion for the `/goal` command (subcommands). */
export function goalArgumentCompletions(argumentPrefix: string): AutocompleteItem[] | null {
  return completeLeadingArg(GOAL_ARG_COMPLETIONS, argumentPrefix);
}

export const BUILTIN_SLASH_COMMANDS = [
  {
    name: 'yolo',
    aliases: ['yes'],
    description: 'Toggle auto-approve mode',
    priority: 100,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'auto',
    aliases: [],
    description: 'Toggle auto permission mode',
    priority: 100,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'permission',
    aliases: [],
    description: 'Select permission mode',
    priority: 100,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'settings',
    aliases: ['config'],
    description: 'Open TUI settings',
    priority: 100,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'plan',
    aliases: [],
    description: 'Toggle plan mode',
    priority: 100,
    availability: (args) => (args.trim().toLowerCase() === 'clear' ? 'idle-only' : 'always'),
    hiddenInModes: ['plan', ...OFFICE_HOURS_HIDDEN],
  },
  {
    name: 'design',
    aliases: [],
    description: 'Toggle design mode (brainstorming / spec exploration)',
    priority: 100,
    availability: (args) => (args.trim().toLowerCase() === 'clear' ? 'idle-only' : 'always'),
    hiddenInModes: ['design', ...OFFICE_HOURS_HIDDEN],
  },
  {
    name: 'design-review',
    aliases: [],
    description: 'Critique the current design with the reviewer model (second-model review)',
    priority: 95,
    availability: 'idle-only',
    hiddenInModes: ['plan', 'normal', ...OFFICE_HOURS_HIDDEN],
  },
  {
    name: 'plan-review',
    aliases: [],
    description: 'Critique the current execution plan with the reviewer model (second-model review)',
    priority: 95,
    availability: 'idle-only',
    hiddenInModes: ['design', 'normal', ...OFFICE_HOURS_HIDDEN],
  },
  {
    name: 'writing-plan',
    aliases: [],
    description: '将指定文件转换为执行计划（仅 plan 模式，需文件参数）',
    priority: 94,
    availability: 'idle-only',
    hiddenInModes: ['design', 'normal', ...OFFICE_HOURS_HIDDEN],
  },
  {
    name: 'model',
    aliases: [],
    description: 'Switch LLM model',
    priority: 100,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'provider',
    aliases: ['providers'],
    description: 'Manage AI providers (add / delete / refresh)',
    priority: 95,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'help',
    aliases: ['h', '?'],
    description: 'Show available commands and shortcuts',
    priority: 80,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'new',
    aliases: ['clear'],
    description: 'Start a fresh session in the current workspace',
    priority: 80,
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'sessions',
    aliases: ['resume'],
    description: 'Browse and resume sessions',
    priority: 80,
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'tasks',
    aliases: ['task'],
    description: 'Browse background tasks',
    priority: 80,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'mcp',
    aliases: [],
    description: 'Show MCP server status',
    priority: 60,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'plugins',
    aliases: [],
    description: 'Manage plugins',
    priority: 60,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'compact',
    aliases: [],
    description: 'Compact the conversation context',
    priority: 80,
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'goal',
    aliases: [],
    description: 'Start or manage an autonomous goal',
    priority: 80,
    experimentalFlag: 'goal-command',
    // No argumentHint: the menu description stays as short as every other
    // command's. The subcommands (status/pause/resume/cancel/replace) surface in
    // the argument autocomplete list once the user types `/goal ` (see
    // completeArgs), so they don't need to be spelled out inline.
    completeArgs: goalArgumentCompletions,
    // status / pause / cancel are always available; creation, replacement, and
    // resume start (or restart) a turn and so are idle-only.
    availability: (args) => {
      const trimmed = args.trim();
      return trimmed === '' || trimmed === 'status' || trimmed === 'pause' || trimmed === 'cancel'
        ? 'always'
        : 'idle-only';
    },
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'init',
    aliases: [],
    description: 'Analyze the codebase and generate AGENTS.md',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'fork',
    aliases: [],
    description: 'Fork the current session',
    priority: 80,
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'title',
    aliases: ['rename'],
    description: 'Set or show session title',
    priority: 60,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'usage',
    aliases: [],
    description: 'Show session tokens + context window + plan quotas',
    priority: 60,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'status',
    aliases: [],
    description: 'Show current session and runtime status',
    priority: 60,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'feedback',
    aliases: [],
    description: 'Send feedback to make Ody Code better',
    priority: 60,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'undo',
    aliases: [],
    description: 'Withdraw the last prompt from the transcript',
    priority: 80,
    availability: 'idle-only',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'editor',
    aliases: [],
    description: 'Set the external editor for Ctrl-G',
    priority: 60,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'theme',
    aliases: [],
    description: 'Set the terminal UI theme',
    priority: 60,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'logout',
    aliases: ['disconnect'],
    description: 'Log out of a configured provider',
    priority: 40,
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'login',
    aliases: [],
    description: 'Select a platform and authenticate',
    priority: 40,
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'export-md',
    aliases: ['export'],
    description: 'Export current session as a Markdown file',
    priority: 40,
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'export-debug-zip',
    aliases: [],
    description: 'Export current session as a debug ZIP archive',
    priority: 40,
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
  {
    name: 'request-code-review',
    aliases: [],
    description: 'Request a code review of the current changes.',
    priority: 80,
    availability: 'idle-only',
    hiddenInModes: ['plan', 'design', 'office-hours'],
  },
  {
    name: 'receive-code-review',
    aliases: [],
    description: 'Enter receiving-code-review mode: switch model and load the receiving skill.',
    priority: 80,
    availability: 'idle-only',
    hiddenInModes: ['plan', 'design', 'office-hours'],
  },
  {
    name: 'exit',
    aliases: ['quit', 'q'],
    description: 'Exit the application',
    priority: 20,
  },
  {
    name: 'version',
    aliases: [],
    description: 'Show version information',
    priority: 20,
    availability: 'always',
    hiddenInModes: OFFICE_HOURS_HIDDEN,
  },
] as const satisfies readonly KimiSlashCommand[];

export type BuiltinSlashCommand = (typeof BUILTIN_SLASH_COMMANDS)[number];
export type BuiltinSlashCommandName = BuiltinSlashCommand['name'];

export function findBuiltInSlashCommand(
  commandName: string,
): KimiSlashCommand<BuiltinSlashCommandName> | undefined {
  const commands = BUILTIN_SLASH_COMMANDS as readonly KimiSlashCommand<BuiltinSlashCommandName>[];
  return commands.find(
    (command) => command.name === commandName || command.aliases.includes(commandName),
  );
}

export function resolveSlashCommandAvailability(
  command: KimiSlashCommand,
  args: string,
): SlashCommandAvailability {
  const availability = command.availability ?? 'idle-only';
  return typeof availability === 'function' ? availability(args) : availability;
}

export function sortSlashCommands(commands: readonly KimiSlashCommand[]): KimiSlashCommand[] {
  return [...commands].toSorted(
    (a, b) => (b.priority ?? 0) - (a.priority ?? 0) || a.name.localeCompare(b.name),
  );
}
