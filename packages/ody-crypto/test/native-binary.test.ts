import { describe, expect, it } from 'vitest';
import { createPrivateKey, createPublicKey, generateKeyPairSync, randomBytes as nodeRandomBytes } from 'node:crypto';
import jwt from 'jsonwebtoken';

import { getOdyCrypto, loadNative } from '../src/loader';
import { tsFallback } from '../src/fallback';

const { sign } = jwt;

describe('native binary loader', () => {
  it('loads the real .node binary for the current platform if available', () => {
    const native = loadNative();

    if (native === null) {
      // No binary for this platform or build artifact missing; skip rather than fail.
      // eslint-disable-next-line no-console
      console.log(`native binary not available for ${process.platform}-${process.arch}, skipping`);
      return;
    }

    expect(native).not.toBe(tsFallback);
    expect(typeof native.randomBytes).toBe('function');
    expect(typeof native.sha256).toBe('function');
    expect(typeof native.pkceChallenge).toBe('function');
    expect(typeof native.verifyIdToken).toBe('function');
  });
});

describe('native binary functions', () => {
  const native = loadNative();

  it('randomBytes returns cryptographically random bytes of requested length', () => {
    if (native === null) return;

    const bytes = native.randomBytes(32);
    expect(bytes).toBeInstanceOf(Buffer);
    expect(bytes.length).toBe(32);
    // Very unlikely to be all zeros.
    expect(bytes.toString('hex')).not.toBe('00'.repeat(32));
  });

  it('sha256 matches node:crypto for known and random inputs', () => {
    if (native === null) return;

    const inputs = ['abc', '', 'hello world', nodeRandomBytes(256).toString('hex')];
    for (const input of inputs) {
      expect(native.sha256(input)).toBe(tsFallback.sha256(input));
    }
  });

  it('pkceChallenge produces verifier and challenge that match the fallback', () => {
    if (native === null) return;

    const nativeChallenge = native.pkceChallenge();
    expect(nativeChallenge.codeVerifier.length).toBe(43);
    expect(nativeChallenge.codeChallenge).toMatch(/^[A-Za-z0-9_-]+$/);

    // Determinism check: same verifier should produce same challenge via fallback SHA-256.
    // Native returns base64url; fallback sha256 returns hex.
    const fallbackChallengeHex = tsFallback.sha256(nativeChallenge.codeVerifier);
    const fallbackChallengeBase64url = Buffer.from(fallbackChallengeHex, 'hex').toString('base64url');
    expect(nativeChallenge.codeChallenge).toBe(fallbackChallengeBase64url);
  });

  it('verifyIdToken validates a real RS256 id_token against its JWK', () => {
    if (native === null) return;

    const { privateKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
    const jwk = createPublicKey(privateKey).export({ format: 'jwk' });
    const token = sign(
      { sub: 'user-1', aud: 'client-1' },
      privateKey,
      { algorithm: 'RS256', issuer: 'issuer.example.test', expiresIn: '1h' },
    );

    const claims = native.verifyIdToken(token, JSON.stringify(jwk), {
      issuer: 'issuer.example.test',
      audience: 'client-1',
      maxAgeSeconds: 3600,
    });

    expect(claims.sub).toBe('user-1');
    expect(claims.aud).toBe('client-1');
    expect(claims.iss).toBe('issuer.example.test');
  });

  it('getOdyCrypto prefers native over tsFallback', () => {
    const crypto = getOdyCrypto();
    if (native === null) {
      expect(crypto).toBe(tsFallback);
    } else {
      expect(crypto).toBe(native);
    }
  });
});
