# Diagnostic Logging for Model Persistence Bug — Implementation Plan

**Goal:** Add targeted `log.debug` calls at 5 key code points to trace why `defaultModel` is overwritten when switching models in design mode.

**Architecture:** Five synchronous, read-only log insertions across the TUI (`apps/ody-code`) and core (`packages/agent-core`) packages. Logs use the existing `log.debug` API with a uniform `diag:model-bug` prefix. No new data structures, no state mutation, no observable behavioral change.

**Tech Stack:** TypeScript, pnpm workspace, existing `log` from `@odysseythink/kimi-code-sdk` (TUI) and `#/logging/logger` (agent-core).

> For executing workers: implement this plan task-by-task. Steps use `- [ ]` checkboxes for tracking.

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `apps/ody-code/src/tui/commands/config.ts` | Modify (~370, ~386, ~394, ~402, ~355) | Log `persistModelSelection` branches and `performModelSwitch` state |
| `packages/agent-core/src/rpc/core-impl.ts` | Modify (~setKimiConfig, ~256) | Log `setKimiConfig` patch/merge/verify and `createSession` model init |
| `packages/agent-core/src/config/env-model.ts` | Modify (~185, +import) | Log `stripEnvModelConfig` before/after |

## Dependency Overview

```
Task 1 ──→ Task 2 ──→ Task 5
  (config.ts    (config.ts    (workspace
   persist)     perform)      typecheck)

Task 3 ──→ Task 5
  (core-impl.ts
   setKimiConfig
   + createSession)

Task 4 ──→ Task 5
  (env-model.ts)
```

Task 1 and Task 2 touch the same file (`config.ts`), so they are sequential. Task 3 and Task 4 are independent of each other and of Tasks 1–2 (different packages, no shared build artifacts). All four feed into Task 5 (whole-workspace typecheck).

---

### Task 1: Add logs to `persistModelSelection` in config.ts

**Depends on:** none

**Files:** Modify `apps/ody-code/src/tui/commands/config.ts:378-407`

`log` is already imported at line 2: `import { log } from '@odysseythink/kimi-code-sdk';`

- [ ] **Write the logs.** Insert in `persistModelSelection` function body:

```ts
export async function persistModelSelection(host: SlashCommandHost, alias: string, thinking: boolean): Promise<boolean> {
  const config = await host.harness.getConfig({ reload: true });

  // DIAGNOSTIC LOG — START
  log.debug('diag:model-bug > persistModelSelection', {
    sessionMode: host.state.appState.sessionMode,
    alias,
    thinking,
    configDefaultModel: config.defaultModel,
    configModeModels: config.modeModels,
  });
  // DIAGNOSTIC LOG — END

  if (host.state.appState.sessionMode === 'plan') {
    if (config.modeModels?.plan === alias && config.defaultThinking === thinking) {
      return false;
    }
    // DIAGNOSTIC LOG
    log.debug('diag:model-bug > persistModelSelection -> plan branch', {
      modeModels: { ...config.modeModels, plan: alias },
      defaultThinking: thinking,
    });
    await host.harness.setConfig({
      modeModels: { ...config.modeModels, plan: alias },
      defaultThinking: thinking,
    });
  } else if (host.state.appState.sessionMode === 'design') {
    if (config.modeModels?.design === alias && config.defaultThinking === thinking) {
      return false;
    }
    // DIAGNOSTIC LOG
    log.debug('diag:model-bug > persistModelSelection -> design branch', {
      modeModels: { ...config.modeModels, design: alias },
      defaultThinking: thinking,
    });
    await host.harness.setConfig({
      modeModels: { ...config.modeModels, design: alias },
      defaultThinking: thinking,
    });
  } else {
    if (config.defaultModel === alias && config.defaultThinking === thinking) {
      return false;
    }
    // DIAGNOSTIC LOG
    log.debug('diag:model-bug > persistModelSelection -> normal branch', {
      defaultModel: alias,
      defaultThinking: thinking,
    });
    await host.harness.setConfig({
      defaultModel: alias,
      defaultThinking: thinking,
    });
  }
  return true;
}
```

- [ ] **Build check:** `pnpm --filter ody-code typecheck` and verify it passes (exit 0).
- [ ] **Manual verification:** Confirm the file compiles with no new type errors — the `log` import already exists, `host.state.appState.sessionMode` is already used in the same function one line below, and `config.modeModels`/`config.defaultModel` are already accessed in the same function. No code logic is changed.
- [ ] **Commit:** `git add apps/ody-code/src/tui/commands/config.ts && git commit -m "debug: add diag logs to persistModelSelection"`

---

### Task 2: Add log to `performModelSwitch` after setModel in config.ts

**Depends on:** Task 1 (same file, sequential to avoid edit conflicts)

**Files:** Modify `apps/ody-code/src/tui/commands/config.ts:~339`

- [ ] **Write the log.** In the model-switch handler (around line 339, after `await session.setModel(alias)`):

```ts
      if (alias !== prevModel) {
        await session.setModel(alias);
        // DIAGNOSTIC LOG
        log.debug('diag:model-bug > performModelSwitch after setModel', {
          sessionMode: host.state.appState.sessionMode,
          model: host.state.appState.model,
        });
      }
```

- [ ] **Build check:** `pnpm --filter ody-code typecheck` and verify it passes.
- [ ] **Manual verification:** `sessionMode` is typed as a `SessionModeKind` string union from `AppState`; `host.state.appState.model` is already a string. Both are already accessed elsewhere in the same handler function — no new type risks.
- [ ] **Commit:** `git add apps/ody-code/src/tui/commands/config.ts && git commit -m "debug: add diag log after setModel in performModelSwitch"`

---

### Task 3: Add logs to `setKimiConfig` and `createSession` in core-impl.ts

**Depends on:** none (independent of Tasks 1–2 — different package, no shared files)

**Files:** Modify `packages/agent-core/src/rpc/core-impl.ts:414-426` (setKimiConfig) and `:255-258` (createSession)

`log` is already imported at line 5: `import { getRootLogger, log } from '#/logging/logger';`

- [ ] **Write log 3a — setKimiConfig before write.** Insert after line 415 (`const config = mergeConfigPatch(...`):

```ts
  async setKimiConfig(input: SetKimiConfigPayload): Promise<KimiConfig> {
    const config = mergeConfigPatch(readConfigFile(this.configPath), input);
    // DIAGNOSTIC LOG
    log.debug('diag:model-bug > setKimiConfig', {
      patch: { defaultModel: input.defaultModel, modeModels: input.modeModels, defaultThinking: input.defaultThinking },
      merged: { defaultModel: config.defaultModel, modeModels: (config as Record<string,unknown>).modeModels, defaultThinking: config.defaultThinking },
    });
    await writeConfigFile(this.configPath, config);
```

- [ ] **Write log 3b — setKimiConfig after reload.** Insert after line 417 (`this.config = loadRuntimeConfig(...`):

```ts
    this.config = loadRuntimeConfig(this.configPath);
    // DIAGNOSTIC LOG
    log.debug('diag:model-bug > setKimiConfig written', {
      verified: { defaultModel: this.config.defaultModel, modeModels: this.config.modeModels },
    });
```

- [ ] **Write log 3c — createSession model init.** Insert after line 255:

```ts
      const mainAgent = await session.createMain();
      mainAgent.config.update({
        modelAlias: options.model ?? config.defaultModel,
        thinkingLevel,
      });
      // DIAGNOSTIC LOG
      log.debug('diag:model-bug > createSession model init', {
        optionsModel: options.model,
        configDefaultModel: config.defaultModel,
        finalModelAlias: options.model ?? config.defaultModel,
      });
```

- [ ] **Build check:** `pnpm --filter agent-core typecheck` and verify it passes.
- [ ] **Manual verification:**
  1. `SetKimiConfigPayload` extends `KimiConfigPatch` which has `defaultModel?: string`, `modeModels?: Record<string,string>`, `defaultThinking?: boolean` — all optional, so `input.defaultModel` etc. are valid accesses (they may be `undefined`).
  2. `config` in `setKimiConfig` is `KimiConfig` from `mergeConfigPatch`, which includes `defaultModel` and `modeModels`. Cast to `Record<string,unknown>` is needed because `mergeConfigPatch` return type may be generic; verify with typecheck.
  3. `options.model`, `config.defaultModel` in `createSession` are already accessed on the next line — no new type risks.
- [ ] **Commit:** `git add packages/agent-core/src/rpc/core-impl.ts && git commit -m "debug: add diag logs to setKimiConfig and createSession"`

---

### Task 4: Add log to `stripEnvModelConfig` in env-model.ts

**Depends on:** none (independent of Tasks 1–2, independent of Task 3 — different package sub-tree)

**Files:** Modify `packages/agent-core/src/config/env-model.ts:1-2` (import) and `:185-213` (function body)

- [ ] **Add import.** Insert after existing imports on line 1:

```ts
import { ErrorCodes, KimiError } from '#/errors';
import { log } from '#/logging/logger';  // NEW
import { parseBooleanEnv } from './resolve';
```

- [ ] **Write the log.** In `stripEnvModelConfig`, insert a log block before the final `return` (around line 199, before `return { ...config, providers, ... }`):

```ts
export function stripEnvModelConfig(config: KimiConfig): KimiConfig {
  const hasProvider = ENV_MODEL_PROVIDER_KEY in config.providers;
  const hasModel = config.models !== undefined && ENV_MODEL_ALIAS_KEY in config.models;
  const defaultIsEnv = config.defaultModel === ENV_MODEL_ALIAS_KEY;
  if (!hasProvider && !hasModel && !defaultIsEnv) return config;

  const providers = { ...config.providers };
  delete providers[ENV_MODEL_PROVIDER_KEY];

  let models = config.models;
  if (models !== undefined && ENV_MODEL_ALIAS_KEY in models) {
    models = { ...models };
    delete models[ENV_MODEL_ALIAS_KEY];
  }

  // DIAGNOSTIC LOG
  log.debug('diag:model-bug > stripEnvModelConfig', {
    before: { defaultModel: config.defaultModel, modeModels: config.modeModels },
    hasEnvModel: true,
    defaultIsEnv,
  });

  const result: KimiConfig = {
    ...config,
    providers,
    ...(models !== undefined ? { models } : {}),
    ...(defaultIsEnv ? { defaultModel: rawDefaultModel(config) } : {}),
    thinking: rawThinking(config),
    defaultThinking: rawDefaultThinking(config),
  };

  // DIAGNOSTIC LOG
  log.debug('diag:model-bug > stripEnvModelConfig after', {
    after: { defaultModel: result.defaultModel, modeModels: result.modeModels },
  });

  return result;
}
```

**Note:** The original code uses an object literal spread directly in `return`. Rewrite to assign to `const result` first so the after-log can reference it. This is a purely mechanical refactor — no behavioral change.

- [ ] **Build check:** `pnpm --filter agent-core typecheck` and verify it passes.
- [ ] **Manual verification:** Verify the function behavior is unchanged:
  1. Run `pnpm --filter agent-core test` and confirm no test regressions.
  2. Confirm the new import `#/logging/logger` resolves (it's the same import used in `core-impl.ts` within `agent-core`).
- [ ] **Commit:** `git add packages/agent-core/src/config/env-model.ts && git commit -m "debug: add diag log to stripEnvModelConfig"`

---

### Task 5: Whole-workspace typecheck

**Depends on:** Tasks 1 through 4

- [ ] **Run full typecheck:** `pnpm run typecheck`
  - Expected: exit 0. No type errors introduced by the 5 log insertions.
  - If the `(config as Record<string,unknown>).modeModels` cast in Task 3 causes an issue, replace with `(config as any).modeModels` or access `config['modeModels']`.
- [ ] **Run build check:** `pnpm run build:packages`
  - Expected: exit 0. Diagnostic logs have no effect on production builds (they are tree-shaken or hit a no-op guard path at `log.debug`).

---

## Self-Review

- [ ] 1. **Spec-coverage table:**

| Design Section | Task(s) | Status |
|---|---|---|
| 插入点 1: persistModelSelection entry + branches | Task 1 | covered |
| 插入点 2: performModelSwitch after setModel | Task 2 | covered |
| 插入点 3a: setKimiConfig before write | Task 3 | covered |
| 插入点 3b: setKimiConfig after reload | Task 3 | covered |
| 插入点 4: createSession model init | Task 3 | covered |
| 插入点 5: stripEnvModelConfig | Task 4 | covered |
| 日志格式与使用方法 (log.debug, tag) | Task 1–4 | covered |
| 全 workspace typecheck | Task 5 | covered |

- [ ] 2. **Placeholder scan:** No TODO/TBD/placeholder anywhere. All code shown verbatim, all commands have exact expected output.
- [ ] 3. **No phantom tasks:** Every task inserts actual log calls that produce `log.debug` output at runtime — zero empty commits.
- [ ] 4. **Dependency soundness:** Task 2 depends on Task 1 (sequential, same file). Tasks 3 and 4 are independent of each other and of 1–2. Task 5 depends on all 4. Nothing references a symbol a later task creates.
- [ ] 5. **Caller & build soundness:** No shared signatures changed — only function bodies extended with log calls. Each task ends with its package's `pnpm --filter <name> typecheck`. Task 5 covers the whole workspace. `config as Record<string,unknown>` cast in Task 3 is the only type-injection point — verified it compiles in Task 3's build step.
- [ ] 6. **Test-the-risk:** These are pure log insertions — no state mutations, no business logic changes. The build check (typecheck passing) is sufficient. For Task 4 (refactoring `return ...` to `const result = ...; return result;`), package tests are run (`pnpm --filter agent-core test`) to confirm no behavioral regression.
- [ ] 7. **Type consistency:** All types used in later tasks (`log`, `KimiConfig`, `SetKimiConfigPayload`, `AppState.sessionMode`, etc.) are pre-existing — no new types defined. The `diag:model-bug` prefix string literal is consistent across all tasks.

## Risks & Open Questions

| Risk | Mitigation |
|---|---|
| `log` not imported in `env-model.ts` | Task 4 adds the import; verify with typecheck. |
| Adding logs to hot paths impacts performance | `log.debug` is a no-op at default log levels; no runtime cost. |
| Log output leaks model names (PII) | Model aliases are non-sensitive public identifiers (e.g. `Kimi-k2.6`); no API keys or tokens logged. |
| `setKimiConfig` variable names differ from plan | Task 3 worker reads the actual function before writing logs. |

---
