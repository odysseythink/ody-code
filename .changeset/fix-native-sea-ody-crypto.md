---
'@odysseythink/ody-crypto': patch
'ody-code': patch
---

Fix native SEA build by exposing `./package.json` from `@odysseythink/ody-crypto` and declaring it as a dependency of `ody-code`, allowing the native asset collector to resolve the crypto host and its platform-specific binary.
