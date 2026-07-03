use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

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
    URL_SAFE_NO_PAD
        .decode(input)
        .map_err(|e| format!("base64url decode failed: {}", e))
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

pub fn verify_id_token(
    jwt: &str,
    jwk_json: &str,
    expected: &IdTokenExpected,
) -> Result<IdTokenClaims, String> {
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

    let jwk: Value =
        serde_json::from_str(jwk_json).map_err(|e| format!("invalid JWK JSON: {}", e))?;
    let kty = jwk.get("kty").and_then(|v| v.as_str()).unwrap_or("");
    match alg {
        Algorithm::RS256 if kty == "RSA" => {}
        Algorithm::ES256 if kty == "EC" => {}
        _ => return Err("JWK kty does not match JWT algorithm".into()),
    }

    let payload_raw = b64url_decode(parts[1])?;
    let payload: JwtPayload =
        serde_json::from_slice(&payload_raw).map_err(|e| format!("invalid JWT payload: {}", e))?;

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

    let jwk_struct: jsonwebtoken::jwk::Jwk =
        serde_json::from_str(jwk_json).map_err(|e| format!("invalid JWK: {}", e))?;
    let decoding_key =
        DecodingKey::from_jwk(&jwk_struct).map_err(|e| format!("failed to load JWK: {}", e))?;
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
            arr.into_iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use rsa::traits::PublicKeyParts;
    use rsa::{pkcs8::EncodePrivateKey, pkcs8::LineEnding, RsaPrivateKey};
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
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    fn rsa_jwk(private_key: &RsaPrivateKey) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        let n = URL_SAFE_NO_PAD.encode(private_key.n().to_bytes_be());
        let e = URL_SAFE_NO_PAD.encode(private_key.e().to_bytes_be());
        format!(r#"{{"kty":"RSA","n":"{}","e":"{}"}}"#, n, e)
    }

    #[test]
    fn verify_id_token_rs256_ok() {
        let mut rng = rand::thread_rng();
        let private_key = RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let pem = private_key.to_pkcs8_pem(LineEnding::LF).unwrap();
        let encoding_key = EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap();

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
        let pem = private_key.to_pkcs8_pem(LineEnding::LF).unwrap();
        let encoding_key = EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap();

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
        let pem = private_key.to_pkcs8_pem(LineEnding::LF).unwrap();
        let encoding_key = EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap();

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
        let tampered_payload = parts[1].replacen('u', "v", 1);
        jwt = format!("{}.{}.{}", parts[0], tampered_payload, parts[2]);

        let expected = IdTokenExpected {
            issuer: "https://issuer.example".into(),
            audience: "my-client-id".into(),
            max_age_seconds: None,
        };
        assert!(verify_id_token(&jwt, &rsa_jwk(&private_key), &expected).is_err());
    }
}
