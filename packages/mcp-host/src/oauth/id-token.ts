import { getOdyCrypto } from '@odysseythink/ody-crypto';
import type { IdTokenClaims } from '@odysseythink/ody-crypto';

export interface IdTokenVerificationContext {
  readonly idToken: string;
  readonly authorizationServerUrl: string;
  readonly authorizationServerMetadata: {
    readonly issuer?: string;
    readonly jwks_uri?: string;
    readonly jwks?: { readonly keys: readonly unknown[] };
  };
  readonly clientId: string;
}

function parseJwtHeader(jwt: string): Record<string, unknown> {
  const parts = jwt.split('.');
  if (parts.length !== 3) {
    throw new Error('id_token is not a valid JWT');
  }
  const decoded = Buffer.from(parts[0]!, 'base64url').toString('utf-8');
  return JSON.parse(decoded) as Record<string, unknown>;
}

async function fetchJwks(metadata: IdTokenVerificationContext['authorizationServerMetadata']): Promise<unknown[]> {
  if (metadata.jwks?.keys) {
    return [...metadata.jwks.keys];
  }
  if (!metadata.jwks_uri) {
    throw new Error('authorization server metadata has no jwks_uri or jwks');
  }
  const response = await fetch(metadata.jwks_uri);
  if (!response.ok) {
    throw new Error(`failed to fetch JWKS: ${response.status} ${response.statusText}`);
  }
  const body = (await response.json()) as { keys?: unknown[] };
  return body.keys ?? [];
}

function findKey(keys: unknown[], kid?: string): unknown {
  if (kid === undefined) {
    return keys[0];
  }
  const match = keys.find((k) => typeof k === 'object' && k !== null && (k as Record<string, unknown>)['kid'] === kid);
  if (match === undefined) {
    throw new Error(`JWKS does not contain key with kid ${kid}`);
  }
  return match;
}

export async function verifyIdToken(context: IdTokenVerificationContext): Promise<IdTokenClaims> {
  const { idToken, authorizationServerUrl, authorizationServerMetadata, clientId } = context;
  const header = parseJwtHeader(idToken);
  const kid = header['kid'] as string | undefined;
  const keys = await fetchJwks(authorizationServerMetadata);
  const key = findKey(keys, kid);
  const expectedIssuer = authorizationServerMetadata.issuer ?? authorizationServerUrl;
  return getOdyCrypto().verifyIdToken(idToken, JSON.stringify(key), {
    issuer: expectedIssuer,
    audience: clientId,
  });
}
