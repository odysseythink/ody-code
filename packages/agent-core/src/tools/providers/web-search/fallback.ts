import type { Logger } from '../../../logging/types';
import type { WebSearchProvider, WebSearchResult } from './types';

export class FallbackWebSearchProvider implements WebSearchProvider {
  readonly name = 'fallback';

  constructor(
    private readonly primary: WebSearchProvider,
    private readonly secondary: WebSearchProvider | undefined,
    private readonly logger: Logger,
  ) {}

  async search(query: string, options?: { limit?: number; includeContent?: boolean; toolCallId?: string }): Promise<WebSearchResult[]> {
    this.logger.debug('web_search.attempt', { provider: this.primary.name });
    try {
      const results = await this.primary.search(query, options);
      this.logger.debug('web_search.success', { provider: this.primary.name, resultCount: results.length });
      return results;
    } catch (primaryError) {
      this.logger.debug('web_search.failure', {
        provider: this.primary.name,
        errorCategory: categorizeError(primaryError),
      });

      if (this.secondary === undefined) {
        throw primaryError;
      }
      if (!isRetryableError(primaryError)) {
        throw primaryError;
      }

      this.logger.debug('web_search.attempt', { provider: this.secondary.name });
      try {
        const results = await this.secondary.search(query, options);
        this.logger.debug('web_search.success', { provider: this.secondary.name, resultCount: results.length });
        return results;
      } catch (secondaryError) {
        this.logger.debug('web_search.failure', {
          provider: this.secondary.name,
          errorCategory: categorizeError(secondaryError),
        });
        throw secondaryError;
      }
    }
  }
}

const AUTH_PATTERN = /\b(401|403|unauthorized|authentication|auth)\b/;
const RATE_LIMIT_PATTERN = /\b429\b/;
const SERVER_ERROR_PATTERN = /\b5\d\d\b/;

export function isRetryableError(error: unknown): boolean {
  const name = error instanceof Error ? error.name : '';
  if (name === 'AbortError') return false;
  if (name === 'TimeoutError') return true;
  const message = String(error instanceof Error ? error.message : error).toLowerCase();
  if (AUTH_PATTERN.test(message)) return false;
  if (RATE_LIMIT_PATTERN.test(message)) return true;
  if (SERVER_ERROR_PATTERN.test(message) || message.includes('http 5')) return true;
  if (message.includes('network') || message.includes('fetch') || message.includes('timeout') || message.includes('timed out')) {
    return true;
  }
  return false;
}

function categorizeError(error: unknown): string {
  const message = String(error instanceof Error ? error.message : error).toLowerCase();
  if (AUTH_PATTERN.test(message)) return 'auth';
  if (RATE_LIMIT_PATTERN.test(message)) return 'rate-limit';
  if (SERVER_ERROR_PATTERN.test(message) || message.includes('http 5')) return 'server';
  if (message.includes('timeout') || message.includes('timed out')) return 'timeout';
  if (message.includes('network') || message.includes('fetch') || error instanceof TypeError) return 'network';
  return 'unknown';
}
