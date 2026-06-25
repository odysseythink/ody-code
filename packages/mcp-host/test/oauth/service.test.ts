import { describe, expect, it, vi, beforeEach } from 'vitest';
import {
  discoverOAuthServerInfo,
  exchangeAuthorization,
  registerClient,
} from '@modelcontextprotocol/sdk/client/auth.js';
import { McpOAuthService } from '../../src/oauth/service';
import { JsonFileStore } from '../../src/oauth/store';

vi.mock('@modelcontextprotocol/sdk/client/auth.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@modelcontextprotocol/sdk/client/auth.js')>();
  return {
    ...actual,
    discoverOAuthServerInfo: vi.fn(),
    registerClient: vi.fn(),
    exchangeAuthorization: vi.fn(),
  };
});

vi.mock('../../src/oauth/callback-server', () => ({
  startCallbackServer: vi.fn(async () => {
    const redirectUri = 'http://127.0.0.1:3118/callback';
    return {
      redirectUri,
      waitForCode: vi.fn().mockResolvedValue({ code: 'auth-code', state: 'flow-state' }),
      close: vi.fn().mockResolvedValue(undefined),
    };
  }),
}));

function makeStore(): JsonFileStore {
  return new JsonFileStore('/tmp/ody-mcp-test-' + Math.random().toString(36).slice(2));
}

describe('McpOAuthService.beginAuthorization', () => {
  beforeEach(() => {
    vi.mocked(discoverOAuthServerInfo).mockReset();
    vi.mocked(registerClient).mockReset();
    vi.mocked(exchangeAuthorization).mockReset();
  });

  it('authorization url contains s256 challenge and state', async () => {
    vi.mocked(discoverOAuthServerInfo).mockResolvedValue({
      authorizationServerUrl: 'https://auth.example/',
      authorizationServerMetadata: {
        authorization_endpoint: 'https://auth.example/authorize',
      } as unknown as Awaited<ReturnType<typeof discoverOAuthServerInfo>>['authorizationServerMetadata'],
      resourceMetadata: undefined,
    });
    vi.mocked(registerClient).mockResolvedValue({
      client_id: 'client-123',
    } as Awaited<ReturnType<typeof registerClient>>);

    const service = new McpOAuthService({ store: makeStore() });
    const result = await service.beginAuthorization('srv', 'https://mcp.example/');

    const url = result.authorizationUrl;
    expect(url.searchParams.get('code_challenge_method')).toBe('S256');
    expect(url.searchParams.get('code_challenge')).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(url.searchParams.get('client_id')).toBe('client-123');
    expect(url.searchParams.get('state')).toBeTruthy();
  });

  it('complete exchanges code and saves tokens', async () => {
    vi.mocked(discoverOAuthServerInfo).mockResolvedValue({
      authorizationServerUrl: 'https://auth.example/',
      authorizationServerMetadata: {
        authorization_endpoint: 'https://auth.example/authorize',
      } as unknown as Awaited<ReturnType<typeof discoverOAuthServerInfo>>['authorizationServerMetadata'],
      resourceMetadata: undefined,
    });
    vi.mocked(registerClient).mockResolvedValue({
      client_id: 'client-123',
    } as Awaited<ReturnType<typeof registerClient>>);
    vi.mocked(exchangeAuthorization).mockResolvedValue({
      access_token: 'tok',
      token_type: 'Bearer',
    });

    // Create a deferred promise BEFORE beginAuthorization so the mock waitForCode
    // returns a promise we can later resolve with the provider's actual state.
    let resolveWaitForCode!: (value: { code: string; state: string | undefined }) => void;
    const waitForCodePromise = new Promise<{ code: string; state: string | undefined }>(
      (resolve) => {
        resolveWaitForCode = resolve;
      },
    );

    const { startCallbackServer } = await import('../../src/oauth/callback-server');
    vi.mocked(startCallbackServer).mockResolvedValueOnce({
      redirectUri: 'http://127.0.0.1:3118/callback',
      waitForCode: vi.fn(async () => await waitForCodePromise),
      close: vi.fn().mockResolvedValue(undefined),
    } as unknown as Awaited<ReturnType<typeof startCallbackServer>>);

    const service = new McpOAuthService({ store: makeStore() });
    const flow = await service.beginAuthorization('srv', 'https://mcp.example/');

    // Resolve the deferred promise with the provider's actual expected state
    const provider = service.getProvider('srv', 'https://mcp.example/');
    resolveWaitForCode({ code: 'auth-code', state: provider.expectedState()! });

    await flow.complete();

    expect(service.hasTokens('srv', 'https://mcp.example/')).toBe(true);
    expect(vi.mocked(exchangeAuthorization)).toHaveBeenCalledWith(
      'https://auth.example/',
      expect.objectContaining({
        authorizationCode: 'auth-code',
        codeVerifier: expect.any(String),
        redirectUri: 'http://127.0.0.1:3118/callback',
      }),
    );
  });

  it('throws on state mismatch', async () => {
    vi.mocked(discoverOAuthServerInfo).mockResolvedValue({
      authorizationServerUrl: 'https://auth.example/',
      authorizationServerMetadata: {
        authorization_endpoint: 'https://auth.example/authorize',
      } as unknown as Awaited<ReturnType<typeof discoverOAuthServerInfo>>['authorizationServerMetadata'],
      resourceMetadata: undefined,
    });
    vi.mocked(registerClient).mockResolvedValue({
      client_id: 'client-123',
    } as Awaited<ReturnType<typeof registerClient>>);

    const { startCallbackServer } = await import('../../src/oauth/callback-server');
    vi.mocked(startCallbackServer).mockResolvedValueOnce({
      redirectUri: 'http://127.0.0.1:3118/callback',
      waitForCode: vi.fn().mockResolvedValue({ code: 'auth-code', state: 'wrong-state' }),
      close: vi.fn().mockResolvedValue(undefined),
    } as unknown as Awaited<ReturnType<typeof startCallbackServer>>);

    const service = new McpOAuthService({ store: makeStore() });
    const flow = await service.beginAuthorization('srv', 'https://mcp.example/');
    await expect(flow.complete()).rejects.toThrow(/state mismatch/i);
  });
});

import jwt from 'jsonwebtoken';
import { generateKeyPairSync } from 'node:crypto';

function makeIdToken(overrides: { exp?: number; aud?: string } = {}): { idToken: string; jwk: Record<string, unknown> } {
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const now = Math.floor(Date.now() / 1000);
  const kid = 'key-1';
  const idToken = jwt.sign(
    {
      sub: 'user-42',
      iss: 'https://auth.example/',
      aud: overrides.aud ?? 'client-123',
      exp: overrides.exp ?? now + 3600,
      iat: now,
    },
    privateKey,
    { algorithm: 'RS256', keyid: kid },
  );
  const jwk = publicKey.export({ format: 'jwk' }) as Record<string, unknown>;
  jwk.kid = kid;
  return { idToken, jwk };
}

it('verifies id_token and saves tokens when present', async () => {
  const { idToken, jwk } = makeIdToken();

  let resolveWaitForCode!: (value: { code: string; state: string | undefined }) => void;
  const waitForCodePromise = new Promise<{ code: string; state: string | undefined }>(
    (resolve) => {
      resolveWaitForCode = resolve;
    },
  );

  vi.mocked(discoverOAuthServerInfo).mockResolvedValue({
    authorizationServerUrl: 'https://auth.example/',
    authorizationServerMetadata: {
      authorization_endpoint: 'https://auth.example/authorize',
      issuer: 'https://auth.example/',
      jwks_uri: 'https://auth.example/.well-known/jwks.json',
    } as Awaited<ReturnType<typeof discoverOAuthServerInfo>>['authorizationServerMetadata'],
    resourceMetadata: undefined,
  });
  vi.mocked(registerClient).mockResolvedValue({
    client_id: 'client-123',
  } as Awaited<ReturnType<typeof registerClient>>);
  vi.mocked(exchangeAuthorization).mockResolvedValue({
    access_token: 'tok',
    token_type: 'Bearer',
    id_token: idToken,
  });
  vi.stubGlobal(
    'fetch',
    vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ keys: [jwk] }),
    }),
  );

  const { startCallbackServer } = await import('../../src/oauth/callback-server');
  vi.mocked(startCallbackServer).mockResolvedValueOnce({
    redirectUri: 'http://127.0.0.1:3118/callback',
    waitForCode: vi.fn(async () => await waitForCodePromise),
    close: vi.fn().mockResolvedValue(undefined),
  } as unknown as Awaited<ReturnType<typeof startCallbackServer>>);

  const service = new McpOAuthService({ store: makeStore() });
  const flow = await service.beginAuthorization('srv', 'https://mcp.example/');

  const provider = service.getProvider('srv', 'https://mcp.example/');
  resolveWaitForCode({ code: 'auth-code', state: provider.expectedState()! });

  await flow.complete();

  expect(service.hasTokens('srv', 'https://mcp.example/')).toBe(true);
});

it('rejects tokens when id_token signature is invalid', async () => {
  const { idToken, jwk } = makeIdToken();
  const tampered = idToken.slice(0, -5) + 'XXXXX';

  let resolveWaitForCode!: (value: { code: string; state: string | undefined }) => void;
  const waitForCodePromise = new Promise<{ code: string; state: string | undefined }>(
    (resolve) => {
      resolveWaitForCode = resolve;
    },
  );

  vi.mocked(discoverOAuthServerInfo).mockResolvedValue({
    authorizationServerUrl: 'https://auth.example/',
    authorizationServerMetadata: {
      authorization_endpoint: 'https://auth.example/authorize',
      issuer: 'https://auth.example/',
      jwks_uri: 'https://auth.example/.well-known/jwks.json',
    } as Awaited<ReturnType<typeof discoverOAuthServerInfo>>['authorizationServerMetadata'],
    resourceMetadata: undefined,
  });
  vi.mocked(registerClient).mockResolvedValue({
    client_id: 'client-123',
  } as Awaited<ReturnType<typeof registerClient>>);
  vi.mocked(exchangeAuthorization).mockResolvedValue({
    access_token: 'tok',
    token_type: 'Bearer',
    id_token: tampered,
  });
  vi.stubGlobal(
    'fetch',
    vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ keys: [jwk] }),
    }),
  );

  const { startCallbackServer } = await import('../../src/oauth/callback-server');
  vi.mocked(startCallbackServer).mockResolvedValueOnce({
    redirectUri: 'http://127.0.0.1:3118/callback',
    waitForCode: vi.fn(async () => await waitForCodePromise),
    close: vi.fn().mockResolvedValue(undefined),
  } as unknown as Awaited<ReturnType<typeof startCallbackServer>>);

  const service = new McpOAuthService({ store: makeStore() });
  const flow = await service.beginAuthorization('srv', 'https://mcp.example/');

  const provider = service.getProvider('srv', 'https://mcp.example/');
  resolveWaitForCode({ code: 'auth-code', state: provider.expectedState()! });

  await expect(flow.complete()).rejects.toThrow();
  expect(service.hasTokens('srv', 'https://mcp.example/')).toBe(false);
});
