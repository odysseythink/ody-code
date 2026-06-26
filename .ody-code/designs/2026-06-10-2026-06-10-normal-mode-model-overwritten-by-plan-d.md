# Bug: normal 模式模型被 plan/design 模式配置覆盖

## 症状

用户操作路径：
1. normal 模式下通过 `/model` 设置模型为 kimi
2. plan 和 design 模式的模型配置为 deepseek
3. `/exit` 退出应用
4. `./ody` 重新启动（normal 模式）
5. **normal 模式的模型变成了 deepseek**（预期应为 kimi）

## 代码分析摘要

### 关键代码路径

#### 1. 模型持久化: `persistModelSelection` (TUI)
文件: `apps/ody-code/src/tui/commands/config.ts:378-407`

```
persistModelSelection(host, alias, thinking):
  config = host.harness.getConfig({ reload: true })

  if sessionMode == 'plan':
    setConfig({ modeModels: { ...config.modeModels, plan: alias }, defaultThinking: thinking })
  else if sessionMode == 'design':
    setConfig({ modeModels: { ...config.modeModels, design: alias }, defaultThinking: thinking })
  else:  // normal
    setConfig({ defaultModel: alias, defaultThinking: thinking })
```

**观察**: normal 模式保存到 `defaultModel`，plan/design 保存到 `modeModels` 对应字段。逻辑正确。

#### 2. 配置合并: `mergeConfigPatch` → `deepMerge`
文件: `packages/agent-core/src/config/merge.ts:10-42`

`deepMerge` 对对象类型递归合并。`modeModels` patch（如 `{ plan: 'deepseek' }`）与 base（如 `{ plan: 'old', design: 'old' }`）合并后，保留 `design` 字段。**此逻辑正确**。

#### 3. 配置写入: `configToTomlData` → `modeModelsToToml`
文件: `packages/agent-core/src/config/toml.ts:279-316, 473-481`

```
modeModelsToToml(modeModels, _raw):
  out = {}
  for [key, value] in modeModels:
    setDefined(out, key, value)  // 忽略 _raw
  return out
```

**问题**: `modeModelsToToml` 完全忽略 `_raw` 参数。`setSection` 将 `converted` 完全赋给 `out[snakeKey]`，覆盖 raw 中的原有内容。虽然 `persistModelSelection` 使用 `...config.modeModels` 保留了字段，但如果 `config.modeModels` 在某些代码路径下不完整，可能导致 `mode_models` 节丢失字段。

#### 4. Env Model 清洗: `stripEnvModelConfig`
文件: `packages/agent-core/src/config/env-model.ts:185-213`

```
stripEnvModelConfig(config):
  return {
    ...config,
    providers,  // 移除 env provider
    ...(models !== undefined ? { models } : {}),  // 移除 env model
    ...(defaultIsEnv ? { defaultModel: rawDefaultModel(config) } : {}),  // 恢复 raw defaultModel
    thinking: rawThinking(config),      // 恢复 raw thinking
    defaultThinking: rawDefaultThinking(config),  // 恢复 raw defaultThinking
  }
```

**问题**: `stripEnvModelConfig` 恢复 `defaultModel`、`thinking`、`defaultThinking` 为 raw 值，但 **完全不恢复 `modeModels`**。当 env model 活跃时，如果 `modeModels` 在运行时被修改过，写入时会保留运行时值，而 `defaultModel` 会被恢复为 raw 值。这可能导致 `defaultModel` 和 `modeModels` 之间的不一致。

#### 5. Session 创建时的模型初始化
文件: `packages/agent-core/src/rpc/core-impl.ts:195-266`

```
createSession(input):
  config = reloadProviderManager()
  ...
  mainAgent.config.update({
    modelAlias: options.model ?? config.defaultModel,
    thinkingLevel,
  })
  ...
  if config.defaultSessionMode !== undefined:
    await mainAgent.sessionMode.enter(..., config.defaultSessionMode)
```

**观察**: 新会话使用 `config.defaultModel` 作为默认模型。如果 `config.defaultModel` 是 deepseek（而非 kimi），normal 模式启动时会使用 deepseek。

#### 6. SessionMode 模型切换
文件: `packages/agent-core/src/agent/session-mode/index.ts:44-103`

```
enter(kind):
  modeModel = agent.kimiConfig?.modeModels?.[kind]
  if modeModel !== undefined and modeModel !== currentModel:
    _preModeModelAlias = { value: currentModel }
    agent.config.update({ modelAlias: modeModel })
```

**观察**: 进入 plan/design 时保存当前模型，退出时（`cancel()`/`exit()`）恢复。但如果用户在 plan/design 模式下直接 `/exit`（不先调用 `session.setSessionMode('normal')`），`exit()` 不会被调用，内存中的 `_preModeModelAlias` 丢失。不过这不影响持久化的 `defaultModel`。

### 根因假设

基于以上分析，**最可能的根本原因是配置持久化层存在多个薄弱环节，在特定操作序列下导致 `defaultModel` 被意外覆盖或恢复为旧值** [C:INFERRED]。

具体假设：

1. **假设 A（最可能）** [C:INFERRED]: 当用户在 plan/design 模式下通过 `/model` 设置模型时，`persistModelSelection` 更新 `modeModels` 并调用 `setConfig`。`setConfig` 路径中的 `mergeConfigPatch` → `writeConfigFile` → `stripEnvModelConfig` 在 env model 场景下会恢复 `defaultModel` 为 raw 值。如果 raw 中 `default_model` 是旧值（deepseek），而用户之前通过 `/model` 设置的 kimi 只存在于运行时内存中，那写入后 `default_model` 被恢复为 deepseek。

2. **假设 B** [C:INFERRED]: `configToTomlData` 中 `modeModelsToToml` 忽略 raw 值。如果 `config.modeModels` 在某种边界条件下不完整（例如只包含 `plan` 而不包含 `design`），`mode_models` 节会被覆盖为不完整状态。

3. **假设 C** [C:INFERRED]: 当 `defaultModel` 在 TOML 中不存在（undefined）时，`rawDefaultModel(config)` 返回 undefined，`stripEnvModelConfig` 会删除 `defaultModel`。如果此时 `modeModels` 中有值，下次启动时 normal 模式找不到 `defaultModel`，可能回退到 `modeModels` 中的某个值（需要进一步验证）。

## 修复方案

### 方案 1: 修复 `stripEnvModelConfig` 恢复 `modeModels` (推荐)

在 `stripEnvModelConfig` 中也恢复 `modeModels` 为 raw 值，确保 env model round-trip 不会丢失用户对 `modeModels` 的修改。

```
stripEnvModelConfig(config):
  ...
  return {
    ...config,
    providers,
    ...(models !== undefined ? { models } : {}),
    ...(defaultIsEnv ? { defaultModel: rawDefaultModel(config) } : {}),
    ...(modeModelsIsEnv ? { modeModels: rawModeModels(config) } : {}),  // 新增
    thinking: rawThinking(config),
    defaultThinking: rawDefaultThinking(config),
  }
```

### 方案 2: 修复 `configToTomlData` 中 `modeModelsToToml` 合并 raw 值

让 `modeModelsToToml` 使用 raw 值作为基础，再覆盖运行时的值，确保不丢失 raw 中的额外字段。

```
modeModelsToToml(modeModels, raw):
  out = cloneRecord(raw)  // 使用 raw 作为基础
  for [key, value] in modeModels:
    setDefined(out, key, value)
  return out
```

### 方案 3: 增加启动时模型选择日志

在 `core-impl.ts:createSession` 中增加 debug 日志，记录 `options.model`、`config.defaultModel` 和最终选择的 `modelAlias`，便于诊断。

## Self-Review

### Security
- 检查了 `stripEnvModelConfig` 是否可能导致敏感数据泄露：不会，该函数只操作配置结构，不涉及日志或外部传输。
- 检查了 `modeModelsToToml` 的字段过滤：只过滤 undefined 值，不会意外泄露或删除合法配置。

### Test
- `deepMerge` 的递归合并已验证：对象属性正确保留，不存在字段丢失。
- `persistModelSelection` 的三个分支覆盖 normal/plan/design，逻辑正确。
- **未发现**: `configToTomlData` 中 `modeModelsToToml` 未使用 `_raw` 的测试用例；需要补充边界测试。

### Ops
- `writeConfigFile` 使用原子写入（`atomicWrite`），不存在并发写入导致文件损坏的问题。
- `stripEnvModelConfig` 在 env model 场景下可能导致配置不一致（`defaultModel` 恢复为 raw 但 `modeModels` 不恢复），这是一个配置 round-trip 风险。

### Integration
- `SessionMode.enter/exit` 的模型切换/恢复逻辑已验证存在且正确调用。
- `core-impl.ts:createSession` 中 `defaultSessionMode` 自动进入 plan/design 的逻辑已验证存在。
- **未验证**: 当 `defaultModel` 为 undefined 时，系统是否有回退到 `modeModels` 中某个值的逻辑（目前代码中未发现）。

### Scope
- 本问题涉及配置持久化层（`agent-core/config`）、TUI 模型命令（`apps/ody-code/tui/commands`）和 Session 启动逻辑（`agent-core/rpc`）。
- 属于单一子系统（配置持久化与模型选择）的 bug，无需分解。

## 验证计划

1. 添加日志后，复现用户操作路径，观察 `defaultModel` 在何时被修改
2. 检查 `config.toml` 在每一步操作后的内容变化
3. 针对假设 A，验证 `stripEnvModelConfig` 在 env model 场景下的行为
4. 编写单元测试覆盖 `mergeConfigPatch` 的 `modeModels` 合并逻辑
5. 编写单元测试覆盖 `configToTomlData` 的 `modeModels` 序列化逻辑

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | 用户在 plan/design 模式下 `/model` 时，`persistModelSelection` 正确走 plan/design 分支 | High | 若走 normal 分支会覆盖 `defaultModel` | 代码静态分析确认 `appState.sessionMode` 类型安全 |
| 2 | `deepMerge` 在合并 `modeModels` 时不会丢失字段 | High | `modeModels` 字段丢失导致配置不完整 | 已有代码逻辑确认；建议补单元测试 |
| 3 | 根因是 `stripEnvModelConfig` 或 `modeModelsToToml` 在特定边界条件下的行为 | Medium | 若根因是其他代码路径（如 OAuth 刷新覆盖），修复方案无效 | 添加运行时日志复现问题 |
| 4 | 用户没有使用 `KIMI_MODEL_*` 环境变量 | Low | 若使用了 env model，`stripEnvModelConfig` 的行为会直接影响结果 | 询问用户确认 |
