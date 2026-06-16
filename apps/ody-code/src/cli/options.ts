export type UIMode = 'shell' | 'print';
export type PromptOutputFormat = 'text' | 'stream-json';

export interface CLIOptions {
  session: string | undefined;
  continue: boolean;
  yolo: boolean;
  auto: boolean;
  sessionMode: 'normal' | 'plan' | 'design' | 'office-hours';
  officeHours: boolean;
  model: string | undefined;
  outputFormat: PromptOutputFormat | undefined;
  prompt: string | undefined;
  skillsDirs: string[];
  loginProvider: string | undefined;
  logoutProvider: string | undefined;
}

export interface ValidatedOptions {
  options: CLIOptions;
  uiMode: UIMode;
}

export class OptionConflictError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'OptionConflictError';
  }
}

export function validateOptions(opts: CLIOptions): ValidatedOptions {
  const prompt = opts.prompt;
  const promptMode = prompt !== undefined;
  if (promptMode && prompt.trim().length === 0) {
    throw new OptionConflictError('Prompt cannot be empty.');
  }
  if (opts.model !== undefined && opts.model.trim().length === 0) {
    throw new OptionConflictError('Model cannot be empty.');
  }
  if (!promptMode && opts.outputFormat !== undefined) {
    throw new OptionConflictError('Output format is only supported in prompt mode.');
  }
  if (promptMode && opts.yolo) {
    throw new OptionConflictError('Cannot combine --prompt with --yolo.');
  }
  if (promptMode && opts.auto) {
    throw new OptionConflictError('Cannot combine --prompt with --auto.');
  }
  if (!['normal', 'plan', 'design', 'office-hours'].includes(opts.sessionMode)) {
    throw new OptionConflictError(`Invalid --session-mode: ${opts.sessionMode}. Must be normal, plan, or design.`);
  }
  if (promptMode && opts.sessionMode !== 'normal') {
    throw new OptionConflictError('Cannot combine --prompt with --session-mode.');
  }
  if (promptMode && opts.session === '') {
    throw new OptionConflictError('Cannot use --session without an id in prompt mode.');
  }
  if (opts.continue && opts.session !== undefined) {
    throw new OptionConflictError('Cannot combine --continue, --session.');
  }
  if (opts.yolo && opts.auto) {
    throw new OptionConflictError('Cannot combine --yolo with --auto.');
  }
  if (!promptMode && (opts.continue || opts.session !== undefined) && opts.yolo) {
    throw new OptionConflictError('Cannot combine --yolo with --continue or --session.');
  }
  if (!promptMode && (opts.continue || opts.session !== undefined) && opts.auto) {
    throw new OptionConflictError('Cannot combine --auto with --continue or --session.');
  }
  if (!promptMode && (opts.continue || opts.session !== undefined) && opts.sessionMode !== 'normal') {
    throw new OptionConflictError('Cannot combine --session-mode with --continue or --session.');
  }
  if (opts.officeHours) {
    if (opts.prompt !== undefined) {
      throw new OptionConflictError('Cannot combine --office-hours with --prompt.');
    }
    if (opts.session !== undefined) {
      throw new OptionConflictError('Cannot combine --office-hours with --session.');
    }
    if (opts.continue) {
      throw new OptionConflictError('Cannot combine --office-hours with --continue.');
    }
    if (opts.sessionMode !== 'normal') {
      throw new OptionConflictError('Cannot combine --office-hours with --session-mode.');
    }
    if (opts.yolo || opts.auto) {
      throw new OptionConflictError('Permission mode is fixed to manual in office-hours mode.');
    }
    return { options: opts, uiMode: 'shell' };
  }
  return { options: opts, uiMode: promptMode ? 'print' : 'shell' };
}
