import { randomUUID } from 'node:crypto';

import { ErrorCodes, OdyError } from '../../errors';
import type { Dispatch, Transport } from '../transport';

export interface WebSocketTransportOptions {
  onError?: (error: Error) => void;
  onWire?: (direction: 'send' | 'recv', bytes: Uint8Array) => void;
}

interface WebSocketEvent {
  readonly type: string;
}

interface WebSocketMessageEvent {
  readonly data: string | Uint8Array;
}

interface WebSocketLike {
  send(data: string): void;
  close(): void;
  onmessage?: ((event: WebSocketMessageEvent) => void) | null;
  onerror?: ((event: WebSocketEvent) => void) | null;
  onclose?: ((event: WebSocketEvent) => void) | null;
}

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

function encodeJson(value: unknown): Uint8Array {
  return new TextEncoder().encode(JSON.stringify(value));
}

function decodeJson(bytes: Uint8Array): unknown {
  return JSON.parse(new TextDecoder().decode(bytes));
}

function toBytes(value: unknown): Uint8Array {
  if (value instanceof Uint8Array) return value;
  if (Array.isArray(value)) return new Uint8Array(value as number[]);
  // Uint8Array.toJSON() in Node.js produces { "0": byte, "1": byte, ... }
  if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
    const entries = Object.entries(value).filter(([k]) => /^\d+$/.test(k));
    if (entries.length > 0) {
      const arr = new Uint8Array(entries.length);
      for (const [k, v] of entries) {
        arr[parseInt(k, 10)] = v as number;
      }
      return arr;
    }
  }
  return new Uint8Array();
}

export function createWebSocketTransport(
  socket: WebSocketLike,
  dispatch: Dispatch,
  options?: WebSocketTransportOptions,
): Transport {
  const pending = new Map<string, PendingDeferred<Uint8Array>>();
  let closed = false;

  function onError(error: Error): void {
    if (closed) return;
    closed = true;
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
        options?.onWire?.('recv', responseBytes);
        sendResponse(msg.reqId, responseBytes);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        sendResponse(msg.reqId, undefined, { message });
      }
    } else if (msg.kind === 'response') {
      const deferred = pending.get(msg.reqId);
      if (deferred === undefined) return;
      pending.delete(msg.reqId);
      options?.onWire?.('recv', msg.bytes ?? new Uint8Array());
      if (msg.error !== undefined) {
        deferred.reject(new OdyError(ErrorCodes.INTERNAL, msg.error.message));
      } else {
        deferred.resolve(msg.bytes!);
      }
    }
  }

  function sendResponse(
    reqId: string,
    bytes?: Uint8Array,
    error?: { message: string; code?: string },
  ): void {
    if (closed) return;
    const msg: WireResponse = { kind: 'response', reqId, bytes, error };
    const wireBytes = encodeJson(msg);
    options?.onWire?.('send', bytes ?? new Uint8Array());
    socket.send(new TextDecoder().decode(wireBytes));
  }

  socket.onmessage = (event) => {
    const data =
      typeof event.data === 'string'
        ? new TextEncoder().encode(event.data)
        : new Uint8Array(event.data);
    options?.onWire?.('recv', data);
    try {
      const parsed = JSON.parse(new TextDecoder().decode(data)) as Record<string, unknown>;
      const msg: WireMessage = {
        kind: parsed['kind'] as 'request' | 'response',
        reqId: parsed['reqId'] as string,
        bytes: parsed['bytes'] !== undefined ? toBytes(parsed['bytes']) : undefined,
        error: parsed['error'] as { message: string; code?: string } | undefined,
      } as WireMessage;
      void handleMessage(msg);
    } catch (error) {
      onError(new OdyError(ErrorCodes.TRANSPORT_INVALID_FRAMING, 'Invalid WebSocket message'));
    }
  };

  socket.onerror = () => {
    onError(new OdyError(ErrorCodes.TRANSPORT_CLOSED, 'WebSocket error'));
  };

  socket.onclose = () => {
    onError(new OdyError(ErrorCodes.TRANSPORT_CLOSED, 'WebSocket closed'));
  };

  return {
    send(bytes: Uint8Array): Promise<Uint8Array> {
      if (closed) {
        return Promise.reject(
          new OdyError(ErrorCodes.TRANSPORT_CLOSED, 'WebSocketTransport closed'),
        );
      }
      const reqId = generateRequestId();
      const deferred = createDeferred<Uint8Array>();
      pending.set(reqId, deferred);
      const msg: WireRequest = { kind: 'request', reqId, bytes };
      const wireBytes = encodeJson(msg);
      options?.onWire?.('send', bytes);
      socket.send(new TextDecoder().decode(wireBytes));
      return deferred.promise;
    },
    onError(error: Error): void {
      onError(error);
    },
    close(): void {
      if (closed) return;
      closed = true;
      socket.close();
      const error = new OdyError(ErrorCodes.TRANSPORT_CLOSED, 'WebSocketTransport closed');
      for (const deferred of pending.values()) {
        deferred.reject(error);
      }
      pending.clear();
      options?.onError?.(error);
    },
  };
}
