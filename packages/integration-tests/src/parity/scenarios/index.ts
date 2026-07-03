import type { ChatProvider } from '@odysseythink/kosong';

import type { Scenario } from '../types';
import { backgroundCronMockLlm, backgroundCronScenario } from './background-cron';
import { fileEditMockLlm, fileEditScenario } from './file-edit';
import { helloWorldMockLlm, helloWorldScenario } from './hello-world';
import { hostConfigMockLlm, hostConfigScenario } from './host-config';
import { kaosOpsScenario } from './kaos-ops';
import { mockPromptMockLlm, mockPromptScenario } from './mock-prompt';
import { multiTurnToolMockLlm, multiTurnToolScenario } from './multi-turn-tool';
import { sessionLifecycleMockLlm, sessionLifecycleScenario } from './session-lifecycle';
import { sessionModeHandoffMockLlm, sessionModeHandoffScenario } from './session-mode-handoff';
import { setModelMockLlm, setModelScenario } from './set-model';
import { webSearchMockLlm, webSearchScenario } from './web-search';
import { bashToolCallMockLlm, bashToolCallScenario } from './bash-tool-call';

export { agentApiL2MockLlm, agentApiL2Scenario } from './agent-api-l2';
export { backgroundCronMockLlm, backgroundCronScenario } from './background-cron';
export { fileEditMockLlm, fileEditScenario } from './file-edit';
export { helloWorldMockLlm, helloWorldScenario } from './hello-world';
export { hostConfigMockLlm, hostConfigScenario } from './host-config';
export { kaosOpsScenario } from './kaos-ops';
export { mockPromptMockLlm, mockPromptScenario } from './mock-prompt';
export { multiTurnToolMockLlm, multiTurnToolScenario } from './multi-turn-tool';
export { sessionLifecycleMockLlm, sessionLifecycleScenario } from './session-lifecycle';
export { sessionModeHandoffMockLlm, sessionModeHandoffScenario } from './session-mode-handoff';
export { setModelMockLlm, setModelScenario } from './set-model';
export { webSearchMockLlm, webSearchScenario } from './web-search';
export { bashToolCallMockLlm, bashToolCallScenario } from './bash-tool-call';

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
  { scenario: hostConfigScenario, mockLlm: hostConfigMockLlm },
  { scenario: sessionModeHandoffScenario, mockLlm: sessionModeHandoffMockLlm },
  { scenario: backgroundCronScenario, mockLlm: backgroundCronMockLlm },
  { scenario: webSearchScenario, mockLlm: webSearchMockLlm },
  { scenario: bashToolCallScenario, mockLlm: bashToolCallMockLlm },
];
