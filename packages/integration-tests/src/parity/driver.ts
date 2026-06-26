import type { AgentEvent } from '@odysseythink/agent-core';
import type { ParityBackend, Scenario, ScenarioSnapshot } from './types';

export interface ParityDriverOptions {
  readonly timeoutMs?: number;
}

export class ParityDriver {
  constructor(private readonly options: ParityDriverOptions = {}) {}

  async runScenario(backend: ParityBackend, scenario: Scenario): Promise<ScenarioSnapshot> {
    const events: AgentEvent[] = [];
    const unsubscribe = backend.client.onEvent((event) => events.push(event));

    try {
      const result = await this.withTimeout(scenario.run(backend));
      return {
        responses: result.responses ?? [],
        events,
        records: result.records,
        fsTree: result.fsTree,
      };
    } finally {
      unsubscribe();
    }
  }

  private async withTimeout<T>(promise: Promise<T>): Promise<T> {
    const timeoutMs = this.options.timeoutMs ?? 30000;
    if (timeoutMs <= 0) return promise;
    return Promise.race([
      promise,
      new Promise<never>((_, reject) => {
        const id = setTimeout(() => reject(new Error(`Scenario timed out after ${timeoutMs}ms`)), timeoutMs);
        promise.then(() => clearTimeout(id), () => clearTimeout(id));
      }),
    ]);
  }
}
