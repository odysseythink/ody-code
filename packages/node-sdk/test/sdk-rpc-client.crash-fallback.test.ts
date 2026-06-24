import { describe, it, expect } from 'vitest';
import { SDKRpcClient } from '../src/rpc';

describe('SDKRpcClient crash fallback', () => {
  it('constructs successfully in default in-process mode', () => {
    const client = new SDKRpcClient({});
    expect(client.core).toBeDefined();
  });

  it('accepts the worker option and still works in in-process mode', () => {
    const client = new SDKRpcClient({ worker: false });
    expect(client.core).toBeDefined();
  });
});
