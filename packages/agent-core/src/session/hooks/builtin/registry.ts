import type { SecretScanConfig } from '@odysseythink/agent-core-shared';

import type { BuiltinHookRegistry } from '../types';

import { EditAccumulatorBuiltin } from './edit-accumulator';
import { SecretLeakScannerBuiltin } from './secret-leak-scanner';
import { SessionMemoryWriterBuiltin } from './session-memory-writer';
import { StopFormatTypecheckBuiltin } from './stop-format-typecheck';

export interface BuiltinHookRegistryConfig {
  readonly secretScan?: SecretScanConfig;
}

export function createBuiltinHookRegistry(
  config: BuiltinHookRegistryConfig = {},
): BuiltinHookRegistry {
  const accumulator = new EditAccumulatorBuiltin();
  const memoryWriter = new SessionMemoryWriterBuiltin();
  const secretScanner = new SecretLeakScannerBuiltin(config.secretScan);
  const builtins = new Map<string, ReturnType<BuiltinHookRegistry['get']>>([
    [accumulator.id, accumulator],
    ['stop-format-typecheck', new StopFormatTypecheckBuiltin(accumulator)],
    [memoryWriter.id, memoryWriter],
    [secretScanner.id, secretScanner],
  ]);

  return {
    get(id: string) {
      return builtins.get(id);
    },
    ids() {
      return Array.from(builtins.keys());
    },
  };
}
