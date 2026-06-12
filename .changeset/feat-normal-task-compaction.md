---
"@odysseythink/agent-core": minor
"ody-code": minor
---

Auto-compact at TodoList task boundaries in normal mode. When executing a plan using `executing-plans` or similar (a continuous multi-task iteration in normal mode), the context can grow unbounded. This feature detects when a task is marked done (TodoList state increases `done` count) and compacts at that safe checkpoint once context usage exceeds a configurable threshold, then continues to the next task on a compacted context. Configurable via `[loop_control] normal_task_compaction_ratio` (default `0.5`; `0` disables it). The global auto-compaction threshold (0.85) is unchanged. Only active in normal mode — plan and design modes are unaffected.
