---
"@odysseythink/agent-core": patch
---

Drop orphaned tool-result messages at the LLM send boundary. A `role:'tool'` message whose `toolCallId` has no matching tool call in the projected history is an orphan, which the provider rejects with `400 tool_call_id is not found`, wedging the session on every request. This could happen to sessions corrupted by a pre-fix design→plan handoff that routed an `ExitDesignMode` result into the plan partition while its call stayed in design. The new `dropOrphanToolResults` guard runs only in `ContextMemory.get messages()` (the full-history send boundary, never on sub-slices used by compaction/token-accounting) so it heals such legacy histories on resume and contains any future partition-routing regression. For healthy sessions it is a strict no-op. The symmetric dangling-call side (a tool call with no result) is documented as a known, lower-severity gap.
