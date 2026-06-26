# Part 4 — CLI Backend Switch Rename

本 Part 把 CLI 的 `--host=inproc` 重命名为 `--host=ts`，并新增 `ODY_BACKEND` 环境变量作为默认值回退。这是一个共享签名变更，必须在同一任务内更新所有调用者（含测试文件）并以全仓库 typecheck 收尾。

---

## Part 4 依赖图

```
D1 Rename --host=inproc → --host=ts + ODY_BACKEND
```

无 Part 4 内部依赖；D1 是单一原子任务。

---

## Part 4 范围说明

- **覆盖**：`CLIOptions.host` 类型、`--host` option 定义与默认值、`ODY_BACKEND` 环境回退、所有测试 fixture、内部错误提示文本。
- **不覆盖**：后端实际启动逻辑（Part 1/3 已完成）、文档改写（可在后续 PR 单独处理）。
- **共享签名**：`CLIOptions.host` 是共享类型；本任务必须更新所有 `host: 'inproc'` 出现位置并以 `pnpm -r typecheck` 验证。

---

### Task D1: Rename `--host=inproc` to `--host=ts` and Add `ODY_BACKEND` Fallback

**Depends on:** none

**Files:**
- Modify: `apps/ody-code/src/cli/options.ts:19`, `:123-124`
- Modify: `apps/ody-code/src/cli/commands.ts:100`, `:149`
- Modify: `apps/ody-code/test/cli/options.test.ts:453-454`
- Modify: 以下测试 fixture 文件中的 `host: 'inproc'` → `host: 'ts'`：
  - `apps/ody-code/test/cli/main.test.ts`
  - `apps/ody-code/test/cli/run-prompt.test.ts`
  - `apps/ody-code/test/cli/office-hours-bootstrap.test.ts`
  - `apps/ody-code/test/cli/run-shell.test.ts`
  - `apps/ody-code/test/tui/signal-handlers.test.ts`
  - `apps/ody-code/test/tui/message-replay.test.ts`
  - `apps/ody-code/test/tui/tui-startup.test.ts`
  - `apps/ody-code/test/tui/activity-pane.test.ts`
  - `apps/ody-code/test/tui/skill-commands-mode.test.ts`
  - `apps/ody-code/test/tui/sync-runtime-state.test.ts`
  - `apps/ody-code/test/tui/tui-message-flow.test.ts`
- Modify: `packages/agent-core-shared/src/errors/codes.ts:450`

**Goal:** 将用户可见的 `--host` 选项从 `inproc` 改名为 `ts`，并允许通过 `ODY_BACKEND` 环境变量设置默认值。

- [ ] 先更新 `apps/ody-code/test/cli/options.test.ts` 的期望值（测试先行）：

```ts
// line 453-454
it('defaults host to ts', () => {
  expect(parse([]).host).toBe('ts');
});
```

- [ ] 运行测试并确认失败：

```bash
pnpm --filter ody-code vitest run test/cli/options.test.ts
```

预期失败：

```
AssertionError: expected 'inproc' to be 'ts'
```

- [ ] 修改 `apps/ody-code/src/cli/options.ts`：

```ts
// line 19
  host: 'ts' | 'rust';
```

```ts
// line 123-124
  if (!['ts', 'rust'].includes(opts.host)) {
    throw new OptionConflictError(`Invalid --host: ${opts.host}. Must be ts or rust.`);
  }
```

- [ ] 修改 `apps/ody-code/src/cli/commands.ts`：

```ts
// line 100
    .addOption(
      new Option('--host <mode>', 'Run core in TypeScript in-process (ts) or external Rust host (rust).')
        .choices(['ts', 'rust'])
        .default('ts'),
    )
```

```ts
// line 149
      host: (raw['host'] as CLIOptions['host']) ?? (process.env['ODY_BACKEND'] as CLIOptions['host']) ?? 'ts',
```

- [ ] 统一替换所有测试 fixture：

```bash
rg -l "host: 'inproc'" apps/ody-code/test/ | xargs sed -i "s/host: 'inproc'/host: 'ts'/g"
```

- [ ] 修改 `packages/agent-core-shared/src/errors/codes.ts:450` 的错误提示（与用户可见名称保持一致）：

```ts
    action: 'Check the worker entry path or set transport to ts.',
```

- [ ] 重新运行受影响的 CLI 测试：

```bash
pnpm --filter ody-code vitest run test/cli/options.test.ts
```

预期：通过。

- [ ] 运行全仓库 typecheck（共享签名变更必须验证所有包与测试）：

```bash
pnpm -r typecheck
```

预期：无编译错误。

- [ ] 手动验证 `ODY_BACKEND` 回退：

```bash
ODY_BACKEND=rust pnpm --filter ody-code cli --help 2>&1 | grep -A2 '\-\-host'
```

预期：`--host` 默认显示为 `ts`；但实际以 `rust` 运行（可通过 smoke 或单元测试验证）。更直接的验证：

```bash
ODY_BACKEND=rust pnpm --filter ody-code vitest run test/cli/options.test.ts -t "accepts --host=rust"
```

确保环境变量路径不会导致运行时异常。

- [ ] 提交：

```bash
git add apps/ody-code/src/cli/options.ts \
           apps/ody-code/src/cli/commands.ts \
           apps/ody-code/test/cli/options.test.ts \
           apps/ody-code/test/cli/main.test.ts \
           apps/ody-code/test/cli/run-prompt.test.ts \
           apps/ody-code/test/cli/office-hours-bootstrap.test.ts \
           apps/ody-code/test/cli/run-shell.test.ts \
           apps/ody-code/test/tui/signal-handlers.test.ts \
           apps/ody-code/test/tui/message-replay.test.ts \
           apps/ody-code/test/tui/tui-startup.test.ts \
           apps/ody-code/test/tui/activity-pane.test.ts \
           apps/ody-code/test/tui/skill-commands-mode.test.ts \
           apps/ody-code/test/tui/sync-runtime-state.test.ts \
           apps/ody-code/test/tui/tui-message-flow.test.ts \
           packages/agent-core-shared/src/errors/codes.ts
git commit -m "feat(ody-code): rename --host=inproc to --host=ts and add ODY_BACKEND fallback"
```

---

## Part 4 本地 Self-Review

| 检查项 | 结论 |
|---|---|
| 1. Spec-coverage | `--host=ts` 重命名 → D1；`ODY_BACKEND` 回退 → D1；全仓库 typecheck → D1。 |
| 2. Placeholder scan | 无 TODO/TBD。 |
| 3. No phantom tasks | D1 是单一原子任务，产生大量文件变更与验证。 |
| 4. Dependency soundness | D1 无内部依赖。 |
| 5. Caller & build soundness | 用 `rg -l` 找出所有 `host: 'inproc'` 并统一替换；任务结束执行 `pnpm -r typecheck`。 |
| 6. Test-the-risk | `options.test.ts` 断言默认值从 `inproc` 变为 `ts`；验证 `ODY_BACKEND` 路径不抛异常。 |
| 7. Type一致性 | `CLIOptions.host` 类型与所有 fixture 同步为 `'ts' \| 'rust'`。 |

- [ ] 1. Spec-coverage table: `--host=ts` rename → D1, `ODY_BACKEND` fallback → D1, whole-tree typecheck → D1.
- [ ] 2. Placeholder scan: 无 TODO/TBD/占位符。
- [ ] 3. No phantom tasks: D1 是单一任务，覆盖所有必要变更。
- [ ] 4. Dependency soundness: D1 无 Part 4 内部依赖。
- [ ] 5. Caller & build soundness: 用 `rg -l "host: 'inproc'" apps/ody-code/test/` 找出所有 fixture 并替换；结束运行 `pnpm -r typecheck`。
- [ ] 6. Test-the-risk: `options.test.ts` 直接断言默认值；`ODY_BACKEND=rust` 手动验证路径。
- [ ] 7. Type一致性: `CLIOptions.host` 与 `--host` option choices、所有 fixture 保持一致。

