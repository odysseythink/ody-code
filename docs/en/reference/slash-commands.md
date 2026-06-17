# Slash Commands

Ody Code CLI provides a set of built-in slash commands for controlling the session, configuring the environment, and managing workflows.

## /microagent

::: info Added
Added in an upcoming release. Requires the `repo-knowledge` experimental flag (`ODY_CODE_EXPERIMENTAL_REPO_KNOWLEDGE=1`).
:::

Create a new knowledge microagent in your project's `.ody-code/microagents/` directory. Knowledge microagents are automatically injected into the conversation when matching trigger keywords appear in user messages.

When you run `/microagent`, an interactive wizard guides you through three steps:

1. **Name** — A short file name using only lowercase letters, digits, hyphens, and underscores (e.g. `reuse-conventions`).
2. **Trigger keywords** — Comma-separated keywords that cause this microagent to be injected. Examples: `component, page, 组件`.
3. **Description** — A one-line summary (up to 200 characters) shown in the microagent's frontmatter.

On first use, Ody Code CLI automatically installs a starter pack of four example microagents (`reuse-conventions`, `glossary`, `testing`, `documentation`) to help you get started.

The generated file follows the standard microagent format:

```markdown
---
name: my-conventions
type: knowledge
triggers:
  - keyword1
  - keyword2
description: What this microagent does
---

# my-conventions

<!-- TODO: Add repo-specific conventions below. -->
```

After creation, edit the file to replace the TODO comment with your project-specific knowledge. The microagent will be picked up on the next turn.
