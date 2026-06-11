---
"@odysseythink/agent-core": minor
"ody-code": minor
---

Auto-compact at split-plan part boundaries. When a large plan (or design) is generated as a split index with multiple part files, the agent now compacts the conversation at each completed-part boundary once context usage crosses a configurable threshold, then continues to the next part on a compacted context. The part boundary is a safe checkpoint: the manifest and already-written part files live on disk, so nothing is lost. Configurable via `[loop_control] split_plan_compaction_ratio` (default `0.5`; `0` disables it). The global auto-compaction threshold (0.85) is unchanged.
