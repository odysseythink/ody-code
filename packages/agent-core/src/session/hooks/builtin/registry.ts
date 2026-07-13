import type { BuiltinHookRegistry } from '../types';

import { EditAccumulatorBuiltin } from './edit-accumulator';
import { StopFormatTypecheckBuiltin } from './stop-format-typecheck';

export function createBuiltinHookRegistry(): BuiltinHookRegistry {
  const accumulator = new EditAccumulatorBuiltin();
  const builtins = new Map<string, ReturnType<BuiltinHookRegistry['get']>>([
    [accumulator.id, accumulator],
    ['stop-format-typecheck', new StopFormatTypecheckBuiltin(accumulator)],
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
