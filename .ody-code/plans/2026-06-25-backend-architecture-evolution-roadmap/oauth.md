# Part 3: MCP OAuth 集成

本 Part 让 MCP OAuth service 使用 `ody-crypto` 生成 PKCE/state，手动构造授权 URL，并用 SDK 的 `exchangeAuthorization` 换 token；当 token 响应含 `id_token` 时做 JWT 校验。

## 依赖关系

```text
Task 6 -> Task 9
Task 6 -> Task 10
Task 9 -> Task 11
Task 10 -> Task 11   (Task 11 的 id-token 校验依赖 provider.state / codeVerifier 已就绪)
```

---

### Task 9: 改造 `McpOAuthService.beginAuthorization` 为手动 PKCE URL + `exchangeAuthorization`

**Depends on:** Task 6

**Files:**
- Modify: `packages/mcp-host/package.json`
- Modify: `packages/mcp-host/src/oauth/service.ts`
- Create: `packages/mcp-host/test/oauth/service.test.ts`

**步骤：**

- [ ] 在 `packages/mcp-host/package.json` 的 `dependencies` 中追加：

```json
    "@odysseythink/ody-crypto": "workspace:^",
```

- [ ] 把 `packages/mcp-host/src/oauth/service.ts` 中对 SDK 的导入替换为：

```ts
import {
  discoverOAuthServerInfo,
  exchangeAuthorization,
  registerClient,
  type OAuthClientProvider,
} from '@modelcontextprotocol/sdk/client/auth.js';
import { getOdyCrypto } from '@odysseythink/ody-crypto';
```

- [ ] 重写 `beginAuthorization` 中生成授权 URL 与 `complete` 中换 token 的逻辑（保留 callback server、provider 缓存、错误包装）：

```ts
    provider.setRedirectUrl(new URL(callbackServer.redirectUri));

    if (provider.tokens() !== undefined) {
      await callbackServer.close();
      throw new AlreadyAuthorizedError(serverName);
    }

    const {
      authorizationServerUrl,
      authorizationServerMetadata: metadata,
      resourceMetadata,
    } = await discoverOAuthServerInfo(serverUrl, {});

    await provider.saveDiscoveryState({
      authorizationServerUrl: String(authorizationServerUrl),
      resourceMetadata,
      authorizationServerMetadata: metadata,
    });

    const resolvedScope =
      resourceMetadata?.scopes_supported?.join(' ') ?? provider.clientMetadata.scope;

    let clientInformation = provider.clientInformation();
    if (clientInformation === undefined) {
      const registered = await registerClient(authorizationServerUrl, {
        metadata,
        clientMetadata: provider.clientMetadata,
        scope: resolvedScope,
      });
      provider.saveClientInformation(registered);
      clientInformation = registered;
    }

    const crypto = getOdyCrypto();
    const challenge = crypto.pkceChallenge();
    const state = provider.state();
    provider.saveCodeVerifier(challenge.codeVerifier);

    const authorizationUrl = metadata?.authorization_endpoint
      ? new URL(metadata.authorization_endpoint)
      : new URL('/authorize', authorizationServerUrl);
    authorizationUrl.searchParams.set('response_type', 'code');
    authorizationUrl.searchParams.set('client_id', clientInformation.client_id);
    authorizationUrl.searchParams.set('code_challenge', challenge.codeChallenge);
    authorizationUrl.searchParams.set('code_challenge_method', 'S256');
    authorizationUrl.searchParams.set('redirect_uri', String(provider.redirectUrl));
    authorizationUrl.searchParams.set('state', state);
    if (resolvedScope) {
      authorizationUrl.searchParams.set('scope', resolvedScope);
    }
    if (resourceMetadata?.resource) {
      authorizationUrl.searchParams.set('resource', resourceMetadata.resource);
    }

    provider.redirectToAuthorization(authorizationUrl);
    authorizationUrl = provider.takeAuthorizationUrl() ?? authorizationUrl;
```

- [ ] 把 `complete` 中的第二次 `auth()` 调用替换为 `exchangeAuthorization`：

```ts
        const expectedState = provider.expectedState();
        if (expectedState !== undefined && state !== expectedState) {
          throw new Error('OAuth state mismatch — possible CSRF; refusing token exchange');
        }
        const tokens = await exchangeAuthorization(authorizationServerUrl, {
          metadata,
          clientInformation,
          authorizationCode: code,
          codeVerifier: provider.codeVerifier(),
          redirectUri: provider.redirectUrl,
          resource: resourceMetadata?.resource ? new URL(resourceMetadata.resource) : undefined,
        });
        await provider.saveTokens(tokens);
```

- [ ] 在 `beginAuthorization` 中保存 `authorizationServerUrl`、`metadata`、`clientInformation`、`resourceMetadata` 到闭包，供 `complete` 使用（直接替换原 `auth()` 相关逻辑）。

- [ ] 写测试 `packages/mcp-host/test/oauth/service.test.ts`：

```ts
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
      } as Awaited<ReturnType<typeof discoverOAuthServerInfo>>['authorizationServerMetadata'],
      resourceMetadata: undefined,
    });
    vi.mocked(registerClient).mockResolvedValue({
      client_id: 'client-123',
    } as Awaited<ReturnType<typeof registerClient>>);
    vi.mocked(exchangeAuthorization).mockResolvedValue({
      access_token: 'tok',
      token_type: 'Bearer',
    });

    const service = new McpOAuthService({ store: makeStore() });
    const flow = await service.beginAuthorization('srv', 'https://mcp.example/');
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
      } as Awaited<ReturnType<typeof discoverOAuthServerInfo>>['authorizationServerMetadata'],
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
```

- [ ] 先运行测试并确认失败：

```bash
pnpm --filter @odysseythink/ody-crypto run build
pnpm --filter @modelcontextprotocol/mcp-host test
```

预期：`service.test.ts` 中 `beginAuthorization` 尚未返回包含 PKCE 参数的 URL，断言失败。

- [ ] 应用上述 service.ts 改动后再次运行测试，确认通过。
- [ ] Commit：`git add packages/mcp-host/src/oauth/service.ts packages/mcp-host/test/oauth/service.test.ts packages/mcp-host/package.json && git commit -m "feat(mcp-host): manual PKCE authorization URL with exchangeAuthorization"`。

---

### Task 10: `McpOAuthClientProvider.state()` 改用 `ody-crypto.randomBytes`

**Depends on:** Task 6

**Files:**
- Modify: `packages/mcp-host/src/oauth/provider.ts`
- Create: `packages/mcp-host/test/oauth/provider.test.ts`

**步骤：**

- [ ] 修改 `packages/mcp-host/src/oauth/provider.ts`：

```ts
// 删除原有 import
// import { randomBytes } from 'node:crypto';

// 新增
import { getOdyCrypto } from '@odysseythink/ody-crypto';
```

- [ ] 替换 `state()` 实现：

```ts
  state(): string {
    this._state ??= getOdyCrypto().randomBytes(16).toString('hex');
    return this._state;
  }
```

- [ ] 写测试 `packages/mcp-host/test/oauth/provider.test.ts`：

```ts
import { describe, expect, it } from 'vitest';
import { JsonFileStore } from '../../src/oauth/store';
import { McpOAuthClientProvider } from '../../src/oauth/provider';

function makeProvider(): McpOAuthClientProvider {
  const store = new JsonFileStore('/tmp/ody-mcp-provider-test-' + Math.random().toString(36).slice(2));
  return new McpOAuthClientProvider({ serverName: 'srv', serverUrl: 'https://mcp.example/', store });
}

describe('McpOAuthClientProvider.state', () => {
  it('returns a 32-character hex string (16 bytes)', () => {
    const provider = makeProvider();
    const state = provider.state();
    expect(state).toMatch(/^[0-9a-f]{32}$/);
  });

  it('returns the same state on repeated calls', () => {
    const provider = makeProvider();
    expect(provider.state()).toBe(provider.state());
  });

  it('produces different states across providers', () => {
    const a = makeProvider().state();
    const b = makeProvider().state();
    expect(a).not.toBe(b);
  });
});
```

- [ ] 运行测试：

```bash
pnpm --filter @modelcontextprotocol/mcp-host test
```

- [ ] 预期通过；Commit：`git add packages/mcp-host/src/oauth/provider.ts packages/mcp-host/test/oauth/provider.test.ts && git commit -m "feat(mcp-host): use ody-crypto for OAuth state"`。

---

### Task 11: 新增 id_token 校验模块并接入 service 完成流程

**Depends on:** Task 9, Task 10

**Files:**
- Create: `packages/mcp-host/src/oauth/id-token.ts`
- Modify: `packages/mcp-host/src/oauth/service.ts`
- Modify: `packages/mcp-host/test/oauth/service.test.ts`

**步骤：**

- [ ] 创建 `packages/mcp-host/src/oauth/id-token.ts`：

```ts
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
  const decoded = Buffer.from(parts[0], 'base64url').toString('utf-8');
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
  const match = keys.find((k) => typeof k === 'object' && k !== null && (k as Record<string, unknown>).kid === kid);
  if (match === undefined) {
    throw new Error(`JWKS does not contain key with kid ${kid}`);
  }
  return match;
}

export async function verifyIdToken(context: IdTokenVerificationContext): Promise<IdTokenClaims> {
  const { idToken, authorizationServerUrl, authorizationServerMetadata, clientId } = context;
  const header = parseJwtHeader(idToken);
  const kid = header.kid as string | undefined;
  const keys = await fetchJwks(authorizationServerMetadata);
  const key = findKey(keys, kid);
  const expectedIssuer = authorizationServerMetadata.issuer ?? authorizationServerUrl;
  return getOdyCrypto().verifyIdToken(idToken, JSON.stringify(key), {
    issuer: expectedIssuer,
    audience: clientId,
  });
}
```

- [ ] 修改 `packages/mcp-host/src/oauth/service.ts`：
  - 在文件顶部导入 `import { verifyIdToken } from './id-token';`。
  - 在 `complete` 中 `provider.saveTokens(tokens)` 之前插入：

```ts
        if (tokens.id_token !== undefined) {
          await verifyIdToken({
            idToken: tokens.id_token,
            authorizationServerUrl,
            authorizationServerMetadata: metadata,
            clientId: clientInformation.client_id,
          });
        }
```

- [ ] 扩展 `packages/mcp-host/test/oauth/service.test.ts`，追加 id_token 测试：

```ts
import jwt from 'jsonwebtoken';
import { generateKeyPairSync } from 'node:crypto';

function makeIdToken(overrides: { exp?: number; aud?: string } = {}): { idToken: string; jwk: unknown } {
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const now = Math.floor(Date.now() / 1000);
  const idToken = jwt.sign(
    {
      sub: 'user-42',
      iss: 'https://auth.example/',
      aud: overrides.aud ?? 'client-123',
      exp: overrides.exp ?? now + 3600,
      iat: now,
    },
    privateKey,
    { algorithm: 'RS256', keyid: 'key-1' },
  );
  const jwk = publicKey.export({ format: 'jwk' });
  return { idToken, jwk };
}

it('verifies id_token and saves tokens when present', async () => {
  const { idToken, jwk } = makeIdToken();
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

  const service = new McpOAuthService({ store: makeStore() });
  const flow = await service.beginAuthorization('srv', 'https://mcp.example/');
  await flow.complete();

  expect(service.hasTokens('srv', 'https://mcp.example/')).toBe(true);
});

it('rejects tokens when id_token signature is invalid', async () => {
  const { idToken, jwk } = makeIdToken();
  const tampered = idToken.slice(0, -5) + 'XXXXX';
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

  const service = new McpOAuthService({ store: makeStore() });
  const flow = await service.beginAuthorization('srv', 'https://mcp.example/');
  await expect(flow.complete()).rejects.toThrow();
  expect(service.hasTokens('srv', 'https://mcp.example/')).toBe(false);
});
```

- [ ] 运行测试：

```bash
pnpm --filter @odysseythink/ody-crypto run build
pnpm --filter @modelcontextprotocol/mcp-host test
```

- [ ] 预期：新增 id_token 测试通过，原有测试仍通过。
- [ ] 运行全 workspace 类型检查：

```bash
pnpm run typecheck
```

- [ ] Commit：`git add packages/mcp-host/src/oauth/id-token.ts packages/mcp-host/src/oauth/service.ts packages/mcp-host/test/oauth/service.test.ts && git commit -m "feat(mcp-host): verify id_token during OAuth completion"`。

---

## Local Self-Review

- [ ] 1. Spec-coverage table：本 Part 覆盖 service 手动 PKCE URL、provider state 使用 native randomBytes、id_token 校验与失败拒绝。
- [ ] 2. Placeholder scan：所有代码/命令已给出，无 `TODO`/`TBD`。
- [ ] 3. No phantom tasks：每个 task 都有可验证的测试或产物。
- [ ] 4. Dependency soundness：Task 9 依赖 Task 6 的 `getOdyCrypto`；Task 10 依赖 Task 6；Task 11 依赖 Task 9/10 的 service/provider 改动。
- [ ] 5. Caller & build soundness：Task 9 修改了 `packages/mcp-host/package.json` 新增 `@odysseythink/ody-crypto` 依赖，需运行 `pnpm install`；Task 11 以全 workspace `pnpm run typecheck` 验证。`McpOAuthService` 公共接口（`beginAuthorization`、`hasTokens`、`invalidate`）未变，无需更新外部调用者。
- [ ] 6. Test-the-risk：state 长度/唯一性、PKCE 参数、token 保存、id_token 有效/篡改均有断言；id_token 篡改后 `hasTokens` 为 `false` 验证安全失败。
- [ ] 7. Type consistency：`verifyIdToken` 使用的 `IdTokenClaims`/`IdTokenExpected` 来自 `@odysseythink/ody-crypto`，与 Part 2 定义一致；`audience` 使用 `clientInformation.client_id`，与 `issuer` 使用 metadata.issuer 或 auth server URL 对齐设计。
