---
"@odysseythink/ody-code-sdk": patch
"@odysseythink/kimi-code": patch
---

Fix `--host=rust` startup hang by correcting the Rust host spawn arguments, adding ready-message handling for stdio transport, and surfacing spawn failures instead of waiting indefinitely.
