import type { AgentEvent } from '@odysseythink/agent-core';
import type { SDKRpcClient } from '@odysseythink/ody-code-sdk';

export interface WaitOptions {
  readonly timeoutMs?: number;
}

export function waitForEvent(
  client: SDKRpcClient,
  predicate: (event: AgentEvent) => boolean,
  options: WaitOptions = {},
): Promise<AgentEvent> {
  const { timeoutMs = 10000 } = options;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      unsubscribe();
      reject(new Error(`Timeout after ${timeoutMs}ms waiting for event`));
    }, timeoutMs);

    const unsubscribe = client.onEvent((event: AgentEvent) => {
      if (predicate(event)) {
        clearTimeout(timer);
        unsubscribe();
        resolve(event);
      }
    });
  });
}

export function waitForTurnEnded(
  client: SDKRpcClient,
  options: WaitOptions = {},
): Promise<AgentEvent> {
  return waitForEvent(client, (event) => event.type === 'turn.ended', options);
}
