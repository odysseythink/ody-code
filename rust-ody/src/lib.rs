//! PoC: hot-path functions in Rust compiled to `wasm32-unknown-unknown` with NO wasm-bindgen.

pub mod abi;
pub use abi::*;

/// Estimate token count from UTF-8 text in `[ptr, ptr+len)`.
///
/// Mirrors the TS heuristic exactly:
///   - code points <= 127 (ASCII) counted ~4 chars/token
///   - all other code points counted ~1 char/token
///   - result = ceil(ascii / 4) + non_ascii
#[no_mangle]
pub extern "C" fn estimate_tokens(ptr: *const u8, len: usize) -> u32 {
    if ptr.is_null() || len == 0 {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return estimate_lossy(bytes),
    };
    estimate_str(text)
}

fn estimate_str(text: &str) -> u32 {
    let mut ascii: u32 = 0;
    let mut non_ascii: u32 = 0;
    for ch in text.chars() {
        if (ch as u32) <= 127 {
            ascii += 1;
        } else {
            non_ascii += 1;
        }
    }
    ceil_div4(ascii) + non_ascii
}

fn estimate_lossy(bytes: &[u8]) -> u32 {
    let text = String::from_utf8_lossy(bytes);
    estimate_str(&text)
}

#[inline]
fn ceil_div4(n: u32) -> u32 {
    // ceil(n / 4) for non-negative n.
    (n + 3) / 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_ts_heuristic() {
        assert_eq!(estimate_str(""), 0);
        assert_eq!(estimate_str("a"), 1); // ceil(1/4)=1
        assert_eq!(estimate_str("abcd"), 1); // ceil(4/4)=1
        assert_eq!(estimate_str("abcde"), 2); // ceil(5/4)=2
        assert_eq!(estimate_str("你好"), 2); // 2 non-ascii
        assert_eq!(estimate_str("ab你"), 2); // ceil(2/4)=1 + 1
    }
}
