# Part 5: L2 fixtures + TS↔Rust 字段对照测试

本部分为 4.3.2 的 Rust 实现补充 L2 字段级对照：由 TS 生成与 Rust fixture 等价但独立的 JSON，Rust 反序列化 TS fixture 断言字段；TS 反序列化 Rust fixture 断言字段。所有 fixture 集中存放在 `rust-ody/crates/agent-rs/tests/fixtures/`，避免路径碎片化。

---

### Task 1: TS fixture 生成脚本

**Depends on:** `config.md` Task 3、`usage.md` Task 2、`tool.md` Task 5（Rust fixture 生成器与目录已就绪）

**Files:**
- Create: `scripts/generate-config-usage-tool-fixtures.ts`
- Create: `rust-ody/crates/agent-rs/tests/fixtures/config-ts.json`
- Create: `rust-ody/crates/agent-rs/tests/fixtures/usage-ts.json`
- Create: `rust-ody/crates/agent-rs/tests/fixtures/tools-ts.json`

**目标：** 用 TS 生成与 Rust 侧语义一致的 JSON fixture，使两边在字段名与数据类型上互相校验。

- [ ] 新建 `scripts/generate-config-usage-tool-fixtures.ts`：

```ts
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.resolve(__dirname, '../rust-ody/crates/agent-rs/tests/fixtures');
mkdirSync(outDir, { recursive: true });

const configFixture = {
  cwd: '/fixture/cwd',
  provider: {
    type: 'kimi',
    model: 'kimi-k2',
  },
  modelAlias: 'kimi-k2',
  modelCapabilities: {
    imageIn: false,
    videoIn: false,
    audioIn: false,
    thinking: true,
    toolUse: true,
    maxContextTokens: 256000,
    maxOutputTokens: 16384,
  },
  profileName: 'fixture',
  thinkingLevel: 'high',
  systemPrompt: 'fixture system prompt',
};

const usageFixture = {
  byModel: {
    'kimi-k2': {
      inputOther: 13,
      output: 7,
      inputCacheRead: 2,
      inputCacheCreation: 1,
    },
  },
  total: {
    inputOther: 13,
    output: 7,
    inputCacheRead: 2,
    inputCacheCreation: 1,
  },
  currentTurn: {
    inputOther: 3,
    output: 2,
    inputCacheRead: 0,
    inputCacheCreation: 0,
  },
};

const toolsFixture = [
  { name: 'Bash', description: 'Execute a shell command.', active: false, source: 'builtin' },
  { name: 'Edit', description: 'Apply a targeted edit to a text file.', active: false, source: 'builtin' },
  { name: 'Glob', description: 'Find files matching a glob pattern.', active: false, source: 'builtin' },
  { name: 'Grep', description: 'Search file contents with a regex.', active: true, source: 'builtin' },
  { name: 'Read', description: 'Read a text file from the local filesystem.', active: true, source: 'builtin' },
  { name: 'Write', description: 'Write or overwrite a text file.', active: false, source: 'builtin' },
  { name: 'custom_user_tool', description: 'A user-registered tool for fixture generation.', active: true, source: 'user' },
];

writeFileSync(path.join(outDir, 'config-ts.json'), JSON.stringify(configFixture, null, 2));
writeFileSync(path.join(outDir, 'usage-ts.json'), JSON.stringify(usageFixture, null, 2));
writeFileSync(path.join(outDir, 'tools-ts.json'), JSON.stringify(toolsFixture, null, 2));

console.log('TS fixtures written to', outDir);
```

- [ ] 生成 fixture：

```bash
cd /Users/ranwei/workspace/ody-code && pnpm exec tsx scripts/generate-config-usage-tool-fixtures.ts
```

预期输出：

```text
TS fixtures written to /Users/ranwei/workspace/ody-code/rust-ody/crates/agent-rs/tests/fixtures
```

并创建 `config-ts.json`、`usage-ts.json`、`tools-ts.json`。

- [ ] 手动验证 fixture 存在且字段为 camelCase：

```bash
ls rust-ody/crates/agent-rs/tests/fixtures/ && head -5 rust-ody/crates/agent-rs/tests/fixtures/config-ts.json
```

预期输出：列出 6 个 JSON 文件（3 个 Rust + 3 个 TS），且 `config-ts.json` 首行包含 `"cwd"`。

- [ ] Commit：`test(agent-rs): add TS-side L2 fixtures for config/usage/tools`

---

### Task 2: Rust 读取 TS fixture 对照测试

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/agent-rs/tests/config_ts_fixture_parity.rs`
- Create: `rust-ody/crates/agent-rs/tests/usage_ts_fixture_parity.rs`
- Create: `rust-ody/crates/agent-rs/tests/tool_ts_fixture_parity.rs`

**目标：** 让 Rust 用自身类型反序列化 TS 生成的 fixture，验证字段名与数据类型互通。

- [ ] 新建 `rust-ody/crates/agent-rs/tests/config_ts_fixture_parity.rs`：

```rust
use agent_rs::config::AgentConfigData;

#[test]
fn ts_config_fixture_matches_rust_expectations() {
    let json = include_str!("fixtures/config-ts.json");
    let data: AgentConfigData = serde_json::from_str(json).unwrap();

    assert_eq!(data.cwd, "/fixture/cwd");
    assert_eq!(data.model_alias, Some("kimi-k2".into()));
    assert_eq!(data.profile_name, Some("fixture".into()));
    assert_eq!(data.thinking_level, "high");
    assert_eq!(data.system_prompt, "fixture system prompt");
    assert!(data.model_capabilities.thinking);
    assert!(data.provider.is_some());
}
```

- [ ] 新建 `rust-ody/crates/agent-rs/tests/usage_ts_fixture_parity.rs`：

```rust
use agent_rs::usage::UsageStatus;

#[test]
fn ts_usage_fixture_matches_rust_expectations() {
    let json = include_str!("fixtures/usage-ts.json");
    let status: UsageStatus = serde_json::from_str(json).unwrap();

    let by_model = status.by_model.as_ref().unwrap();
    let kimi = by_model.get("kimi-k2").unwrap();
    assert_eq!(kimi.input_other, 13);
    assert_eq!(kimi.output, 7);
    assert_eq!(kimi.input_cache_read, 2);
    assert_eq!(kimi.input_cache_creation, 1);
    assert_eq!(status.total.unwrap().output, 7);
    assert_eq!(status.current_turn.unwrap().output, 2);
}
```

- [ ] 新建 `rust-ody/crates/agent-rs/tests/tool_ts_fixture_parity.rs`：

```rust
use agent_rs::tool::{ToolInfo, ToolSource};

#[test]
fn ts_tools_fixture_matches_rust_expectations() {
    let json = include_str!("fixtures/tools-ts.json");
    let infos: Vec<ToolInfo> = serde_json::from_str(json).unwrap();

    let active: Vec<_> = infos
        .iter()
        .filter(|i| i.active)
        .map(|i| i.name.as_str())
        .collect();
    assert!(active.contains(&"Read"));
    assert!(active.contains(&"Grep"));
    assert!(active.contains(&"custom_user_tool"));

    let custom = infos.iter().find(|i| i.name == "custom_user_tool").unwrap();
    assert_eq!(custom.source, ToolSource::User);
}
```

- [ ] 运行 Rust parity 测试：

```bash
cd rust-ody && cargo test -p agent-rs parity
```

预期输出：包含 `config_ts_fixture_parity::ts_config_fixture_matches_rust_expectations`、`usage_ts_fixture_parity::ts_usage_fixture_matches_rust_expectations`、`tool_ts_fixture_parity::ts_tools_fixture_matches_rust_expectations` 均通过，以及此前 Rust fixture round-trip 测试也通过；最终 `test result: ok.`。

- [ ] Commit：`test(agent-rs): add Rust tests that read TS-generated fixtures`

---

### Task 3: TS 读取 Rust fixture 对照测试

**Depends on:** Task 1（Rust fixture 已存在）

**Files:**
- Create: `packages/agent-core/src/agent/config/config.parity.test.ts`
- Create: `packages/agent-core/src/agent/tool/tool.parity.test.ts`
- Create: `packages/agent-core/src/agent/usage/usage.parity.test.ts`

**目标：** 让 TS 用普通对象读取 Rust 生成的 fixture，验证字段名与数据类型互通。

- [ ] 新建 `packages/agent-core/src/agent/config/config.parity.test.ts`：

```ts
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { test, expect } from 'vitest';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const fixture = path.resolve(
  __dirname,
  '../../../../../rust-ody/crates/agent-rs/tests/fixtures/config-rust.json',
);

test('rust config fixture matches TS expectations', () => {
  const data = JSON.parse(readFileSync(fixture, 'utf8'));
  expect(data.cwd).toBe('/fixture/cwd');
  expect(data.modelAlias).toBe('kimi-k2');
  expect(data.profileName).toBe('fixture');
  expect(data.thinkingLevel).toBe('high');
  expect(data.systemPrompt).toBe('fixture system prompt');
  expect(data.modelCapabilities).toEqual({
    imageIn: false,
    videoIn: false,
    audioIn: false,
    thinking: true,
    toolUse: true,
    maxContextTokens: 256000,
    maxOutputTokens: 16384,
  });
  expect(data.provider.type).toBe('kimi');
  expect(data.provider.model).toBe('kimi-k2');
});
```

- [ ] 新建 `packages/agent-core/src/agent/tool/tool.parity.test.ts`：

```ts
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { test, expect } from 'vitest';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const fixture = path.resolve(
  __dirname,
  '../../../../../rust-ody/crates/agent-rs/tests/fixtures/tools-rust.json',
);

test('rust tools fixture matches TS expectations', () => {
  const infos = JSON.parse(readFileSync(fixture, 'utf8'));
  const active = infos
    .filter((i: { active: boolean }) => i.active)
    .map((i: { name: string }) => i.name);
  expect(active).toContain('Read');
  expect(active).toContain('Grep');
  expect(active).toContain('custom_user_tool');

  const custom = infos.find((i: { name: string }) => i.name === 'custom_user_tool');
  expect(custom.source).toBe('user');
});
```

- [ ] 新建 `packages/agent-core/src/agent/usage/usage.parity.test.ts`：

```ts
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { test, expect } from 'vitest';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const fixture = path.resolve(
  __dirname,
  '../../../../../rust-ody/crates/agent-rs/tests/fixtures/usage-rust.json',
);

test('rust usage fixture matches TS expectations', () => {
  const data = JSON.parse(readFileSync(fixture, 'utf8'));
  expect(data.byModel['kimi-k2']).toEqual({
    inputOther: 13,
    output: 7,
    inputCacheRead: 2,
    inputCacheCreation: 1,
  });
  expect(data.total).toEqual({
    inputOther: 13,
    output: 7,
    inputCacheRead: 2,
    inputCacheCreation: 1,
  });
  expect(data.currentTurn).toEqual({
    inputOther: 3,
    output: 2,
    inputCacheRead: 0,
    inputCacheCreation: 0,
  });
});
```

- [ ] 运行 TS parity 测试：

```bash
cd packages/agent-core && pnpm vitest run parity
```

预期输出：3 个 parity 测试全部通过。

- [ ] 运行全仓库类型检查：

```bash
cd /Users/ranwei/workspace/ody-code && pnpm -r typecheck
```

预期输出：无类型错误。

- [ ] 运行 Rust 完整测试：

```bash
cd rust-ody && cargo test -p agent-rs --workspace --tests
```

预期输出：`test result: ok.`，所有 4.3.2 新增测试通过。

- [ ] Commit：`test(agent-core): add TS parity tests reading Rust fixtures`

---

## Local Self-Review

- [ ] 1. Spec-coverage：本部分覆盖 Roadmap 4.3.2.5（L2 fixture / TS↔Rust 字段对照）。
- [ ] 2. Placeholder扫描：无 TODO/TBD；fixture 路径与数据均为具体值。
- [ ] 3. No phantom tasks：Task 1 生成 3 个 TS fixture；Task 2 新增 3 个 Rust 读取测试；Task 3 新增 3 个 TS 读取测试。
- [ ] 4. Dependency soundness：Task 1 依赖前四 part 的 fixture 生成器；Task 2/3 依赖 Task 1；无反向依赖。
- [ ] 5. Caller & build soundness：本部分新增测试文件，未修改共享签名；Task 3 以 `pnpm -r typecheck` 做全仓类型检查，以 `cargo test -p agent-rs --workspace --tests` 做 Rust 全测试。
- [ ] 6. Test-the-risk：每个测试断言具体字段值，这些值与 TS 生成脚本和 Rust fixture 生成器中的常量一致；若字段名不匹配，反序列化或 JSON 读取会失败。
- [ ] 7. Type一致性：测试中使用的字段名（`cwd`、`modelAlias`、`thinkingLevel`、`inputOther`、`inputCacheRead`、`byModel`、`currentTurn`、`source` 等）与 TS/Rust 类型定义一致。
