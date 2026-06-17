import {
  Container,
  ProcessTerminal,
  TUI,
} from '@earendil-works/pi-tui';

import { FooterComponent } from './components/chrome/footer';
import { GutterContainer } from './components/chrome/gutter-container';
import type { MoonLoader, SpinnerStyle } from './components/chrome/moon-loader';
import { TodoPanelComponent } from './components/chrome/todo-panel';
import type { SessionRow } from './components/dialogs/session-picker';
import { CustomEditor } from './components/editor/custom-editor';
import { CHROME_GUTTER } from './constant/rendering';
import type { TasksBrowserState } from './controllers/tasks-browser';
import { createOdyTUIThemeBundle, type OdyTUIThemeBundle } from './theme/bundle';
import { createTerminalState, type TerminalState } from './utils/terminal-state';
import {
  INITIAL_LIVE_PANE,
  type AppState,
  type OdyTUIOptions,
  type LivePaneState,
  type QueuedMessage,
  type TranscriptEntry,
  type TUIStartupState,
} from './types';

export { GutterContainer };

export interface TUIState {
  ui: TUI;
  terminal: ProcessTerminal;
  transcriptContainer: Container;
  activityContainer: Container;
  todoPanelContainer: Container;
  todoPanel: TodoPanelComponent;
  queueContainer: Container;
  editorContainer: Container;
  footer: FooterComponent;
  editor: CustomEditor;
  theme: OdyTUIThemeBundle;
  appState: AppState;
  startupState: TUIStartupState;
  livePane: LivePaneState;
  transcriptEntries: TranscriptEntry[];
  /** Per-mode entry arrays. `transcriptEntries` is always the same reference as `modeTranscriptEntries[activeMode]`. */
  modeTranscriptEntries: Record<string, TranscriptEntry[]>;
  /** Per-mode transcript containers. `transcriptContainer` is always the same reference as `modeContainers[activeMode]`. */
  modeContainers: Record<string, GutterContainer>;
  terminalState: TerminalState;
  activitySpinner: { instance: MoonLoader; style: SpinnerStyle } | null;
  toolOutputExpanded: boolean;
  planExpanded: boolean;
  sessions: SessionRow[];
  loadingSessions: boolean;
  activeDialog: 'session-picker' | 'help' | null;
  tasksBrowser: TasksBrowserState | undefined;
  externalEditorRunning: boolean;
  queuedMessages: QueuedMessage[];
}

export function createTUIState(options: OdyTUIOptions): TUIState {
  const initialAppState = options.initialAppState;
  const theme = createOdyTUIThemeBundle(initialAppState.theme, options.resolvedTheme);

  const terminal = new ProcessTerminal();
  const ui = new TUI(terminal);

  const transcriptContainer = new GutterContainer(CHROME_GUTTER, CHROME_GUTTER);
  const activityContainer = new GutterContainer(CHROME_GUTTER, CHROME_GUTTER);
  const todoPanelContainer = new GutterContainer(CHROME_GUTTER, CHROME_GUTTER);
  const todoPanel = new TodoPanelComponent(theme.colors);
  const queueContainer = new GutterContainer(CHROME_GUTTER, CHROME_GUTTER);
  const editorContainer = new GutterContainer(CHROME_GUTTER, CHROME_GUTTER);
  const editor = new CustomEditor(ui, theme.colors);
  const footer = new FooterComponent({ ...initialAppState }, theme.colors, () => {
    ui.requestRender();
  });

  // Normal-mode entries share the same reference as transcriptEntries so that
  // plain pushes to transcriptEntries are automatically tracked per-mode.
  const normalEntries: TranscriptEntry[] = [];

  return {
    ui,
    terminal,
    transcriptContainer,
    activityContainer,
    todoPanelContainer,
    todoPanel,
    queueContainer,
    editorContainer,
    footer,
    editor,
    theme,
    appState: { ...initialAppState },
    startupState: 'pending',
    livePane: { ...INITIAL_LIVE_PANE },
    transcriptEntries: normalEntries,
    modeTranscriptEntries: { normal: normalEntries, plan: [], design: [] },
    modeContainers: {
      normal: transcriptContainer,
      plan: new GutterContainer(CHROME_GUTTER, CHROME_GUTTER),
      design: new GutterContainer(CHROME_GUTTER, CHROME_GUTTER),
    },
    terminalState: createTerminalState(),
    activitySpinner: null,
    toolOutputExpanded: false,
    planExpanded: false,
    sessions: [],
    loadingSessions: false,
    activeDialog: null,
    tasksBrowser: undefined,
    externalEditorRunning: false,
    queuedMessages: [],
  };
}
