# 斜杠命令

Ody Code CLI 提供了一系列内置斜杠命令，用于控制会话、配置环境和管理工作流。

## /microagent

::: info 新增
新增于即将发布的版本。需要启用 `repo-knowledge` 实验性功能标志（`ODY_CODE_EXPERIMENTAL_REPO_KNOWLEDGE=1`）。
:::

在项目的 `.ody-code/microagents/` 目录中创建一个新的「知识微 Agent」。当用户消息中出现匹配的触发关键词时，知识微 Agent 会自动注入到对话上下文中。

运行 `/microagent` 后，交互式向导会引导你完成三个步骤：

1. **名称** — 一个简短的微 Agent 文件名，只能包含小写字母、数字、连字符和下划线（例如 `reuse-conventions`）。
2. **触发关键词** — 逗号分隔的关键词，当用户消息中包含这些关键词时触发注入。例如：`组件, page, 组件`。
3. **描述** — 一行简短描述（最多 200 个字符），显示在微 Agent 的 frontmatter 中。

首次使用时，Ody Code CLI 会自动安装入门模板包，包含四个示例微 Agent（`reuse-conventions`、`glossary`、`testing`、`documentation`），帮助你快速上手。

生成的文件遵循标准微 Agent 格式：

```markdown
---
name: my-conventions
type: knowledge
triggers:
  - keyword1
  - keyword2
description: 此微 Agent 的用途说明
---

# my-conventions

<!-- TODO: 在下方添加项目专属的规范内容。 -->
```

创建完成后，编辑文件将 TODO 注释替换为你的项目专属知识。微 Agent 将在下一轮对话中被自动加载。
