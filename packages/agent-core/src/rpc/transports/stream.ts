import { randomUUID } from 'node:crypto';

import { ErrorCodes, OdyError } from '../../errors';
import type { Dispatch, Transport } from '../transport';
import { BytesBuffer } from './bytes-buffer';

export type Framing = 'length-prefixed' | 'ndjson';

export interface StreamTransportOptions {
  /** 固定 framing；设置后跳过 handshake。用于 stdio/UDS 等已互信通道。 */
  framing?: Framing;
  /** 发送 handshake 时声明的 framing。默认 'length-prefixed'。 */
  handshakeFraming?: Framing;
  /** 发送 handshake 时携带的 token。 */
  token?: string;
  /** 接收 handshake 时要求匹配的 token。 */
  requiredToken?: string;
  onError?: (error: Error) => void;
  onWire?: (direction: 'send' | 'recv', bytes: Uint8Array) => void;
}

interface ReadableLike {
  on(event: 'data', listener: (chunk: Uint8Array) => void): this;
  on(event: 'error', listener: (error: Error) => void): this;
  on(event: 'end', listener: () => void): this;
}

interface WritableLike {
  write(chunk: Uint8Array, cb?: (error?: Error | null) => void): boolean;
  end(cb?: () => void): this;
  on(event: 'error', listener: (error: Error) => void): this;
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

interface HandshakeMessage {
  readonly framing: Framing;
  readonly token?: string;
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

const MAX_FRAME_SIZE = 64 * 1024 * 1024;

function u32le(value: number): Uint8Array {
  const buf = new Uint8Array(4);
  const view = new DataView(buf.buffer);
  view.setUint32(0, value, true);
  return buf;
}

function readU32le(buffer: Uint8Array, offset: number): number {
  const view = new DataView(buffer.buffer, buffer.byteOffset + offset, 4);
  return view.getUint32(0, true);
}

function concat(a: Uint8Array, b: Uint8Array): Uint8Array {
  const result = new Uint8Array(a.length + b.length);
  result.set(a, 0);
  result.set(b, a.length);
  return result;
}

function encodeUtf8(text: string): Uint8Array {
  return new TextEncoder().encode(text);
}

function decodeUtf8(bytes: Uint8Array): string {
  return new TextDecoder().decode(bytes);
}

function encodeFrame(envelope: WireMessage, framing: Framing): Uint8Array {
  const payload = encodeUtf8(JSON.stringify(envelope));
  if (framing === 'length-prefixed') {
    return concat(u32le(payload.length), payload);
  }
  return concat(payload, encodeUtf8('\n'));
}

export function createStreamTransport(
  input: ReadableLike,
  output: WritableLike,
  dispatch: Dispatch,
  options?: StreamTransportOptions,
): Transport {
  const buffer = new BytesBuffer();
  let state: 'handshake' | 'connected' | 'closed' = 'handshake';
  let framing: Framing | undefined = options?.framing;
  let handshakeSent = false;
  const pending = new Map<string, PendingDeferred<Uint8Array>>();

  function onError(error: Error): void {
    if (state === 'closed') return;
    state = 'closed';
    const odyError =
      error instanceof OdyError ? error : new OdyError(ErrorCodes.INTERNAL, error.message);
    for (const deferred of pending.values()) {
      deferred.reject(odyError);
    }
    pending.clear();
    options?.onError?.(odyError);
  }

  function closeWithError(error: Error): void {
    onError(error);
    try {
      output.end();
    } catch {
      // ignore
    }
  }

  function tryParseHandshake(): HandshakeMessage | null {
    const newlineIndex = buffer.indexOf(0x0a);
    if (newlineIndex === -1) return null;
    const line = buffer.slice(0, newlineIndex);
    buffer.discard(newlineIndex + 1);
    try {
      const json = JSON.parse(decodeUtf8(line)) as HandshakeMessage;
      if (json.framing !== 'length-prefixed' && json.framing !== 'ndjson') {
        throw new OdyError(
          ErrorCodes.TRANSPORT_INVALID_FRAMING,
          `Invalid framing: ${String(json.framing)}`,
        );
      }
      return json;
    } catch (error) {
      if (error instanceof OdyError) throw error;
      throw new OdyError(ErrorCodes.TRANSPORT_INVALID_FRAMING, 'Invalid handshake');
    }
  }

  function validateHandshake(handshake: HandshakeMessage): void {
    if (options?.requiredToken !== undefined && handshake.token !== options.requiredToken) {
      throw new OdyError(ErrorCodes.TRANSPORT_UNAUTHORIZED, 'Token mismatch');
    }
  }

  function toBytes(value: unknown): Uint8Array | undefined {
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
    return undefined;
  }

  function reviveWireMessage(key: string, value: unknown): unknown {
    if (key === 'bytes' && value !== null && typeof value === 'object') {
      const revived = toBytes(value);
      if (revived !== undefined) return revived;
    }
    return value;
  }

  function parseFrame(): WireMessage | null {
    if (framing === 'length-prefixed') {
      if (buffer.length < 4) return null;
      const length = readU32le(buffer.slice(0, 4), 0);
      if (length > MAX_FRAME_SIZE) {
        throw new OdyError(
          ErrorCodes.TRANSPORT_INVALID_FRAMING,
          `Frame too large: ${length}`,
        );
      }
      if (buffer.length < 4 + length) return null;
      const payload = buffer.slice(4, 4 + length);
      buffer.discard(4 + length);
      return JSON.parse(decodeUtf8(payload), reviveWireMessage) as WireMessage;
    }
    // ndjson
    const newlineIndex = buffer.indexOf(0x0a);
    if (newlineIndex === -1) return null;
    const payload = buffer.slice(0, newlineIndex);
    buffer.discard(newlineIndex + 1);
    return JSON.parse(decodeUtf8(payload), reviveWireMessage) as WireMessage;
  }

  async function handleFrame(frame: WireMessage): Promise<void> {
    if (frame.kind === 'request') {
      try {
        const responseBytes = await dispatch(frame.bytes);
        options?.onWire?.('recv', responseBytes);
        sendResponse(frame.reqId, responseBytes);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        sendResponse(frame.reqId, undefined, { message });
      }
    } else if (frame.kind === 'response') {
      const deferred = pending.get(frame.reqId);
      if (deferred === undefined) return;
      pending.delete(frame.reqId);
      options?.onWire?.('recv', frame.bytes ?? new Uint8Array());
      if (frame.error != null) {
        deferred.reject(new OdyError(ErrorCodes.INTERNAL, frame.error.message));
      } else {
        deferred.resolve(frame.bytes!);
      }
    }
  }

  function flushFrames(): void {
    while (state === 'connected') {
      try {
        const frame = parseFrame();
        if (frame === null) break;
        void handleFrame(frame);
      } catch (error) {
        closeWithError(error instanceof Error ? error : new Error(String(error)));
        return;
      }
    }
  }

  function sendResponse(
    reqId: string,
    bytes?: Uint8Array,
    error?: { message: string; code?: string },
  ): void {
    if (state === 'closed') return;
    const msg: WireResponse = { kind: 'response', reqId, bytes, error };
    const frame = encodeFrame(msg, framing!);
    options?.onWire?.('send', bytes ?? new Uint8Array());
    output.write(frame);
  }

  input.on('data', (chunk: Uint8Array) => {
    if (state === 'closed') return;
    buffer.append(chunk);
    if (state === 'handshake') {
      if (framing !== undefined) {
        state = 'connected';
        flushFrames();
        return;
      }
      try {
        const handshake = tryParseHandshake();
        if (handshake === null) return;
        validateHandshake(handshake);
        framing = handshake.framing;
        state = 'connected';
        flushFrames();
      } catch (error) {
        closeWithError(error instanceof Error ? error : new Error(String(error)));
      }
    } else {
      flushFrames();
    }
  });

  input.on('error', (error: Error) => onError(error));
  input.on('end', () => {
    if (state !== 'closed') {
      onError(new OdyError(ErrorCodes.TRANSPORT_CLOSED, 'stream ended'));
    }
  });

  return {
    send(bytes: Uint8Array): Promise<Uint8Array> {
      if (state === 'closed') {
        return Promise.reject(
          new OdyError(ErrorCodes.TRANSPORT_CLOSED, 'StreamTransport closed'),
        );
      }
      if (state === 'handshake' && framing === undefined && !handshakeSent) {
        framing = options?.handshakeFraming ?? 'length-prefixed';
        const handshake: HandshakeMessage = { framing, token: options?.token };
        output.write(encodeUtf8(JSON.stringify(handshake) + '\n'));
        handshakeSent = true;
        state = 'connected';
      }
      const reqId = generateRequestId();
      const deferred = createDeferred<Uint8Array>();
      pending.set(reqId, deferred);
      const msg: WireRequest = { kind: 'request', reqId, bytes };
      const frame = encodeFrame(msg, framing!);
      options?.onWire?.('send', bytes);
      output.write(frame);
      return deferred.promise;
    },
    onError(error: Error): void {
      onError(error);
    },
    close(): void {
      if (state === 'closed') return;
      state = 'closed';
      output.end();
      const error = new OdyError(ErrorCodes.TRANSPORT_CLOSED, 'StreamTransport closed');
      for (const deferred of pending.values()) {
        deferred.reject(error);
      }
      pending.clear();
      options?.onError?.(error);
    },
  };
}
