#![deny(clippy::all)]

pub mod crypto;
pub mod jwt;
pub mod pkce;

use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::collections::HashMap;

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
        .map(|c| PkceChallenge {
            code_verifier: c.code_verifier,
            code_challenge: c.code_challenge,
        })
        .map_err(|e| Error::new(Status::GenericFailure, e))
}

#[napi]
pub fn verify_id_token(
    jwt: String,
    jwk_json: String,
    expected: IdTokenExpected,
) -> Result<IdTokenClaims> {
    jwt::verify_id_token(
        &jwt,
        &jwk_json,
        &jwt::IdTokenExpected {
            issuer: expected.issuer.clone(),
            audience: expected.audience.clone(),
            max_age_seconds: expected.max_age_seconds,
        },
    )
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
