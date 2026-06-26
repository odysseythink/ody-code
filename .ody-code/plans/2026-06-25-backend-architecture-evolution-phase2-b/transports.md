# Part 1: Transport Primitives (agent-core)

**Goal:** 在 `packages/agent-core` 实现 `StreamTransport`（stdio/UDS/TCP，支持 length-prefixed + NDJSON framing 与 handshake/token 鉴权）与 `WebSocketTransport`，并通过单元测试与 parity 测试验证 wire 语义。

**Architecture:** `StreamTransport` 在 Node 字节流之上维护一个 `BytesBuffer`，连接建立后首条消息为 NDJSON handshake（声明 framing 与可选 token）；后续消息按协商格式编解码。`WebSocketTransport` 复用标准 WebSocket API，每个 text frame 直接传输 JSON 封装的 request/response。两者均沿用 `createMessagePortTransport` 的 pending/deferred/closed 状态机。

**Tech Stack:** TypeScript 6.0 / Node.js ≥24.15 / Vitest.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task). Steps use - [ ] checkboxes for tracking.

---

### Task 1: 新增 transport 错误码与共享 BytesBuffer

**Depends on:** none

**Files:**
- Modify: `packages/agent-core-shared/src/errors/codes.ts:81-86`
- Modify: `packages/agent-core-shared/src/errors/codes.ts:455-461`
- Modify: `packages/agent-core-shared/test/errors/codes.test.ts:11-27`
- Create: `packages/agent-core/src/rpc/transports/bytes-buffer.ts`
- Create: `packages/agent-core/test/rpc/transports/bytes-buffer.test.ts`

- [ ] Write the failing test：扩展 `packages/agent-core-shared/test/errors/codes.test.ts`，断言新增的 transport 错误码存在且有元数据；创建 `bytes-buffer.test.ts` 覆盖追加、切片、丢弃、跨 chunk 边界。

```ts
// packages/agent-core-shared/test/errors/codes.test.ts
import { describe, expect, it } from 'vitest';

import {
  ErrorCodes,
  fromOdyErrorPayload,
  OdyError,
  ODY_ERROR_INFO,
  toOdyErrorPayload,
} from '../../src/errors';

describe('worker/transport error codes', () => {
  it('exposes worker and transport codes', () => {
    expect(ErrorCodes.WORKER_SPAWN_FAILED).toBe('worker.spawn_failed');
    expect(ErrorCodes.WORKER_EXITED).toBe('worker.exited');
    expect(ErrorCodes.TRANSPORT_CLOSED).toBe('transport.closed');
    expect(ErrorCodes.TRANSPORT_UNAUTHORIZED).toBe('transport.unauthorized');
    expect(ErrorCodes.TRANSPORT_INVALID_FRAMING).toBe('transport.invalid_framing');
    expect(ErrorCodes.TRANSPORT_ALREADY_CONNECTED).toBe('transport.already_connected');
  });

  it('has metadata for every new code', () => {
    const codes = [
      ErrorCodes.WORKER_SPAWN_FAILED,
      ErrorCodes.WORKER_EXITED,
      ErrorCodes.TRANSPORT_CLOSED,
      ErrorCodes.TRANSPORT_UNAUTHORIZED,
      ErrorCodes.TRANSPORT_INVALID_FRAMING,
      ErrorCodes.TRANSPORT_ALREADY_CONNECTED,
    ];
    for (const code of codes) {
      const info = ODY_ERROR_INFO[code];
      expect(info).toBeDefined();
      expect(info.title).toBeTruthy();
      expect(typeof info.retryable).toBe('boolean');
      expect(typeof info.public).toBe('boolean');
    }
  });

  it('round-trips through OdyError payload', () => {
    const error = new OdyError(ErrorCodes.WORKER_EXITED, 'worker died');
    const payload = toOdyErrorPayload(error);
    expect(payload.code).toBe('worker.exited');
    expect(payload.retryable).toBe(false);

    const restored = fromOdyErrorPayload(payload);
    expect(restored.code).toBe('worker.exited');
    expect(restored.message).toBe('worker died');
  });
});
```

```ts
// packages/agent-core/test/rpc/transports/bytes-buffer.test.ts
import { describe, expect, it } from 'vitest';

import { BytesBuffer } from '../../../src/rpc/transports/bytes-buffer';

function s(text: string): Uint8Array {
  return new TextEncoder().encode(text);
}

describe('BytesBuffer', () => {
  it('appends and slices across chunk boundaries', () => {
    const buf = new BytesBuffer();
    buf.append(s('hello'));
    buf.append(s(' world'));
    expect(buf.length).toBe(11);
    expect(new TextDecoder().decode(buf.slice(0, 5))).toBe('hello');
    expect(new TextDecoder().decode(buf.slice(6, 11))).toBe('world');
  });

  it('finds newline and discards bytes', () => {
    const buf = new BytesBuffer();
    buf.append(s('abc'));
    buf.append(s('d\nef'));
    expect(buf.indexOf(0x0a)).toBe(4);
    buf.discard(5);
    expect(buf.length).toBe(1);
    expect(new TextDecoder().decode(buf.slice(0, 1))).toBe('f');
  });

  it('expands internal capacity when appending large chunks', () => {
    const buf = new BytesBuffer();
    const big = new Uint8Array(4096).fill(0xab);
    buf.append(big);
    expect(buf.length).toBe(4096);
    expect(buf.slice(0, 1)[0]).toBe(0xab);
    expect(buf.slice(4095, 4096)[0]).toBe(0xab);
  });
});
```

- [ ] Run it and verify it FAILS：

```bash
pnpm --filter @odysseythink/agent-core-shared test
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/rpc/transports/bytes-buffer.test.ts
```

Expected failure：`TRANSPORT_UNAUTHORIZED` / `TRANSPORT_INVALID_FRAMING` / `TRANSPORT_ALREADY_CONNECTED` not defined；`BytesBuffer` module not found。

- [ ] Write the minimal implementation：

```ts
// packages/agent-core-shared/src/errors/codes.ts
// 在 TRANSPORT_CLOSED 之后新增（约第 81 行）
  TRANSPORT_CLOSED: 'transport.closed',
  TRANSPORT_UNAUTHORIZED: 'transport.unauthorized',
  TRANSPORT_INVALID_FRAMING: 'transport.invalid_framing',
  TRANSPORT_ALREADY_CONNECTED: 'transport.already_connected',
```

```ts
// packages/agent-core-shared/src/errors/codes.ts
// 在 'transport.closed' info 之后新增（约第 455 行）
  'transport.closed': {
    title: 'Transport closed',
    retryable: false,
    public: true,
    action: 'The worker connection was closed; create a new session.',
  },
  'transport.unauthorized': {
    title: 'Transport unauthorized',
    retryable: false,
    public: true,
    action: 'Check the token passed in the handshake or use a UDS/socket transport.',
  },
  'transport.invalid_framing': {
    title: 'Invalid transport framing',
    retryable: false,
    public: true,
    action: 'Ensure the handshake declares length-prefixed or ndjson framing and frames are well-formed.',
  },
  'transport.already_connected': {
    title: 'Transport already connected',
    retryable: false,
    public: true,
    action: 'This server only accepts one client at a time; disconnect the existing client first.',
  },
```

```ts
// packages/agent-core/src/rpc/transports/bytes-buffer.ts
export class BytesBuffer {
  private buffer = new Uint8Array(1024);
  private size = 0;

  append(chunk: Uint8Array): void {
    if (this.size + chunk.length > this.buffer.length) {
      let newCapacity = this.buffer.length * 2;
      while (newCapacity < this.size + chunk.length) {
        newCapacity *= 2;
      }
      const newBuffer = new Uint8Array(newCapacity);
      newBuffer.set(this.buffer.subarray(0, this.size));
      this.buffer = newBuffer;
    }
    this.buffer.set(chunk, this.size);
    this.size += chunk.length;
  }

  get length(): number {
    return this.size;
  }

  indexOf(byte: number): number {
    for (let i = 0; i < this.size; i++) {
      if (this.buffer[i] === byte) return i;
    }
    return -1;
  }

  slice(start: number, end: number): Uint8Array {
    return this.buffer.subarray(start, end);
  }

  discard(count: number): void {
    if (count >= this.size) {
      this.size = 0;
      return;
    }
    this.buffer.copyWithin(0, count, this.size);
    this.size -= count;
  }
}
```

- [ ] Run it and verify it PASSES：

```bash
pnpm --filter @odysseythink/agent-core-shared test
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/rpc/transports/bytes-buffer.test.ts
```

Expected：两个测试套件全绿。

- [ ] Commit：`git add -A && git commit -m "feat(agent-core): add transport error codes and BytesBuffer"`

- [ ] Shared-signature whole-tree typecheck（新增 ErrorCodes 是共享符号，但仅新增未改名，无调用方需要更新）：

```bash
pnpm -r typecheck
```

Expected：全绿。

---

### Task 2: 实现 StreamTransport

**Depends on:** Task 1

**Files:**
- Create: `packages/agent-core/src/rpc/transports/stream.ts`
- Modify: `packages/agent-core/src/rpc/index.ts:18-21`

- [ ] Write the failing test：先创建 `packages/agent-core/test/rpc/transports/stream-transport.test.ts`，覆盖 length-prefixed 请求-响应、NDJSON handshake 协商、token 校验失败、单帧超大关闭。

```ts
// packages/agent-core/test/rpc/transports/stream-transport.test.ts
import { EventEmitter } from 'node:events';
import { describe, expect, it, vi } from 'vitest';

import { ErrorCodes } from '@odysseythink/agent-core-shared';
import { createStreamTransport } from '../../../src/rpc/transports/stream';
import { decodeJson, encodeJson } from '../../../src/rpc/transport';

interface MockStream extends EventEmitter {
  write(chunk: Uint8Array, cb?: (err?: Error | null) => void): boolean;
  end(cb?: () => void): this;
}

function createMockStreams(): {
  input: MockStream;
  output: MockStream;
  received: Uint8Array[];
} {
  const input = new EventEmitter() as MockStream;
  const output = new EventEmitter() as MockStream;
  const received: Uint8Array[] = [];
  output.write = (chunk: Uint8Array) => {
    received.push(new Uint8Array(chunk));
    return true;
  };
  output.end = () => output;
  return { input, output, received };
}

describe('stream transport', () => {
  it('round-trips request/response with length-prefixed framing', async () => {
    const { input: clientInput, output: clientOutput } = createMockStreams();
    const { input: serverInput, output: serverOutput, received: serverReceived } = createMockStreams();

    const client = createStreamTransport(clientInput, serverOutput, async () => encodeJson('unused'), {
      framing: 'length-prefixed',
    });
    const server = createStreamTransport(serverInput, clientOutput, async (bytes) => {
      expect(decodeJson(bytes)).toBe('ping');
      return encodeJson('pong');
    });

    const response = await client.send(encodeJson('ping'));
    expect(decodeJson(response)).toBe('pong');
    expect(serverReceived.length).toBeGreaterThan(0);
  });

  it('negotiates ndjson framing via handshake', async () => {
    const { input: clientInput, output: clientOutput } = createMockStreams();
    const { input: serverInput, output: serverOutput } = createMockStreams();

    const client = createStreamTransport(clientInput, serverOutput, async () => encodeJson('unused'), {
      handshakeFraming: 'ndjson',
    });
    const server = createStreamTransport(serverInput, clientOutput, async (bytes) => {
      return encodeJson(`echo:${decodeJson(bytes)}`);
    });

    const response = await client.send(encodeJson('hello'));
    expect(decodeJson(response)).toBe('echo:hello');
  });

  it('rejects handshake with wrong token', async () => {
    const { input: clientInput, output: clientOutput } = createMockStreams();
    const { input: serverInput, output: serverOutput } = createMockStreams();

    const client = createStreamTransport(clientInput, serverOutput, async () => encodeJson('unused'), {
      token: 'ody_wrong',
    });
    createStreamTransport(serverInput, clientOutput, async () => encodeJson('unused'), {
      requiredToken: 'ody_correct',
    });

    const pending = client.send(encodeJson('x'));
    await expect(pending).rejects.toMatchObject({ code: ErrorCodes.TRANSPORT_CLOSED });
  });

  it('rejects pending sends after close', async () => {
    const { input, output } = createMockStreams();
    const transport = createStreamTransport(input, output, async () => new Promise(() => {}), {
      framing: 'length-prefixed',
    });

    const pending = transport.send(encodeJson('hang'));
    transport.close?.();
    await expect(pending).rejects.toMatchObject({ code: ErrorCodes.TRANSPORT_CLOSED });
  });
});
```

- [ ] Run it and verify it FAILS：

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/rpc/transports/stream-transport.test.ts
```

Expected failure：`createStreamTransport` module not found。

- [ ] Write the minimal implementation：

```ts
// packages/agent-core/src/rpc/transports/stream.ts
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
      return JSON.parse(decodeUtf8(payload)) as WireMessage;
    }
    // ndjson
    const newlineIndex = buffer.indexOf(0x0a);
    if (newlineIndex === -1) return null;
    const payload = buffer.slice(0, newlineIndex);
    buffer.discard(newlineIndex + 1);
    return JSON.parse(decodeUtf8(payload)) as WireMessage;
  }

  async function handleFrame(frame: WireMessage): Promise<void> {
    if (frame.kind === 'request') {
      try {
        const responseBytes = await dispatch(frame.bytes);
        sendResponse(frame.reqId, responseBytes);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        sendResponse(frame.reqId, undefined, { message });
      }
    } else if (frame.kind === 'response') {
      const deferred = pending.get(frame.reqId);
      if (deferred === undefined) return;
      pending.delete(frame.reqId);
      if (frame.error !== undefined) {
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
```

```ts
// packages/agent-core/src/rpc/index.ts
// 在 message-port 导出后新增
export {
  createStreamTransport,
  type Framing,
  type StreamTransportOptions,
} from './transports/stream';
```

- [ ] Run it and verify it PASSES：

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/rpc/transports/stream-transport.test.ts
```

Expected：4 个测试全绿。

- [ ] Commit：`git add -A && git commit -m "feat(agent-core): implement StreamTransport with handshake and framing"`

- [ ] Whole-tree typecheck：

```bash
pnpm -r typecheck
```

Expected：全绿。

---

### Task 3: StreamTransport 边界与并发测试

**Depends on:** Task 2

**Files:**
- Modify: `packages/agent-core/test/rpc/transports/stream-transport.test.ts`

- [ ] Write the failing test：新增并发请求按 reqId 关联、frame 超大关闭、onWire 回调、NDJSON payload 含转义换行不破坏帧边界。

```ts
// 追加到 packages/agent-core/test/rpc/transports/stream-transport.test.ts
import { PassThrough } from 'node:stream';

function createStreamTransportPair(
  dispatchLeft: (bytes: Uint8Array) => Promise<Uint8Array>,
  dispatchRight: (bytes: Uint8Array) => Promise<Uint8Array>,
  options?: { framing?: 'length-prefixed' | 'ndjson'; token?: string; requiredToken?: string },
): [Transport, Transport] {
  const leftToRight = new PassThrough();
  const rightToLeft = new PassThrough();

  const left = createStreamTransport(rightToLeft as unknown as ReadableLike, leftToRight as unknown as WritableLike, dispatchLeft, {
    framing: options?.framing,
    handshakeFraming: options?.framing,
    token: options?.token,
  });
  const right = createStreamTransport(leftToRight as unknown as ReadableLike, rightToLeft as unknown as WritableLike, dispatchRight, {
    framing: options?.framing,
    requiredToken: options?.requiredToken,
  });

  return [left, right];
}

describe('stream transport advanced', () => {
  it('correlates concurrent requests by reqId', async () => {
    const [left, right] = createStreamTransportPair(
      async () => encodeJson('unused'),
      async (bytes) => {
        const delay = decodeJson(bytes) as number;
        await new Promise((resolve) => setTimeout(resolve, delay));
        return encodeJson(`pong:${delay}`);
      },
    );

    const [a, b] = await Promise.all([left.send(encodeJson(30)), left.send(encodeJson(10))]);
    expect(decodeJson(a)).toBe('pong:30');
    expect(decodeJson(b)).toBe('pong:10');
    left.close?.();
    right.close?.();
  });

  it('calls onWire for send and recv', async () => {
    const wire: { direction: 'send' | 'recv'; json: unknown }[] = [];
    const [left, right] = createStreamTransportPair(
      async () => encodeJson('unused'),
      async (bytes) => encodeJson(`echo:${decodeJson(bytes)}`),
      { framing: 'length-prefixed' },
    );
    left.onWire = (direction, bytes) => wire.push({ direction, json: decodeJson(bytes) });

    await left.send(encodeJson('ping'));
    expect(wire.length).toBeGreaterThanOrEqual(2);
    expect(wire[0]).toEqual({ direction: 'send', json: 'ping' });

    left.close?.();
    right.close?.();
  });

  it('preserves ndjson frame boundaries with escaped newlines in payload', async () => {
    const [left, right] = createStreamTransportPair(
      async () => encodeJson('unused'),
      async (bytes) => bytes,
      { framing: 'ndjson' },
    );

    const payload = encodeJson({ text: 'line1\nline2' });
    const response = await left.send(payload);
    expect(decodeJson(response)).toEqual({ text: 'line1\nline2' });

    left.close?.();
    right.close?.();
  });
});
```

- [ ] Run it and verify it FAILS：

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/rpc/transports/stream-transport.test.ts
```

Expected failure：新增的 `createStreamTransportPair` 与 advanced tests 找不到，因为尚未追加。

- [ ] Write the minimal implementation：将上述测试代码追加到 `stream-transport.test.ts` 即可；实现已在 Task 2 完成，无需新增实现代码。

- [ ] Run it and verify it PASSES：

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/rpc/transports/stream-transport.test.ts
```

Expected：所有测试全绿。

- [ ] Commit：`git add -A && git commit -m "test(agent-core): add stream transport concurrency and boundary tests"`

---

### Task 4: 实现 WebSocketTransport

**Depends on:** Task 1

**Files:**
- Create: `packages/agent-core/src/rpc/transports/websocket.ts`
- Modify: `packages/agent-core/src/rpc/index.ts:18-21`

- [ ] Write the failing test：创建 `packages/agent-core/test/rpc/transports/websocket-transport.test.ts`，覆盖 text frame 请求-响应与关闭后 pending reject。

```ts
// packages/agent-core/test/rpc/transports/websocket-transport.test.ts
import { describe, expect, it, vi } from 'vitest';

import { ErrorCodes } from '@odysseythink/agent-core-shared';
import { createWebSocketTransport } from '../../../src/rpc/transports/websocket';
import { decodeJson, encodeJson } from '../../../src/rpc/transport';

interface FakeWebSocket {
  send(data: string): void;
  close(): void;
  onmessage?: ((event: { data: string }) => void) | null;
  onerror?: ((event: { type: string }) => void) | null;
  onclose?: ((event: { type: string }) => void) | null;
}

function createFakeSocketPair(): [FakeWebSocket, FakeWebSocket] {
  const a: FakeWebSocket = { send: vi.fn(), close: vi.fn() };
  const b: FakeWebSocket = { send: vi.fn(), close: vi.fn() };
  a.send = vi.fn((data: string) => {
    const parsed = JSON.parse(data);
    queueMicrotask(() => b.onmessage?.({ data: JSON.stringify(parsed) }));
  });
  b.send = vi.fn((data: string) => {
    const parsed = JSON.parse(data);
    queueMicrotask(() => a.onmessage?.({ data: JSON.stringify(parsed) }));
  });
  return [a, b];
}

describe('websocket transport', () => {
  it('round-trips request/response over text frames', async () => {
    const [sockA, sockB] = createFakeSocketPair();

    const left = createWebSocketTransport(sockA, async () => encodeJson('unused'));
    createWebSocketTransport(sockB, async (bytes) => {
      expect(decodeJson(bytes)).toBe('ping');
      return encodeJson('pong');
    });

    const response = await left.send(encodeJson('ping'));
    expect(decodeJson(response)).toBe('pong');
  });

  it('rejects pending requests when socket closes', async () => {
    const [sockA, sockB] = createFakeSocketPair();

    const left = createWebSocketTransport(sockA, async () => new Promise(() => {}));
    const right = createWebSocketTransport(sockB, async () => new Promise(() => {}));

    const pending = left.send(encodeJson('hang'));
    right.close();

    await expect(pending).rejects.toMatchObject({ code: ErrorCodes.TRANSPORT_CLOSED });
  });
});
```

- [ ] Run it and verify it FAILS：

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/rpc/transports/websocket-transport.test.ts
```

Expected failure：`createWebSocketTransport` module not found。

- [ ] Write the minimal implementation：

```ts
// packages/agent-core/src/rpc/transports/websocket.ts
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
        sendResponse(msg.reqId, responseBytes);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        sendResponse(msg.reqId, undefined, { message });
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
      const msg = decodeJson(data) as WireMessage;
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
```

```ts
// packages/agent-core/src/rpc/index.ts
// 在 stream 导出后新增
export {
  createWebSocketTransport,
  type WebSocketTransportOptions,
} from './transports/websocket';
```

- [ ] Run it and verify it PASSES：

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/rpc/transports/websocket-transport.test.ts
```

Expected：2 个测试全绿。

- [ ] Commit：`git add -A && git commit -m "feat(agent-core): implement WebSocketTransport over standard WebSocket API"`

- [ ] Whole-tree typecheck：

```bash
pnpm -r typecheck
```

Expected：全绿。

---

### Task 5: Transport Parity 扩展

**Depends on:** Task 2, Task 4

**Files:**
- Modify: `packages/agent-core/test/rpc/transports/transport-parity.test.ts`

- [ ] Write the failing test：扩展 parity 测试，让 stream transport（固定 length-prefixed）与 WebSocket transport 跑一遍 `runScenario` 的消息流，断言语义与 inproc 一致。

```ts
// packages/agent-core/test/rpc/transports/transport-parity.test.ts
import { describe, expect, it, vi } from 'vitest';
import { PassThrough } from 'node:stream';

import { createRPC } from '../../src/rpc';
import { createStreamTransport } from '../../src/rpc/transports/stream';
import { createWebSocketTransport } from '../../src/rpc/transports/websocket';
import {
  createInProcessTransportPair,
  decodeJson,
  type Dispatch,
  type TransportPair,
} from '../../src/rpc/transport';

interface CoreSide {
  getConfig(payload: { sessionId: string }): { model: string };
}

interface HostSide {
  emitEvent(event: { type: string; payload: { value: number } }): void;
  requestApproval(request: { requestId: string; toolName: string }): Promise<{ decision: string }>;
  fail(request: { code: string }): Promise<void>;
}

type WireEntry = {
  direction: 'send' | 'recv';
  json: unknown;
};

async function runScenario(
  connectCore: (self: CoreSide) => Promise<unknown>,
  connectHost: (self: HostSide) => Promise<unknown>,
): Promise<void> {
  const hostImpl = {
    emitEvent: vi.fn(),
    requestApproval: vi.fn(async (request: { requestId: string; toolName: string }) => ({
      decision: `approved:${request.toolName}`,
    })),
    fail: vi.fn(async () => {
      throw new Error('host failed:boom');
    }),
  };

  const hostProxyPromise = connectCore({
    getConfig: ({ sessionId }) => ({ model: `model-for:${sessionId}` }),
  });
  const coreProxy = (await connectHost(hostImpl)) as { getConfig: CoreSide['getConfig'] };
  const hostProxy = (await hostProxyPromise) as HostSide;

  await hostProxy.emitEvent({ type: 'agent.status.updated', payload: { value: 1 } });
  await hostProxy.requestApproval({ requestId: 'approval-1', toolName: 'Bash' });
  await expect(hostProxy.fail({ code: 'boom' })).rejects.toMatchObject({ code: 'internal' });
  await coreProxy.getConfig({ sessionId: 'session-1' });
}

function createRecordingFactory(
  leftWire: WireEntry[],
  rightWire: WireEntry[],
): (dispatchLeft: Dispatch, dispatchRight: Dispatch) => TransportPair {
  return (dispatchLeft, dispatchRight) => {
    const [left, right] = createInProcessTransportPair(dispatchLeft, dispatchRight);
    left.onWire = (direction, bytes) => leftWire.push({ direction, json: decodeJson(bytes) });
    right.onWire = (direction, bytes) => rightWire.push({ direction, json: decodeJson(bytes) });
    return [left, right];
  };
}

function createStreamTransportFactory(
  leftWire: WireEntry[],
  rightWire: WireEntry[],
): (dispatchLeft: Dispatch, dispatchRight: Dispatch) => TransportPair {
  return (dispatchLeft, dispatchRight) => {
    const leftToRight = new PassThrough();
    const rightToLeft = new PassThrough();

    const left = createStreamTransport(rightToLeft, leftToRight, dispatchLeft, {
      framing: 'length-prefixed',
    });
    const right = createStreamTransport(leftToRight, rightToLeft, dispatchRight, {
      framing: 'length-prefixed',
    });

    left.onWire = (direction, bytes) => leftWire.push({ direction, json: decodeJson(bytes) });
    right.onWire = (direction, bytes) => rightWire.push({ direction, json: decodeJson(bytes) });
    return [left, right];
  };
}

function createWebSocketTransportFactory(
  leftWire: WireEntry[],
  rightWire: WireEntry[],
): (dispatchLeft: Dispatch, dispatchRight: Dispatch) => TransportPair {
  return (dispatchLeft, dispatchRight) => {
    interface FakeSocket {
      send(data: string): void;
      close(): void;
      onmessage?: ((event: { data: string }) => void) | null;
    }
    const leftSocket: FakeSocket = { send: () => {}, close: () => {} };
    const rightSocket: FakeSocket = { send: () => {}, close: () => {} };
    leftSocket.send = (data: string) => {
      queueMicrotask(() => rightSocket.onmessage?.({ data }));
    };
    rightSocket.send = (data: string) => {
      queueMicrotask(() => leftSocket.onmessage?.({ data }));
    };

    const left = createWebSocketTransport(leftSocket, dispatchLeft);
    const right = createWebSocketTransport(rightSocket, dispatchRight);

    left.onWire = (direction, bytes) => leftWire.push({ direction, json: decodeJson(bytes) });
    right.onWire = (direction, bytes) => rightWire.push({ direction, json: decodeJson(bytes) });
    return [left, right];
  };
}

describe('transport parity', () => {
  it('default path and explicit InProcessTransport produce identical wire semantics', async () => {
    const defaultLeftWire: WireEntry[] = [];
    const defaultRightWire: WireEntry[] = [];
    const [connectCoreDefault, connectHostDefault] = createRPC<CoreSide, HostSide>({
      transport: createRecordingFactory(defaultLeftWire, defaultRightWire),
    });

    const explicitLeftWire: WireEntry[] = [];
    const explicitRightWire: WireEntry[] = [];
    const [connectCoreExplicit, connectHostExplicit] = createRPC<CoreSide, HostSide>({
      transport: createRecordingFactory(explicitLeftWire, explicitRightWire),
    });

    await runScenario(connectCoreDefault, connectHostDefault);
    await runScenario(connectCoreExplicit, connectHostExplicit);

    expect(defaultLeftWire).toEqual(explicitLeftWire);
    expect(defaultRightWire).toEqual(explicitRightWire);
    expect(defaultLeftWire.length).toBeGreaterThan(0);
    expect(defaultRightWire.length).toBeGreaterThan(0);
  });

  it('stream transport preserves wire semantics', async () => {
    const leftWire: WireEntry[] = [];
    const rightWire: WireEntry[] = [];
    const [connectCore, connectHost] = createRPC<CoreSide, HostSide>({
      transport: createStreamTransportFactory(leftWire, rightWire),
    });

    await runScenario(connectCore, connectHost);

    expect(leftWire.length).toBeGreaterThan(0);
    expect(rightWire.length).toBeGreaterThan(0);
  });

  it('websocket transport preserves wire semantics', async () => {
    const leftWire: WireEntry[] = [];
    const rightWire: WireEntry[] = [];
    const [connectCore, connectHost] = createRPC<CoreSide, HostSide>({
      transport: createWebSocketTransportFactory(leftWire, rightWire),
    });

    await runScenario(connectCore, connectHost);

    expect(leftWire.length).toBeGreaterThan(0);
    expect(rightWire.length).toBeGreaterThan(0);
  });
});
```

- [ ] Run it and verify it FAILS：

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/rpc/transports/transport-parity.test.ts
```

Expected failure：stream / websocket parity tests 未实现，runScenario 失败。

- [ ] Write the minimal implementation：将上述测试代码追加到 `transport-parity.test.ts`；`createStreamTransport` 与 `createWebSocketTransport` 的实现已在 Task 2/4 完成，无需新增实现代码。

- [ ] Run it and verify it PASSES：

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/rpc/transports/transport-parity.test.ts
```

Expected：3 个 parity 测试全绿。

- [ ] Commit：`git add -A && git commit -m "test(agent-core): extend transport parity to stream and websocket"`

- [ ] Part 1 全量回归：

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/rpc/transports/
pnpm -r typecheck
```

Expected：全绿。

---

## Local Self-Review

- [ ] 1. Spec-coverage table（本 Part）：StreamTransport 支持 stdio/UDS/TCP 与两种 framing → Task 2-3；WebSocketTransport → Task 4-5；handshake/token 鉴权 → Task 2-3；transport parity → Task 5。
- [ ] 2. Placeholder scan：本 Part 无 `TODO`/`TBD`/`implement later`；所有代码、命令、预期输出已完整给出。
- [ ] 3. No phantom tasks：每个 Task 均产出文件变更、测试/验证步骤与 commit；无 `--allow-empty`。
- [ ] 4. Dependency soundness：Task 2/4 依赖 Task 1；Task 3 依赖 Task 2；Task 5 依赖 Task 2/4；无向后引用。
- [ ] 5. Caller & build soundness：Task 1 修改共享 `ErrorCodes` 仅新增常量，无调用方需要更新，但仍在该 Task 末尾运行 `pnpm -r typecheck`；Task 2/4 修改 `packages/agent-core/src/rpc/index.ts` 新增导出，无现有调用方破坏。
- [ ] 6. Test-the-risk：stream/WebSocket 均有请求-响应 round-trip、关闭后 pending reject、并发请求关联；stream 有 token 校验失败、NDJSON 转义换行边界；parity 测试覆盖 wire 语义等价性。
- [ ] 7. Type consistency：`Framing`、`StreamTransportOptions`、`WebSocketTransportOptions`、`BytesBuffer`、`createStreamTransport`、`createWebSocketTransport` 的类型与属性名在本 Part 内一致，并为后续 Part 复用。
