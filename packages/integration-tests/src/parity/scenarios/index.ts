import type { ChatProvider } from '@odysseythink/kosong';

import type { Scenario } from '../types';
import { fileEditMockLlm, fileEditScenario } from './file-edit';
import { helloWorldMockLlm, helloWorldScenario } from './hello-world';
import { multiTurnToolMockLlm, multiTurnToolScenario } from './multi-turn-tool';

export interface ScenarioEntry {
  readonly scenario: Scenario;
  readonly mockLlm: ChatProvider;
}

export const scenarios: readonly ScenarioEntry[] = [
  { scenario: helloWorldScenario, mockLlm: helloWorldMockLlm },
  { scenario: fileEditScenario, mockLlm: fileEditMockLlm },
  { scenario: multiTurnToolScenario, mockLlm: multiTurnToolMockLlm },
];
