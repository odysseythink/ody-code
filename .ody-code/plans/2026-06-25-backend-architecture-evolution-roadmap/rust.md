# Part 1: Rust crate 与 napi 绑定

本 Part 完成 `rust-ody/crates/ody-crypto`，提供 `randomBytes`、`sha256`、`pkceChallenge`、`verifyIdToken` 的 Rust 实现与 napi-rs 导出。

## 依赖关系

```text
Task 1 -> Task 2 -> Task 3 -> Task 4 -> Task 5
```

---

### Task 1: 将 rust-ody 改造为 Cargo workspace 并创建 ody-crypto crate 骨架

**Depends on:** none

**Files:**
- Create: `rust-ody/Cargo.toml`
- Create: `rust-ody/crates/ody-rust/Cargo.toml`
- Create: `rust-ody/crates/ody-crypto/Cargo.toml`
- Create: `rust-ody/crates/ody-crypto/build.rs`
- Create: `rust-ody/crates/ody-crypto/src/lib.rs`
- Create: `rust-ody/build-crypto.sh`
- Modify: `rust-ody/build.sh`
- Move: `rust-ody/src/*` -> `rust-ody/crates/ody-rust/src/`

**步骤：**

- [ ] 在 `rust-ody/` 下用 `git mv src crates/ody-rust/src`，把现有 Wasm PoC 移入子 crate。
- [ ] 覆盖 `rust-ody/Cargo.toml` 为 workspace 定义：

```toml
[workspace]
members = ["crates/ody-rust", "crates/ody-crypto"]
resolver = "2"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

- [ ] 新建 `rust-ody/crates/ody-rust/Cargo.toml`，保留原配置：

```toml
[package]
name = "ody-rust"
version = "0.1.0"
edition = "2021"
description = "PoC: hot-path logic in Rust compiled to Wasm for ody-code"
license = "MIT"

[lib]
crate-type = ["cdylib"]

[dependencies]
similar = "2.6.0"
globset = "0.4.14"
```

- [ ] 新建 `rust-ody/crates/ody-crypto/Cargo.toml`：

```toml
[package]
name = "ody-crypto"
version = "0.1.0"
edition = "2021"
description = "Native crypto helpers for ody-code via napi-rs"
license = "MIT"

[lib]
crate-type = ["cdylib"]

[dependencies]
napi = "3"
napi-derive = "3"
rand = "0.8"
sha2 = "0.10"
base64 = "0.22"
jsonwebtoken = "9"
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
rsa = "0.9"

[build-dependencies]
napi-build = "3"
```

- [ ] 新建 `rust-ody/crates/ody-crypto/build.rs`：

```rust
fn main() {
    napi_build::setup();
}
```

- [ ] 新建 `rust-ody/crates/ody-crypto/src/lib.rs`，只声明后续要实现的模块：

```rust
pub mod crypto;
pub mod jwt;
pub mod pkce;
```

- [ ] 修改 `rust-ody/build.sh`：把 Wasm 构建限制在 `ody-rust`，避免 `ody-crypto` 被构建为 wasm32：

```bash
# 第 12 行
 cargo build --release --target wasm32-unknown-unknown
# 改为
 cargo build -p ody-rust --release --target wasm32-unknown-unknown
# 第 14 行
 ls -la target/wasm32-unknown-unknown/release/ody_rust.wasm
# 改为
 ls -la target/wasm32-unknown-unknown/release/ody_rust.wasm
```

- [ ] 新建 `rust-ody/build-crypto.sh`，用于本地当前平台构建 `.node`：

```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

TARGET="${TARGET:-$(rustc -vV | awk '/host:/ {print $2}')}"

case "$TARGET" in
  x86_64-apple-darwin)   NPM_TARGET=darwin-x64 ;;
  aarch64-apple-darwin)  NPM_TARGET=darwin-arm64 ;;
  x86_64-unknown-linux-gnu) NPM_TARGET=linux-x64 ;;
  aarch64-unknown-linux-gnu) NPM_TARGET=linux-arm64 ;;
  x86_64-pc-windows-msvc) NPM_TARGET=win32-x64 ;;
  *) echo "unsupported rust target $TARGET"; exit 1 ;;
esac

cargo build -p ody-crypto --release --target "$TARGET"

LIB_BASENAME="target/$TARGET/release/libody_crypto"
if [[ "$TARGET" == *windows* ]]; then
  LIB_PATH="${LIB_BASENAME}.dll"
elif [[ "$TARGET" == *apple* ]]; then
  LIB_PATH="${LIB_BASENAME}.dylib"
else
  LIB_PATH="${LIB_BASENAME}.so"
fi

DEST="packages/ody-crypto-${NPM_TARGET}/ody-crypto.node"
mkdir -p "$(dirname "$DEST")"
cp "$LIB_PATH" "$DEST"
echo "==> produced $DEST"
ls -lh "$DEST"
```

- [ ] 运行验证：

```bash
cd rust-ody
cargo test -p ody-rust --quiet
chmod +x build-crypto.sh
TARGET=$(rustc -vV | awk '/host:/ {print $2}') ./build-crypto.sh
```

- [ ] 预期：`cargo test -p ody-rust` 通过；`build-crypto.sh` 在当前平台生成 `packages/ody-crypto-<target>/ody-crypto.node`。
- [ ] Commit：`git add rust-ody/ && git commit -m "chore(rust): workspace + ody-crypto skeleton"`。

---

### Task 2: 实现 randomBytes 与 sha256

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/ody-crypto/src/crypto.rs`
- Modify: `rust-ody/crates/ody-crypto/src/lib.rs`

**步骤：**

- [ ] 在 `rust-ody/crates/ody-crypto/src/crypto.rs` 中先写测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_bytes_returns_requested_length() {
        let buf = random_bytes(16);
        assert_eq!(buf.len(), 16);
    }

    #[test]
    fn random_bytes_produces_different_values() {
        let a = random_bytes(16);
        let b = random_bytes(16);
        assert_ne!(a.as_ref(), b.as_ref());
    }

    #[test]
    fn sha256_known_vector() {
        assert_eq!(
            sha256_impl("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_bytes_match_string() {
        assert_eq!(sha256_impl("abc"), sha256_impl("abc".as_bytes()));
    }
}
```

- [ ] 运行测试并确认失败：

```bash
cargo test -p ody-crypto --lib
```

预期输出包含 `error: cannot find function random_bytes` / `sha256_impl` 等编译失败。

- [ ] 实现函数：

```rust
use rand::Rng;
use sha2::{Digest, Sha256};

const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

pub fn random_bytes(length: u32) -> Vec<u8> {
    let mut buf = vec![0u8; length as usize];
    rand::thread_rng().fill(&mut buf[..]);
    buf
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX_CHARS[(b >> 4) as usize] as char);
        out.push(HEX_CHARS[(b & 0xf) as usize] as char);
    }
    out
}

pub fn sha256_impl<T: AsRef<[u8]>>(input: T) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_ref());
    bytes_to_hex(&hasher.finalize())
}

pub fn sha256_string(input: &str) -> String {
    sha256_impl(input)
}

pub fn sha256_bytes(input: &[u8]) -> String {
    sha256_impl(input)
}
```

- [ ] 再次运行测试并通过：

```bash
cargo test -p ody-crypto --lib
```

- [ ] Commit：`git add rust-ody/crates/ody-crypto/src/crypto.rs rust-ody/crates/ody-crypto/src/lib.rs && git commit -m "feat(rust/ody-crypto): randomBytes and sha256"`。

---

### Task 3: 实现 pkceChallenge

**Depends on:** Task 2

**Files:**
- Create: `rust-ody/crates/ody-crypto/src/pkce.rs`
- Modify: `rust-ody/crates/ody-crypto/src/lib.rs`

**步骤：**

- [ ] 先写测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_default_length_is_43() {
        let result = pkce_challenge(None).unwrap();
        assert_eq!(result.code_verifier.len(), 43);
        assert!(!result.code_challenge.is_empty());
    }

    #[test]
    fn pkce_challenge_s256_matches() {
        let result = pkce_challenge(None).unwrap();
        let expected = {
            use sha2::{Digest, Sha256};
            use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
            let mut hasher = Sha256::new();
            hasher.update(result.code_verifier.as_bytes());
            URL_SAFE_NO_PAD.encode(&hasher.finalize())
        };
        assert_eq!(result.code_challenge, expected);
    }

    #[test]
    fn pkce_challenge_custom_length_128() {
        let result = pkce_challenge(Some(128)).unwrap();
        assert_eq!(result.code_verifier.len(), 128);
    }

    #[test]
    fn pkce_challenge_rejects_42() {
        assert!(pkce_challenge(Some(42)).is_err());
    }

    #[test]
    fn pkce_challenge_rejects_129() {
        assert!(pkce_challenge(Some(129)).is_err());
    }
}
```

- [ ] 运行并确认失败：

```bash
cargo test -p ody-crypto --lib
```

- [ ] 实现：

```rust
use rand::Rng;
use sha2::{Digest, Sha256};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

const PKCE_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

pub struct PkceChallenge {
    pub code_verifier: String,
    pub code_challenge: String,
}

pub fn pkce_challenge(length: Option<u32>) -> Result<PkceChallenge, String> {
    let n = length.unwrap_or(43) as usize;
    if n < 43 || n > 128 {
        return Err(format!("PKCE verifier length {} out of range [43, 128]", n));
    }
    let mut rng = rand::thread_rng();
    let verifier: String = (0..n)
        .map(|_| PKCE_ALPHABET[rng.gen_range(0..PKCE_ALPHABET.len())] as char)
        .collect();

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(&hasher.finalize());

    Ok(PkceChallenge { code_verifier: verifier, code_challenge: challenge })
}
```

- [ ] 运行并通过：

```bash
cargo test -p ody-crypto --lib
```

- [ ] Commit：`git add rust-ody/crates/ody-crypto/src/pkce.rs rust-ody/crates/ody-crypto/src/lib.rs && git commit -m "feat(rust/ody-crypto): pkceChallenge"`。

---

### Task 4: 实现 verifyIdToken

**Depends on:** Task 3

**Files:**
- Create: `rust-ody/crates/ody-crypto/src/jwt.rs`
- Modify: `rust-ody/crates/ody-crypto/src/lib.rs`
- Modify: `rust-ody/crates/ody-crypto/Cargo.toml`（增加 `rsa` dev-dep 已在 Task 1 中声明，无需重复）

**步骤：**

- [ ] 先写测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use rsa::{RsaPrivateKey, pkcs8::EncodePrivateKey};
    use serde::Serialize;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Serialize)]
    struct TestClaims {
        sub: String,
        iss: String,
        aud: String,
        exp: i64,
        iat: i64,
    }

    fn now() -> i64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
    }

    fn rsa_jwk(private_key: &RsaPrivateKey) -> String {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        let n = URL_SAFE_NO_PAD.encode(private_key.n().to_bytes_be());
        let e = URL_SAFE_NO_PAD.encode(private_key.e().to_bytes_be());
        format!(r#"{{"kty":"RSA","n":"{}","e":"{}"}}"#, n, e)
    }

    #[test]
    fn verify_id_token_rs256_ok() {
        let mut rng = rand::thread_rng();
        let private_key = RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let der = private_key.to_pkcs8_der().unwrap();
        let encoding_key = EncodingKey::from_rsa_der(der.as_bytes()).unwrap();

        let now_ts = now();
        let claims = TestClaims {
            sub: "user-42".into(),
            iss: "https://issuer.example".into(),
            aud: "my-client-id".into(),
            exp: now_ts + 3600,
            iat: now_ts,
        };
        let jwt = encode(&Header::new(Algorithm::RS256), &claims, &encoding_key).unwrap();

        let expected = IdTokenExpected {
            issuer: "https://issuer.example".into(),
            audience: "my-client-id".into(),
            max_age_seconds: None,
        };
        let result = verify_id_token(&jwt, &rsa_jwk(&private_key), &expected).unwrap();
        assert_eq!(result.sub, "user-42");
        assert_eq!(result.iss, "https://issuer.example");
    }

    #[test]
    fn verify_id_token_expired_rejected() {
        let mut rng = rand::thread_rng();
        let private_key = RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let der = private_key.to_pkcs8_der().unwrap();
        let encoding_key = EncodingKey::from_rsa_der(der.as_bytes()).unwrap();

        let claims = TestClaims {
            sub: "user-42".into(),
            iss: "https://issuer.example".into(),
            aud: "my-client-id".into(),
            exp: 1,
            iat: 1,
        };
        let jwt = encode(&Header::new(Algorithm::RS256), &claims, &encoding_key).unwrap();
        let expected = IdTokenExpected {
            issuer: "https://issuer.example".into(),
            audience: "my-client-id".into(),
            max_age_seconds: None,
        };
        assert!(verify_id_token(&jwt, &rsa_jwk(&private_key), &expected).is_err());
    }

    #[test]
    fn verify_id_token_bad_signature_rejected() {
        let mut rng = rand::thread_rng();
        let private_key = RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let der = private_key.to_pkcs8_der().unwrap();
        let encoding_key = EncodingKey::from_rsa_der(der.as_bytes()).unwrap();

        let now_ts = now();
        let claims = TestClaims {
            sub: "user-42".into(),
            iss: "https://issuer.example".into(),
            aud: "my-client-id".into(),
            exp: now_ts + 3600,
            iat: now_ts,
        };
        let mut jwt = encode(&Header::new(Algorithm::RS256), &claims, &encoding_key).unwrap();
        // 修改 payload 中一个字符（第二个 segment 是 payload）
        let parts: Vec<&str> = jwt.split('.').collect();
        let tampered_payload = parts[1].replacen('u', 'v', 1);
        jwt = format!("{}.{}.{}" , parts[0], tampered_payload, parts[2]);

        let expected = IdTokenExpected {
            issuer: "https://issuer.example".into(),
            audience: "my-client-id".into(),
            max_age_seconds: None,
        };
        assert!(verify_id_token(&jwt, &rsa_jwk(&private_key), &expected).is_err());
    }
}
```

- [ ] 运行并确认失败：

```bash
cargo test -p ody-crypto --lib
```

- [ ] 实现 `rust-ody/crates/ody-crypto/src/jwt.rs`：

```rust
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{decode, Algorithm, DecodingKey, TokenData, Validation};
use serde::Deserialize;
use serde_json::Value;

pub struct IdTokenExpected {
    pub issuer: String,
    pub audience: String,
    pub max_age_seconds: Option<i64>,
}

pub struct IdTokenClaims {
    pub sub: String,
    pub iss: String,
    pub aud: StringOrStrings,
    pub exp: i64,
    pub iat: i64,
    pub extra: HashMap<String, String>,
}

pub enum StringOrStrings {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Deserialize)]
struct JwtHeader {
    alg: String,
}

#[derive(Deserialize)]
struct JwtPayload {
    sub: String,
    iss: String,
    aud: Value,
    exp: i64,
    iat: i64,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

fn b64url_decode(input: &str) -> Result<Vec<u8>, String> {
    URL_SAFE_NO_PAD.decode(input).map_err(|e| format!("base64url decode failed: {}", e))
}

fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn check_audience(aud: &Value, expected: &str) -> Result<(), String> {
    match aud {
        Value::String(s) if s == expected => Ok(()),
        Value::Array(arr) if arr.iter().any(|v| v.as_str() == Some(expected)) => Ok(()),
        _ => Err("audience mismatch".into()),
    }
}

pub fn verify_id_token(jwt: &str, jwk_json: &str, expected: &IdTokenExpected) -> Result<IdTokenClaims, String> {
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() != 3 {
        return Err("malformed JWT".into());
    }

    let header: JwtHeader = serde_json::from_slice(&b64url_decode(parts[0])?)
        .map_err(|e| format!("invalid JWT header: {}", e))?;

    let alg = match header.alg.as_str() {
        "RS256" => Algorithm::RS256,
        "ES256" => Algorithm::ES256,
        other => return Err(format!("unsupported or rejected algorithm: {}", other)),
    };

    let jwk: Value = serde_json::from_str(jwk_json)
        .map_err(|e| format!("invalid JWK JSON: {}", e))?;
    let kty = jwk.get("kty").and_then(|v| v.as_str()).unwrap_or("");
    match alg {
        Algorithm::RS256 if kty == "RSA" => {}
        Algorithm::ES256 if kty == "EC" => {}
        _ => return Err("JWK kty does not match JWT algorithm".into()),
    }

    let payload_raw = b64url_decode(parts[1])?;
    let payload: JwtPayload = serde_json::from_slice(&payload_raw)
        .map_err(|e| format!("invalid JWT payload: {}", e))?;

    if payload.iss != expected.issuer {
        return Err("issuer mismatch".into());
    }
    check_audience(&payload.aud, &expected.audience)?;

    let now = current_unix_seconds();
    if payload.exp <= now {
        return Err("token expired".into());
    }
    if let Some(max_age) = expected.max_age_seconds {
        if payload.iat + max_age <= now {
            return Err("token exceeds max age".into());
        }
    }

    let jwk_struct: jsonwebtoken::jwk::Jwk = serde_json::from_str(jwk_json)
        .map_err(|e| format!("invalid JWK: {}", e))?;
    let decoding_key = DecodingKey::from_jwk(&jwk_struct)
        .map_err(|e| format!("failed to load JWK: {}", e))?;
    let mut validation = Validation::new(alg);
    validation.validate_exp = false;
    validation.validate_nbf = false;
    validation.set_issuer(&[expected.issuer.as_str()]);
    validation.set_audience(&[expected.audience.as_str()]);

    #[derive(Deserialize)]
    struct Empty {}
    decode::<Empty>(jwt, &decoding_key, &validation)
        .map_err(|e| format!("signature verification failed: {}", e))?;

    let aud = match payload.aud {
        Value::String(s) => StringOrStrings::Single(s),
        Value::Array(arr) => StringOrStrings::Multiple(
            arr.into_iter().filter_map(|v| v.as_str().map(String::from)).collect()
        ),
        _ => StringOrStrings::Single(String::new()),
    };

    let mut extra = HashMap::new();
    for (k, v) in payload.extra {
        if matches!(k.as_str(), "sub" | "iss" | "aud" | "exp" | "iat") {
            continue;
        }
        let s = match &v {
            Value::String(s) => s.clone(),
            _ => serde_json::to_string(&v).unwrap_or_default(),
        };
        extra.insert(k, s);
    }

    Ok(IdTokenClaims {
        sub: payload.sub,
        iss: payload.iss,
        aud,
        exp: payload.exp,
        iat: payload.iat,
        extra,
    })
}
```

- [ ] 运行并通过：

```bash
cargo test -p ody-crypto --lib
```

- [ ] Commit：`git add rust-ody/crates/ody-crypto/src/jwt.rs rust-ody/crates/ody-crypto/src/lib.rs && git commit -m "feat(rust/ody-crypto): verifyIdToken"`。

---

### Task 5: 添加 napi-rs 导出层并构建当前平台 .node

**Depends on:** Task 4

**Files:**
- Modify: `rust-ody/crates/ody-crypto/src/lib.rs`
- Modify: `rust-ody/build-crypto.sh`

**步骤：**

- [ ] 将 `rust-ody/crates/ody-crypto/src/lib.rs` 替换为 napi 导出层（实际算法仍调用各模块）：

```rust
#![deny(clippy::all)]

pub mod crypto;
pub mod jwt;
pub mod pkce;

use std::collections::HashMap;
use napi::bindgen_prelude::*;

#[napi(object)]
pub struct PkceChallenge {
    pub code_verifier: String,
    pub code_challenge: String,
}

#[napi(object)]
pub struct IdTokenExpected {
    pub issuer: String,
    pub audience: String,
    pub max_age_seconds: Option<i64>,
}

#[napi(object)]
pub struct IdTokenClaims {
    pub sub: String,
    pub iss: String,
    pub aud: Either<String, Vec<String>>,
    pub exp: i64,
    pub iat: i64,
    pub extra: HashMap<String, String>,
}

#[napi]
pub fn random_bytes(length: u32) -> Buffer {
    crypto::random_bytes(length).into()
}

#[napi]
pub fn sha256(input: Either<String, Buffer>) -> String {
    match input {
        Either::A(s) => crypto::sha256_impl(s.as_str()),
        Either::B(b) => crypto::sha256_impl(b.as_ref()),
    }
}

#[napi]
pub fn pkce_challenge(length: Option<u32>) -> Result<PkceChallenge> {
    pkce::pkce_challenge(length)
        .map(|c| PkceChallenge { code_verifier: c.code_verifier, code_challenge: c.code_challenge })
        .map_err(|e| Error::new(Status::GenericFailure, e))
}

#[napi]
pub fn verify_id_token(jwt: String, jwk_json: String, expected: &IdTokenExpected) -> Result<IdTokenClaims> {
    jwt::verify_id_token(&jwt, &jwk_json, &jwt::IdTokenExpected {
        issuer: expected.issuer.clone(),
        audience: expected.audience.clone(),
        max_age_seconds: expected.max_age_seconds,
    })
    .map(|c| IdTokenClaims {
        sub: c.sub,
        iss: c.iss,
        aud: match c.aud {
            jwt::StringOrStrings::Single(s) => Either::A(s),
            jwt::StringOrStrings::Multiple(v) => Either::B(v),
        },
        exp: c.exp,
        iat: c.iat,
        extra: c.extra,
    })
    .map_err(|e| Error::new(Status::GenericFailure, e))
}
```

- [ ] 编译验证：

```bash
cargo test -p ody-crypto
```

- [ ] 运行构建脚本产出当前平台 `.node`：

```bash
cd rust-ody
chmod +x build-crypto.sh
./build-crypto.sh
```

- [ ] 预期：在 `packages/ody-crypto-<current-target>/ody-crypto.node` 生成一个非空 `.node` 文件（体积通常 1–3 MB）。
- [ ] 手动验证 native 符号加载可用：

```bash
node -e "const addon = require('./packages/ody-crypto-$(node -e 'console.log(process.platform+"-"+process.arch)')/ody-crypto.node'); console.log(addon.sha256('abc'))"
```

预期输出：`ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad`。

- [ ] Commit：`git add rust-ody/crates/ody-crypto/src/lib.rs rust-ody/build-crypto.sh packages/ody-crypto-*/ody-crypto.node && git commit -m "feat(rust/ody-crypto): napi bindings and native build"`。

---

## Local Self-Review

- [ ] 1. Spec-coverage table：本 Part 覆盖设计文档中 Rust crate 全部 4 个函数、算法、RS256 JWT 校验、PKCE 边界、构建脚本。
- [ ] 2. Placeholder scan：所有步骤均给出真实代码/命令，无 `TODO`/`TBD`。
- [ ] 3. No phantom tasks：Task 1 迁移 workspace，Task 5 产出 `.node`，均有可验证产物。
- [ ] 4. Dependency soundness：Task N 仅依赖 Task N-1 已定义的模块/类型。
- [ ] 5. Caller & build soundness：本 Part 不涉及既有 TS/Rust caller 签名变更；Task 1 修改 `build.sh` 限制为 `-p ody-rust`，需运行 `cargo test -p ody-rust` 验证原有 Wasm PoC 未坏。
- [ ] 6. Test-the-risk：PKCE 长度边界、JWT 过期/篡改、随机字节长度/唯一性均有 must-reject/must-pass 断言。
- [ ] 7. Type consistency：napi 导出对象字段名（`code_verifier`, `code_challenge`, `issuer`, `audience`, `max_age_seconds`, `sub`, `iss`, `aud`, `exp`, `iat`, `extra`）与 Part 2 TS 类型一致。
