export interface Transport {
  send(bytes: Uint8Array): Promise<Uint8Array>;
  onError?(error: Error): void;
  onWire?(direction: 'send' | 'recv', bytes: Uint8Array): void;
  close?(): void;
}

export type TransportPair = [Transport, Transport];

export type Dispatch = (bytes: Uint8Array) => Promise<Uint8Array>;

export interface CreateRPCOptions {
  transport?: TransportPair | ((dispatchLeft: Dispatch, dispatchRight: Dispatch) => TransportPair);
}

export function encodeJson(value: unknown): Uint8Array {
  const json = JSON.stringify(value);
  if (json === undefined) {
    return new Uint8Array();
  }
  return new TextEncoder().encode(json);
}

export function decodeJson(bytes: Uint8Array): unknown {
  if (bytes.length === 0) {
    return undefined;
  }
  return JSON.parse(new TextDecoder().decode(bytes));
}

export function createInProcessTransportPair(
  dispatchLeft: Dispatch,
  dispatchRight: Dispatch,
): TransportPair {
  const left: Transport = {
    send(bytes: Uint8Array): Promise<Uint8Array> {
      return new Promise((resolve, reject) => {
        left.onWire?.('send', bytes);
        right.onWire?.('recv', bytes);
        setTimeout(() => {
          dispatchRight(bytes)
            .then((responseBytes) => {
              right.onWire?.('send', responseBytes);
              left.onWire?.('recv', responseBytes);
              resolve(responseBytes);
            })
            .catch(reject);
        }, 0);
      });
    },
    close(): void {
      // no-op for in-process transport
    },
  };

  const right: Transport = {
    send(bytes: Uint8Array): Promise<Uint8Array> {
      return new Promise((resolve, reject) => {
        right.onWire?.('send', bytes);
        left.onWire?.('recv', bytes);
        setTimeout(() => {
          dispatchLeft(bytes)
            .then((responseBytes) => {
              left.onWire?.('send', responseBytes);
              right.onWire?.('recv', responseBytes);
              resolve(responseBytes);
            })
            .catch(reject);
        }, 0);
      });
    },
    close(): void {
      // no-op for in-process transport
    },
  };

  return [left, right];
}
