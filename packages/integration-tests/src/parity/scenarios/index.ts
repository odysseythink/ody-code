import type { ChatProvider } from '@odysseythink/kosong';

import type { Scenario } from '../types';
import { fileEditMockLlm, fileEditScenario } from './file-edit';
import { helloWorldMockLlm, helloWorldScenario } from './hello-world';
import { mockPromptMockLlm, mockPromptScenario } from './mock-prompt';
import { multiTurnToolMockLlm, multiTurnToolScenario } from './multi-turn-tool';
import { sessionLifecycleMockLlm, sessionLifecycleScenario } from './session-lifecycle';
import { setModelMockLlm, setModelScenario } from './set-model';

export { fileEditMockLlm, fileEditScenario } from './file-edit';
export { helloWorldMockLlm, helloWorldScenario } from './hello-world';
export { mockPromptMockLlm, mockPromptScenario } from './mock-prompt';
export { multiTurnToolMockLlm, multiTurnToolScenario } from './multi-turn-tool';
export { sessionLifecycleMockLlm, sessionLifecycleScenario } from './session-lifecycle';
export { setModelMockLlm, setModelScenario } from './set-model';

export interface ScenarioEntry {
  readonly scenario: Scenario;
  readonly mockLlm: ChatProvider;
}

export const scenarios: readonly ScenarioEntry[] = [
  { scenario: sessionLifecycleScenario, mockLlm: sessionLifecycleMockLlm },
  { scenario: setModelScenario, mockLlm: setModelMockLlm },
  { scenario: mockPromptScenario, mockLlm: mockPromptMockLlm },
  { scenario: helloWorldScenario, mockLlm: helloWorldMockLlm },
  { scenario: fileEditScenario, mockLlm: fileEditMockLlm },
  { scenario: multiTurnToolScenario, mockLlm: multiTurnToolMockLlm },
];
