import type { ChildProcess } from 'node:child_process';
import { SDKRpcClient, type SDKRpcClientConnectOptions } from '@odysseythink/ody-code-sdk';

export type RustHostMode = 'stdio' | 'socket' | 'tcp';

export interface RustHostConnectorOptions {
  readonly mode: RustHostMode;
  readonly binaryPath: string;
  readonly socketPath?: string;
  readonly host?: string;
  readonly port?: number;
  readonly configPath?: string;
  readonly homeDir?: string;
}

export type Unsubscribe = () => void;

export class RustHostConnector {
  private client: SDKRpcClient | undefined;
  private readonly disconnectListeners = new Set<(error: Error) => void>();

  async connect(options: RustHostConnectorOptions): Promise<SDKRpcClient> {
    const connectOptions = this.buildConnectOptions(options);
    this.client = await SDKRpcClient.connect(connectOptions);
    this.attachDisconnectHandlers();
    return this.client;
  }

  onDisconnect(listener: (error: Error) => void): Unsubscribe {
    this.disconnectListeners.add(listener);
    return () => {
      this.disconnectListeners.delete(listener);
    };
  }

  static async connect(options: RustHostConnectorOptions): Promise<SDKRpcClient> {
    const connector = new RustHostConnector();
    return connector.connect(options);
  }

  private buildConnectOptions(options: RustHostConnectorOptions): SDKRpcClientConnectOptions {
    const base: {
      binaryPath: string;
      homeDir?: string;
      configPath?: string;
      transport: SDKRpcClientConnectOptions['transport'];
    } = {
      binaryPath: options.binaryPath,
      homeDir: options.homeDir,
      configPath: options.configPath,
      transport: 'stdio',
    };
    if (options.mode === 'socket') {
      if (options.socketPath === undefined) {
        throw new Error('--host-socket requires a path.');
      }
      base.transport = { socketPath: options.socketPath, spawn: true };
    } else if (options.mode === 'tcp') {
      if (options.host === undefined || options.port === undefined) {
        throw new Error('--host-tcp requires host:port.');
      }
      base.transport = { host: options.host, port: options.port, spawn: true };
    }
    return base;
  }

  private attachDisconnectHandlers(): void {
    const client = this.client;
    if (client === undefined) return;

    const proc = (client as any)._hostProc as ChildProcess | undefined;
    if (proc !== undefined) {
      proc.once('exit', (code) => {
        this.emitDisconnect(new Error(`Rust host disconnected with code ${String(code)}`));
      });
      proc.once('error', (err) => {
        this.emitDisconnect(new Error(`Rust host disconnected: ${err.message}`));
      });
    }
  }

  private emitDisconnect(error: Error): void {
    for (const listener of this.disconnectListeners) {
      listener(error);
    }
  }
}
