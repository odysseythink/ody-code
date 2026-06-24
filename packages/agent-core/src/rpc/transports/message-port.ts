import { randomUUID } from 'node:crypto';
import type { MessagePort } from 'node:worker_threads';

import { ErrorCodes, OdyError } from '../../errors';
import type { Dispatch, Transport } from '../transport';

interface WireRequest {
  readonly kind: 'request';
  readonly reqId: string;
  readonly bytes: Uint8Array;
}

interface WireResponse {
  readonly kind: 'response';
  readonly reqId: string;
  readonly bytes?: Uint8Array;
  readonly error?: { readonly message: string; readonly code?: string };
}

type WireMessage = WireRequest | WireResponse;

export interface MessagePortTransportOptions {
  onError?: (error: Error) => void;
  onWire?: (direction: 'send' | 'recv', bytes: Uint8Array) => void;
}

interface PendingDeferred<T> {
  readonly promise: Promise<T>;
  readonly resolve: (value: T) => void;
  readonly reject: (reason: unknown) => void;
}

function createDeferred<T>(): PendingDeferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function generateRequestId(): string {
  return randomUUID();
}

export function createMessagePortTransport(
  port: MessagePort,
  dispatch: Dispatch,
  options?: MessagePortTransportOptions,
): Transport {
  const pending = new Map<string, PendingDeferred<Uint8Array>>();
  let closed = false;

  function onError(error: Error): void {
    if (closed) return;
    const odyError =
      error instanceof OdyError ? error : new OdyError(ErrorCodes.INTERNAL, error.message);
    for (const deferred of pending.values()) {
      deferred.reject(odyError);
    }
    pending.clear();
    options?.onError?.(odyError);
  }

  async function handleMessage(msg: WireMessage): Promise<void> {
    if (closed) return;
    if (msg.kind === 'request') {
      try {
        const responseBytes = await dispatch(msg.bytes);
        port.postMessage({ kind: 'response', reqId: msg.reqId, bytes: responseBytes });
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        port.postMessage({
          kind: 'response',
          reqId: msg.reqId,
          error: { message },
        });
      }
    } else if (msg.kind === 'response') {
      const deferred = pending.get(msg.reqId);
      if (deferred === undefined) return;
      pending.delete(msg.reqId);
      if (msg.error !== undefined) {
        deferred.reject(new OdyError(ErrorCodes.INTERNAL, msg.error.message));
      } else {
        deferred.resolve(msg.bytes!);
      }
    }
  }

  port.on('message', (msg: WireMessage) => {
    void handleMessage(msg);
  });
  port.on('messageerror', (error: Error) => {
    onError(error);
  });

  return {
    send(bytes: Uint8Array): Promise<Uint8Array> {
      if (closed) {
        return Promise.reject(
          new OdyError(ErrorCodes.TRANSPORT_CLOSED, 'MessagePort closed'),
        );
      }
      const reqId = generateRequestId();
      const deferred = createDeferred<Uint8Array>();
      pending.set(reqId, deferred);
      const msg: WireRequest = { kind: 'request', reqId, bytes };
      options?.onWire?.('send', bytes);
      port.postMessage(msg);
      return deferred.promise.then((responseBytes) => {
        options?.onWire?.('recv', responseBytes);
        return responseBytes;
      });
    },
    onError(error) {
      onError(error);
    },
    close() {
      if (closed) return;
      closed = true;
      port.close();
      const error = new OdyError(ErrorCodes.TRANSPORT_CLOSED, 'MessagePort closed');
      for (const deferred of pending.values()) {
        deferred.reject(error);
      }
      pending.clear();
      options?.onError?.(error);
    },
  };
}
