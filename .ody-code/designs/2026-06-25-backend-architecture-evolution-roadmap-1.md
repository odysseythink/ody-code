# Phase 2-E 设计 —— 首个 Native 模块: `ody-crypto`(napi-rs)

> **所属总路线**: `.ody-code/roadmaps/backend-architecture-evolution-roadmap.md` 的 Phase 2-E(策略 N)。
> **审计级别**: Deep
> **状态**: 待批准

## Scope In/Out

**In scope** [C:USER]:
1. Rust crate `ody-crypto` 位于 `rust-ody/crates/ody-crypto`,用 `napi-rs` 暴露四个原子函数:
   - `randomBytes(length: number): Buffer`
   - `sha256(input: Buffer | string): string(hex)`
   - `pkceChallenge(length?: number): { codeVerifier: string, codeChallenge: string }`
   - `verifyIdToken(jwt: string, jwk: string, expected: IdTokenExpected): IdTokenClaims`
2. TS 加载器/封装包 `packages/ody-crypto`,含自动平台探测、`require` 正确平台子包 `.node`、失败时静默降级到 TS 实现。
3. TS loader 支持 6 个目标(darwin-arm64/x64、linux-arm64/x64、win32-arm64/x64);平台子包当前发布 5 个(darwin-arm64/x64、linux-arm64/x64、win32-x64),`win32-arm64` 缺失时自动走 TS fallback。
4. `packages/mcp-host/src/oauth/service.ts` 改造:授权 URL 不再调用 SDK `auth()` 的 redirect 分支,改为用 `ody-crypto.pkceChallenge()` 生成 verifier/challenge 并手动拼 URL;token 交换继续复用 SDK `exchangeAuthorization()`。
5. `packages/mcp-host/src/oauth/provider.ts` 的 `state()` 改为调用 `ody-crypto.randomBytes()`。
6. 新增 `packages/mcp-host/src/oauth/id-token.ts`,在 token 响应包含 `id_token` 时调用 `ody-crypto.verifyIdToken()` 做 RS256/ES256 签名与标准 claims 校验。
7. 更新 `apps/ody-code/scripts/native/native-deps.mjs`,把 `ody-crypto` 按 clipboard 模式注册进 native asset 收集。
8. 产出《Phase 2-E Native SEA 成本报告》:构建时间、`.node` 体积、SEA 注入耗时、smoke test 结果、与 Wasm 分发的对比结论。

**Out of scope** [C:USER] / [C:DEFERRED]:
- 不替换 `packages/oauth` 设备码流(device code flow 无 PKCE 需求)。
- 不迁移 `kosong`/`kaos` 等 I/O 核心 Rust 化(那是策略 H,依赖 G3)。
- 不实现 JWT 加密(JWE)、refresh token 签名、DCR client assertion 签名;这些归入后续 Native 迭代或 H 阶段。
- 不追求 `linux-musl`;`win32-arm64` 由 loader 支持但暂不发布平台子包,走 TS fallback。目标矩阵先聚焦 SEA 已构建的四平台 CI(darwin-arm64/x64、linux-x64、win32-x64),`linux-arm64` 由本地或自托管 runner 补充。
- 不做运行时性能基准:PKCE/JWT 不是端到端热点,Phase 2-E 只记录边界调用耗时,不产《W 收益报告》式的端到端数据。

## Prior Art

[C:INFERRED] 本项目属内部演进,无外部系统需要完整移植。参考对象均为仓库内既有模式:
- `rust-ody/` 已验证 Rust→Wasm→SEA 链路,本阶段改为 Rust→napi-rs→.node→SEA。
- `apps/ody-code/scripts/native/native-deps.mjs` 中 clipboard/koffi 的 "host package + 平台子包 + native asset 收集"模式可直接复用。
- `@modelcontextprotocol/sdk` 的 `auth()`/`exchangeAuthorization()` 是现有依赖,改造时不重写其协议逻辑。

## Reuse Analysis

| 候选 | 路径 | 复用方式 |
|---|---|---|
| MCP OAuth Service | `packages/mcp-host/src/oauth/service.ts` | 改造:保留 discovery/DCR/refresh,仅把 redirect 分支从 `auth()` 切到手动 URL 构造 + `exchangeAuthorization()`。 |
| MCP OAuth Provider | `packages/mcp-host/src/oauth/provider.ts` | 改造:`state()` 调用 `ody-crypto.randomBytes()`;`saveCodeVerifier`/`codeVerifier` 保留,verifier 来源改为 Native/TS fallback。 |
| Native asset 收集器 | `apps/ody-code/scripts/native/native-deps.mjs` | 扩展:新增 `ody-crypto` host + 平台子包条目,`collect: 'native-files'`。 |
| Native asset 运行时加载 | `apps/ody-code/src/native/native-assets.ts` | 复用:SEA 下 `.node` 会被解压到缓存目录,`ody-crypto` loader 通过 manifest 定位。 |
| Rust 构建脚本 | `rust-ody/build.sh`、`rust-ody/Cargo.toml` | 借鉴:新增 crate 用同样 release profile(LTO、strip、panic=abort),但 crate-type 改为 `cdylib` for napi-rs。 |
| PKCE 参考实现 | `node_modules/pkce-challenge` | 行为对齐:默认 verifier length = 43,S256,base64url;TS fallback 直接用它或等效实现。 |
| SDK token 交换 | `@modelcontextprotocol/sdk/dist/esm/client/auth.js` | 复用:调用导出的 `exchangeAuthorization()` 完成 code→token。 |

## Architecture & Data Flow

```text
[packages/mcp-host]
  McpOAuthService.beginAuthorization
    -> CallbackServer.startCallbackServer()                    (I/O, 不变)
    -> OdyCrypto.pkceChallenge({ length: 43 })                 (Native or TS fallback)
    -> provider.setRedirectUrl(redirectUri)
    -> provider.state()   // 由 OdyCrypto.randomBytes 生成
    -> 手动构造 authorization URL(code_challenge=S256, state, client_id, ...)
    -> provider.redirectToAuthorization(url)

  用户浏览器完成授权
  CallbackServer.waitForCode() -> { code, state }
    -> provider.expectedState() 对比 state
    -> SDK exchangeAuthorization(authServerUrl, { clientInfo, code, codeVerifier, redirectUri })
    -> 若响应含 id_token
         -> fetch JWKS (TS)
         -> OdyCrypto.verifyIdToken(jwt, jwk, { iss, aud })
    -> provider.saveTokens(tokens)

[packages/ody-crypto]
  index.ts
    -> 按 process.platform + process.arch 选择平台子包 .node
    -> try require('.node')
       失败 -> debug log -> 返回 TS fallback 实现
    -> 导出 randomBytes / sha256 / pkceChallenge / verifyIdToken

[rust-ody/crates/ody-crypto]
  src/lib.rs
    -> napi-rs 导出上述 4 个函数
    -> 依赖:rand、sha2、base64、jsonwebtoken(Rust) 等
```

数据变化:
- `code_verifier` 由 `pkce-challenge`(npm) 生成 → `ody-crypto.pkceChallenge()` 生成;格式不变(base64url S256 challenge + 43-char verifier)。
- `state` 由 `node:crypto.randomBytes` → `ody-crypto.randomBytes`;格式不变(hex string)。
- `id_token` 此前不验证 → 新增 `verifyIdToken` 调用,验证失败抛错,不保存 tokens。
- SEA 产物中新增 `ody-crypto-<target>.node`,大小取决于 Rust 依赖(预估 1–3 MB,需实测)。

## Data Models

所有类型在 `packages/ody-crypto/src/types.ts` 定义 [C:USER]:

```ts
export interface PkceChallenge {
  readonly codeVerifier: string;
  readonly codeChallenge: string;
}

export interface IdTokenExpected {
  readonly issuer: string;
  readonly audience: string;
  readonly maxAgeSeconds?: number;
}

export interface IdTokenClaims {
  readonly sub: string;
  readonly iss: string;
  readonly aud: string | string[];
  readonly exp: number;
  readonly iat: number;
  readonly [claim: string]: unknown;
}

export interface OdyCrypto {
  randomBytes(length: number): Buffer;
  sha256(input: string | Buffer): string;
  pkceChallenge(length?: number): PkceChallenge;
  verifyIdToken(jwt: string, jwkJson: string, expected: IdTokenExpected): IdTokenClaims;
}
```

Rust 侧 napi 导出签名 [C:INFERRED]:

```rust
#[napi]
pub fn random_bytes(length: u32) -> Buffer;

#[napi]
pub fn sha256(input: Either<String, Buffer>) -> String;

#[napi(object)]
pub struct PkceChallenge { pub code_verifier: String, pub code_challenge: String }

#[napi]
pub fn pkce_challenge(length: Option<u32>) -> PkceChallenge;

#[napi(object)]
pub struct IdTokenExpected { pub issuer: String, pub audience: String, pub max_age_seconds: Option<i64> }

#[napi(object)]
pub struct IdTokenClaims { pub sub: String, pub iss: String, pub aud: Either<String, Vec<String>>, pub exp: i64, pub iat: i64, pub extra: HashMap<String, String> }

#[napi]
pub fn verify_id_token(jwt: String, jwk_json: String, expected: IdTokenExpected) -> Result<IdTokenClaims, Error>;
```

## Algorithms

**Algorithm 1 — `pkceChallenge(length?)`** [C:UPSTREAM] 对齐 `pkce-challenge` 行为:

```
pkceChallenge(length):
  n = length ?? 43
  if n < 43 or n > 128: throw RangeError
  alphabet = [A-Z, a-z, 0-9, '-', '.', '_', '~']  // 66 chars
  verifier = generateRandomString(n, alphabet)
  challenge = base64url(sha256(utf8(verifier)))
  return { codeVerifier: verifier, codeChallenge: challenge }
```

**Algorithm 2 — `verifyIdToken(jwt, jwkJson, expected)`** [C:USER]:

```
verifyIdToken(jwt, jwkJson, expected):
  parts = split jwt by '.'
  if len(parts) != 3: throw MalformedJwtError
  header = json(base64urlDecode(parts[0]))
  payload = json(base64urlDecode(parts[1]))
  if header.alg not in ['RS256', 'ES256']: throw UnsupportedAlgError
  if payload.iss != expected.issuer: throw IssuerMismatchError
  if payload.aud is array:
    if expected.audience not in payload.aud: throw AudienceMismatchError
  else:
    if payload.aud != expected.audience: throw AudienceMismatchError
  now = currentUnixSeconds()
  if payload.exp is missing or payload.exp <= now: throw ExpiredError
  jwk = json(jwkJson)
  if jwk.kty != (alg == RS256 ? 'RSA' : 'EC'): throw KeyTypeMismatchError
  verifySignature(parts, jwk, header.alg)
  return payload as IdTokenClaims
```

**Algorithm 3 — TS loader 平台选择** [C:INFERRED]:

```
loadNative():
  target = `${process.platform}-${process.arch}`
  if target not in SUPPORTED_TARGETS: return null
  packageName = `@odysseythink/ody-crypto-${target}`
  try:
    addon = require(packageName)
    return addon as OdyCrypto
  catch err:
    debug('ody-crypto native load failed', target, err.message)
    return null

getOdyCrypto():
  native = loadNative()
  if native != null: return native
  return loadTsFallback()
```

## Error Handling

| 错误类 | 立即处理 | 降级路径 | 恢复条件 |
|---|---|---|---|
| Native `.node` require 失败(dlopen 不兼容、SEA asset 缺失) | 捕获并 debug log | 返回 TS fallback(`node:crypto` + `jsonwebtoken`) | 下一次进程启动重试;若 SEA asset 修复则自动恢复 |
| Native 函数调用 panic/异常 | 捕获并 log | 同一次调用转 TS fallback | 同左 |
| `verifyIdToken` 签名/claims 失败 | 抛错,不保存 tokens | 无(安全失败) | 用户重试登录或更换授权服务器 |
| `pkceChallenge` 参数越界(<43 或 >128) | 抛 RangeError | 不需要(调用方固定 43) | 修复调用参数 |
| `state` 回调不匹配 | 抛 OAuthStateMismatchError | 无 | 用户重新发起授权 |

## Security Considerations

[C:USER]
- Native 模块只处理公开算法输入,不持久化 secrets;`code_verifier` 与 tokens 仍由 `McpOAuthClientProvider`/磁盘 store 管理。
- `randomBytes` 使用 Rust `rand::thread_rng()`(或 `getrandom`),不暴露 seed。
- JWT 校验在 Native 完成,但 JWK 由 TS 侧通过现有 `fetch` 拉取,避免 Native 内嵌网络逻辑。
- `verifyIdToken` 不对称比较字符串失败即抛错,不向日志打印 JWT 内容。
- 平台子包 `.node` 在 SEA 中通过 sha256 manifest 校验,防止注入替换。

## Observability

[C:USER]
- TS loader在 Native 加载失败或 fallback 时输出 `debug('ody-crypto: <message>')`,使用现有 `debug` 命名空间(若有)或 `console.debug`。
- Phase 2-E 交付《Native SEA 成本报告》,至少包含:
  - 各 target `cargo build --release` 耗时
  - `.node` 文件大小(stripped 与未 stripped)
  - SEA 注入/解压 smoke test 耗时
  - 启动时 `require` 成功率
  - 与 `rust-ody` Wasm(~17 KB 通用产物)的分发成本对比表
- 不新增运行时 telemetry 指标或用户可见事件。

## Operations

[C:USER]
- 默认启用 Native 模块,无环境变量 flag。加载失败自动降级,不影响功能。
- 本地开发:安装依赖时,pnpm 通过 `optionalDependencies` 只安装当前平台子包;非当前平台子包缺失不中断安装。
- CI:新增 `build:native:crypto` 脚本,在对应 runner 上为每个 target 编译 `.node`,并复制到平台子包目录。GitHub Actions 矩阵:macos-13(x64)、macos-14(arm64)、ubuntu-24.04(x64)、windows-2022(x64)。`linux-arm64` 与 `win32-arm64` 未纳入 GitHub-hosted runner,由本地或自托管 runner 补充。
- SEA 构建:更新 `apps/ody-code/scripts/native/native-deps.mjs`:

```js
{
  id: 'ody-crypto-host',
  name: () => '@odysseythink/ody-crypto',
  collect: 'js-only',
  parent: null,
},
{
  id: 'ody-crypto-target',
  name: (target) => `@odysseythink/ody-crypto-${target}`,
  collect: 'native-files',
  parent: 'ody-crypto-host',
},
```

- 发布流程:平台子包作为独立 package 发布;主包 `ody-crypto` 的 `optionalDependencies` 指向它们。SEA release 构建时只打包当前 target 的 `.node`。

## Test Plan

[C:USER] 每个测试都有明确的 must-pass / must-reject 断言:

1. **Rust unit tests** (`cargo test -p ody-crypto`):
   - `pkce_challenge_default_length`: `result.codeVerifier.length == 43`。
   - `pkce_challenge_s256_matches`: 对同一个 verifier 用 TS `pkce-challenge` 的 `generateChallenge` 计算,`result.codeChallenge == expected`。
   - `pkce_challenge_rejects_42`: 调用 `pkce_challenge(42)` 必须 panic/throw RangeError。
   - `sha256_known_vector`: `sha256("abc") == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"`。
   - `verify_id_token_rs256_ok`: 用自签名 RSA JWK 生成 JWT,校验返回相同 `sub`。
   - `verify_id_token_expired_rejected`: exp 为 1 的 JWT 必须返回 `ExpiredError`。
   - `verify_id_token_bad_signature_rejected`: 改 JWT payload 一个字符后必须返回 `SignatureError`。

2. **TS loader tests** (`packages/ody-crypto/test/loader.test.ts`):
   - `returns native when .node loads`: 模拟成功 require,断言返回对象四个函数均存在。
   - `falls back to ts on require failure`: 模拟 require throw,断言返回 fallback,且 `pkceChallenge` 产生合法 verifier/challenge。

3. **MCP OAuth integration tests** (`packages/mcp-host/test/oauth/service.test.ts`):
   - `authorization url contains s256 challenge`: mock server metadata,调用 `beginAuthorization`,断言返回 URL 的 `code_challenge_method=S256` 且 `code_challenge` 为 base64url。
   - `state matches callback`: 完成完整 flow,断言 callback 的 `state` 与 provider `expectedState()` 一致。
   - `id token verified when present`: mock token endpoint 返回含 `id_token`,断言 `verifyIdToken` 被调用且成功后保存 tokens。

4. **SEA smoke test** (CI):
   - 构建当前 target 的 SEA 后运行 `ody-crypto-smoke.mjs`,断言 `ody-crypto.randomBytes(16).length == 16` 且 `pkceChallenge().codeVerifier.length == 43`。

**Done criteria**:
- `cargo test -p ody-crypto` passes
- `pnpm --filter @odysseythink/ody-crypto test` passes
- `pnpm --filter @modelcontextprotocol/mcp-host test` passes(或至少 oauth 相关测试)
- `pnpm build:native:crypto` 为当前平台产出 `.node`
- SEA smoke test passes on CI for darwin-arm64/x64、linux-x64、win32-x64

## Risk Register

| 编号 | 风险 | 可能性 | 影响 | 缓解 |
|---|---|---|---|---|
| R-E1 构建矩阵爆炸 | 中 | 高 | 新增 5 个平台子包发布 + CI runner 成本 | 与现有 `SUPPORTED_TARGETS` 对齐;先不加入 win32-arm64/linux-musl;平台子包用 optionalDependencies |
| R-E2 napi-rs 与 Node 24 不兼容 | 低 | 高 | SEA 启动即崩溃 | 用 napi-rs v3(LTS),CI 跑 smoke test;Node 版本锁 >=24.15 |
| R-E3 Native 降级路径未覆盖 | 低 | 高 | 某平台 .node 损坏导致 OAuth 完全不可用 | 每个 Native 函数都有 TS fallback;CI 必须测试 fallback 分支 |
| R-E4 JWT 校验引入新依赖/攻击面 | 中 | 中 | Rust `jsonwebtoken` crate 漏洞或算法误用 | 固定算法白名单(RS256/ES256),禁用 none;依赖用 cargo-audit 扫描 |
| R-E5 SDK 升级破坏 exchangeAuthorization 接口 | 低 | 中 | token 交换失败 | 锁定 SDK 版本;升级时单独 PR 并跑 oauth 全量测试 |
| R-E6 平台子包体积过大 | 中 | 中 | SEA 单二进制变大 | release profile 开 strip/LTO;体积写入报告,若超预期则回退或拆包 |
| R-E7 团队 Rust/napi-rs 经验不足 | 中 | 中 | 开发阻塞或质量差 | 复用 rust-ody 已有工具链;先小范围 PKCE,再扩展到 JWT |

## Assumptions & Unverified Items

| # | 假设 | 置信度 | 如果错误的影响 | 验证方式 |
|---|---|---|---|---|
| A1 | `@modelcontextprotocol/sdk` 导出的 `exchangeAuthorization` 足以替代 `auth()` 的 code exchange 分支,不需要调用内部 `fetchToken` 的其它逻辑。 | 高 | 需要重写更多 SDK 内部逻辑,增加改造面。 | 阅读 SDK 源码确认 `authInternal` 在 redirect 后调用 `startAuthorization`,exchange 阶段用 `fetchToken` → `exchangeAuthorization`;已验证 `exchangeAuthorization` 导出。 |
| A2 | `pkce-challenge` 默认 verifier length = 43,alphabet 为 URL-safe 66 字符;Native 实现与它逐字节兼容即可通过 golden 测试。 | 高 | 与现有授权服务器不兼容,授权失败。 | 阅读 `pkce-challenge` dist 源码(已完成),并在测试中对比 challenge。 |
| A3 | napi-rs v3 能在 Node 24.15 SEA 中加载 `.node` 并正确读取 `node:sea` 解压后的路径。 | 中 | SEA 启动时 Native 加载失败,全部走 fallback,报告数据失真。 | CI smoke test:SEA 构建后调用 `ody-crypto.randomBytes()`。 |
| A4 | Rust `jsonwebtoken` crate 与 npm `jsonwebtoken` 均支持用 JWK(JSON string) 直接验证 RS256/ES256,无需额外 PEM 转换。 | 高 | TS fallback 需要先把 JWK 转 PEM,增加边界复杂度和失败点;Rust 侧也无法直接用 JWK 则需重写加载逻辑。 | 已验证:Rust 侧 `DecodingKey::from_jwk` 与 npm `createPublicKey({ key: jwk, format: 'jwk' })` 均可用。 |
| A5 | 平台子包通过 `optionalDependencies` 引用,缺失时不会破坏 `pnpm install --frozen-lockfile`。 | 高 | 开发/CI 安装失败。 | 已验证:host 包 `optionalDependencies` 指向 workspace 子包,非目标平台子包未安装时 install 不中断。 |
| A6 | 新增 `packages/ody-crypto` 与五个平台子包不需要变更 `pnpm-workspace.yaml` 与 `flake.nix` 之外的内容即可被 native asset 收集器识别。 | 中 | 构建产物无法进入 SEA。 | 改造 `native-deps.mjs` 后跑 `collectNativeAssets` 单测/SEA 构建。 |

## Self-Review

**最昂贵的三个决策及 adversarial 用例** [C:INFERRED]:

1. **Native loader 的平台映射决定某平台是走 Native 还是 TS fallback。**
   - 用例 A:`process.platform='darwin', process.arch='arm64'` → 目标 `darwin-arm64`,平台子包存在,走 Native。
   - 用例 B:`process.platform='win32', process.arch='arm64'` → 目标 `win32-arm64`,在 `SUPPORTED_TARGETS` 中但平台子包未发布,`require` 失败,走 TS fallback。
   - 用例 C:`process.platform='linux', process.arch='ia32'` → 目标 `linux-ia32`,不在 `SUPPORTED_TARGETS`,直接返回 null 走 TS fallback。
   - 已验证 `${process.platform}-${process.arch}` 与 `SUPPORTED_TARGETS` 的 6 项映射。

2. **JWT 校验的算法白名单决定安全性基线。**
   - 用例 A:`alg=RS256, kty=RSA` → 验证通过(合法 id_token)。
   - 用例 B:`alg=none` → 明确拒绝,防止 algorithm stripping。
   - 用例 C:`alg=HS256, kty=oct` → 明确拒绝(本阶段不支持对称密钥)。

3. **`verifyIdToken` 的 TS fallback 依赖 `jsonwebtoken` 库支持 JWK RS256/ES256。**
   - 用例 A:合法 RSA JWK + RS256 JWT → `jsonwebtoken.verify` 应返回 claims。
   - 用例 B:过期 JWT → `jsonwebtoken.verify` 应抛出 `TokenExpiredError`。
   - 用例 C:签名被篡改 → `jsonwebtoken.verify` 应抛出 `JsonWebTokenError`。
   - 已验证 `jsonwebtoken` 已作为 `packages/ody-crypto` 的 dependency 安装。

**四透镜扫描**:

- **Security**:检查了 `verifyIdToken` 的 alg 白名单、aud 处理(支持字符串与数组)、exp 校验、敏感值不打印。未发现 secrets 泄漏到日志;修正:明确拒绝 `alg=none` 与 HS256。
- **Test**:每个行为都有 must-pass/must-reject 断言;测试表已列出。修正:明确 `win32-arm64` 平台子包未发布,loader 会 fallback;实际单元测试覆盖 loader mock 的 require 失败路径。
- **Ops**:Native `.node` 有 per-platform 构建矩阵;加载失败自动降级;SEA 通过 manifest 校验 .node 完整性。风险:若某平台 .node 未发布,用户自动用 TS fallback,功能不中断但报告数据缺失。
- **Integration**:已验证 `exchangeAuthorization` 在 SDK 中导出;`provider.state()`/`saveCodeVerifier`/`redirectToAuthorization` 在 `provider.ts` 中存在;native asset 收集器 `native-deps.mjs` 支持新增 native 包;`CallbackServer` 可复用。未发现设计依赖不存在的 hook。
- **Scope**:本设计仍聚焦 Phase 2-E(首个 Native 模块),不跨越到 H/W/B 等其它轨道;JWT 校验限定为 RS256/ES256 signature + 标准 claims,不扩展为完整 OIDC/OAuth2 库。

## User Final Approval

- 设计文档状态:**已批准**(待 ExitDesignMode 最终确认)
- 审计级别:Deep
- 关键论断:已确认接受 Phase 2-E 用 napi-rs 实现首个 Native 模块 `ody-crypto`,承载 MCP OAuth PKCE + JWT 校验,默认启用并自动降级到 TS,核心交付物为 SEA 构建矩阵成本报告。
- 假设签署:
  - A1 接受
  - A2 接受
  - A3 接受
  - A4 接受
  - A5 接受
  - A6 接受
- 批准方式:通过 ExitDesignMode