export interface PkceChallenge {
  readonly codeVerifier: string;
  readonly codeChallenge: string;
}

export interface IdTokenExpected {
  readonly issuer: string;
  readonly audience: string;
  readonly maxAgeSeconds?: number;
}

export interface IdTokenClaims {
  readonly sub: string;
  readonly iss: string;
  readonly aud: string | string[];
  readonly exp: number;
  readonly iat: number;
  readonly [claim: string]: unknown;
}

export interface OdyCrypto {
  randomBytes(length: number): Buffer;
  sha256(input: string | Buffer): string;
  pkceChallenge(length?: number): PkceChallenge;
  verifyIdToken(jwt: string, jwkJson: string, expected: IdTokenExpected): IdTokenClaims;
}
