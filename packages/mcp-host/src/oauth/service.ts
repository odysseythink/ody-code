/**
 * Per-process OAuth orchestrator for MCP HTTP servers.
 *
 * The service owns one {@link McpOAuthClientProvider} per server/resource and
 * mediates the synthetic `mcp__<server>__authenticate` tool flow:
 *
 *  1. `getProvider(serverName, serverUrl)` returns the cached provider.
 *     `HttpMcpClient` hands this to `StreamableHTTPClientTransport.authProvider`
 *     only when the server has no static bearer token configured **and** the
 *     provider has stored tokens for that same server URL — first-time
 *     connections that lack tokens skip the provider entirely so a 401 surfaces
 *     as `UnauthorizedError` from the transport instead of being swallowed by an
 *     in-flight `auth()` attempt.
 *  2. `beginAuthorization(serverName, serverUrl)` spins up a one-shot
 *     localhost callback listener, sets the redirect URL on the provider,
 *     discovers OAuth server metadata, registers the client (if not already
 *     registered), generates the PKCE challenge and state, manually constructs
 *     the authorization URL, and returns that URL plus a `complete()` callback
 *     that validates state and performs the code exchange.
 *  3. After `complete()` resolves successfully the provider has tokens on
 *     disk; the caller (the synthetic tool) drives a manager-level
 *     `reconnect` to swap the synthetic tool out for the real MCP tools.
 */

import {
  discoverOAuthServerInfo,
  exchangeAuthorization,
  registerClient,
  type OAuthClientProvider,
} from '@modelcontextprotocol/sdk/client/auth.js';
import type { OAuthClientInformationMixed } from '@modelcontextprotocol/sdk/shared/auth.js';
import { getOdyCrypto } from '@odysseythink/ody-crypto';

import { verifyIdToken } from './id-token';
import { startCallbackServer, type CallbackServer } from './callback-server';
import { McpOAuthClientProvider } from './provider';
import { JsonFileStore, mcpCredentialsDir, mcpOAuthStoreKey } from './store';

export interface McpOAuthServiceOptions {
  /** Storage backend; overrides `kimiHomeDir` when supplied. */
  readonly store?: JsonFileStore;
  /** Resolved Kimi home; credentials default to `<kimiHomeDir>/credentials/mcp/`. */
  readonly kimiHomeDir?: string;
  /** Override for the label embedded in DCR `client_name`. */
  readonly clientLabel?: string;
}

export interface BeginAuthorizationOptions {
  /** Override the `client_name` embedded in the DCR registration request. */
  readonly clientLabel?: string;
}

export interface BeginAuthorizationResult {
  /** The authorization URL the user must open in their browser. */
  readonly authorizationUrl: URL;
  /**
   * Awaits the OAuth callback, validates `state`, exchanges the code for
   * tokens, and persists them via the provider. Resolves on success;
   * rejects on abort, timeout, or auth-server error.
   */
  complete(opts?: { signal?: AbortSignal; timeoutMs?: number }): Promise<void>;
  /**
   * Tears down the callback listener without finishing the flow. Safe to
   * call repeatedly; called automatically by `complete()`.
   */
  cancel(): Promise<void>;
}

type OAuthDiscoveryResult = Awaited<ReturnType<typeof discoverOAuthServerInfo>>;

export class McpOAuthService {
  private readonly store: JsonFileStore;
  private readonly clientLabel: string | undefined;
  private readonly providers = new Map<string, McpOAuthClientProvider>();

  constructor(options: McpOAuthServiceOptions = {}) {
    this.store =
      options.store ??
      new JsonFileStore(
        options.kimiHomeDir === undefined ? undefined : mcpCredentialsDir(options.kimiHomeDir),
      );
    this.clientLabel = options.clientLabel;
  }

  /** Returns the cached provider for `serverName` + `serverUrl`, constructing it on first use. */
  getProvider(serverName: string, serverUrl: string | URL): McpOAuthClientProvider {
    const storeKey = mcpOAuthStoreKey(serverName, serverUrl);
    let provider = this.providers.get(storeKey);
    if (provider === undefined) {
      provider = new McpOAuthClientProvider({
        serverName,
        serverUrl,
        store: this.store,
        clientLabel: this.clientLabel,
      });
      this.providers.set(provider.storeKey, provider);
    }
    return provider;
  }

  /** True once the provider has persisted tokens for this server/resource identity. */
  hasTokens(serverName: string, serverUrl: string | URL): boolean {
    return this.getProvider(serverName, serverUrl).tokens() !== undefined;
  }

  /**
   * Discover OAuth server metadata, register the client (if not already
   * registered), generate PKCE challenge and state, and construct the
   * authorization URL manually. Returns the URL plus a `complete()` callback
   * that validates state and exchanges the code for tokens.
   */
  async beginAuthorization(
    serverName: string,
    serverUrl: string | URL,
    options: BeginAuthorizationOptions = {},
  ): Promise<BeginAuthorizationResult> {
    const provider = options.clientLabel === undefined
      ? this.getProvider(serverName, serverUrl)
      : new McpOAuthClientProvider({
          serverName,
          serverUrl,
          store: this.store,
          clientLabel: options.clientLabel,
        });
    if (options.clientLabel !== undefined) {
      this.providers.set(provider.storeKey, provider);
    }

    provider.resetFlow();

    let callbackServer: CallbackServer;
    try {
      callbackServer = await startCallbackServer();
    } catch (error) {
      throw wrapAuthError('failed to start OAuth callback listener', error);
    }

    provider.setRedirectUrl(new URL(callbackServer.redirectUri));

    // Captured in closure for complete()
    let authorizationServerUrl!: string;
    let metadata!: OAuthDiscoveryResult['authorizationServerMetadata'];
    let resourceMetadata!: OAuthDiscoveryResult['resourceMetadata'];
    let clientInformation!: OAuthClientInformationMixed;

    let authorizationUrl: URL | undefined;
    try {
      if (provider.tokens() !== undefined) {
        await callbackServer.close();
        throw new AlreadyAuthorizedError(serverName);
      }

      const discovery = await discoverOAuthServerInfo(serverUrl, {});
      authorizationServerUrl = discovery.authorizationServerUrl;
      metadata = discovery.authorizationServerMetadata;
      resourceMetadata = discovery.resourceMetadata;

      await provider.saveDiscoveryState({
        authorizationServerUrl: String(authorizationServerUrl),
        resourceMetadata,
        authorizationServerMetadata: metadata,
      });

      const resolvedScope =
        resourceMetadata?.scopes_supported?.join(' ') ?? provider.clientMetadata.scope;

      let existingClient = provider.clientInformation();
      if (existingClient === undefined) {
        existingClient = await registerClient(authorizationServerUrl, {
          metadata,
          clientMetadata: provider.clientMetadata,
          scope: resolvedScope,
        });
        provider.saveClientInformation(existingClient);
      }
      clientInformation = existingClient;

      const crypto = getOdyCrypto();
      const challenge = crypto.pkceChallenge();
      const state = provider.state();
      provider.saveCodeVerifier(challenge.codeVerifier);

      const authUrl = metadata?.authorization_endpoint
        ? new URL(metadata.authorization_endpoint)
        : new URL('/authorize', authorizationServerUrl);
      authUrl.searchParams.set('response_type', 'code');
      authUrl.searchParams.set('client_id', clientInformation.client_id);
      authUrl.searchParams.set('code_challenge', challenge.codeChallenge);
      authUrl.searchParams.set('code_challenge_method', 'S256');
      authUrl.searchParams.set('redirect_uri', String(provider.redirectUrl));
      authUrl.searchParams.set('state', state);
      if (resolvedScope) {
        authUrl.searchParams.set('scope', resolvedScope);
      }
      if (resourceMetadata?.resource) {
        authUrl.searchParams.set('resource', resourceMetadata.resource);
      }

      provider.redirectToAuthorization(authUrl);
      authorizationUrl = provider.takeAuthorizationUrl() ?? authUrl;
    } catch (error) {
      await callbackServer.close().catch(() => undefined);
      provider.resetFlow();
      if (error instanceof AlreadyAuthorizedError) throw error;
      throw wrapAuthError(`failed to start OAuth flow for "${serverName}"`, error);
    }

    let settled = false;
    const cancel = async (): Promise<void> => {
      if (settled) return;
      settled = true;
      await callbackServer.close().catch(() => undefined);
      provider.resetFlow();
    };

    const complete: BeginAuthorizationResult['complete'] = async (opts = {}) => {
      if (settled) {
        throw new Error('OAuth flow already completed or cancelled');
      }
      try {
        const { code, state } = await callbackServer.waitForCode({
          signal: opts.signal,
          timeoutMs: opts.timeoutMs,
        });
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
        if (tokens.id_token !== undefined) {
          await verifyIdToken({
            idToken: tokens.id_token,
            authorizationServerUrl: String(authorizationServerUrl),
            authorizationServerMetadata: metadata!,
            clientId: clientInformation.client_id,
          });
        }
        await provider.saveTokens(tokens);
      } catch (error) {
        await cancel();
        throw wrapAuthError(`OAuth flow for "${serverName}" failed`, error);
      }
      settled = true;
      await callbackServer.close().catch(() => undefined);
      provider.resetFlow();
    };

    return { authorizationUrl, complete, cancel };
  }

  /**
   * Clear stored credentials for a server. Use `'all'` after the user
   * explicitly signs out; use `'tokens'` to force a re-auth while keeping
   * the registered DCR client.
   */
  invalidate(
    serverName: string,
    serverUrl: string | URL,
    scope: 'all' | 'client' | 'tokens' | 'discovery' = 'all',
  ): void {
    this.getProvider(serverName, serverUrl).invalidateCredentials(scope);
  }
}

/** Thrown by `beginAuthorization` when stored tokens already satisfy the server. */
export class AlreadyAuthorizedError extends Error {
  constructor(serverName: string) {
    super(`"${serverName}" is already authorized; no browser flow needed`);
    this.name = 'AlreadyAuthorizedError';
  }
}

function wrapAuthError(prefix: string, error: unknown): Error {
  if (error instanceof Error) {
    const wrapped = new Error(`${prefix}: ${error.message}`);
    wrapped.cause = error;
    return wrapped;
  }
  return new Error(`${prefix}: ${String(error)}`);
}
