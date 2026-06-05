import { describe, expect, it, vi } from 'vitest';

import {
  SessionEventHandler,
  type SessionEventHost,
} from '#/tui/controllers/session-event-handler';
import type { AppState } from '#/tui/types';
import type { Event } from '@odysseythink/kimi-code-sdk';

function makeHost(): SessionEventHost {
  return {
    state: {} as SessionEventHost['state'],
    session: undefined,
    aborted: false,
    sessionEventUnsubscribe: undefined,
    streamingUI: {
      setTurnId: vi.fn(),
      setThinking: vi.fn(),
      setToolCall: vi.fn(),
      setToolResult: vi.fn(),
      setAssistantDelta: vi.fn(),
      setThinkingDelta: vi.fn(),
      setHookResult: vi.fn(),
      setError: vi.fn(),
      setWarning: vi.fn(),
      setStatus: vi.fn(),
      setGoal: vi.fn(),
      setTodoList: vi.fn(),
      setMcpServerStatus: vi.fn(),
      setCompacting: vi.fn(),
      setBackgroundTaskStarted: vi.fn(),
      setBackgroundTaskTerminated: vi.fn(),
      patchLivePane: vi.fn(),
      resetLivePane: vi.fn(),
      showNotice: vi.fn(),
      showError: vi.fn(),
      showStatus: vi.fn(),
      appendTranscriptEntry: vi.fn(),
      setSessionTitle: vi.fn(),
      setLoadingSessions: vi.fn(),
      setSessions: vi.fn(),
      setActiveDialog: vi.fn(),
      setPlanExpanded: vi.fn(),
      setToolOutputExpanded: vi.fn(),
      setEditorCommand: vi.fn(),
      setTheme: vi.fn(),
      setModel: vi.fn(),
      setPermissionMode: vi.fn(),
      setPlanMode: vi.fn(),
      setDesignMode: vi.fn(),
      setContextUsage: vi.fn(),
      setContextTokens: vi.fn(),
      setMaxContextTokens: vi.fn(),
      setVersion: vi.fn(),
      setWorkDir: vi.fn(),
      setAvailableModels: vi.fn(),
      setAvailableProviders: vi.fn(),
      setMcpServersSummary: vi.fn(),
      setTasksBrowser: vi.fn(),
      setExternalEditorRunning: vi.fn(),
      setQueuedMessages: vi.fn(),
      setIsCompacting: vi.fn(),
      setIsReplaying: vi.fn(),
      setStreamingPhase: vi.fn(),
      setStreamingStartTime: vi.fn(),
      setSessionId: vi.fn(),
      setSessionTitleDirect: vi.fn(),
      setGoalDirect: vi.fn(),
      setNotifications: vi.fn(),
      setUpgrade: vi.fn(),
      setEditorRunning: vi.fn(),
      requestRender: vi.fn(),
    } as unknown as SessionEventHost['streamingUI'],
    requireSession: vi.fn(),
    setAppState: vi.fn() as unknown as SessionEventHost['setAppState'],
    patchLivePane: vi.fn(),
    resetLivePane: vi.fn(),
    showError: vi.fn(),
    showStatus: vi.fn(),
    showNotice: vi.fn(),
    appendTranscriptEntry: vi.fn(),
    sendQueuedMessage: vi.fn(),
    shiftQueuedMessage: vi.fn(),
    tasksBrowserController: {
      setTasks: vi.fn(),
      setVisible: vi.fn(),
    } as unknown as SessionEventHost['tasksBrowserController'],
  } as unknown as SessionEventHost;
}

describe('SessionEventHandler handleStatusUpdate', () => {
  it('propagates advancedSessionModeFilePath from agent.status.updated event', () => {
    const host = makeHost();
    const handler = new SessionEventHandler(host);

    const event = {
      type: 'agent.status.updated',
      agentId: 'main',
      sessionId: 'ses-1',
      advancedSessionModeFilePath: '/tmp/plans/test-plan.md',
    } as Event;

    handler.handleEvent(event, vi.fn());

    expect(host.setAppState).toHaveBeenCalledWith(
      expect.objectContaining({
        advancedSessionModeFilePath: '/tmp/plans/test-plan.md',
      }),
    );
  });

  it('does not include advancedSessionModeFilePath when event lacks it', () => {
    const host = makeHost();
    const handler = new SessionEventHandler(host);

    const event = {
      type: 'agent.status.updated',
      agentId: 'main',
      sessionId: 'ses-1',
      sessionMode: 'plan',
    } as Event;

    handler.handleEvent(event, vi.fn());

    expect(host.setAppState).toHaveBeenCalledTimes(1);
    const patch = vi.mocked(host.setAppState).mock.calls[0]![0] as Record<string, unknown>;
    expect('advancedSessionModeFilePath' in patch).toBe(false);
  });
});
