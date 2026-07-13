export type CLIRuntimeMode = 'normal' | 'plan' | 'design' | 'product' | 'game-design';
export type UIMode = 'shell' | 'print';
export type PromptOutputFormat = 'text' | 'stream-json';

export interface CLIOptions {
  session: string | undefined;
  continue: boolean;
  yolo: boolean;
  auto: boolean;
  sessionMode: CLIRuntimeMode;
  product: boolean;
  gameDesign: boolean;
  model: string | undefined;
  outputFormat: PromptOutputFormat | undefined;
  prompt: string | undefined;
  skillsDirs: string[];
  loginProvider: string | undefined;
  logoutProvider: string | undefined;
  host: 'ts' | 'rust';
  hostStdio: boolean;
  hostSocket: string | undefined;
  hostTcp: string | undefined;
  hostBinary: string | undefined;
  /** Non-interactive smoke test: create a session and exit without rendering the TUI. */
  smokeTest?: boolean;
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
  if (!['normal', 'plan', 'design', 'product', 'game-design'].includes(opts.sessionMode)) {
    throw new OptionConflictError(`Invalid --session-mode: ${opts.sessionMode}. Must be normal, plan, design, product, or game-design.`);
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
  if (opts.product) {
    if (opts.prompt !== undefined) {
      throw new OptionConflictError('Cannot combine --product with --prompt.');
    }
    if (opts.session !== undefined) {
      throw new OptionConflictError('Cannot combine --product with --session.');
    }
    if (opts.continue) {
      throw new OptionConflictError('Cannot combine --product with --continue.');
    }
    if (opts.sessionMode !== 'normal') {
      throw new OptionConflictError('Cannot combine --product with --session-mode.');
    }
    if (opts.yolo || opts.auto) {
      throw new OptionConflictError('Permission mode is fixed to manual in product mode.');
    }
    return { options: opts, uiMode: 'shell' };
  }

  if (opts.gameDesign) {
    if (opts.prompt !== undefined) {
      throw new OptionConflictError('Cannot combine --game-design with --prompt.');
    }
    if (opts.session !== undefined) {
      throw new OptionConflictError('Cannot combine --game-design with --session.');
    }
    if (opts.continue) {
      throw new OptionConflictError('Cannot combine --game-design with --continue.');
    }
    if (opts.sessionMode !== 'normal') {
      throw new OptionConflictError('Cannot combine --game-design with --session-mode.');
    }
    if (opts.yolo || opts.auto) {
      throw new OptionConflictError('Permission mode is fixed to manual in game-design mode.');
    }
    if (opts.product) {
      throw new OptionConflictError('Cannot combine --game-design with --product.');
    }
    return { options: opts, uiMode: 'shell' };
  }

  if (!['ts', 'rust'].includes(opts.host)) {
    throw new OptionConflictError(`Invalid --host: ${opts.host}. Must be ts or rust.`);
  }
  if (opts.smokeTest && opts.host !== 'rust') {
    throw new OptionConflictError('--smoke-test requires --host=rust.');
  }
  if (opts.host === 'rust') {
    if (opts.prompt !== undefined) {
      throw new OptionConflictError('Cannot combine --host=rust with --prompt.');
    }
    if (opts.product || opts.gameDesign) {
      throw new OptionConflictError('Cannot combine --host=rust with --product or --game-design.');
    }
    if (opts.hostStdio && opts.hostSocket !== undefined) {
      throw new OptionConflictError('Cannot combine --host-stdio with --host-socket.');
    }
    if (opts.hostStdio && opts.hostTcp !== undefined) {
      throw new OptionConflictError('Cannot combine --host-stdio with --host-tcp.');
    }
    if (opts.hostSocket !== undefined && opts.hostTcp !== undefined) {
      throw new OptionConflictError('Cannot combine --host-socket with --host-tcp.');
    }
    if (!opts.hostStdio && opts.hostSocket === undefined && opts.hostTcp === undefined) {
      // Default to stdio when --host=rust is given without a transport flag.
      opts.hostStdio = true;
    }
    return { options: opts, uiMode: 'shell' };
  }

  return { options: opts, uiMode: promptMode ? 'print' : 'shell' };
}
