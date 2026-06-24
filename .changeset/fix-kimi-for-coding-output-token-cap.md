---
"@odysseythink/kimi-code-oauth": patch
"ody-code": patch
---

Fix `kimi-for-coding` output token cap during provider import by applying a known 32K `max_output_size` override, preventing truncated tool calls in plan mode.
