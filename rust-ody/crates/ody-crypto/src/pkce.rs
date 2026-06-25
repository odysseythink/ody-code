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
