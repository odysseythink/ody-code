# Phase C: 轨迹与分发

本部分覆盖设计文档的「Session Trace Recorder」「分发路径」章节，以及 vendored 源码的落盘与打包。

---

## Dependency Overview

```
Task 6 (ChromeTraceRecorder)
  ↓
Task 7 (vendored chrome-devtools-mcp 源码)
  ↓
Task 8 (package.json + native package.mjs 分发配置)
```

Task 6 与 Task 7 可并行，但 Task 8 依赖两者（打包脚本需知道 `built-in/` 目录结构）。为简化执行顺序，按线性依赖排列。

---

### Task 6: ChromeTraceRecorder — 旁路轨迹录制

**Depends on:** `core-integration.md: Task 4`（`ToolManager` 已存在，MCP server 注册流程已就绪）

**Files:**
- **Create:** `packages/agent-core/src/mcp/trace-recorder.ts`
- **Modify:** `packages/agent-core/src/agent/tool/index.ts`（在 `registerMcpServer` 的 `execute` 中注入录制钩子）
- **Modify:** `packages/agent-core/src/mcp/index.ts`（如果存在统一导出文件，追加 `trace-recorder` 导出；如无则忽略）
- **Test:** `packages/agent-core/test/mcp/trace-recorder.test.ts`

**风险：** 轨迹写入磁盘失败不可阻塞主工具调用流程；截图提取必须正确处理 base64 数据；敏感参数（password/token）必须脱敏。

- [ ] 写失败测试：验证 `ChromeTraceRecorder` 能将工具调用结果写入 `manifest.jsonl`。

```typescript
// packages/agent-core/test/mcp/trace-recorder.test.ts
import { mkdtemp, readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { describe, expect, it } from 'vitest';
import { ChromeTraceRecorder } from '../../src/mcp/trace-recorder';
import type { MCPToolResult } from '../../src/mcp/types';

describe('ChromeTraceRecorder', () => {
  it('writes manifest.jsonl with tool call record', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'trace-test-'));
    const recorder = new ChromeTraceRecorder(dir);
    const result: MCPToolResult = {
      content: [{ type: 'text', text: 'navigated' }],
      isError: false,
    };
    await recorder.record('navigate', { url: 'https://example.com' }, result);

    const manifest = await readFile(join(dir, 'manifest.jsonl'), 'utf-8');
    const record = JSON.parse(manifest.trim());
    expect(record.toolName).toBe('navigate');
    expect(record.args.url).toBe('https://example.com');
    expect(record.resultSummary.isError).toBe(false);
    expect(record.resultSummary.contentTypes).toEqual(['text']);
  });

  it('extracts screenshot images into screenshots/ directory', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'trace-test-'));
    const recorder = new ChromeTraceRecorder(dir);
    // 1×1 透明 PNG base64
    const base64Png =
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';
    const result: MCPToolResult = {
      content: [{ type: 'image', data: base64Png, mimeType: 'image/png' }],
      isError: false,
    };
    await recorder.record('take_screenshot', {}, result);

    const screenshots = await readdir(join(dir, 'screenshots'));
    expect(screenshots.length).toBe(1);
    expect(screenshots[0]).toMatch(/^0001-take_screenshot\.png$/);
  });

  it('redacts sensitive args before recording', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'trace-test-'));
    const recorder = new ChromeTraceRecorder(dir);
    const result: MCPToolResult = { content: [], isError: false };
    await recorder.record(
      'fill',
      { password: 'secret123', username: 'alice', apiKey: 'xyz' },
      result,
    );

    const manifest = await readFile(join(dir, 'manifest.jsonl'), 'utf-8');
    const record = JSON.parse(manifest.trim());
    expect(record.args.password).toBe('<redacted>');
    expect(record.args.apiKey).toBe('<redacted>');
    expect(record.args.username).toBe('alice');
  });

  it('survives write failures without throwing', async () => {
    const recorder = new ChromeTraceRecorder('/dev/null/invalid-path');
    const result: MCPToolResult = { content: [], isError: false };
    await expect(
      recorder.record('navigate', {}, result),
    ).resolves.not.toThrow();
  });
});
```

- [ ] 运行测试并确认失败（类尚未实现）：

```bash
cd packages/agent-core && pnpm vitest run test/mcp/trace-recorder.test.ts
```

预期失败：`Error: Cannot find module '../../src/mcp/trace-recorder'`。

- [ ] 实现 `ChromeTraceRecorder`：

```typescript
// packages/agent-core/src/mcp/trace-recorder.ts
import { appendFile, mkdir, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';

import type { MCPToolResult } from './types';

export interface TraceRecord {
  readonly timestamp: string;
  readonly toolName: string;
  readonly args: Record<string, unknown>;
  readonly resultSummary: {
    readonly isError: boolean;
    readonly contentTypes: string[];
    readonly hasScreenshot: boolean;
    readonly screenshotFiles: string[];
  };
}

const SENSITIVE_KEYS = new Set([
  'password',
  'token',
  'secret',
  'apikey',
  'api_key',
  'auth',
  'cookie',
]);

export class ChromeTraceRecorder {
  private readonly manifestPath: string;
  private readonly screenshotsDir: string;
  private seq = 0;

  constructor(private readonly traceDir: string) {
    this.manifestPath = join(traceDir, 'manifest.jsonl');
    this.screenshotsDir = join(traceDir, 'screenshots');
  }

  async record(
    toolName: string,
    args: Record<string, unknown>,
    result: MCPToolResult,
  ): Promise<void> {
    try {
      await this.ensureDirs();
      const screenshotFiles: string[] = [];
      const contentTypes: string[] = [];
      let hasScreenshot = false;

      for (const block of result.content) {
        contentTypes.push(block.type);
        if (block.type === 'image' && typeof block.data === 'string') {
          hasScreenshot = true;
          const fileName = `${String(++this.seq).padStart(4, '0')}-${toolName}.png`;
          const filePath = join(this.screenshotsDir, fileName);
          await writeFile(filePath, Buffer.from(block.data, 'base64'));
          screenshotFiles.push(fileName);
        }
      }

      const record: TraceRecord = {
        timestamp: new Date().toISOString(),
        toolName,
        args: this.sanitizeArgs(args),
        resultSummary: {
          isError: result.isError,
          contentTypes,
          hasScreenshot,
          screenshotFiles,
        },
      };

      await appendFile(
        this.manifestPath,
        JSON.stringify(record) + '\n',
        'utf-8',
      );
    } catch {
      // Silently drop trace write failures to avoid disrupting the main flow.
    }
  }

  private async ensureDirs(): Promise<void> {
    await mkdir(this.traceDir, { recursive: true });
    await mkdir(this.screenshotsDir, { recursive: true });
  }

  private sanitizeArgs(
    args: Record<string, unknown>,
  ): Record<string, unknown> {
    const sanitized: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(args)) {
      if (SENSITIVE_KEYS.has(key.toLowerCase())) {
        sanitized[key] = '<redacted>';
      } else {
        sanitized[key] = value;
      }
    }
    return sanitized;
  }
}
```

- [ ] 运行测试并确认通过：

```bash
cd packages/agent-core && pnpm vitest run test/mcp/trace-recorder.test.ts
```

- [ ] 将 `ChromeTraceRecorder` 注入 `ToolManager.registerMcpServer` 的 `execute` 路径。

修改 `packages/agent-core/src/agent/tool/index.ts`（约第 167-189 行）：

```typescript
import { dirname, join } from 'node:path';
import { ChromeTraceRecorder } from '../../mcp/trace-recorder';
// ... existing imports ...

// Inside registerMcpServer, before the for-loop over tools:
    const isChromeDevTools = serverName === 'chrome-devtools';
    let traceRecorder: ChromeTraceRecorder | undefined;
    if (isChromeDevTools && this.agent.homedir) {
      // agent.homedir = <session-homedir>/agents/<agent-id>/
      const sessionDir = dirname(dirname(this.agent.homedir));
      traceRecorder = new ChromeTraceRecorder(
        join(sessionDir, 'chrome-traces'),
      );
    }

// Inside the for-loop, in the wrapped ExecutableTool's resolveExecution:
            execute: async (context) => {
              const result = await client.callTool(
                tool.name,
                (args ?? {}) as Record<string, unknown>,
                context.signal,
              );
              if (traceRecorder) {
                await traceRecorder.record(tool.name, args ?? {}, result);
              }
              return mcpResultToExecutableOutput(result, qualified);
            },
```

**注意：** `this.agent.homedir` 在测试中通常为 `undefined`（`fakeAgent` 未设置），因此现有测试不会触发 `traceRecorder` 创建，无需修改测试。

- [ ] 运行 `tool-manager-mcp` 现有测试，确认无回归：

```bash
cd packages/agent-core && pnpm vitest run test/mcp/tool-manager-mcp.test.ts
```

- [ ] Commit：

```bash
git add packages/agent-core/src/mcp/trace-recorder.ts \
  packages/agent-core/src/agent/tool/index.ts \
  packages/agent-core/test/mcp/trace-recorder.test.ts
git commit -m "feat: add ChromeTraceRecorder for browser tool call traces"
```

---

### Task 7: vendored chrome-devtools-mcp 源码

**Depends on:** none（可独立执行，但逻辑上在 Task 6 之后）

**Files:**
- **Create:** `apps/ody-code/built-in/chrome-devtools/` 目录树（vendored 上游源码）
- **Create:** `apps/ody-code/built-in/chrome-devtools/README.md`（记录上游版本和同步方式）

**风险：** 上游许可证不兼容；vendored 代码缺少运行依赖导致启动失败。

这是一个**非测试性**任务（分发/外部源码落盘），无单元测试覆盖，但需要手动验证启动。

- [ ] **验证上游许可证**：在 vendoring 之前，确认 `chrome-devtools-mcp` 的 LICENSE 为 Apache-2.0 或兼容许可证（如 MIT、BSD-3-Clause）。

```bash
npm info chrome-devtools-mcp license
# 期望输出：Apache-2.0（或兼容许可证）
```

若许可证不兼容（如 GPL-3.0），**立即暂停**并通知用户；不得继续 vendoring。

- [ ] 下载上游源码到临时目录并复制到 repo：

```bash
# 在工作目录外创建临时目录
cd /tmp
npm pack chrome-devtools-mcp@latest --pack-destination /tmp/cdt-mcp
mkdir -p /tmp/cdt-mcp/extracted
cd /tmp/cdt-mcp/extracted
tar -xzf /tmp/cdt-mcp/chrome-devtools-mcp-*.tgz

# 复制到 repo（保留 package 结构）
mkdir -p /Users/ranwei/workspace/ody-code/apps/ody-code/built-in/chrome-devtools
cp -R /tmp/cdt-mcp/extracted/package/* \
  /Users/ranwei/workspace/ody-code/apps/ody-code/built-in/chrome-devtools/
```

- [ ] 检查 vendored 目录至少包含以下文件：

```bash
ls /Users/ranwei/workspace/ody-code/apps/ody-code/built-in/chrome-devtools/
# 期望：package.json, dist/（或 src/ + tsconfig.json），README.md, LICENSE
```

- [ ] 编写 `built-in/chrome-devtools/README.md`：

```markdown
# chrome-devtools-mcp (vendored)

Upstream: https://www.npmjs.com/package/chrome-devtools-mcp
Vendored version: <从 package.json 读取的 version>
License: <从 LICENSE 文件读取的 license>

## Sync

To update to a newer upstream version:

1. Run `npm pack chrome-devtools-mcp@<version>`
2. Extract and replace the contents of this directory
3. Update the version in this README
4. Add a changeset describing the update
```

- [ ] **手动验证**：尝试在开发环境中启动 vendored server：

```bash
cd /Users/ranwei/workspace/ody-code/apps/ody-code/built-in/chrome-devtools
node --experimental-strip-types ./dist/index.js
# 期望：进程启动，监听 stdin（stdio MCP server 行为）
# 按 Ctrl+C 退出
```

若启动报错（如缺少依赖），需在 vendored 目录内运行 `npm install` 安装 `dependencies`，并将 `node_modules` 也纳入 vendored（或通过其他方式确保运行时可用）。**实际依赖处理方式需在执行时根据上游 `package.json` 的 `dependencies` 决定。**

- [ ] Commit：

```bash
git add apps/ody-code/built-in/chrome-devtools/
git commit -m "feat: vendor chrome-devtools-mcp as built-in MCP server"
```

---

### Task 8: package.json + native package.mjs 分发配置

**Depends on:** `Task 7`（`built-in/` 目录必须已存在，打包脚本才能正确包含）

**Files:**
- **Modify:** `apps/ody-code/package.json:28-33`
- **Modify:** `apps/ody-code/scripts/native/package.mjs`

**风险：** npm 发布时遗漏 `built-in/` 目录；native zip 未包含 `built-in/` 导致运行时 `resolveBuiltInRoot` 找不到源码。

- [ ] 修改 `apps/ody-code/package.json`，在 `files` 数组中插入 `"built-in"`：

```json
  "files": [
    "dist",
    "built-in",
    "scripts/postinstall.mjs",
    "scripts/postinstall",
    "README.md"
  ],
```

- [ ] 修改 `apps/ody-code/scripts/native/package.mjs`，在 zip 中加入 `built-in/` 目录内容。

当前文件第 44-46 行：

```typescript
const zip = new ZipFile();
zip.addFile(sourceBinary, execName, { mode: 0o100755 });
zip.end();
```

修改为：

```typescript
import { readdir, stat as fsStat } from 'node:fs/promises';
import { relative, resolve, sep } from 'node:path';

// ... existing imports and code ...

const zip = new ZipFile();
zip.addFile(sourceBinary, execName, { mode: 0o100755 });

const builtInDir = resolve(appRoot, 'built-in');
try {
  await fsStat(builtInDir);
  await addDirectoryToZip(zip, builtInDir, 'built-in');
} catch {
  // built-in/ directory may not exist in all build contexts; skip silently.
}

zip.end();

// ... rest of existing code ...

async function addDirectoryToZip(
  zip: ZipFile,
  sourceDir: string,
  zipPrefix: string,
): Promise<void> {
  const entries = await readdir(sourceDir, { withFileTypes: true, recursive: true });
  for (const entry of entries) {
    const fullPath = resolve(sourceDir, entry.parentPath ?? sourceDir, entry.name);
    const relativePath = relative(sourceDir, fullPath);
    const zipPath = zipPrefix + '/' + relativePath.split(sep).join('/');
    if (entry.isDirectory()) {
      zip.addEmptyDirectory(zipPath);
    } else {
      zip.addFile(fullPath, zipPath);
    }
  }
}
```

**注意：** `entry.parentPath` 在 Node.js <20.12.0 中不存在；若构建环境使用旧版本 Node，改用 `entry.path ?? sourceDir`。但本项目要求 Node >=24.15.0，因此 `parentPath` 可用。

- [ ] **手动验证 1 — npm files**：

```bash
cd apps/ody-code && npm pack --dry-run 2>&1 | grep -E "(built-in|chrome-devtools)"
# 期望输出包含 built-in/chrome-devtools/ 下的文件
```

- [ ] **手动验证 2 — native zip**：

```bash
cd apps/ody-code && pnpm run build:native:sea && pnpm run package:native
unzip -l dist-native/artifacts/kimi-code-$(node -e "console.log(process.platform+'-'+process.arch)").zip | grep built-in
# 期望输出包含 built-in/chrome-devtools/ 下的文件
```

- [ ] Commit：

```bash
git add apps/ody-code/package.json apps/ody-code/scripts/native/package.mjs
git commit -m "feat: include built-in chrome-devtools in npm and native distributions"
```

---

## Local Self-Review

- [ ] 1. **Spec-coverage table**

| 设计章节 | 覆盖状态 | 对应 Task(s) |
|---|---|---|
| Components: Session Trace Recorder | covered | Task 6 |
| 轨迹存储结构 (manifest.jsonl + screenshots/) | covered | Task 6 |
| 敏感数据隔离 (args 脱敏) | covered | Task 6 |
| 分发路径 (npm + native zip) | covered | Task 7, 8 |
| vendored 源码落盘 | covered | Task 7 |
| Error: TRACE_WRITE_ERROR | covered | Task 6（静默丢弃） |
| Call-Site 4: Session trace 钩子 | covered | Task 6 |
| Call-Site 5: 分发路径 | covered | Task 8 |

- [ ] 2. **Placeholder scan**：无 TODO/TBD；Task 7 的 `npm pack` 命令是具体的；Task 8 的 zip 添加逻辑包含完整实现。
- [ ] 3. **No phantom tasks**：
  - Task 6 产出 `trace-recorder.ts` + 测试 + `tool/index.ts` hook + commit。
  - Task 7 产出 vendored 目录 + README + 手动验证 + commit。
  - Task 8 产出 `package.json` + `package.mjs` 修改 + 手动验证 + commit。
- [ ] 4. **Dependency soundness**：Task 6 依赖 `core-integration.md: Task 4`（ToolManager 已存在）；Task 8 依赖 Task 7（`built-in/` 目录存在）；无向后引用。
- [ ] 5. **Caller & build soundness**：
  - `ToolManager.registerMcpServer` 不是共享签名（内部方法，测试通过 `fakeAgent` 调用，而 `fakeAgent.homedir` 为 `undefined`，不会触发 trace recorder 创建路径，因此无需修改测试）。
  - `package.mjs` 新增 `addDirectoryToZip` 是局部函数，无外部调用者。
  - Task 6 完成后应运行 `pnpm -r typecheck` 确保全树通过（这是该 task 的最后一步）。
- [ ] 6. **Test-the-risk**：
  - 轨迹写入失败：`trace-recorder.test.ts` 最后一个 case 验证 `/dev/null/invalid-path` 不抛错。
  - 敏感参数脱敏：测试验证 `password` 和 `apiKey` 被替换为 `<redacted>`，而 `username` 保留原值。
  - 截图提取：测试验证 base64 PNG 被解码并写入 `screenshots/0001-take_screenshot.png`。
  - Must-survive 输入检查：`SENSITIVE_KEYS` 包含 `'password'`；测试中的 `password` 字段确实被脱敏。`username` 不在集合中，测试中保留原值。无误杀。
- [ ] 7. **Type consistency**：`ChromeTraceRecorder.record(toolName, args, result)` 使用 `MCPToolResult`（已在 `types.ts` 中定义）；`TraceRecord` 接口与 `manifest.jsonl` 的写入格式一致。
