# Part 1：RuntimeMode 类型与守卫

本 part 建立 Phase 2-D 的两层类型基础：`SessionModeKind`（4 种交互阶段）与 `RuntimeMode`（含 `normal`），并提供类型守卫与降级函数。

## Task 1.1：创建 `packages/agent-core/src/agent/session-mode/types.ts`

**Depends on:** none

**Files:**
- Create: `packages/agent-core/src/agent/session-mode/types.ts`
- Test: `packages/agent-core/src/agent/session-mode/__tests__/mode-types.test.ts`

### 步骤

- [ ] **Write the failing test**

  新建 `packages/agent-core/src/agent/session-mode/__tests__/mode-types.test.ts`：

  ```ts
  import { describe, it, expect, vi } from 'vitest';
  import {
    SESSION_MODE_KINDS,
    RUNTIME_MODES,
    isSessionModeKind,
    isRuntimeMode,
    normalizeRuntimeMode,
  } from '../types';

  describe('mode types', () => {
    it('SESSION_MODE_KINDS has exactly the four interaction phases', () => {
      expect(SESSION_MODE_KINDS).toEqual(['plan', 'design', 'office-hours', 'game-design']);
    });

    it('RUNTIME_MODES appends normal to session mode kinds', () => {
      expect(RUNTIME_MODES).toEqual([...SESSION_MODE_KINDS, 'normal']);
    });

    it('isSessionModeKind accepts the four kinds and rejects others', () => {
      expect(isSessionModeKind('plan')).toBe(true);
      expect(isSessionModeKind('office-hours')).toBe(true);
      expect(isSessionModeKind('normal')).toBe(false);
      expect(isSessionModeKind('foo')).toBe(false);
    });

    it('isRuntimeMode accepts all runtime modes and rejects others', () => {
      for (const mode of RUNTIME_MODES) {
        expect(isRuntimeMode(mode)).toBe(true);
      }
      expect(isRuntimeMode('foo')).toBe(false);
      expect(isRuntimeMode('')).toBe(false);
    });

    it('normalizeRuntimeMode returns valid modes unchanged and warns on unknown', () => {
      const warn = vi.fn();
      expect(normalizeRuntimeMode('plan', warn)).toBe('plan');
      expect(normalizeRuntimeMode('normal', warn)).toBe('normal');
      expect(normalizeRuntimeMode('foo', warn)).toBe('normal');
      expect(warn).toHaveBeenCalledTimes(1);
      expect(warn).toHaveBeenCalledWith('Unknown runtime mode "foo", falling back to "normal"');
    });
  });
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__/mode-types.test.ts
  ```

  预期失败：模块找不到 `../types`。

- [ ] **Write the minimal implementation**

  新建 `packages/agent-core/src/agent/session-mode/types.ts`：

  ```ts
  export const SESSION_MODE_KINDS = ['plan', 'design', 'office-hours', 'game-design'] as const;
  export type SessionModeKind = typeof SESSION_MODE_KINDS[number];

  export const RUNTIME_MODES = [...SESSION_MODE_KINDS, 'normal'] as const;
  export type RuntimeMode = typeof RUNTIME_MODES[number];

  export function isSessionModeKind(value: string): value is SessionModeKind {
    return (SESSION_MODE_KINDS as readonly string[]).includes(value);
  }

  export function isRuntimeMode(value: string): value is RuntimeMode {
    return (RUNTIME_MODES as readonly string[]).includes(value);
  }

  export function normalizeRuntimeMode(
    value: string,
    warn: (message: string) => void = console.warn,
  ): RuntimeMode {
    if (isRuntimeMode(value)) return value;
    warn(`Unknown runtime mode "${value}", falling back to "normal"`);
    return 'normal';
  }
  ```

- [ ] **Run it and verify it PASSES**

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__/mode-types.test.ts
  ```

  预期：5 个测试全部通过。

- [ ] **Commit**

  ```bash
  git add packages/agent-core/src/agent/session-mode/types.ts packages/agent-core/src/agent/session-mode/__tests__/mode-types.test.ts
  git commit -m "feat(agent-core): add SessionModeKind and RuntimeMode types with guards"
  ```

## Task 1.2：从 `packages/agent-core/src/agent/session-mode/index.ts` 统一导出类型

**Depends on:** Task 1.1

**Files:**
- Modify: `packages/agent-core/src/agent/session-mode/index.ts`（导出部分）

### 步骤

- [ ] **Write the failing test**

  在 `packages/agent-core/src/agent/session-mode/__tests__/mode-types.test.ts` 追加：

  ```ts
  import { SessionModeKind, RuntimeMode } from '../index';

  describe('session-mode/index re-exports', () => {
    it('exports RuntimeMode and SessionModeKind from index', () => {
      // Type-only compile check; value assertions above exercise runtime.
      const kind: SessionModeKind = 'plan';
      const mode: RuntimeMode = 'normal';
      expect(kind).toBe('plan');
      expect(mode).toBe('normal');
    });
  });
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__/mode-types.test.ts
  ```

  预期失败：`RuntimeMode` 未从 `../index` 导出。

- [ ] **Write the minimal implementation**

  在 `packages/agent-core/src/agent/session-mode/index.ts` 顶部添加：

  ```ts
  export {
    SESSION_MODE_KINDS,
    RUNTIME_MODES,
    isSessionModeKind,
    isRuntimeMode,
    normalizeRuntimeMode,
    type SessionModeKind,
    type RuntimeMode,
  } from './types';
  ```

  注意：保留该文件后续对 `SessionModeKind` 的本地使用；当前文件内 `SessionModeKind` 仍需存在或改为从 `./types` 导入。本任务只负责导出，本地使用替换在 Part 5 处理。

- [ ] **Run it and verify it PASSES**

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__/mode-types.test.ts
  ```

  预期：测试通过。

- [ ] **Commit**

  ```bash
  git add packages/agent-core/src/agent/session-mode/index.ts packages/agent-core/src/agent/session-mode/__tests__/mode-types.test.ts
  git commit -m "feat(agent-core): re-export RuntimeMode types from session-mode index"
  ```

## Local Self-Review

- [ ] 1. Spec-coverage：本 part 覆盖 Scope In #1、Error Handling 中“未知 mode 字符串回退”部分。
- [ ] 2. Placeholder scan：无 TODO/TBD，所有代码完整给出。
- [ ] 3. No phantom tasks：Task 1.1 创建文件，Task 1.2 统一导出，均有测试与 commit。
- [ ] 4. Dependency soundness：Task 1.2 仅依赖 Task 1.1 的 `./types` 模块。
- [ ] 5. Caller & build soundness：本 part 不修改共享签名，不触发全树 typecheck；单包测试通过即可。
- [ ] 6. Test-the-risk：测试枚举了所有合法 mode 与非法 mode；`normalizeRuntimeMode` 的 warn 消息与常量一致。
- [ ] 7. Type consistency：`SessionModeKind` / `RuntimeMode` 在本 part 定义，后续 part 引用同名导出。
