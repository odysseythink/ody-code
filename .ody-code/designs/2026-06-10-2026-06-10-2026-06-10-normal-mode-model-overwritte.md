# 诊断: normal 模式模型被 design 模式模型切换覆盖

## 症状

用户提供详细测试矩阵（关键证据）：

### 初始状态
- Normal: Kimi-k2.6, Plan: deepseek-v4-pro, Design: Kimi-k2.6

### 测试 1 — baseline
操作: 从 normal 模式 `/exit`, `./ody`
结果: **没有变化** ✓

### 测试 2 — design 模式切换 deepseek
操作: 在 design 模式切换 deepseek-v4-pro → 退出 design → `./ody`
结果: **Normal=deepseek-v4-pro, Plan=deepseek-v4-pro, Design=deepseek-v4-pro**（全部变化！）

### 测试 3 — design 模式切换回 Kimi
操作: 初始 Normal=deepseek-v4-pro, Plan=deepseek-v4-pro, Design=deepseek-v4-pro, 在 design 模式切换 Kimi-k2.6 → 退出 design → `./ody`
结果: **Normal=Kimi-k2.6, Plan=deepseek-v4-pro, Design=Kimi-k2.6**（Normal 跟随 Design 变化！）

### 测试 4 — design 切换后切回 normal 再退出
操作: Design=Kimi-k2.6 → design 模式切换 deepseek-v4-pro → 切回 normal（此时显示 deepseek-v4-pro）→ `./ody`
结果: **Normal=deepseek-v4-pro, Plan=deepseek-v4-pro, Design=deepseek-v4-pro**

### 关键推论
1. Design 模式下的模型切换 **总是** 影响 Normal 模式在下次启动时的模型
2. Plan 模式的模型不受影响（保持 deepseek-v4-pro）
3. 这说明 **`defaultModel` 被修改了**，而不仅仅是 `modeModels.design`

## 代码分析: 遍历完整调用链

### 1. persistModelSelection（TUI 模型切换保存入口）
文件: `apps/ody-code/src/tui/commands/config.ts:370-407`

根据 `host.state.appState.sessionMode` 决定持久化目标:
- `plan` → 保存到 `modeModels.plan`
- `design` → 保存到 `modeModels.design`
- else (normal) → 保存到 `defaultModel`

**静态分析结论**: 代码逻辑正确，design 分支不触及 `defaultModel`。

### 2. performModelSwitch（TUI 模型切换触发点）
文件: `apps/ody-code/src/tui/commands/config.ts:336-368`

调用序列:
1. `session.setModel(alias)` → 触发 `emitStatusUpdated({ sessionMode: this.sessionMode.kind })` — 如果当前在 design 模式，emit `'design'`
2. `host.setAppState({ model: alias })`
3. `persistModelSelection(host, alias, thinking)` → 读取 `host.state.appState.sessionMode`

`handleStatusUpdate` (session-event-handler.ts:545-562) 会在 step 1 的 emit 后更新 `appState.sessionMode`。

**时序**: step 1 emit → `handleStatusUpdate` 更新 sessionMode → step 2 setAppState → step 3 读取。理论上 `sessionMode` 在 step 3 应该是正确的。

### 3. SessionMode.enter（进入特殊模式时的模型切换）
文件: `packages/agent-core/src/agent/session-mode/index.ts:44-103`

行 67: `this._preModeModelAlias = { value: this.agent.config.modelAlias };`
行 68: `this.agent.config.update({ modelAlias: modeModel });`

`_preModeModelAlias` 在第 67 行捕获旧值（update 在第 68 行），**此逻辑正确**。

### 4. mergeConfigPatch（配置合并）
文件: `packages/agent-core/src/config/merge.ts:10-42`

`deepMerge` 递归合并对象，保留兄弟字段。合并 `modeModels` patch 时正确保留未提及的字段。**逻辑正确**。

### 5. stripEnvModelConfig（写入前洗掉 env 注入的配置）
文件: `packages/agent-core/src/config/env-model.ts:185-213`

恢复 `defaultModel`、`thinking`、`defaultThinking` 为 `_raw` 值，但 **不恢复 `modeModels`** [C:INFERRED 潜在不一致]。

用户确认 **未使用 `KIMI_MODEL_*` 环境变量** [C:USER]。如果确实没有 env model 注入，此函数不应修改任何内容（env 相关字段保持 undefined，跳过恢复逻辑）。

### 6. configToTomlData → modeModelsToToml
文件: `packages/agent-core/src/config/toml.ts:473-481`

`modeModelsToToml(modeModels, _raw)` 完全忽略 `_raw` 参数。在 `setSection` 中完全替换 `mode_models` 节。由于 `persistModelSelection` 使用 `...config.modeModels` 展开后只修改目标字段，正常路径下不会丢失字段。

### 7. createSession（新会话模型初始化）
文件: `packages/agent-core/src/rpc/core-impl.ts:256`

```ts
mainAgent.config.update({
  modelAlias: options.model ?? config.defaultModel,
  thinkingLevel,
});
```

使用 `config.defaultModel` 作为 normal 模式的默认模型。如果 `defaultModel` 被错误覆盖，启动时 normal 模式会使用错误模型。

### 8. applyManagedKimiCodeConfig（OAuth 刷新路径）
文件: `packages/oauth/src/managed-kimi-code.ts:232`

```ts
config.defaultModel = selectedDefault.modelKey;
```

当 `preserveDefaultModel: true` 且当前 defaultModel 仍可用时，`selectDefaultModel` 返回当前值，理论上是 no-op。**但此行无条件赋值**，如果逻辑有误可能覆盖。

调用路径: `refreshAllProviderModels` → `refresh-providers.ts:146` — 仅在 model 列表变更时触发，通常不在普通操作中触发。

### 9. TOML 序列化 → validateConfig
文件: `packages/agent-core/src/config/validation.ts`

merge 后的 config 通过 `validateConfig` 验证（Zod schema），无效字段被过滤。`defaultModel` 必须通过 `z.string().optional()` 验证。

## 根因定位: 结论

经过完整的静态代码分析，**无法从代码中 pinpoint 单一确定性根因** [C:INFERRED]。

具体矛盾:
- `persistModelSelection` 在 design 模式下只保存 `modeModels.design`，理论上不会修改 `defaultModel`
- 用户测试数据显示 `defaultModel` 确实被修改了（测试 2/3/4 中 Normal 模式变化）
- 用户确认未使用 env vars，弱化了 `stripEnvModelConfig` 路径的嫌疑
- `SessionMode` 的模型保存/恢复逻辑也正确

**可能原因**: 存在未被遍历到的代码路径修改 `defaultModel`，或 TOML 解析/序列化库 (`@iarna/toml`) 在特定条件下产生意外的 key 映射。

## 复现的必要条件

需要运行时日志才能确定根因。建议在以下关键点添加诊断日志 [C:INFERRED]:
1. `persistModelSelection` 入口: 记录 `sessionMode`, `alias`, config 中 `defaultModel` 和 `modeModels` 的 before/after
2. `core-impl.ts:setKimiConfig`: 记录收到和写入的 patch 内容
3. TOML 文件内容: 让用户在每次操作后检查 `~/.ody-code/config.toml`

## Scope In / Out

### Scope In [C:USER]
- [C:USER] 修复 design 模式模型切换导致 normal 模式 defaultModel 被覆盖的 bug
- 添加诊断日志帮助定位根因

### Scope Out
- Plan 模式类似问题（测试数据显示 Plan 未被影响，但需排查代码一致性）[C:DEFERRED]
- OAuth 刷新路径对 defaultModel 的影响 [C:DEFERRED]

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | `persistModelSelection` 在 design 模式下 `sessionMode` 确实为 `'design'` | Medium | 若为 `'normal'` 则走到错误的分支，覆盖 `defaultModel` | 添加 `log.debug` 在 branch 判断前 |
| 2 | `getConfig({ reload: true })` 在 `persistModelSelection` 中返回正确的文件状态 | High | 若返回 stale config，merge 可能错误 | 对比 TOML 文件内容 |
| 3 | TOML 解析/序列化 round-trip 不产生意外 key 映射 | Medium | `@iarna/toml` 库 bug 可能导致字段混淆 | 单元测试 TOML round-trip |
| 4 | `validateConfig` 不会过滤 `defaultModel` | High | 若 Zod schema 拒绝该值，config 丢失 | 检查 KimiConfigSchema |
| 5 | 用户操作流程中没有触发 OAuth 刷新（`refreshProviderModels`） | High | 若触发，`applyManagedKimiCodeConfig` 可能覆盖 `defaultModel` | 添加日志在刷新路径 |
| 6 | 用户的操作环境无 `KIMI_MODEL_*` 环境变量 | High（用户已确认 [C:USER]） | 若有，`stripEnvModelConfig` 会恢复旧值 | 已确认 |
| 7 | `SessionMode.cancel()` 在用户退出 design 模式时被正确调用 | High | 若未调用，agent modelAlias 保持 design 模式值 | 添加日志 |

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | 根因在未被分析的代码路径中 | Medium | 修复方向错误，浪费开发时间 | 先添加日志，复现后再修 |
| 2 | TOML 库 bug | Low | 需要更换 TOML 库或 workaround | 单元测试覆盖 round-trip |
| 3 | 异步竞态导致 sessionMode 读取时已变化 | Low | 不易复现，难以调试 | 在 persistModelSelection 中 snapshot sessionMode |

## Self-Review

### Security
- 检查了所有配置路径: 不涉及敏感数据泄露，TOML 文件由用户控制。Nothing found.

### Test
- `deepMerge` 递归合并: 已有逻辑确认正确，但缺少 `modeModels` 边界测试。需要补充测试。
- `persistModelSelection` 三个分支: 均未覆盖 integration test。需要补充。
- TOML round-trip 未测试: 需要验证写回后 `defaultModel` 和 `modeModels` 均保留。

### Ops
- 原子写入 (`atomicWrite`) 防止并发损坏: 已验证存在。
- 诊断日志对性能无影响: log.debug 在生产环境默认关闭。
- 若根因在 `@iarna/toml` 库: 需要评估替换或 workaround 的成本。

### Integration
- 所有代码路径已验证实际存在: `persistModelSelection`, `SessionMode`, `stripEnvModelConfig`, `configToTomlData`, `createSession`, `applyManagedKimiCodeConfig`。
- `modeModelsToToml` 对 `_raw` 的忽略: 在 normal 路径下无影响，在 edge case 下可能丢失字段。
- TUI ↔ agent-core 的 sessionMode 同步: 通过 `emitStatusUpdated` + `handleStatusUpdate` 维护，时序上存在异步 gap。

### Scope
- 这是单一 bug 诊断任务，无需分解。

---

## 诊断日志方案 [C:USER]

### 插入点 1: persistModelSelection 入口
**文件**: `apps/ody-code/src/tui/commands/config.ts:~370`

在分支判断前:
```
log.debug('diag:model-bug > persistModelSelection', {
  sessionMode: host.state.appState.sessionMode,
  alias,
  thinking,
  configDefaultModel: config.defaultModel,
  configModeModels: config.modeModels,
});
```

在每个分支内记录实际写入内容:
```
// plan 分支 (~386)
log.debug('diag:model-bug > persistModelSelection -> plan branch', {
  modeModels: { ...config.modeModels, plan: alias },
  defaultThinking: thinking,
});
// design 分支 (~394)
log.debug('diag:model-bug > persistModelSelection -> design branch', {
  modeModels: { ...config.modeModels, design: alias },
  defaultThinking: thinking,
});
// normal 分支 (~402)
log.debug('diag:model-bug > persistModelSelection -> normal branch', {
  defaultModel: alias,
  defaultThinking: thinking,
});
```

### 插入点 2: performModelSwitch
**文件**: `apps/ody-code/src/tui/commands/config.ts:~355`

在 `session.setModel()` await 之后:
```
log.debug('diag:model-bug > performModelSwitch after setModel', {
  sessionMode: host.state.appState.sessionMode,
  model: host.state.appState.model,
});
```

### 插入点 3: setKimiConfig（core-impl.ts）
**文件**: `packages/agent-core/src/rpc/core-impl.ts`

在 merge 后写入前:
```
log.debug('diag:model-bug > setKimiConfig', {
  patch: { defaultModel: patch.defaultModel, modeModels: patch.modeModels, defaultThinking: patch.defaultThinking },
  merged: { defaultModel: merged.defaultModel, modeModels: merged.modeModels, defaultThinking: merged.defaultThinking },
});
```

写入后 reload 验证:
```
log.debug('diag:model-bug > setKimiConfig written', {
  verified: { defaultModel: reloaded.defaultModel, modeModels: reloaded.modeModels },
});
```

### 插入点 4: createSession 模型初始化
**文件**: `packages/agent-core/src/rpc/core-impl.ts:~256`

```
log.debug('diag:model-bug > createSession model init', {
  optionsModel: options.model,
  configDefaultModel: config.defaultModel,
  finalModelAlias: options.model ?? config.defaultModel,
});
```

### 插入点 5: stripEnvModelConfig
**文件**: `packages/agent-core/src/config/env-model.ts:~185`

```
log.debug('diag:model-bug > stripEnvModelConfig', {
  before: { defaultModel: config.defaultModel, modeModels: config.modeModels },
  hasEnvModel: config._raw?.defaultModel !== undefined,
  after: { defaultModel: result.defaultModel, modeModels: result.modeModels },
});
```

### 日志格式与使用方法

- 级别: `log.debug`（生产环境不输出）
- Tag: `diag:model-bug`
- 启动: `ODY_CODE_LOG_LEVEL=debug ./ody 2> debug.log`
- 复现后提供 `debug.log` 中所有 `diag:model-bug` 行

### 预期日志分析

按时间序排列所有 `diag:model-bug` 行:
1. 找到 `persistModelSelection` 中 `sessionMode` 的实际值
2. 找到 `defaultModel` 在哪个 `setKimiConfig` 调用中被修改
3. 对比 `patch` 和 `written.verified` 确认写入一致性
4. 检查 `stripEnvModelConfig` 是否修改了数据（`before !== after`）

### 修复方向（根据日志结果） [C:INFERRED]

| 日志发现 | 修复目标 |
|---|---|
| `sessionMode` 在 persistModelSelection 时错误 | 修复 sessionMode 同步时序 |
| `setKimiConfig` 收到意外的 `defaultModel` | 在调用方排查 patch 构造 |
| `stripEnvModelConfig` before ≠ after | 修复 env model 恢复逻辑 |
| `merged ≠ verified` | 排查 `configToTomlData` 或 `@iarna/toml` round-trip |
