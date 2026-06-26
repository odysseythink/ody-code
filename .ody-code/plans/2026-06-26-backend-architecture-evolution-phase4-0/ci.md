# Part 5 — CI Parity Integration

本 Part 把 parity 测试接入 npm scripts 与 GitHub Actions：开发本地可一键运行 TS-vs-TS 与 TS-vs-Rust 测试套件，CI 在 Rust host 构建完成后自动执行 TS-vs-Rust parity smoke。

---

## Part 5 依赖图

```
E1 package.json scripts
  │
  ▼
E2 GitHub Actions parity job
```

E1 独立；E2 依赖 E1 的 script 名称。

---

## Part 5 范围说明

- **覆盖**：`packages/integration-tests/package.json` parity scripts、根 `package.json` 便捷脚本、`.github/workflows/rust-host.yml` 新增 parity 步骤。
- **不覆盖**：新增测试代码或框架改动（已在 Part 1–4 完成）。
- **共享签名**：无共享类型变更；仅新增 npm scripts 与 workflow steps。

---

### Task E1: Parity npm Scripts

**Depends on:** none

**Files:**
- Modify: `packages/integration-tests/package.json:17-21`
- Modify: `package.json:7-36`

**Goal:** 在 `integration-tests` 包与根 monorepo 各加一组可执行的 parity script，让本地开发和 CI 调用路径一致。

- [ ] 修改 `packages/integration-tests/package.json`，在 `scripts` 中追加：

```json
{
  "scripts": {
    "test": "vitest run",
    "test:parity": "vitest run test/parity",
    "test:parity:ts-vs-ts": "vitest run test/parity/ts-vs-ts.test.ts",
    "test:parity:ts-vs-rust": "vitest run test/parity/ts-vs-rust.test.ts",
    "typecheck": "tsc -p tsconfig.json --noEmit",
    "clean": "rm -rf dist"
  }
}
```

- [ ] 修改根 `package.json`，在 `scripts` 中追加便捷入口：

```json
{
  "scripts": {
    "build": "pnpm -r run build",
    "...": "...",
    "test:parity": "pnpm --filter integration-tests test:parity",
    "test:parity:ts-vs-ts": "pnpm --filter integration-tests test:parity:ts-vs-ts",
    "test:parity:ts-vs-rust": "pnpm --filter integration-tests test:parity:ts-vs-rust"
  }
}
```

- [ ] 运行 TS-vs-TS script 验证脚本可用：

```bash
pnpm run test:parity:ts-vs-ts
```

预期：`packages/integration-tests/test/parity/ts-vs-ts.test.ts` 中的 3 个用例全部通过。

- [ ] 运行完整 parity script：

```bash
pnpm run test:parity
```

预期：所有 `test/parity/**/*.test.ts` 用例执行（TS-vs-Rust 在本地若无二进制会被跳过）。

- [ ] 提交：

```bash
git add packages/integration-tests/package.json package.json
git commit -m "feat(integration-tests): parity npm scripts"
```

---

### Task E2: GitHub Actions Parity Smoke Step

**Depends on:** Task E1

**Files:**
- Modify: `.github/workflows/rust-host.yml:46-62`

**Goal:** 在现有 Rust Host Smoke workflow 的矩阵任务里，于 Phase A3 验证之后加入 parity smoke 步骤；复用同一次构建产物，避免重复编译 Rust host。

- [ ] 修改 `.github/workflows/rust-host.yml`，在 `Phase A3 verification` 步骤之后、`Upload Phase A3 report` 步骤之前插入：

```yaml
      - name: Parity smoke tests
        id: parity-smoke
        run: pnpm run test:parity:ts-vs-rust
        shell: bash
        env:
          ODY_HOST_BINARY_PATH: ${{ github.workspace }}/rust-ody/target/release/ody-host
```

修改后的关键片段应为：

```yaml
      - name: Phase A3 verification
        id: phase-a3
        run: pnpm run verify:phase-a3
        shell: bash
        env:
          ODY_HOST_BINARY_PATH: ${{ github.workspace }}/rust-ody/target/release/ody-host
          ODY_CODE_REPORT_DIR: ${{ github.workspace }}/.ody-code/reports

      - name: Parity smoke tests
        id: parity-smoke
        run: pnpm run test:parity:ts-vs-rust
        shell: bash
        env:
          ODY_HOST_BINARY_PATH: ${{ github.workspace }}/rust-ody/target/release/ody-host

      - name: Upload Phase A3 report
        if: always()
        ...
```

- [ ] 验证 workflow YAML 格式：

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/rust-host.yml'))" && echo "YAML OK"
```

预期输出：`YAML OK`。

- [ ] 手动验证：在已构建 Rust host 的本地环境运行与 CI 相同的命令：

```bash
ODY_HOST_BINARY_PATH=$(pwd)/rust-ody/target/release/ody-host pnpm run test:parity:ts-vs-rust
```

预期：3 个 TS-vs-Rust 用例全部通过（或被 known gap 覆盖）。

- [ ] 提交：

```bash
git add .github/workflows/rust-host.yml
git commit -m "ci: run TS-vs-Rust parity smoke in rust-host workflow"
```

---

## Part 5 本地 Self-Review

| 检查项 | 结论 |
|---|---|
| 1. Spec-coverage | npm scripts → E1；GitHub Actions parity job → E2。 |
| 2. Placeholder scan | 无 TODO/TBD。 |
| 3. No phantom tasks | E1/E2 均产生可验证变更（script 可运行、workflow 可解析）。 |
| 4. Dependency soundness | E1 独立；E2 依赖 E1 的 script 名称。 |
| 5. Caller & build soundness | 无共享签名变更；仅新增 scripts 与 workflow steps。 |
| 6. Test-the-risk | E1 运行 script 验证测试确实执行；E2 本地运行与 CI 等价的命令验证。 |
| 7. Type一致性 | 无类型变更。 |

- [ ] 1. Spec-coverage table: npm scripts → E1, GitHub Actions parity job → E2.
- [ ] 2. Placeholder scan: 无 TODO/TBD/占位符。
- [ ] 3. No phantom tasks: E1 产生 package.json 变更并运行验证；E2 产生 workflow 变更并解析验证。
- [ ] 4. Dependency soundness: E1 独立；E2 依赖 E1。
- [ ] 5. Caller & build soundness: 无共享签名变更。
- [ ] 6. Test-the-risk: E1 运行 script；E2 本地复现 CI 命令。
- [ ] 7. Type一致性: 无新增类型。

