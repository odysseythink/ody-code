# Design→Plan Handoff 不再携带完整设计正文 Implementation Plan

> **Goal:** 让 design 模式退出到 plan 模式时，tool result 与首条 plan reminder 只引用设计文件路径/文件名，不再嵌入完整设计正文，从而避免上下文膨胀。

> **Architecture:** 在 `SessionMode` 层把 design→plan 的交接 artifact 从 `{ content, path }` 瘦身为 `{ path, filename, selectedLabel? }`；`ExitDesignModeTool` 和 `DesignModeInjector` 分别据此调整输出模板与测试断言。改动集中在 `packages/agent-core` 的三个文件及其对应测试。

> **Tech Stack:** TypeScript, Vitest, pnpm monorepo (`packages/agent-core`).

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

| Responsibility | File |
|---|---|
| 交接 artifact 类型与生成逻辑 | `packages/agent-core/src/agent/session-mode/index.ts` |
| 交接 artifact 消费与 reminder 渲染 | `packages/agent-core/src/agent/injection/design-mode.ts` |
| design 退出 tool 的输出渲染 | `packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts` |
| 共享的 "Selected approach" 前缀工具 | `packages/agent-core/src/tools/builtin/planning/exit-mode-output.ts`（只读复用，不修改） |
| SessionMode handoff 行为测试 | `packages/agent-core/test/agent/session-mode.test.ts` |
| DesignModeInjector handoff reminder 测试 | `packages/agent-core/test/agent/injection/design-mode.test.ts` |
| ExitDesignModeTool 输出测试 | `packages/agent-core/test/tools/exit-design-mode.test.ts` |

## Dependency Overview

```
Task 1: Slim SessionMode handoff artifact + DesignModeInjector consumption
    │
    ├── updates session-mode/index.ts
    ├── updates design-mode.ts (caller + reminder template)
    ├── updates session-mode.test.ts
    ├── updates design-mode.test.ts
    └── ends with whole-tree typecheck
    │
    ▼
Task 2: Slim ExitDesignModeTool output
    │
    ├── updates exit-design-mode.ts
    └── updates exit-design-mode.test.ts
    │
    ▼
Task 3: Final verification
    └── runs targeted tests + typecheck + changeset
```

Task 1 is the shared-signature task: it changes `SessionMode.consumePendingHandoffForPlan()` return type, so the same task must update every caller (`design-mode.ts`) and end with a whole-tree typecheck.

## Risks & Open Questions

| # | Risk | Mitigation |
|---|---|---|
| 1 | `handoffTo('plan')` 改用 `data.path.length > 0` 判断后，空 content + 有效 path 的场景不再报错；旧的 `exit-design-mode.test.ts` 中有 "returns an error when no design content is available" 测试，需要按新语义改为成功 handoff。 | Task 2 同步更新该测试断言。 |
| 2 | `basename(data.path)` 对空字符串返回 `''`，但 `handoffTo` 已先判断 `data.path.length > 0`，不会传入空路径。 | Task 1 的测试中覆盖空 path 返回 null artifact。 |
| 3 | 修改共享类型后，如果遗漏某个 caller，全树 typecheck 会失败。 | Task 1 用 `grep` 搜索 `consumePendingHandoffForPlan` 与 `_pendingHandoffForPlan` 全部出现位置并更新。 |

---

### Task 1: Slim `SessionMode` handoff artifact and update `DesignModeInjector` consumption

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/agent/session-mode/index.ts:38-43`, `251-309`
- Modify: `packages/agent-core/src/agent/injection/design-mode.ts:42-44`, `123-126`
- Modify: `packages/agent-core/test/agent/session-mode.test.ts:272-388`
- Modify: `packages/agent-core/test/agent/injection/design-mode.test.ts:273-305`
- Test via: `pnpm test -- test/agent/session-mode.test.ts test/agent/injection/design-mode.test.ts`

**Rationale:** 这是共享签名变更任务。`consumePendingHandoffForPlan()` 的返回类型从 `{ content: string; path: string }` 改为 `{ path: string; filename: string; selectedLabel?: string }`，因此同一任务必须更新所有 caller（`design-mode.ts`）以及对应的测试，并结束于全树 typecheck。

- [ ] **Write the failing tests.**

  1. 在 `packages/agent-core/test/agent/session-mode.test.ts` 中，把 `describe('handoffTo')` 里的相关测试替换为：

     ```ts
     describe('handoffTo', () => {
       it('handoffTo("plan") exits design, enters plan, stores path/filename artifact', async () => {
         const agent = makeAgent();
         vi.mocked(agent.kaos.readText).mockResolvedValue('# My Design\n\nSome content');
         const sm = new SessionMode(agent);
         await sm.enter('design-id', undefined, false, 'design');
         await sm.resolveFilePathFromModelRequest('.ody-code/designs/my-feature.md', '# My Design\nSome content');

         vi.mocked(agent.records.logRecord).mockClear();

         await sm.handoffTo('plan');

         expect(sm.isActive).toBe(true);
         expect(sm.kind).toBe('plan');

         const handoff = sm.consumePendingHandoffForPlan();
         expect(handoff).not.toBeNull();
         expect(handoff).not.toHaveProperty('content');
         expect(handoff?.path).toMatch(/my-feature\.md$/);
         expect(handoff?.filename).toBe('my-feature.md');

         expect(sm.consumePendingHandoffForPlan()).toBeNull();
       });

       it('handoffTo("plan") stores selectedLabel when provided', async () => {
         const agent = makeAgent();
         vi.mocked(agent.kaos.readText).mockResolvedValue('# My Design');
         const sm = new SessionMode(agent);
         await sm.enter('design-id', undefined, false, 'design');
         await sm.resolveFilePathFromModelRequest('.ody-code/designs/my-feature.md', '# My Design');

         await sm.handoffTo('plan', { selectedLabel: 'Approach A' });

         const handoff = sm.consumePendingHandoffForPlan();
         expect(handoff?.selectedLabel).toBe('Approach A');
       });

       it('handoffTo("plan") stores artifact when content is empty but path exists', async () => {
         const agent = makeAgent();
         vi.mocked(agent.kaos.readText).mockResolvedValue('');
         const sm = new SessionMode(agent);
         await sm.enter('design-id', undefined, false, 'design');
         await sm.resolveFilePathFromModelRequest('.ody-code/designs/my-feature.md', '');

         await sm.handoffTo('plan');

         const handoff = sm.consumePendingHandoffForPlan();
         expect(handoff).not.toBeNull();
         expect(handoff?.path).toMatch(/my-feature\.md$/);
         expect(handoff?.filename).toBe('my-feature.md');
       });

       it('handoffTo("plan") stores null artifact when no file path is set', async () => {
         const agent = makeAgent();
         const sm = new SessionMode(agent);
         await sm.enter('design-id', undefined, false, 'design');

         await sm.handoffTo('plan');

         expect(sm.consumePendingHandoffForPlan()).toBeNull();
       });

       it('handoffTo("normal") exits plan, stores content/path artifact unchanged', async () => {
         const agent = makeAgent();
         vi.mocked(agent.kaos.readText).mockResolvedValue('## Step 1\n\nDo this');
         const sm = new SessionMode(agent);
         await sm.enter('plan-id', undefined, false, 'plan');
         await sm.resolveFilePathFromModelRequest('.ody-code/plans/my-plan.md', '## Step 1\nDo this');

         vi.mocked(agent.records.logRecord).mockClear();

         await sm.handoffTo('normal');

         expect(sm.isActive).toBe(false);

         const handoff = sm.consumePendingHandoffForNormal();
         expect(handoff).not.toBeNull();
         expect(handoff?.content).toBe('## Step 1\n\nDo this');
         expect(handoff?.path).toMatch(/my-plan\.md$/);

         expect(sm.consumePendingHandoffForNormal()).toBeNull();
       });

       it('handoffTo("normal") stores null artifact when plan file is empty', async () => {
         const agent = makeAgent();
         vi.mocked(agent.kaos.readText).mockResolvedValue('');
         const sm = new SessionMode(agent);
         await sm.enter('plan-id', undefined, false, 'plan');
         await sm.resolveFilePathFromModelRequest('.ody-code/plans/my-plan.md', '');

         await sm.handoffTo('normal');

         expect(sm.consumePendingHandoffForNormal()).toBeNull();
       });

       it('handoffTo("plan") clears _pendingHandoffForPlan when enter throws', async () => {
         const agent = makeAgent();
         vi.mocked(agent.kaos.readText).mockResolvedValue('# Design');
         const sm = new SessionMode(agent);
         vi.mocked(agent.kaos.mkdir).mockResolvedValue(undefined);
         await sm.enter('design-id', undefined, false, 'design');
         await sm.resolveFilePathFromModelRequest('.ody-code/designs/foo.md', '# Design');
         vi.mocked(agent.kaos.mkdir).mockRejectedValue(new Error('disk full'));

         await expect(sm.handoffTo('plan')).rejects.toThrow('disk full');

         expect(sm.consumePendingHandoffForPlan()).toBeNull();
       });

       it('cancel() does NOT store a pending handoff', async () => {
         const agent = makeAgent();
         vi.mocked(agent.kaos.readText).mockResolvedValue('## Plan content');
         const sm = new SessionMode(agent);
         await sm.enter('plan-id', undefined, false, 'plan');
         await sm.resolveFilePathFromModelRequest('.ody-code/plans/my-plan.md', '## Plan content');

         sm.cancel();

         expect(sm.consumePendingHandoffForNormal()).toBeNull();
         expect(sm.consumePendingHandoffForPlan()).toBeNull();
       });
     });
     ```

  2. 在 `packages/agent-core/test/agent/injection/design-mode.test.ts` 中，替换测试 `injects the handoff reminder (with design artifact) when a pending handoff for plan is set`（第 273-305 行）为：

     ```ts
     it('injects the handoff reminder (with design artifact) when a pending handoff for plan is set', async () => {
       const stub: DesignModeStub = { isActive: true, sessionModeFilePath: '/tmp/design.md' };
       let pendingHandoff: { path: string; filename: string; selectedLabel?: string } | null = {
         path: '/tmp/design.md',
         filename: 'design.md',
       };
       const agent = {
         ...designAgent(stub),
         sessionMode: {
           ...designAgent(stub).sessionMode,
           get isActive() { return stub.isActive; },
           get kind() { return 'design'; },
           get sessionModeFilePath() { return stub.sessionModeFilePath ?? null; },
           data: async () => stub.content === undefined ? null : { id: 'd1', content: stub.content, path: stub.sessionModeFilePath ?? '', kind: 'design' },
           consumePendingHandoffForPlan: () => {
             const p = pendingHandoff;
             pendingHandoff = null;
             return p;
           },
         },
       } as unknown as import('../../../src/agent').Agent;
       const injector = new DesignModeInjector(agent);

       await injector.inject();
       stub.isActive = false;
       await injector.inject();

       const text = lastReminder(agent);
       expect(text).toContain('Design mode completed');
       expect(text).toContain('plan mode');
       expect(text).toContain('Design saved to: /tmp/design.md');
       expect(text).toContain("approved design in 'design.md'");
       expect(text).not.toContain('# My Design');
     });
     ```

     并在文件末尾新增一个测试，覆盖 `selectedLabel` 分支：

     ```ts
     it('includes selected approach in handoff reminder when selectedLabel is present', async () => {
       const stub: DesignModeStub = { isActive: true, sessionModeFilePath: '/tmp/design.md' };
       let pendingHandoff: { path: string; filename: string; selectedLabel?: string } | null = {
         path: '/tmp/design.md',
         filename: 'design.md',
         selectedLabel: 'Approach A',
       };
       const baseAgent = designAgent(stub);
       const agent = {
         ...baseAgent,
         sessionMode: {
           ...baseAgent.sessionMode,
           get isActive() { return stub.isActive; },
           get kind() { return 'design'; },
           get sessionModeFilePath() { return stub.sessionModeFilePath ?? null; },
           data: async () => stub.content === undefined ? null : { id: 'd1', content: stub.content, path: stub.sessionModeFilePath ?? '', kind: 'design' },
           consumePendingHandoffForPlan: () => {
             const p = pendingHandoff;
             pendingHandoff = null;
             return p;
           },
         },
       } as unknown as import('../../../src/agent').Agent;
       const injector = new DesignModeInjector(agent);

       await injector.inject();
       stub.isActive = false;
       await injector.inject();

       const text = lastReminder(agent);
       expect(text).toContain('Selected approach: Approach A');
       expect(text).toContain('Execute ONLY the selected approach');
       expect(text).not.toContain('# My Design');
     });
     ```

- [ ] **Run them and verify they FAIL.**

  ```bash
  cd packages/agent-core
  pnpm test -- test/agent/session-mode.test.ts test/agent/injection/design-mode.test.ts
  ```

  期望失败：新断言 `not.toHaveProperty('content')`、`filename` 字段、`approved design in 'design.md'`、`not.toContain('# My Design')` 等均会失败，因为实现还没改。

- [ ] **Write the minimal implementation.**

  1. 在 `packages/agent-core/src/agent/session-mode/index.ts` 中：

     ```ts
     // 替换第 38-43 行
     private _pendingHandoffForPlan: {
       path: string;
       filename: string;
       selectedLabel?: string;
     } | null = null;
     ```

  2. 替换 `consumePendingHandoffForPlan`（第 251-256 行）：

     ```ts
     /** Consume and return the pending design→plan handoff artifact (if any). */
     consumePendingHandoffForPlan(): {
       path: string;
       filename: string;
       selectedLabel?: string;
     } | null {
       const p = this._pendingHandoffForPlan;
       this._pendingHandoffForPlan = null;
       return p;
     }
     ```

  3. 替换 `handoffTo` 中的 design→plan 分支（第 278-309 行）：

     ```ts
     async handoffTo(
       target: 'plan' | 'normal',
       opts?: { selectedLabel?: string },
     ): Promise<void> {
       const data = await this.data();

       if (target === 'plan') {
         const artifact =
           data !== null && data.path.length > 0
             ? {
                 path: data.path,
                 filename: basename(data.path),
                 selectedLabel: opts?.selectedLabel,
               }
             : null;
         this._pendingHandoffForPlan = artifact;
         this.exit();
         try {
           await this.enter(this.createSessionModeId(), false, true, 'plan');
         } catch (error) {
           this._pendingHandoffForPlan = null; // prevent ghost injection on next turn
           throw error;
         }
       } else {
         const artifact =
           data !== null && data.content.trim().length > 0
             ? { content: data.content, path: data.path }
             : null;
         const selectedLabel = opts?.selectedLabel;
         this._pendingHandoffForNormal =
           artifact === null
             ? null
             : selectedLabel !== undefined && selectedLabel.length > 0
               ? { ...artifact, selectedLabel }
               : artifact;
         this.exit();
       }
     }
     ```

     注意：`basename` 已从 `pathe` 导入（文件顶部已有 `import { basename, ... } from 'pathe';`）。

  4. 在 `packages/agent-core/src/agent/injection/design-mode.ts` 中：

     - 替换调用点（第 42-44 行）：

       ```ts
       const handoff = this.agent.sessionMode.consumePendingHandoffForPlan();
       if (handoff !== null) {
         return designToPlanHandoffReminder(handoff.path, handoff.filename, handoff.selectedLabel);
       }
       ```

     - 替换 `designToPlanHandoffReminder`（第 123-126 行）为最终模板：

       ```ts
       function designToPlanHandoffReminder(
         path: string,
         filename: string,
         selectedLabel?: string,
       ): string {
         const savedTo = path ? `Design saved to: ${path}\n\n` : '';
         const selectedLabelPrefix =
           selectedLabel !== undefined && selectedLabel.length > 0
             ? `Selected approach: ${selectedLabel}. Execute ONLY the selected approach; do not execute any unselected alternatives.\n\n`
             : '';
         return `Design mode completed. The approved design has been handed off — you are now in plan mode.\n\n${savedTo}${selectedLabelPrefix}Create a concrete, step-by-step implementation plan based on the approved design in \`${filename}\`. Do not implement anything yet.`;
       }
       ```

- [ ] **Run them and verify they PASS.**

  ```bash
  cd packages/agent-core
  pnpm test -- test/agent/session-mode.test.ts test/agent/injection/design-mode.test.ts
  ```

  期望：所有相关测试通过。

- [ ] **Update every caller and run whole-tree typecheck.**

  搜索所有使用点：

  ```bash
  cd /Users/ranwei/workspace/ody-code
  rg -n "consumePendingHandoffForPlan" packages/
  rg -n "_pendingHandoffForPlan" packages/
  ```

  确认只有 `session-mode/index.ts` 与 `design-mode.ts` 两处使用。如有新增 caller，同步更新。

  全树 typecheck：

  ```bash
  pnpm -r typecheck
  ```

  期望：无类型错误。

- [ ] **Commit.**

  ```bash
  git add packages/agent-core/src/agent/session-mode/index.ts \
          packages/agent-core/src/agent/injection/design-mode.ts \
          packages/agent-core/test/agent/session-mode.test.ts \
          packages/agent-core/test/agent/injection/design-mode.test.ts
  git commit -m "refactor(agent-core): slim design→plan handoff artifact to path/filename"
  ```

### Task 2: Slim `ExitDesignModeTool` output

**Depends on:** Task 1

**Files:**
- Modify: `packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts:67-207`
- Modify: `packages/agent-core/test/tools/exit-design-mode.test.ts:16-141`
- Test via: `pnpm test -- test/tools/exit-design-mode.test.ts`

**Rationale:** 这是用户可见的 tool result 变更点。`formatDesignHandoffOutput` 不再接收 `design` 参数，输出中移除 `## Approved Design:\n${design}`；`resolveDesign()` 只返回 `path`；`handoffToPlan()` 把 `selectedLabel` 传给 `SessionMode.handoffTo('plan', { selectedLabel })`。

- [ ] **Write the failing test.**

  在 `packages/agent-core/test/tools/exit-design-mode.test.ts` 中：

  1. 修改 `makeAgent` 的 mock，让 `handoffTo` 接收参数并记录：

     ```ts
     function makeAgent(
       input: {
         readonly active?: boolean | undefined;
         readonly design?: string | null | undefined;
         readonly path?: string | undefined;
         readonly sessionModeFilePath?: string | null | undefined;
         readonly emit?: ((event: unknown) => void) | undefined;
       } = {},
     ): { agent: Agent; requestApproval: ReturnType<typeof vi.fn>; emit: ReturnType<typeof vi.fn>; handoffTo: ReturnType<typeof vi.fn> } {
       let active = input.active ?? true;
       const requestApproval = vi.fn(async () => ({ decision: 'approved' }));
       const emit = vi.fn((event: unknown) => {
         input.emit?.(event);
         if ((event as { type?: string }).type === 'session_mode.exit') active = false;
       });
       const handoffTo = vi.fn(async () => undefined);
       const agent = {
         sessionMode: {
           get isActive() {
             return active;
           },
           get sessionModeFilePath() {
             return input.sessionModeFilePath ?? null;
           },
           data: vi.fn(async () => {
             if (input.design === null) return null;
             return {
               content: input.design ?? 'Step 1: brainstorm\nStep 2: evaluate',
               path: input.path ?? '/tmp/kimi-design.md',
             };
           }),
           finalizeFileName: vi.fn().mockResolvedValue(null),
           handoffTo,
           exit: () => {
             emit({ type: 'session_mode.exit' });
           },
         },
         rpc: { requestApproval },
         telemetry: { track: vi.fn() },
         emit,
       } as unknown as Agent;
       return { agent, requestApproval, emit, handoffTo };
     }
     ```

     并同步更新所有解构调用（`const { agent, ... } = makeAgent(...)`）以接收 `handoffTo`。

  2. 替换 `exits with the current design without consulting permission approval`（第 90-108 行）为：

     ```ts
     it('exits with the current design without consulting permission approval', async () => {
       const { agent, requestApproval, emit, handoffTo } = makeAgent({
         design: '# File Design',
         path: '/tmp/kimi-design.md',
       });

       const result = await executeTool(new ExitDesignModeTool(agent), {
         turnId: '0',
         toolCallId: 'call_1',
         args: {},
         signal,
       });

       expect(result.isError).toBe(false);
       expect(requestApproval).not.toHaveBeenCalled();
       expect(emit).toHaveBeenCalledWith({ type: 'session_mode.exit' });
       expect(result.output).toContain('Design saved to: /tmp/kimi-design.md');
       expect(result.output).toContain('Design mode deactivated');
       expect(result.output).not.toContain('# File Design');
       expect(handoffTo).toHaveBeenCalledWith('plan', { selectedLabel: undefined });
     });
     ```

  3. 替换 `returns an error when no design content is available`（第 125-141 行）为：

     ```ts
     it('allows empty design content when a valid path exists', async () => {
       const { agent, emit, handoffTo } = makeAgent({
         design: '',
         path: '/tmp/kimi-design.md',
       });

       const result = await executeTool(new ExitDesignModeTool(agent), {
         turnId: '0',
         toolCallId: 'call_empty',
         args: {},
         signal,
       });

       expect(result.isError).toBe(false);
       expect(result.output).toContain('Design saved to: /tmp/kimi-design.md');
       expect(handoffTo).toHaveBeenCalledWith('plan', { selectedLabel: undefined });
     });
     ```

  4. 在第 108 行后新增一个测试，验证 `selectedLabel` 传递：

     ```ts
     it('passes the declared selected label to handoffTo', async () => {
       const { agent, handoffTo } = makeAgent({
         design: '# File Design',
         path: '/tmp/kimi-design.md',
       });

       const result = await executeTool(new ExitDesignModeTool(agent), {
         turnId: '0',
         toolCallId: 'call_label',
         args: { options: [{ label: 'Approach A', description: 'Do A' }] },
         signal,
         metadata: { selectedLabel: 'Approach A' },
       });

       expect(result.isError).toBe(false);
       expect(result.output).toContain('Selected approach: Approach A');
       expect(handoffTo).toHaveBeenCalledWith('plan', { selectedLabel: 'Approach A' });
     });
     ```

- [ ] **Run it and verify it FAILS.**

  ```bash
  cd packages/agent-core
  pnpm test -- test/tools/exit-design-mode.test.ts
  ```

  期望失败：
  - 新断言 `not.toContain('# File Design')` 失败；
  - `allows empty design content...` 预期 `isError: false` 但旧实现返回错误；
  - `passes the declared selected label...` 中 `handoffTo` 被旧实现以无参方式调用。

- [ ] **Write the minimal implementation.**

  在 `packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts` 中：

  1. 替换 `ResolveDesignResult`（第 67-72 行）为：

     ```ts
     interface ResolveDesignResult {
       ok: boolean;
       path?: string | undefined;
       error?: ExecutableToolResult;
     }
     ```

  2. 替换 `execution`（第 114-137 行）为：

     ```ts
     private async execution(args: ExitDesignModeInput, metadata?: unknown): Promise<ExecutableToolResult> {
       if (!this.agent.sessionMode.isActive) {
         return {
           isError: true,
           output:
             'ExitDesignMode can only be called while design mode is active. Use EnterDesignMode (or /design) first.',
         };
       }

       const resolved = await this.resolveDesign();
       if (!resolved.ok) return resolved.error as ExecutableToolResult;

       const optionLabel = declaredOptionLabel(args.options, selectedLabelOf(metadata));

       const failed = await this.handoffToPlan(optionLabel);
       if (failed !== undefined) return failed;

       return {
         isError: false,
         output: formatDesignHandoffOutput(resolved.path, optionLabel),
       };
     }
     ```

  3. 替换 `handoffToPlan`（第 139-149 行）为：

     ```ts
     private async handoffToPlan(selectedLabel?: string): Promise<ExecutableToolResult | undefined> {
       try {
         await this.agent.sessionMode.handoffTo('plan', { selectedLabel });
       } catch (error) {
         const message = error instanceof Error ? error.message : 'Failed to hand off to plan mode.';
         return {
           isError: true,
           output: `Failed to exit design mode: ${message}`,
         };
       }
     }
     ```

  4. 替换 `resolveDesign`（第 151-178 行）为：

     ```ts
     private async resolveDesign(): Promise<ResolveDesignResult> {
       let data: SessionModeData;
       try {
         data = await this.agent.sessionMode.data();
       } catch (error) {
         const message = error instanceof Error ? error.message : 'Failed to read design file.';
         return {
           ok: false,
           error: { isError: true, output: `Failed to read design file: ${message}` },
         };
       }

       if (data !== null && data.path.length > 0) {
         return { ok: true, path: data.path };
       }

       const path = data?.path ?? this.agent.sessionMode.sessionModeFilePath;
       return {
         ok: false,
         error: {
           isError: true,
           output:
             path === null
               ? 'No design file found. Write the design to the current design file first, then call ExitDesignMode.'
               : `No design file found. Write your design to ${path} first, then call ExitDesignMode.`,
         },
       };
     }
     ```

  5. 替换 `formatDesignHandoffOutput`（第 199-207 行）为：

     ```ts
     function formatDesignHandoffOutput(
       path: string | undefined,
       selectedLabel: string | undefined,
     ): string {
       const optionPrefix = selectedApproachPrefix(selectedLabel);
       const savedTo = path !== undefined ? `Design saved to: ${path}\n\n` : '';
       return `${optionPrefix}Design mode deactivated. Now in plan mode.\n\n${savedTo}Create a concrete, step-by-step implementation plan based on the approved design saved above.`;
     }
     ```

- [ ] **Run it and verify it PASSES.**

  ```bash
  cd packages/agent-core
  pnpm test -- test/tools/exit-design-mode.test.ts
  ```

  期望：所有测试通过。

- [ ] **Commit.**

  ```bash
  git add packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts \
          packages/agent-core/test/tools/exit-design-mode.test.ts
  git commit -m "refactor(agent-core): remove full design content from ExitDesignMode output"
  ```

### Task 3: Final verification

**Depends on:** Task 2

**Files:**
- Test: `packages/agent-core/test/agent/session-mode.test.ts`
- Test: `packages/agent-core/test/agent/injection/design-mode.test.ts`
- Test: `packages/agent-core/test/tools/exit-design-mode.test.ts`

**Rationale:** 运行设计文档中要求的 must-pass 命令，确保三个修改点协同工作，无类型回归。

- [ ] **Run targeted tests.**

  ```bash
  cd packages/agent-core
  pnpm test -- test/tools/exit-design-mode.test.ts test/agent/injection/design-mode.test.ts test/agent/session-mode.test.ts
  ```

  期望：三个测试文件全部通过。

- [ ] **Run package typecheck.**

  ```bash
  cd packages/agent-core
  pnpm typecheck
  ```

  期望：无类型错误。

- [ ] **(Recommended) Run whole-tree typecheck to guard against cross-package callers.**

  ```bash
  cd /Users/ranwei/workspace/ody-code
  pnpm -r typecheck
  ```

  期望：无类型错误。

- [ ] **Generate a changeset if this is the end of the branch.**

  按照项目 `gen-changesets` skill 生成 changeset（默认 `minor`，除非用户明确要求 `major`）。

---

## Self-Review

- [ ] 1. **Spec-coverage table:**

  | 设计文档需求 | 覆盖任务 | 状态 |
  |---|---|---|
  | `ExitDesignModeTool` tool result 去正文 | Task 2 | covered |
  | design→plan handoff artifact 瘦身 | Task 1 | covered |
  | plan 模式首条 reminder 去正文 | Task 1 | covered |
  | 测试同步更新 | Task 1, Task 2 | covered |
  | `handoffTo('plan')` 以 `path` 存在性为核心 | Task 1 | covered |
  | 空 content + 有效 path 仍 handoff | Task 1 (test), Task 2 (test) | covered |
  | `selectedLabel` 透传 | Task 1, Task 2 | covered |
  | 无 `summary` 字段 | Out of scope | no-op |
  | 无 feature flag | Out of scope | no-op |

- [ ] 2. **Placeholder scan:** 无 `TODO`/`TBD`/"later"/"appropriate"；每个任务都有完整代码、命令与预期输出。

- [ ] 3. **No phantom tasks:** 每个任务都产生可验证变更；Task 3 是显式验证步骤，非空提交。

- [ ] 4. **Dependency soundness:** Task 1 无依赖；Task 2 依赖 Task 1；Task 3 依赖 Task 2。无后向引用。

- [ ] 5. **Caller & build soundness:**
  - Task 1 修改了共享签名 `consumePendingHandoffForPlan`，同任务更新了 `design-mode.ts` 调用点、`session-mode.test.ts` 与 `design-mode.test.ts`；
  - Task 1 以 `rg` 搜索全部使用点并以 `pnpm -r typecheck` 结束；
  - `_pendingHandoffForPlan` 的 shape 变更未影响 `plan→normal` 路径；
  - `filename` 来自 `basename(data.path)`，下游 `DesignModeInjector` 只用它渲染文本，无权限守卫或路径匹配器依赖该字段。

- [ ] 6. **Test-the-risk:**
  - Task 1 测试断言 artifact 不再含 `content`，并验证空 content + 有效 path 仍产生 artifact；
  - Task 1 测试断言 reminder 不含 `# My Design` 但包含文件名；
  - Task 2 测试断言 tool result 不含 `# File Design`，且 `handoffTo` 收到 `{ selectedLabel }`。

- [ ] 7. **Type consistency:**
  - `consumePendingHandoffForPlan` 返回 `{ path, filename, selectedLabel? }`；
  - `designToPlanHandoffReminder(path, filename, selectedLabel?)` 与调用点一致；
  - `formatDesignHandoffOutput(path, selectedLabel)` 与调用点一致；
  - `SessionMode.handoffTo('plan', { selectedLabel? })` 与 `ExitDesignModeTool.handoffToPlan(selectedLabel?)` 一致。

