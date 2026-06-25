import { createHash, createPublicKey, randomBytes as nodeRandomBytes } from 'node:crypto';
import jwt from 'jsonwebtoken';

import type { IdTokenClaims, IdTokenExpected, OdyCrypto, PkceChallenge } from './types';

const PKCE_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~';

function randomBytes(length: number): Buffer {
  return nodeRandomBytes(length);
}

function sha256(input: string | Buffer): string {
  return createHash('sha256').update(input).digest('hex');
}

function pkceChallenge(length = 43): PkceChallenge {
  if (length < 43 || length > 128) {
    throw new RangeError(`PKCE verifier length ${length} out of range [43, 128]`);
  }
  let verifier = '';
  const bytes = randomBytes(length);
  for (let i = 0; i < length; i++) {
    verifier += PKCE_ALPHABET[bytes[i]! % PKCE_ALPHABET.length];
  }
  const challenge = createHash('sha256').update(verifier).digest('base64url');
  return { codeVerifier: verifier, codeChallenge: challenge };
}

function verifyIdToken(jwtString: string, jwkJson: string, expected: IdTokenExpected): IdTokenClaims {
  const jwk = JSON.parse(jwkJson) as Record<string, unknown>;
  const key = createPublicKey({ key: jwk, format: 'jwk' });
  const payload = jwt.verify(jwtString, key, {
    algorithms: ['RS256', 'ES256'],
    issuer: expected.issuer,
    audience: expected.audience,
    maxAge: expected.maxAgeSeconds,
  }) as Record<string, unknown>;
  return payload as IdTokenClaims;
}

export const tsFallback: OdyCrypto = { randomBytes, sha256, pkceChallenge, verifyIdToken };
