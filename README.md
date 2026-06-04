# Ody Code CLI

The Starting Point for Next-Gen Agents.

> **Ody Code** 是一个面向下一代智能体的命令行工具（CLI / TUI），帮助开发者通过自然语言与代码库交互，完成编码、调试、重构等任务。

---

## 环境要求

| 工具 | 版本要求 |
|------|---------|
| Node.js | >= **24.15.0** |
| pnpm | **10.33.0** |

> 建议使用 [nvm](https://github.com/nvm-sh/nvm)、[fnm](https://github.com/Schniz/fnm) 或 [mise](https://mise.jdx.dev/) 管理 Node.js 版本。仓库中的 `.nvmrc` 文件已锁定最小推荐版本。

---

## 快速开始

### 1. 安装依赖

```bash
pnpm install
```

或者使用 Makefile：

```bash
make prepare
```

### 2. 构建项目

构建所有 workspace 包：

```bash
pnpm run build
```

仅构建 `packages/` 下的核心包：

```bash
pnpm run build:packages
```

或者使用 Makefile：

```bash
make build
```

### 3. 启动开发环境

启动 CLI 开发模式：

```bash
pnpm run dev:cli
```

或者：

```bash
make dev
```

启动可视化调试工具（vis）：

```bash
pnpm run vis
```

或者：

```bash
make vis
```

---

## 编译说明

### 完整构建流程

```bash
# 1. 安装依赖
pnpm install

# 2. 类型检查
pnpm run typecheck

# 3. 构建所有包
pnpm run build

# 4. 运行测试
pnpm run test

# 5. 代码检查
pnpm run lint
```

### 使用 Nix 构建（可选）

本项目支持通过 [Nix](https://nixos.org/) 构建可移植的原生二进制文件（SEA）：

```bash
# 进入 Nix 开发环境
nix develop

# 构建原生 CLI 二进制
nix build

# 运行构建产物
./result/bin/ody
```

> Nix 构建会自动处理 Node.js 和 pnpm 的版本匹配，适合需要可复现构建的场景。

### Makefile 命令速查

| 命令 | 说明 |
|------|------|
| `make prepare` | 安装依赖 |
| `make build` | 构建所有包 |
| `make typecheck` | TypeScript 类型检查 |
| `make lint` | 代码检查（oxlint） |
| `make lint-fix` | 自动修复代码问题 |
| `make test` | 运行测试 |
| `make test-watch` | 监听模式运行测试 |
| `make test-coverage` | 运行测试并生成覆盖率报告 |
| `make clean` | 清理构建产物 |
| `make dev` | 启动 CLI 开发模式 |
| `make vis` | 启动 vis 开发模式 |
| `make changeset` | 创建 changeset |
| `make version` | 版本升级 |
| `make publish` | 发布包 |

---

## 项目结构

这是一个基于 pnpm workspace 的 TypeScript 单体仓库：

```
├── apps/
│   ├── ody-code/          # CLI / TUI 主应用
│   └── vis/               # 可视化调试工具
├── packages/
│   ├── agent-core/        # 智能体引擎核心
│   ├── node-sdk/          # TypeScript SDK
│   ├── kaos/              # 执行环境与进程抽象
│   ├── kosong/            # LLM / Provider 抽象层
│   ├── oauth/             # OAuth 认证工具
│   └── telemetry/         # 遥测与监控
├── docs/                  # 文档站点（VitePress）
├── build/                 # 构建工具与插件
└── plugins/               # 插件市场
```

---

## 贡献指南

1. Fork 本仓库
2. 创建功能分支：`git checkout -b feature/xxx`
3. 提交更改：`git commit -m "feat: xxx"`
4. 推送分支：`git push origin feature/xxx`
5. 创建 Pull Request

请确保提交前通过所有测试和代码检查：

```bash
pnpm run typecheck && pnpm run lint && pnpm run test
```

---

## 许可证

[MIT](LICENSE)
