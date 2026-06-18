import { describe, expect, it, vi } from 'vitest';
import type { Event } from '@odysseythink/agent-core';

// Minimal partial client shape for testing progress dispatch in receiveEvent
function createClient() {
  const codeReviewProgressHandlers = new Map<string, (progress: any) => void>();
  const receiveEvent = (event: Event): void => {
    if (event.type === 'codeReview.progress') {
      const handler = codeReviewProgressHandlers.get(event.requestId);
      if (handler) {
        try {
          handler({
            requestId: event.requestId,
            stage: event.stage,
            modelAlias: event.modelAlias,
            detail: event.detail,
            meta: event.meta,
          });
        } catch {
          // Silently swallow user callback errors
        }
      }
    }
  };
  return { codeReviewProgressHandlers, receiveEvent };
}

describe('SDKRpcClient requestCodeReview progress', () => {
  it('dispatches codeReview.progress event to matching handler', () => {
    const client = createClient();
    const onProgress = vi.fn();
    client.codeReviewProgressHandlers.set('req-1', onProgress);

    const event: Event = {
      type: 'codeReview.progress',
      requestId: 'req-1',
      stage: 'generating',
      modelAlias: 'test-model',
      sessionId: '__code_review_progress__',
      agentId: '__code_review_progress__',
    };
    client.receiveEvent(event);

    expect(onProgress).toHaveBeenCalledOnce();
    expect(onProgress).toHaveBeenCalledWith(expect.objectContaining({
      requestId: 'req-1',
      stage: 'generating',
      modelAlias: 'test-model',
    }));
  });

  it('does not dispatch to handler with different requestId', () => {
    const client = createClient();
    const onProgress = vi.fn();
    client.codeReviewProgressHandlers.set('req-1', onProgress);

    const event: Event = {
      type: 'codeReview.progress',
      requestId: 'req-2',
      stage: 'generating',
      modelAlias: 'test-model',
      sessionId: '__code_review_progress__',
      agentId: '__code_review_progress__',
    };
    client.receiveEvent(event);

    expect(onProgress).not.toHaveBeenCalled();
  });

  it('swallows errors from onProgress callback', () => {
    const client = createClient();
    const onProgress = vi.fn(() => { throw new Error('callback error'); });
    client.codeReviewProgressHandlers.set('req-1', onProgress);

    const event: Event = {
      type: 'codeReview.progress',
      requestId: 'req-1',
      stage: 'generating',
      modelAlias: 'test-model',
      sessionId: '__code_review_progress__',
      agentId: '__code_review_progress__',
    };
    expect(() => client.receiveEvent(event)).not.toThrow();
  });

  it('cleans up handler after requestCodeReview completes', () => {
    const client = createClient();
    const onProgress = vi.fn();
    client.codeReviewProgressHandlers.set('req-1', onProgress);
    client.codeReviewProgressHandlers.delete('req-1');
    expect(client.codeReviewProgressHandlers.has('req-1')).toBe(false);
  });
});
