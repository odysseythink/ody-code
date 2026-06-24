import { describe, expect, it, vi } from 'vitest';

import {
  createInProcessTransportPair,
  decodeJson,
  encodeJson,
  type Dispatch,
} from '../../src/rpc/transport';

describe('transport', () => {
  describe('encodeJson / decodeJson', () => {
    it('round-trips undefined as empty bytes', () => {
      const bytes = encodeJson(undefined);
      expect(bytes).toBeInstanceOf(Uint8Array);
      expect(bytes.length).toBe(0);
      expect(decodeJson(bytes)).toBe(undefined);
    });

    it('round-trips null, string, and objects', () => {
      expect(decodeJson(encodeJson(null))).toBe(null);
      expect(decodeJson(encodeJson(''))).toBe('');
      expect(decodeJson(encodeJson({ x: 1 }))).toEqual({ x: 1 });
    });

    it('matches JSON.stringify edge semantics', () => {
      const input = {
        at: new Date('2026-05-18T00:00:00.000Z'),
        notFinite: Number.NaN,
        dropped: undefined,
        nested: { ok: true },
      };
      expect(decodeJson(encodeJson(input))).toEqual({
        at: '2026-05-18T00:00:00.000Z',
        notFinite: null,
        nested: { ok: true },
      });
    });
  });

  describe('createInProcessTransportPair', () => {
    it('delivers bytes to peer dispatch via setTimeout(0)', async () => {
      const leftHandler = vi.fn<Dispatch>(async (bytes) => {
        expect(decodeJson(bytes)).toBe('ping-from-right');
        return encodeJson('pong-from-left');
      });
      const rightHandler = vi.fn<Dispatch>(async (bytes) => {
        expect(decodeJson(bytes)).toBe('ping-from-left');
        return encodeJson('pong-from-right');
      });

      const [left, right] = createInProcessTransportPair(leftHandler, rightHandler);

      const leftPromise = left.send(encodeJson('ping-from-left'));
      const rightPromise = right.send(encodeJson('ping-from-right'));

      await expect(leftPromise).resolves.toEqual(encodeJson('pong-from-right'));
      await expect(rightPromise).resolves.toEqual(encodeJson('pong-from-left'));
      expect(leftHandler).toHaveBeenCalledTimes(1);
      expect(rightHandler).toHaveBeenCalledTimes(1);
    });

    it('calls onWire for each send and recv', async () => {
      const leftHandler: Dispatch = async () => encodeJson('left-response');
      const rightHandler: Dispatch = async () => encodeJson('right-response');
      const leftWire: { direction: 'send' | 'recv'; json: unknown }[] = [];
      const rightWire: { direction: 'send' | 'recv'; json: unknown }[] = [];

      const [left, right] = createInProcessTransportPair(leftHandler, rightHandler);
      left.onWire = (direction, bytes) => leftWire.push({ direction, json: decodeJson(bytes) });
      right.onWire = (direction, bytes) => rightWire.push({ direction, json: decodeJson(bytes) });

      await left.send(encodeJson('hello'));

      expect(leftWire).toEqual([
        { direction: 'send', json: 'hello' },
        { direction: 'recv', json: 'right-response' },
      ]);
      expect(rightWire).toEqual([
        { direction: 'recv', json: 'hello' },
        { direction: 'send', json: 'right-response' },
      ]);
    });

    it('close is a no-op and does not break subsequent sends', async () => {
      const handler: Dispatch = async () => encodeJson('ok');
      const [left, right] = createInProcessTransportPair(handler, handler);
      left.close?.();
      right.close?.();
      await expect(left.send(encodeJson('x'))).resolves.toEqual(encodeJson('ok'));
    });
  });
});
