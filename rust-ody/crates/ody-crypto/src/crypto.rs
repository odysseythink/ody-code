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
        assert_ne!(a.as_slice(), b.as_slice());
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
