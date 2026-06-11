---
"@odysseythink/agent-core": patch
"ody-code": patch
---

Fix a `400 tool_call_id is not found` error when a design session hands off to plan mode. Approving `ExitDesignMode` switched the active context partition to plan *during* the tool exchange, so the tool result landed in the plan partition while its tool call stayed in design — orphaning the `tool_call_id` and making the first plan-mode request fail. The context-partition switch is now deferred to the end of the step whenever a tool exchange is open (for every target mode, not just normal), so the mode-transitioning tool's call and result always stay together and the new partition starts clean. This also fixes the same latent orphan in the model-invoked `EnterPlanMode`/`EnterDesignMode` tools.
