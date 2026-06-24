import type { Logger, LogPayload } from '@odysseythink/agent-core-shared';

class FallbackLogger implements Logger {
  error(message: string, payload?: LogPayload): void {
    console.error(message, payload);
  }

  warn(message: string, payload?: LogPayload): void {
    console.warn(message, payload);
  }

  info(message: string, payload?: LogPayload): void {
    console.info(message, payload);
  }

  debug(message: string, payload?: LogPayload): void {
    console.debug(message, payload);
  }

  createChild(): Logger {
    return this;
  }
}

export const fallbackLogger: Logger = new FallbackLogger();
