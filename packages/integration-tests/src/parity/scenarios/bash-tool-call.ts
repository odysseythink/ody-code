import type { ChatProvider } from '@odysseythink/kosong';
import type { AgentEvent } from '@odysseythink/agent-core';
import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';
import { waitForEvent } from './utils';

interface RawRpcClient {
  rpc: Record<string, (payload: unknown) => Promise<unknown>>;
}

interface NormalizedToolCall {
  type: 'tool.call.started';
  name: string;
  command: string;
}

interface NormalizedToolResult {
  type: 'tool.result';
  outputFirstLine: string;
  isError: boolean;
}

function normalizeOutput(output: unknown): string {
  if (output === null || output === undefined) return '';
  const text = typeof output === 'string' ? output : JSON.stringify(output);
  return text
    .trim()
    .split(/\r?\n/)[0]
    ?.trim() ?? '';
}

function eventType(event: AgentEvent): string {
  return (event as unknown as Record<string, unknown>)['type'] as string;
}

export const bashToolCallMockLlm: ChatProvider = new MockChatProvider([
  [
    {
      type: 'function',
      id: 'tc-bash-1',
      name: 'Bash',
      arguments: JSON.stringify({ command: 'echo hello', description: 'greet' }),
    },
  ],
  [{ type: 'text', text: 'Done.' }],
]);

export const bashToolCallScenario: Scenario = {
  name: 'bash-tool-call',
  async run(backend) {
    const rpc = (backend.client as unknown as RawRpcClient).rpc;
    const summary = await backend.client.createSession({
      workDir: backend.homeDir,
      id: 'bash-tool-call-001',
      permission: 'auto',
      model: 'mock',
    });
    const base = { sessionId: summary.id, agentId: 'main' };

    // Ensure Bash is active; both TS and Rust expose it as a builtin.
    await rpc['setActiveTools']!({ ...base, names: ['Bash'] });

    const toolEvents: (NormalizedToolCall | NormalizedToolResult)[] = [];
    const unsubscribe = backend.client.onEvent((event) => {
      const e = event as unknown as Record<string, unknown>;
      if (e['type'] === 'tool.call.started' || e['type'] === 'tool.call') {
        const args =
          typeof e['args'] === 'object' && e['args'] !== null
            ? (e['args'] as Record<string, unknown>)
            : {};
        toolEvents.push({
          type: 'tool.call.started',
          name: String(e['name'] ?? e['toolName'] ?? ''),
          command: String(args['command'] ?? ''),
        });
      }
      if (e['type'] === 'tool.result') {
        toolEvents.push({
          type: 'tool.result',
          outputFirstLine: normalizeOutput(e['output'] ?? e['result']),
          isError: Boolean(e['isError'] ?? false),
        });
      }
    });

    try {
      await backend.client.prompt({
        sessionId: summary.id,
        input: [{ type: 'text', text: 'Run a bash command' }],
      });
      // TS executes the Bash tool and emits tool.result; the Rust mock provider
      // currently ends the turn without issuing the tool call. Wait for whichever
      // signal arrives first so both backends complete cleanly.
      await Promise.race([
        waitForEvent(backend.client, (event) => eventType(event) === 'tool.result', {
          timeoutMs: 10000,
        }),
        waitForEvent(backend.client, (event) => eventType(event) === 'turn.ended', {
          timeoutMs: 10000,
        }),
      ]);
      // Give any trailing events a moment to arrive.
      await new Promise((resolve) => setTimeout(resolve, 250));
    } finally {
      unsubscribe();
    }

    return { responses: [toolEvents], events: [] };
  },
};
