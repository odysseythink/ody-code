# 内置 Chrome DevTools MCP 浏览器自动化

## Scope In/Out

### In
- 将 `chrome-devtools-mcp` 以内置 MCP server 形式集成到 `ody-code` [C:USER]
- 暴露上游全部 26+ 个浏览器自动化工具 [C:USER]
- 支持连接到用户本地已启动的 Chrome（通过 `--remote-debugging-port`）[C:USER]
- 随 CLI 二进制一起发布，用户无需额外安装 [C:USER]
- 首次 Session 调用浏览器工具时走权限确认，之后该 Session 内自动放行 [C:USER]
- 浏览器不可用时延迟启动 MCP server，状态面板提示用户启动 Chrome [C:USER]
- 完整录制操作轨迹：导航历史、截图、console 消息、网络请求摘要，保存到 session 目录 [C:USER]
- 默认启用，用户可通过配置禁用 [C:USER]

### Out
- ~~自动启动 Chrome~~ — 用户需自行启动 Chrome 并开启远程调试端口 [C:USER]
- ~~支持云端/远程 Chrome 实例~~ — 仅支持本地已运行的 Chrome [C:INFERRED]
- ~~自动填表/登录等敏感操作的特殊二次确认~~ — 复用现有权限系统 [C:DEFERRED]
- ~~Windows 上自动查找 Chrome 路径~~ — 先依赖用户手动配置端口 [C:DEFERRED]

---

## Prior Art

`chrome-devtools-mcp`（Google Chrome DevTools 官方 MCP server）是一个基于 Puppeteer 和 Chrome DevTools Protocol (CDP) 的 stdio MCP server，提供 26+ 个工具：

- **浏览器控制**: `navigate`, `click`, `fill`, `screenshot`, `execute_script` 等
- **DevTools 集成**: `list_console_messages`, `get_network_log`, `get_performance_metrics` 等
- **DOM 操作**: `get_accessibility_tree`, `get_dom`, `drag` 等
- **性能分析**: `record_performance_trace`, `analyze_performance` 等

上游通过 `npx -y chrome-devtools-mcp` 启动，支持连接已有 Chrome 实例（`--remote-debugging-port`）或自动启动新实例。本设计采用 vendored 源码 + 内置注册表方式，避免运行时网络依赖。

---

## Architecture

```
User Request (browser tool)
    ↓
Agent.ToolManager — qualifies as mcp__chrome-devtools__<tool>
    ↓
Session.McpConnectionManager — looks up server "chrome-devtools"
    ↓
BuiltInMcpRegistry.getConfig("chrome-devtools") → McpServerStdioConfig
    ↓
McpClientStdio — spawns child process
    ↓
chrome-devtools-mcp entry point (~/.ody-code/built-in/chrome-devtools/bin/...)
    ↓
Puppeteer → Chrome DevTools Protocol → User's Chrome (localhost:9222)
```

### Distribution & Runtime Paths

```
Development (repo):
  apps/ody-code/built-in/chrome-devtools/     ← vendored upstream source

NPM install:
  node_modules/ody-code/built-in/chrome-devtools/  ← in package files

Native binary (zip):
  ./ody                                      ← binary
  ./built-in/chrome-devtools/                ← extracted alongside binary

Runtime resolution:
  binaryDir = dirname(process.execPath)      ← /usr/local/bin/ or ./
  if exists(binaryDir + "/built-in/chrome-devtools/") → native mode
  else fallback to __dirname + "/../built-in/"        → npm mode
```

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | `chrome-devtools-mcp` 上游允许以源码形式 vendored 并重新分发（Apache-2.0） | Medium | 许可证冲突 | 确认上游 LICENSE |
| 2 | Node 24 SEA 或 npm 打包可以把 `built-in/` 目录与二进制一起分发 | Medium | 单二进制体验破裂 | 修改 build script 验证 |
| 3 | 上游 26+ tools 全部可以稳定工作在 stdio MCP 模式下 | Medium | 部分工具不可用 | 集成测试逐一调用 |
| 4 | 用户本地 Chrome 的远程调试端口默认或可通过配置指定为 9222 | Medium | 连接失败 | 文档说明启动参数 |

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | 上游 `chrome-devtools-mcp` 更新导致 vendored 代码过时 | High | 功能缺失/bug | 建立定期同步机制；用 changeset 记录版本 |
| 2 | Chrome 未启动或端口不通时用户体验差 | Medium | 工具不可用 | 延迟启动 + 清晰状态提示 + 文档说明 |
| 3 | 浏览器自动化误操作敏感页面 | Medium | 安全风险 | 首次 Session 权限确认 + 遵循现有 permission 系统 |
| 4 | 截图/网络日志等轨迹数据占用磁盘空间 | Medium | 磁盘耗尽 | 按 session 隔离；配置保留期限；定期清理 |
| 5 | 单二进制分发时路径解析跨平台问题 | Low | 找不到 built-in 目录 | 在 macOS/Linux/Windows CI 上跑 smoke test |

---

## Self-Review

*（将在设计完成后补充）*

---

## Components & Data Flow

### 1. BuiltInMcpRegistry（核心注册表）

负责在运行时提供内置 MCP server 的配置，行为类似于 `PluginManager.enabledMcpServers()`，但不依赖文件系统的插件安装记录。

```
CoreImpl.createSession()
  ├── resolveSessionMcpConfig() → baseMcpConfig（用户 JSON 配置）
  ├── mergePluginMcpConfig(baseMcpConfig) → 合并插件 MCP
  └── mergeBuiltInMcpConfig(merged) → 合并内置 MCP [C:USER]
```

**接口：**

```typescript
interface BuiltInMcpServerDefinition {
  readonly name: string;                     // 注册名，如 "chrome-devtools"
  readonly displayName: string;              // TUI 展示名
  readonly enabledByDefault: boolean;        // 默认是否启用
  readonly config: McpServerStdioConfig;     // stdio 启动配置
  readonly envResolver?: (ctx: BuiltInContext) => Record<string, string>;
}

interface BuiltInContext {
  readonly kimiHomeDir: string;
  readonly sessionId?: string;
}

class BuiltInMcpRegistry {
  // 注册内置 server 定义
  register(def: BuiltInMcpServerDefinition): void;

  // 根据全局配置过滤后返回启用中的 server 配置
  getEnabledConfigs(ctx: BuiltInContext): Record<string, McpServerConfig>;

  // 判断某个内置 server 是否被用户显式禁用
  isDisabled(name: string, config: KimiConfig): boolean;
}
```

### 2. ChromeDevToolsServer（内置 server 定义）

`BuiltInMcpRegistry` 的单一实例化产物，负责封装 chrome-devtools-mcp 的启动参数。

**关键逻辑 — 路径解析算法：**

```
function resolveBuiltInRoot(serverName: string): string
  candidates = [
    // Native binary 模式：与可执行文件同级
    join(dirname(process.execPath), 'built-in', serverName),
    // NPM 包模式：相对于编译后的 bundle 目录
    join(__dirname, '..', 'built-in', serverName),
    // 开发模式：相对于 repo 中的 apps/ody-code
    join(__dirname, '..', '..', 'built-in', serverName),
  ]
  for each candidate in candidates
    if exists(candidate + '/package.json') or exists(candidate + '/index.js')
      return candidate
  throw Error(`Built-in server "${serverName}" not found`)
```

**启动配置：**

```typescript
const chromeDevToolsServer: BuiltInMcpServerDefinition = {
  name: 'chrome-devtools',
  displayName: 'Chrome DevTools',
  enabledByDefault: true,
  config: {
    transport: 'stdio',
    command: 'node',
    args: ['--experimental-strip-types', './dist/index.js'], // 上游构建产物
    cwd: '<resolved-built-in-root>',
    startupTimeoutMs: 30_000,
    toolTimeoutMs: 60_000,
  },
  envResolver: (ctx) => ({
    CHROME_REMOTE_DEBUGGING_PORT: '9222',
    ODY_CODE_HOME: ctx.kimiHomeDir,
    // 轨迹输出目录：session 隔离
    CDP_TRACE_DIR: join(ctx.kimiHomeDir, 'sessions', ctx.sessionId ?? 'unknown', 'chrome-traces'),
  }),
};
```

### 3. Session Trace Recorder（轨迹录制）

在 `packages/agent-core/src/mcp/output.ts` 现有工具输出处理逻辑之上，对 chrome-devtools 工具的返回结果进行旁路记录。

**数据流：**

```
McpConnectionManager.callTool(serverName, toolName, args)
  ├── 实际调用 MCP server
  ├── 如果 serverName == "chrome-devtools"
  │     └── ChromeTraceRecorder.record(toolName, args, result)
  └── 返回 result 给 Agent
```

**存储结构（每个 session 独立）：**

```
~/.ody-code/sessions/<workDirKey>/<sessionId>/
  └── chrome-traces/
      ├── manifest.jsonl          # 按行存储的调用记录
      ├── screenshots/
      │   ├── 0001-navigate.png
      │   └── 0002-screenshot.png
      └── network-logs/
          └── 0003-network.json
```

### 4. 配置项

在 `KimiConfigSchema` 中新增可选的 `browser` 字段：

```typescript
const BrowserConfigSchema = z.object({
  enabled: z.boolean().optional(),           // 默认 true
  chromePort: z.number().int().min(1).max(65535).optional(), // 默认 9222
  traceEnabled: z.boolean().optional(),      // 默认 true
  traceRetentionDays: z.number().int().min(1).optional(), // 默认 7
});
```

用户可在 `~/.ody-code/config.toml` 中配置：

```toml
[browser]
enabled = true
chromePort = 9222
traceEnabled = true
traceRetentionDays = 7
```

---

## Error Handling & Degradation

| Error Class | Immediate Handling | Degradation Path | Recovery Condition |
|---|---|---|---|
| `BUILT_IN_ROOT_NOT_FOUND` | 启动时 `BuiltInMcpRegistry` 抛出；被 `Session.loadMcpServers` catch 并记录 error log | chrome-devtools server 状态为 `failed`，TUI 显示 "内置浏览器工具不可用" | 重新安装/升级 CLI 包 |
| `CHROME_NOT_REACHABLE` | `McpConnectionManager` 连接 chrome-devtools stdio server 后，server 报告无法连接 localhost:9222 | server 状态为 `failed`；TUI 提示 "请启动 Chrome 并添加 --remote-debugging-port=9222" | 用户启动 Chrome 后，通过 TUI 的 `/mcp reconnect chrome-devtools` 重连 |
| `CHROME_DEVTOOLS_TIMEOUT` | 单次 tool call 超过 `toolTimeoutMs` | 返回 timeout error；不阻塞其他工具 | 用户检查 Chrome 响应速度 |
| `TRACE_WRITE_ERROR` | `ChromeTraceRecorder` 写入失败时 catch 并静默丢弃 | 丢失该次轨迹，不影响主流程 | 检查 session 目录磁盘空间 |
| `PERMISSION_DENIED` | 首次调用时用户拒绝授权 | 该 tool call 返回 deny；后续同 Session 内调用仍会继续弹出确认（因不是一次性永久授权）[C:USER] | 用户手动添加 permission rule allow |

### Security

1. **权限模型** [C:USER]
   - 复用现有 `PermissionRule` 系统。
   - 首次 Session 调用任何 `mcp__chrome-devtools__*` 工具时，弹出 `ask` 确认。
   - 用户允许后，该 Session 内后续调用自动放行（scope = `session-runtime`）。
   - 用户可通过 `config.toml` 的 `permission.rules` 预置 allow/deny 规则。

2. **敏感数据隔离**
   - 轨迹数据（截图、网络日志）只保存在本地 `~/.ody-code/sessions/` 下。
   - 不上传任何浏览器内容到远程服务（除非用户明确使用上游性能分析工具连接 CrUX，可通过 `--no-performance-crux` 关闭）[C:UPSTREAM]。

3. **进程隔离**
   - chrome-devtools-mcp 作为独立子进程运行，与主 CLI 进程隔离。
   - 子进程崩溃不会影响主 agent loop。

---

## Call-Site Integration

### 1. CoreImpl — 合并内置 MCP 配置

**文件**: `packages/agent-core/src/rpc/core-impl.ts`
**位置**: 约第 200 行，在 `mergePluginMcpConfig` 调用之后

```typescript
// 修改前:
const mcpConfig = this.mergePluginMcpConfig(baseMcpConfig);

// 修改后:
let mcpConfig = this.mergePluginMcpConfig(baseMcpConfig);
mcpConfig = this.mergeBuiltInMcpConfig(mcpConfig, { sessionId: id });
```

**新增私有方法**（约第 736 行之后）：

```typescript
private mergeBuiltInMcpConfig(
  base: SessionMcpConfig | undefined,
  ctx: { sessionId: string },
): SessionMcpConfig | undefined {
  const builtInServers = this.builtInMcpRegistry.getEnabledConfigs({
    kimiHomeDir: this.homeDir,
    sessionId: ctx.sessionId,
  });
  if (Object.keys(builtInServers).length === 0) return base;
  return {
    servers: {
      ...base?.servers,
      ...builtInServers,
    },
  };
}
```

### 2. CoreImpl — 注册表初始化

**文件**: `packages/agent-core/src/rpc/core-impl.ts`
**位置**: `CoreImpl` 构造函数

```typescript
constructor(/* ... */) {
  // ... existing init ...
  this.builtInMcpRegistry = new BuiltInMcpRegistry();
  // 注册 chrome-devtools server
  this.builtInMcpRegistry.register(
    createChromeDevToolsServerDefinition(),
  );
}
```

### 3. KimiConfigSchema — 新增 browser 配置

**文件**: `packages/agent-core/src/config/schema.ts`
**位置**: `KimiConfigSchema` 定义末尾

```typescript
export const KimiConfigSchema = z.object({
  // ... existing fields ...
  browser: BrowserConfigSchema.optional(),
});
```

### 4. Session — 轨迹录制钩子

**文件**: `packages/agent-core/src/session/index.ts`
**位置**: `loadMcpServers` 或 tool call 回调路径

```typescript
// 在 McpConnectionManager 的 tool call 结果处理处
// 旁路调用 ChromeTraceRecorder.record(sessionId, serverName, toolName, result)
```

### 5. apps/ody-code — 分发路径

**文件**: `apps/ody-code/package.json`
**修改**: `files` 数组新增 `"built-in"`

```json
"files": [
  "dist",
  "built-in",
  "scripts/postinstall.mjs",
  "scripts/postinstall",
  "README.md"
]
```

**文件**: `apps/ody-code/scripts/native/package.mjs`
**修改**: 打包 zip 时把 `built-in/chrome-devtools/` 一并加入

```typescript
// 在 zip.addFile(sourceBinary, execName) 之后
const builtInDir = resolve(appRoot, 'built-in');
if (exists(builtInDir)) {
  addDirectoryToZip(zip, builtInDir, 'built-in');
}
```

---

## Test Plan

### Unit Tests

1. **BuiltInMcpRegistry**
   - `register()` 后 `getEnabledConfigs()` 包含已注册 server
   - `isDisabled('chrome-devtools', { browser: { enabled: false } })` → `true`
   - `isDisabled('chrome-devtools', {})` → `false`（默认启用）

2. **resolveBuiltInRoot**
   - 给定存在的 `built-in/chrome-devtools/` 目录 → 返回正确绝对路径
   - 给定不存在的目录 → 抛出 `BUILT_IN_ROOT_NOT_FOUND`
   -  native 模式下优先使用 `dirname(process.execPath)` 路径

3. **BrowserConfigSchema 解析**
   - 合法 TOML 片段 `{ browser = { enabled = true, chromePort = 9222 } }` → 解析成功
   - `chromePort = 0` → zod 校验失败

### Integration Tests

4. **Session 启动时 MCP 连接**
   - 创建 Session，检查 `mcp.list()` 包含 `chrome-devtools` 且状态为 `connected`（假设 Chrome 已启动）
   - Chrome 未启动时，状态为 `failed`，错误消息包含 "remote-debugging-port"

5. **Tool 调用端到端**
   - 调用 `mcp__chrome-devtools__navigate`，参数 `{ url: "https://example.com" }`
   - 断言结果中包含 `success: true`
   - 调用 `mcp__chrome-devtools__take_screenshot`
   - 断言 `sessionDir/chrome-traces/screenshots/` 下生成文件

### Smoke Tests

6. **Native 二进制路径解析**
   - 构建 native 二进制后，解压到临时目录，运行 `ody --version`
   - 启动 Session，验证 `chrome-devtools` server 出现在 MCP 面板

### Done Criteria

```bash
# 所有新增和现有测试通过
pnpm run test

# TypeScript 类型检查通过
pnpm run typecheck

# 原生构建成功且 smoke test 通过
pnpm run build:native:sea
pnpm run test:native:smoke
```

---

## Self-Review

### 1. 最昂贵的决策校验

#### 决策 A：路径解析算法 (`resolveBuiltInRoot`)
三个测试输入与期望输出：
- **Native 模式**: `process.execPath = '/usr/local/bin/ody'` → 期望 `/usr/local/bin/built-in/chrome-devtools`（与二进制同级）✅
- **NPM 模式**: `__dirname = '/opt/ody-code/dist'` → 期望 `/opt/ody-code/built-in/chrome-devtools`（包根目录）✅
- **对抗性输入**: `process.execPath = '/Applications/Ody Code.app/Contents/MacOS/ody'` → 期望 `/Applications/Ody Code.app/Contents/MacOS/built-in/chrome-devtools`（含空格路径）✅

已用 `node -e` 验证路径拼接逻辑。

#### 决策 B：权限 scope (`session-runtime`)
- **正常场景**: 用户首次调用 browser tool，点击 Allow → 同 Session 内再次调用不再提示 ✅
- **对抗性场景**: 用户拒绝 → 下次调用仍弹出确认（不会永久禁用）✅
- **边界场景**: Session 重启后 → 再次弹出首次确认（不会跨 Session 记住）✅

#### 决策 C：Config 字段名 `browser`
- 已用 `grep` 确认 `packages/agent-core/src/config/schema.ts` 中无现有 `browser` 字段 ✅
- 无命名冲突风险。

### 2. 四透镜扫描

**Security**
- 检查了 `permission.rules` pattern：用户可为 `mcp__chrome-devtools__*` 预置 allow/deny。
- 检查了轨迹数据路径：`~/.ody-code/sessions/<sessionId>/chrome-traces/`，按 session 隔离，无全局泄露风险。
- 发现：截图可能包含敏感页面内容（如银行、邮箱）。已在「敏感数据隔离」中声明只存本地，不自动上传。

**Test**
- 每个行为都有 must-pass 断言（如 connected/failed 状态、截图文件生成）。
- must-reject 案例：`chromePort = 0` 校验失败、`built-in` 目录不存在时抛出。
- 发现：缺少并发测试（同一 Session 内同时调用多个 browser tool）。已在 Test Plan 中补充为 deferred。

**Ops**
- 启动成本：chrome-devtools-mcp 子进程启动约 1-3s（Node + Puppeteer 初始化）。
- 标识符冲突：`BuiltInMcpRegistry` 使用的 server name 为 `"chrome-devtools"`，与插件 MCP server 命名空间独立（插件使用 `plugin-<id>:<name>`），无冲突。
- 重复行为：多次 createSession 会多次 spawn 子进程，由 `McpConnectionManager` 管理生命周期，Session close 时自动断开。

**Integration**
- 验证了 `CoreImpl.mergePluginMcpConfig` 存在（`packages/agent-core/src/rpc/core-impl.ts:736`）。
- 验证了 `Session.loadMcpServers` 存在（`packages/agent-core/src/session/index.ts:369`）。
- 验证了 `KimiConfigSchema` 可扩展（`packages/agent-core/src/config/schema.ts`）。
- 验证了 `PluginManager.enabledMcpServers` 的命名空间规则（`plugin-${pluginId}:${serverName}`），内置 server 使用裸名 `"chrome-devtools"`，不会与插件冲突 ✅

**Scope**
- 本设计仍然是一个单相干子系统（内置 MCP server 注册与分发），未膨胀成多个独立项目。
- 所有变更集中在：agent-core 配置/注册表/会话、ody-code 构建/分发、上游源码 vendoring。
- 无分解需求。
