# Phase A: 配置与注册表基础

---

### Task 1: BrowserConfigSchema + KimiConfigSchema/KimiConfigPatchSchema 更新

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/config/schema.ts`
- Create: `packages/agent-core/test/config/browser-config.test.ts`

**步骤:**

- [ ] 在 `packages/agent-core/src/config/schema.ts` 中 `KimiConfigSchema` 定义之前添加：

```typescript
export const BrowserConfigSchema = z.object({
  enabled: z.boolean().optional(),
  chromePort: z.number().int().min(1).max(65535).optional(),
  traceEnabled: z.boolean().optional(),
  traceRetentionDays: z.number().int().min(1).optional(),
});

export type BrowserConfig = z.infer<typeof BrowserConfigSchema>;
```

- [ ] 修改 `KimiConfigSchema`（约 L186），在 `modeModels` 字段之后、`raw` 字段之前插入 `browser: BrowserConfigSchema.optional()`：

```typescript
export const KimiConfigSchema = z.object({
  providers: z.record(z.string(), ProviderConfigSchema).default({}),
  defaultProvider: z.string().optional(),
  defaultModel: z.string().optional(),
  models: z.record(z.string(), ModelAliasSchema).optional(),
  thinking: ThinkingConfigSchema.optional(),
  sessionMode: z.enum(['plan', 'design']).optional(),
  yolo: z.boolean().optional(),
  defaultThinking: z.boolean().optional(),
  defaultPermissionMode: PermissionModeSchema.optional(),
  defaultSessionMode: z.enum(['plan', 'design']).optional(),
  permission: PermissionConfigSchema.optional(),
  hooks: z.array(HookDefSchema).optional(),
  services: ServicesConfigSchema.optional(),
  mergeAllAvailableSkills: z.boolean().optional(),
  extraSkillDirs: z.array(z.string()).optional(),
  loopControl: LoopControlSchema.optional(),
  background: BackgroundConfigSchema.optional(),
  telemetry: z.boolean().optional(),
  modeModels: z.object({
    plan: z.string().optional(),
    design: z.string().optional(),
    review: z.string().optional(),
  }).optional(),
  browser: BrowserConfigSchema.optional(),
  raw: z.record(z.string(), z.unknown()).optional(),
});
```

- [ ] 修改 `KimiConfigPatchSchema`（约 L227），在相同相对位置插入 `browser: BrowserConfigSchema.optional()`：

```typescript
export const KimiConfigPatchSchema = z
  .object({
    providers: z.record(z.string(), ProviderConfigPatchSchema).optional(),
    defaultProvider: z.string().optional(),
    defaultModel: z.string().optional(),
    models: z.record(z.string(), ModelAliasPatchSchema).optional(),
    thinking: ThinkingConfigPatchSchema.optional(),
    sessionMode: z.enum(['plan', 'design']).optional(),
    yolo: z.boolean().optional(),
    defaultThinking: z.boolean().optional(),
    defaultPermissionMode: PermissionModeSchema.optional(),
    defaultSessionMode: z.enum(['plan', 'design']).optional(),
    permission: PermissionConfigPatchSchema.optional(),
    hooks: z.array(HookDefSchema).optional(),
    services: ServicesConfigSchema.optional(),
    mergeAllAvailableSkills: z.boolean().optional(),
    extraSkillDirs: z.array(z.string()).optional(),
    loopControl: LoopControlPatchSchema.optional(),
    background: BackgroundConfigPatchSchema.optional(),
    telemetry: z.boolean().optional(),
    modeModels: z.object({
      plan: z.string().optional(),
      design: z.string().optional(),
      review: z.string().optional(),
    }).optional(),
    browser: BrowserConfigSchema.optional(),
  })
  .strict();
```

- [ ] 运行现有配置测试确认无回归：

```bash
cd packages/agent-core && pnpm vitest run test/config/configs.test.ts
```

期望：全部通过（zod 默认会剔除 undefined 的 optional 字段，因此 `KimiConfigSchema.parse({})` 仍返回 `{ providers: {} }`）。

- [ ] 编写并运行新测试 `packages/agent-core/test/config/browser-config.test.ts`：

```typescript
import { describe, expect, it } from 'vitest';
import {
  BrowserConfigSchema,
  KimiConfigSchema,
  KimiConfigPatchSchema,
} from '../../src/config/schema';

describe('BrowserConfigSchema', () => {
  it('parses valid browser config with all fields', () => {
    const parsed = BrowserConfigSchema.parse({
      enabled: true,
      chromePort: 9222,
      traceEnabled: true,
      traceRetentionDays: 7,
    });
    expect(parsed).toEqual({
      enabled: true,
      chromePort: 9222,
      traceEnabled: true,
      traceRetentionDays: 7,
    });
  });

  it('parses empty object as all undefined', () => {
    expect(BrowserConfigSchema.parse({})).toEqual({});
  });

  it('rejects chromePort = 0', () => {
    expect(() => BrowserConfigSchema.parse({ chromePort: 0 })).toThrow();
  });

  it('rejects chromePort > 65535', () => {
    expect(() => BrowserConfigSchema.parse({ chromePort: 70000 })).toThrow();
  });

  it('rejects traceRetentionDays = 0', () => {
    expect(() => BrowserConfigSchema.parse({ traceRetentionDays: 0 })).toThrow();
  });

  it('is accepted by KimiConfigSchema as optional field', () => {
    const config = KimiConfigSchema.parse({
      providers: {},
      browser: { enabled: true, chromePort: 9222 },
    });
    expect(config.browser).toEqual({ enabled: true, chromePort: 9222 });
  });

  it('is accepted by KimiConfigPatchSchema', () => {
    const patch = KimiConfigPatchSchema.parse({ browser: { enabled: false } });
    expect(patch.browser).toEqual({ enabled: false });
  });
});
```

- [ ] 运行新测试：

```bash
cd packages/agent-core && pnpm vitest run test/config/browser-config.test.ts
```

期望：7 个断言全部通过。

- [ ] 运行全树 typecheck（共享签名变更后必须）：

```bash
pnpm -r typecheck
```

期望：无类型错误。

- [ ] Commit：

```bash
git add packages/agent-core/src/config/schema.ts packages/agent-core/test/config/browser-config.test.ts
git commit -m "feat(config): add BrowserConfigSchema for built-in chrome-devtools MCP"
```

---

### Task 2: BuiltInMcpRegistry + resolveBuiltInRoot

**Depends on:** Task 1

**Files:**
- Create: `packages/agent-core/src/mcp/built-in/registry.ts`
- Create: `packages/agent-core/src/mcp/built-in/resolve-root.ts`
- Create: `packages/agent-core/src/mcp/built-in/index.ts`
- Create: `packages/agent-core/test/mcp/built-in/registry.test.ts`
- Create: `packages/agent-core/test/mcp/built-in/resolve-root.test.ts`

**步骤:**

- [ ] 创建 `packages/agent-core/src/mcp/built-in/resolve-root.ts`：

```typescript
import { existsSync } from 'node:fs';
import { dirname, join } from 'pathe';

export class BuiltInRootNotFoundError extends Error {
  constructor(public readonly serverName: string) {
    super(`Built-in server "${serverName}" not found`);
  }
}

export function resolveBuiltInRoot(serverName: string, candidates?: readonly string[]): string {
  const resolvedCandidates = candidates ?? [
    join(dirname(process.execPath), 'built-in', serverName),
    join(__dirname, '..', '..', 'built-in', serverName),
    join(__dirname, '..', '..', '..', 'built-in', serverName),
  ];
  for (const candidate of resolvedCandidates) {
    if (existsSync(join(candidate, 'package.json')) || existsSync(join(candidate, 'index.js'))) {
      return candidate;
    }
  }
  throw new BuiltInRootNotFoundError(serverName);
}
```

- [ ] 创建 `packages/agent-core/src/mcp/built-in/registry.ts`：

```typescript
import type { KimiConfig, McpServerConfig } from '#/config/schema';

export interface BuiltInContext {
  readonly kimiHomeDir: string;
  readonly sessionId?: string;
  readonly chromePort?: number;
}

export interface BuiltInMcpServerDefinition {
  readonly name: string;
  readonly displayName: string;
  readonly enabledByDefault: boolean;
  readonly config: McpServerConfig;
  readonly envResolver?: (ctx: BuiltInContext) => Record<string, string>;
}

export class BuiltInMcpRegistry {
  private readonly definitions = new Map<string, BuiltInMcpServerDefinition>();

  register(def: BuiltInMcpServerDefinition): void {
    this.definitions.set(def.name, def);
  }

  getEnabledConfigs(ctx: BuiltInContext, config: KimiConfig): Record<string, McpServerConfig> {
    const result: Record<string, McpServerConfig> = {};
    for (const [name, def] of this.definitions) {
      if (this.isDisabled(name, config)) continue;
      const env = def.envResolver?.(ctx);
      result[name] = {
        ...def.config,
        env: env ? { ...(def.config.env ?? {}), ...env } : def.config.env,
      };
    }
    return result;
  }

  isDisabled(name: string, config: KimiConfig): boolean {
    const def = this.definitions.get(name);
    if (def === undefined) return true;
    if (name === 'chrome-devtools') {
      return config.browser?.enabled === false;
    }
    return !def.enabledByDefault;
  }
}
```

- [ ] 创建 `packages/agent-core/src/mcp/built-in/index.ts`：

```typescript
export * from './registry';
export * from './resolve-root';
```

- [ ] 编写测试 `packages/agent-core/test/mcp/built-in/resolve-root.test.ts`：

```typescript
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { describe, expect, it, afterEach } from 'vitest';
import { resolveBuiltInRoot, BuiltInRootNotFoundError } from '../../../src/mcp/built-in/resolve-root';

const tempDirs: string[] = [];

afterEach(() => {
  for (const dir of tempDirs.splice(0)) {
    rmSync(dir, { recursive: true, force: true });
  }
});

function makeTempDir(): string {
  const dir = mkdtempSync(join(tmpdir(), 'kimi-built-in-'));
  tempDirs.push(dir);
  return dir;
}

describe('resolveBuiltInRoot', () => {
  it('returns the first candidate containing package.json', () => {
    const dir = makeTempDir();
    const serverDir = join(dir, 'chrome-devtools');
    mkdirSync(serverDir, { recursive: true });
    writeFileSync(join(serverDir, 'package.json'), '{}');
    const result = resolveBuiltInRoot('chrome-devtools', [
      join(dir, 'does-not-exist'),
      serverDir,
    ]);
    expect(result).toBe(serverDir);
  });

  it('returns the first candidate containing index.js', () => {
    const dir = makeTempDir();
    const serverDir = join(dir, 'chrome-devtools');
    mkdirSync(serverDir, { recursive: true });
    writeFileSync(join(serverDir, 'index.js'), '');
    const result = resolveBuiltInRoot('chrome-devtools', [serverDir]);
    expect(result).toBe(serverDir);
  });

  it('throws BuiltInRootNotFoundError when no candidate matches', () => {
    expect(() =>
      resolveBuiltInRoot('nonexistent-server-xyz', [join(tmpdir(), 'nonexistent-123')]),
    ).toThrow(BuiltInRootNotFoundError);
    try {
      resolveBuiltInRoot('nonexistent-server-xyz', [join(tmpdir(), 'nonexistent-123')]);
    } catch (error) {
      expect(error).toBeInstanceOf(BuiltInRootNotFoundError);
      expect((error as BuiltInRootNotFoundError).serverName).toBe('nonexistent-server-xyz');
    }
  });

  it('prefers earlier candidate when both match', () => {
    const dir = makeTempDir();
    const first = join(dir, 'first', 'chrome-devtools');
    const second = join(dir, 'second', 'chrome-devtools');
    mkdirSync(first, { recursive: true });
    mkdirSync(second, { recursive: true });
    writeFileSync(join(first, 'package.json'), '{}');
    writeFileSync(join(second, 'package.json'), '{}');
    const result = resolveBuiltInRoot('chrome-devtools', [first, second]);
    expect(result).toBe(first);
  });
});
```

- [ ] 编写测试 `packages/agent-core/test/mcp/built-in/registry.test.ts`：

```typescript
import { describe, expect, it } from 'vitest';
import { BuiltInMcpRegistry } from '../../../src/mcp/built-in/registry';
import type { KimiConfig } from '../../../src/config/schema';

describe('BuiltInMcpRegistry', () => {
  it('register then getEnabledConfigs returns the server', () => {
    const registry = new BuiltInMcpRegistry();
    registry.register({
      name: 'test-server',
      displayName: 'Test Server',
      enabledByDefault: true,
      config: { transport: 'stdio' as const, command: 'echo' },
    });
    const configs = registry.getEnabledConfigs(
      { kimiHomeDir: '/tmp/home' },
      { providers: {} },
    );
    expect(configs).toHaveProperty('test-server');
    expect(configs['test-server']).toMatchObject({ transport: 'stdio', command: 'echo' });
  });

  it('isDisabled returns true when chrome-devtools is explicitly disabled', () => {
    const registry = new BuiltInMcpRegistry();
    registry.register({
      name: 'chrome-devtools',
      displayName: 'Chrome DevTools',
      enabledByDefault: true,
      config: { transport: 'stdio' as const, command: 'node' },
    });
    const config: KimiConfig = { providers: {}, browser: { enabled: false } };
    expect(registry.isDisabled('chrome-devtools', config)).toBe(true);
  });

  it('isDisabled returns false for chrome-devtools by default', () => {
    const registry = new BuiltInMcpRegistry();
    registry.register({
      name: 'chrome-devtools',
      displayName: 'Chrome DevTools',
      enabledByDefault: true,
      config: { transport: 'stdio' as const, command: 'node' },
    });
    const config: KimiConfig = { providers: {} };
    expect(registry.isDisabled('chrome-devtools', config)).toBe(false);
  });

  it('isDisabled returns true for unknown server names', () => {
    const registry = new BuiltInMcpRegistry();
    expect(registry.isDisabled('unknown', { providers: {} })).toBe(true);
  });

  it('envResolver merges env into base config env', () => {
    const registry = new BuiltInMcpRegistry();
    registry.register({
      name: 'test-server',
      displayName: 'Test',
      enabledByDefault: true,
      config: { transport: 'stdio' as const, command: 'node', env: { BASE: '1' } },
      envResolver: () => ({ EXTRA: '2' }),
    });
    const configs = registry.getEnabledConfigs(
      { kimiHomeDir: '/tmp/home' },
      { providers: {} },
    );
    expect(configs['test-server'].env).toEqual({ BASE: '1', EXTRA: '2' });
  });

  it('getEnabledConfigs skips disabled servers', () => {
    const registry = new BuiltInMcpRegistry();
    registry.register({
      name: 'enabled-server',
      displayName: 'Enabled',
      enabledByDefault: true,
      config: { transport: 'stdio' as const, command: 'node' },
    });
    registry.register({
      name: 'disabled-server',
      displayName: 'Disabled',
      enabledByDefault: false,
      config: { transport: 'stdio' as const, command: 'node' },
    });
    const configs = registry.getEnabledConfigs(
      { kimiHomeDir: '/tmp/home' },
      { providers: {} },
    );
    expect(Object.keys(configs)).toEqual(['enabled-server']);
  });

  it('chromePort is forwarded to envResolver via context', () => {
    const registry = new BuiltInMcpRegistry();
    registry.register({
      name: 'test-server',
      displayName: 'Test',
      enabledByDefault: true,
      config: { transport: 'stdio' as const, command: 'node' },
      envResolver: (ctx) => ({ PORT: String(ctx.chromePort ?? 9222) }),
    });
    const configs = registry.getEnabledConfigs(
      { kimiHomeDir: '/tmp/home', chromePort: 9333 },
      { providers: {} },
    );
    expect(configs['test-server'].env).toEqual({ PORT: '9333' });
  });
});
```

- [ ] 运行所有新增测试：

```bash
cd packages/agent-core && pnpm vitest run test/mcp/built-in/
```

期望：全部通过。

- [ ] Commit：

```bash
git add packages/agent-core/src/mcp/built-in/ packages/agent-core/test/mcp/built-in/
git commit -m "feat(mcp): add BuiltInMcpRegistry and resolveBuiltInRoot"
```

---

### Task 3: ChromeDevToolsServer 定义

**Depends on:** Task 2

**Files:**
- Create: `packages/agent-core/src/mcp/built-in/chrome-devtools.ts`
- Create: `packages/agent-core/test/mcp/built-in/chrome-devtools.test.ts`

**步骤:**

- [ ] 创建 `packages/agent-core/src/mcp/built-in/chrome-devtools.ts`：

```typescript
import { join } from 'pathe';
import type { BuiltInContext, BuiltInMcpServerDefinition } from './registry';
import { resolveBuiltInRoot } from './resolve-root';

export function createChromeDevToolsServerDefinition(
  rootPath?: string,
): BuiltInMcpServerDefinition {
  return {
    name: 'chrome-devtools',
    displayName: 'Chrome DevTools',
    enabledByDefault: true,
    config: {
      transport: 'stdio',
      command: 'node',
      args: ['--experimental-strip-types', './dist/index.js'],
      cwd: rootPath ?? resolveBuiltInRoot('chrome-devtools'),
      startupTimeoutMs: 30_000,
      toolTimeoutMs: 60_000,
    },
    envResolver: (ctx: BuiltInContext) => ({
      CHROME_REMOTE_DEBUGGING_PORT: String(ctx.chromePort ?? 9222),
      ODY_CODE_HOME: ctx.kimiHomeDir,
      CDP_TRACE_DIR: join(
        ctx.kimiHomeDir,
        'sessions',
        ctx.sessionId ?? 'unknown',
        'chrome-traces',
      ),
    }),
  };
}
```

- [ ] 编写测试 `packages/agent-core/test/mcp/built-in/chrome-devtools.test.ts`：

```typescript
import { describe, expect, it } from 'vitest';
import { createChromeDevToolsServerDefinition } from '../../../src/mcp/built-in/chrome-devtools';

describe('createChromeDevToolsServerDefinition', () => {
  it('returns a stdio server definition with correct defaults', () => {
    const def = createChromeDevToolsServerDefinition('/mock/built-in/chrome-devtools');
    expect(def.name).toBe('chrome-devtools');
    expect(def.displayName).toBe('Chrome DevTools');
    expect(def.enabledByDefault).toBe(true);
    expect(def.config.transport).toBe('stdio');
    expect(def.config.command).toBe('node');
    expect(def.config.args).toEqual(['--experimental-strip-types', './dist/index.js']);
    expect(def.config.cwd).toBe('/mock/built-in/chrome-devtools');
    expect(def.config.startupTimeoutMs).toBe(30_000);
    expect(def.config.toolTimeoutMs).toBe(60_000);
  });

  it('envResolver produces correct environment variables with custom port', () => {
    const def = createChromeDevToolsServerDefinition('/mock');
    const env = def.envResolver?.({
      kimiHomeDir: '/home/user/.ody-code',
      sessionId: 'session_abc123',
      chromePort: 9333,
    });
    expect(env).toEqual({
      CHROME_REMOTE_DEBUGGING_PORT: '9333',
      ODY_CODE_HOME: '/home/user/.ody-code',
      CDP_TRACE_DIR: '/home/user/.ody-code/sessions/session_abc123/chrome-traces',
    });
  });

  it('envResolver falls back to default port 9222', () => {
    const def = createChromeDevToolsServerDefinition('/mock');
    const env = def.envResolver?.({
      kimiHomeDir: '/home/user/.ody-code',
      sessionId: 'session_abc123',
    });
    expect(env?.CHROME_REMOTE_DEBUGGING_PORT).toBe('9222');
  });

  it('envResolver handles missing sessionId', () => {
    const def = createChromeDevToolsServerDefinition('/mock');
    const env = def.envResolver?.({
      kimiHomeDir: '/home/user/.ody-code',
    });
    expect(env?.CDP_TRACE_DIR).toBe('/home/user/.ody-code/sessions/unknown/chrome-traces');
  });
});
```

- [ ] 运行测试：

```bash
cd packages/agent-core && pnpm vitest run test/mcp/built-in/chrome-devtools.test.ts
```

期望：全部通过。

- [ ] Commit：

```bash
git add packages/agent-core/src/mcp/built-in/chrome-devtools.ts packages/agent-core/test/mcp/built-in/chrome-devtools.test.ts
git commit -m "feat(mcp): define ChromeDevTools built-in MCP server"
```

---

## Local Self-Review (Phase A)

- [ ] 1. Spec-coverage table: Task 1 覆盖 BrowserConfigSchema；Task 2 覆盖 BuiltInMcpRegistry + resolveBuiltInRoot；Task 3 覆盖 ChromeDevToolsServer 定义。全部 covered。
- [ ] 2. Placeholder scan: 无 TODO/TBD，无 deferred-by-dependency 借口。`rootPath` 和 `candidates` 可选参数在代码中显式定义并测试。
- [ ] 3. No phantom tasks: 每个 Task 都有 Create/Modify 文件、测试代码、运行命令、commit 步骤。
- [ ] 4. Dependency soundness: Task 1 无依赖；Task 2 依赖 Task 1（`isDisabled` 读取 `config.browser`）；Task 3 依赖 Task 2（使用 `BuiltInMcpServerDefinition`, `BuiltInContext`, `resolveBuiltInRoot`）。
- [ ] 5. Caller & build soundness: Task 1 修改共享 schema（添加 optional 字段），运行了 `pnpm -r typecheck`；添加 optional 字段不会破坏现有调用者，现有测试 `configs.test.ts` 无回归。
- [ ] 6. Test-the-risk: `BrowserConfigSchema` 的边界测试覆盖了 chromePort=0（reject）、chromePort>65535（reject）、traceRetentionDays=0（reject）。`resolveBuiltInRoot` 测试覆盖了 candidate 优先顺序和 not-found 异常。`BuiltInMcpRegistry` 测试覆盖了 env 合并和 disabled 过滤。`createChromeDevToolsServerDefinition` 测试覆盖了 envResolver 的默认值和路径拼接。
- [ ] 7. Type consistency: `BuiltInContext` 定义中包含 `chromePort?: number`，与 `BrowserConfigSchema` 的 `chromePort` 类型一致；`KimiConfigPatchSchema` 中的 `browser` 字段类型与 `KimiConfigSchema` 一致。
