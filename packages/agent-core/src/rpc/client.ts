import type { PromisableMethods, Promisify } from '#/utils/types';
import { createControlledPromise, objectMap } from '@antfu/utils';

import {
  ErrorCodes,
  fromOdyErrorPayload,
  OdyError,
  type OdyErrorPayload,
  toOdyErrorPayload,
} from '../errors';
import { abortable } from '../utils/abort';
import type { CoreAPI } from './core-api';
import type { SDKAPI } from './sdk-api';
import {
  createInProcessTransportPair,
  decodeJson,
  encodeJson,
  type CreateRPCOptions,
  type Dispatch,
  type Transport,
  type TransportPair,
} from './transport';

export type { CreateRPCOptions, Transport, TransportPair } from './transport';

export interface RPCCallOptions {
  signal?: AbortSignal;
}

export interface PendingDeferred<T> {
  promise: Promise<T>;
  resolve(value: T): void;
  reject(reason: unknown): void;
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

export interface RPCEndpoint<Self extends Record<string, any>, Other extends Record<string, any>> {
  readonly dispatch: Dispatch;
  setTransport(transport: Transport): void;
  readonly client: RPCClient<Self, Other>;
}

type RpcResponse =
  | { readonly ok: true; readonly value: unknown }
  | { readonly ok: false; readonly error: OdyErrorPayload };

export type RPCMethods<T> = {
  [K in keyof T]: T[K] extends (payload: infer Payload) => infer Return
    ? (payload: Payload, options?: RPCCallOptions) => Promisify<Return>
    : never;
};

export type RPCClient<Self extends Record<string, any>, Other extends Record<string, any>> = (
  self: PromisableMethods<Self>,
) => Promise<RPCMethods<Other>>;

function bindAllFunctions<T extends Record<string, any>>(obj: T): T {
  const bound: Record<string, unknown> = {};
  let current: object | null = obj;

  while (current !== null && current !== Object.prototype) {
    for (const key of Object.getOwnPropertyNames(current)) {
      if (key === 'constructor' || Object.hasOwn(bound, key)) {
        continue;
      }

      const descriptor = Object.getOwnPropertyDescriptor(current, key);
      if (typeof descriptor?.value === 'function') {
        bound[key] = descriptor.value.bind(obj);
      }
    }

    current = Object.getPrototypeOf(current);
  }

  return bound as T;
}

export function createRPC<Left extends Record<string, any>, Right extends Record<string, any>>(
  options?: CreateRPCOptions,
): [RPCClient<Left, Right>, RPCClient<Right, Left>] {
  const leftReady = createControlledPromise<PromisableMethods<Left>>();
  const rightReady = createControlledPromise<PromisableMethods<Right>>();

  const pending = new Set<PendingDeferred<Uint8Array>>();

  function attachTransportErrorHandling(transport: Transport): void {
    const originalOnError = transport.onError;
    transport.onError = (error: Error) => {
      const errorToThrow =
        error instanceof OdyError ? error : new OdyError(ErrorCodes.INTERNAL, error.message);
      for (const deferred of pending) {
        deferred.reject(errorToThrow);
      }
      pending.clear();
      originalOnError?.(error);
    };
  }

  async function dispatchLeft(bytes: Uint8Array): Promise<Uint8Array> {
    const payload = decodeJson(bytes) as { method: string; args: unknown[] };
    const boundSelf = await leftReady;
    return handleRpcCall(boundSelf, payload);
  }

  async function dispatchRight(bytes: Uint8Array): Promise<Uint8Array> {
    const payload = decodeJson(bytes) as { method: string; args: unknown[] };
    const boundSelf = await rightReady;
    return handleRpcCall(boundSelf, payload);
  }

  const transportPair: TransportPair =
    typeof options?.transport === 'function'
      ? options.transport(dispatchLeft, dispatchRight)
      : options?.transport ?? createInProcessTransportPair(dispatchLeft, dispatchRight);

  const [leftTransport, rightTransport] = transportPair;

  attachTransportErrorHandling(leftTransport);
  attachTransportErrorHandling(rightTransport);

  function abortableRpc<T>(promise: Promise<T>, signal?: AbortSignal): Promise<T> {
    return signal === undefined ? promise : abortable(promise, signal);
  }

  async function handleRpcCall(
    boundSelf: PromisableMethods<Left | Right>,
    payload: { method: string; args: unknown[] },
  ): Promise<Uint8Array> {
    const fn = (boundSelf as Record<string, unknown>)[payload.method] as Function | undefined;
    if (typeof fn !== 'function') {
      return encodeJson({
        ok: false,
        error: toOdyErrorPayload(new Error(`RPC method not found: ${payload.method}`)),
      });
    }
    try {
      const value = await abortableRpc(Promise.resolve(fn(...payload.args)));
      return encodeJson({ ok: true, value });
    } catch (error) {
      return encodeJson({ ok: false, error: toOdyErrorPayload(error) });
    }
  }

  function mapRpcFunction(methodName: string, fn: Function, transport: Transport): Function {
    return async (payload: any, options?: RPCCallOptions) => {
      const signal = options?.signal;
      signal?.throwIfAborted();
      const requestBytes = encodeJson({ method: methodName, args: [payload] });
      transport.onWire?.('send', requestBytes);

      const deferred = createDeferred<Uint8Array>();
      pending.add(deferred);
      transport.send(requestBytes).then(deferred.resolve, deferred.reject).finally(() => {
        pending.delete(deferred);
      });

      const responseBytes = await abortableRpc(deferred.promise, signal);
      transport.onWire?.('recv', responseBytes);
      const response = decodeJson(responseBytes) as RpcResponse;
      signal?.throwIfAborted();
      if (response.ok) return response.value;
      throw fromOdyErrorPayload(response.error);
    };
  }

  async function leftClient(self: PromisableMethods<Left>): Promise<RPCMethods<Right>> {
    leftReady.resolve(bindAllFunctions(self));
    return objectMap(await rightReady, (key, fn) => [key, mapRpcFunction(key, fn, leftTransport)]) as RPCMethods<Right>;
  }

  async function rightClient(self: PromisableMethods<Right>): Promise<RPCMethods<Left>> {
    rightReady.resolve(bindAllFunctions(self));
    return objectMap(await leftReady, (key, fn) => [key, mapRpcFunction(key, fn, rightTransport)]) as RPCMethods<Left>;
  }

  return [leftClient, rightClient];
}

export function createRPCEndpoint<
  Self extends Record<string, any>,
  Other extends Record<string, any>,
>(): RPCEndpoint<Self, Other> {
  const selfReady = createControlledPromise<PromisableMethods<Self>>();
  let transport: Transport | undefined;
  const pending = new Set<PendingDeferred<Uint8Array>>();

  function attachTransportErrorHandling(t: Transport): void {
    const originalOnError = t.onError;
    t.onError = (error: Error) => {
      const errorToThrow =
        error instanceof OdyError ? error : new OdyError(ErrorCodes.INTERNAL, error.message);
      for (const deferred of pending) {
        deferred.reject(errorToThrow);
      }
      pending.clear();
      originalOnError?.(error);
    };
  }

  async function dispatch(bytes: Uint8Array): Promise<Uint8Array> {
    const payload = decodeJson(bytes) as { method: string; args: unknown[] };
    const boundSelf = await selfReady;
    const fn = (boundSelf as Record<string, unknown>)[payload.method] as Function | undefined;
    if (typeof fn !== 'function') {
      return encodeJson({
        ok: false,
        error: toOdyErrorPayload(new Error(`RPC method not found: ${payload.method}`)),
      });
    }
    try {
      const value = await Promise.resolve(fn(...payload.args));
      return encodeJson({ ok: true, value });
    } catch (error) {
      return encodeJson({ ok: false, error: toOdyErrorPayload(error) });
    }
  }

  function mapMethod(methodName: string): Function {
    return async (payload: any, options?: RPCCallOptions) => {
      if (transport === undefined) {
        throw new OdyError(ErrorCodes.INTERNAL, 'RPC endpoint transport not set');
      }
      const signal = options?.signal;
      signal?.throwIfAborted();
      const requestBytes = encodeJson({ method: methodName, args: [payload] });
      transport.onWire?.('send', requestBytes);

      const deferred = createDeferred<Uint8Array>();
      pending.add(deferred);
      transport.send(requestBytes).then(deferred.resolve, deferred.reject).finally(() => {
        pending.delete(deferred);
      });

      const responseBytes = signal ? await abortable(deferred.promise, signal) : await deferred.promise;
      transport.onWire?.('recv', responseBytes);
      const response = decodeJson(responseBytes) as RpcResponse;
      signal?.throwIfAborted();
      if (response.ok) return response.value;
      throw fromOdyErrorPayload(response.error);
    };
  }

  async function client(self: PromisableMethods<Self>): Promise<RPCMethods<Other>> {
    selfReady.resolve(bindAllFunctions(self));
    return new Proxy({} as RPCMethods<Other>, {
      get(_target, prop) {
        if (typeof prop !== 'string') return undefined;
        // Avoid the Proxy being treated as a thenable by Promise / await
        if (prop === 'then' || prop === 'catch' || prop === 'finally') return undefined;
        return mapMethod(prop);
      },
    });
  }

  function setTransport(t: Transport): void {
    transport = t;
    attachTransportErrorHandling(t);
  }

  return { dispatch, setTransport, client };
}

export type CoreRPCClient = RPCClient<CoreAPI, SDKAPI>;
export type SDKRPCClient = RPCClient<SDKAPI, CoreAPI>;

export type CoreRPC = RPCMethods<CoreAPI>;
