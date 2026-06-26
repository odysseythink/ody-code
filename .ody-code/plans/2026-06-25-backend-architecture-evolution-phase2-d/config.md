# Part 2：OdyConfig schema 扩展

本 part 将 `OdyConfigSchema` 与 `OdyConfigPatchSchema` 中的 `sessionMode` / `defaultSessionMode` 从 `'plan' | 'design'` 扩展为 `RuntimeMode`，保持对旧值的兼容。

## Task 2.1：在 `agent-core-shared` 中复用 `RuntimeMode` 定义

**Depends on:** Task 1.1（types.ts 已存在）

**Files:**
- Modify: `packages/agent-core-shared/src/config.ts`
- Test: `packages/agent-core/test/config/configs.test.ts`

### 分析与决策

`agent-core-shared` 不能依赖 `agent-core`（shared 在依赖关系更底层），因此不能把 `RuntimeMode` 类型从 `agent-core` 导入到 `agent-core-shared`。本任务采用**本地内联常量**的方式保持类型一致：在 `agent-core-shared/src/config.ts` 中定义与 Part 1 完全相同的 `RUNTIME_MODES` 常量与 `RuntimeMode` 类型，并添加注释说明需与 `agent-core` 同步。

### 步骤

- [ ] **Write the failing test**

  修改 `packages/agent-core/test/config/configs.test.ts`，新增用例（若文件不存在则创建）：

  ```ts
  import { describe, it, expect } from 'vitest';
  import { OdyConfigSchema, validateConfig } from '@odysseythink/agent-core-shared';

  describe('OdyConfig sessionMode schema', () => {
    it('accepts all runtime modes for sessionMode and defaultSessionMode', () => {
      for (const mode of ['plan', 'design', 'office-hours', 'game-design', 'normal'] as const) {
        expect(validateConfig({ sessionMode: mode }).sessionMode).toBe(mode);
        expect(validateConfig({ defaultSessionMode: mode }).defaultSessionMode).toBe(mode);
      }
    });

    it('rejects unknown modes', () => {
      expect(() => validateConfig({ sessionMode: 'foo' as any })).toThrow();
      expect(() => validateConfig({ defaultSessionMode: 'foo' as any })).toThrow();
    });
  });
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  pnpm test packages/agent-core/test/config/configs.test.ts
  ```

  预期失败：`'office-hours'` / `'game-design'` / `'normal'` 不被当前 schema 接受。

- [ ] **Write the minimal implementation**

  修改 `packages/agent-core-shared/src/config.ts`：

  1. 在文件顶部（`ProviderTypeSchema` 之前）添加：

  ```ts
  /**
   * Runtime modes mirror packages/agent-core/src/agent/session-mode/types.ts.
   * Keep these two sources in sync — agent-core-shared cannot import from agent-core.
   */
  export const RUNTIME_MODES = ['plan', 'design', 'office-hours', 'game-design', 'normal'] as const;
  export type RuntimeMode = typeof RUNTIME_MODES[number];
  export const SESSION_MODE_KINDS = RUNTIME_MODES.slice(0, -1) as ['plan', 'design', 'office-hours', 'game-design'];
  ```

  2. 将 `OdyConfigSchema` 中：

  ```ts
  sessionMode: z.enum(['plan', 'design']).optional(),
  ```

  改为：

  ```ts
  sessionMode: z.enum(RUNTIME_MODES as [string, ...string[]]).optional(),
  ```

  将：

  ```ts
  defaultSessionMode: z.enum(['plan', 'design']).optional(),
  ```

  改为：

  ```ts
  defaultSessionMode: z.enum(RUNTIME_MODES as [string, ...string[]]).optional(),
  ```

  3. 将 `OdyConfigPatchSchema` 中同样两处 `'plan', 'design'` enum 替换为 `z.enum(RUNTIME_MODES as [string, ...string[]])`。

  4. 在文件末尾导出类型：

  ```ts
  export type { RuntimeMode };
  ```

  （已内联导出，此步可省略；确保 `index.ts` 有 `export * from './config'` 即可。）

- [ ] **Run it and verify it PASSES**

  ```bash
  pnpm test packages/agent-core/test/config/configs.test.ts
  ```

  预期：所有测试通过。

- [ ] **Commit**

  ```bash
  git add packages/agent-core-shared/src/config.ts packages/agent-core/test/config/configs.test.ts
  git commit -m "feat(agent-core-shared): extend sessionMode/defaultSessionMode schema to RuntimeMode"
  ```

## Task 2.2：验证 TOML 写回不会破坏 `normal` 等新模式值

**Depends on:** Task 2.1

**Files:**
- Test: `packages/agent-core/test/config/configs.test.ts`

### 步骤

- [ ] **Write the failing test**

  在 `packages/agent-core/test/config/configs.test.ts` 追加：

  ```ts
  import { configToTomlData } from '../../src/config/toml';

  describe('configToTomlData round-trips runtime modes', () => {
    it('preserves normal and office-hours in TOML output', () => {
      const config = validateConfig({
        sessionMode: 'office-hours',
        defaultSessionMode: 'normal',
      });
      const tomlData = configToTomlData(config);
      expect(tomlData.sessionMode).toBe('office-hours');
      expect(tomlData.defaultSessionMode).toBe('normal');
    });
  });
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  pnpm test packages/agent-core/test/config/configs.test.ts
  ```

  预期失败：若 `configToTomlData` 对未知枚举值有处理则通过；否则测试会暴露问题。当前实现只是透传值，应通过；若失败则修复 `configToTomlData` 的 scalarFields 循环。

- [ ] **Write the minimal implementation**

  若测试失败，检查 `packages/agent-core/src/config/toml.ts` 中 `scalarFields` 是否包含 `'sessionMode'` 和 `'defaultSessionMode`（已包含）。`setDefined` 会透传任意值，无需额外修改。

- [ ] **Run it and verify it PASSES**

  ```bash
  pnpm test packages/agent-core/test/config/configs.test.ts
  ```

- [ ] **Commit**

  ```bash
  git add packages/agent-core/test/config/configs.test.ts
  git commit -m "test(agent-core): verify RuntimeMode round-trips through TOML"
  ```

## Local Self-Review

- [ ] 1. Spec-coverage：覆盖 Scope In #3（扩展 `OdyConfig.sessionMode` / `defaultSessionMode`）。
- [ ] 2. Placeholder scan：无 TODO；常量与注释完整。
- [ ] 3. No phantom tasks：Task 2.2 即使实现无代码变更，也以测试记录验证结果。
- [ ] 4. Dependency soundness：Task 2.1 依赖 Part 1 的常量形状；Task 2.2 依赖 Task 2.1。
- [ ] 5. Caller & build soundness：本 part 修改 shared 配置类型，但尚未替换调用点；全树 typecheck 在 Part 6 最后统一执行。
- [ ] 6. Test-the-risk：测试枚举所有 5 个 runtime mode 与 1 个非法值；断言与 `RUNTIME_MODES` 常量一致。
- [ ] 7. Type consistency：`RuntimeMode` 在 shared 层内联定义，与 agent-core 的 `RUNTIME_MODES` 顺序/内容一致。
