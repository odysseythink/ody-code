//! PoC: a single hot-path function (token estimation) reimplemented in Rust,
//! compiled to `wasm32-unknown-unknown` with NO wasm-bindgen.
//!
//! The ABI is deliberately raw so the JS<->Wasm boundary cost is explicit and
//! measurable: the caller allocates a buffer via `alloc`, copies UTF-8 bytes
//! into linear memory, calls `estimate_tokens(ptr, len)`, then frees via
//! `dealloc`. This is exactly the cost we want to benchmark against pure JS.

use std::alloc::{alloc as sys_alloc, dealloc as sys_dealloc, Layout};

/// Allocate `len` bytes in wasm linear memory and return a pointer the JS side
/// can write UTF-8 into. Memory is leaked here and reclaimed by `dealloc`.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    // 1-byte alignment is fine for a UTF-8 byte buffer.
    let layout = Layout::from_size_align(len, 1).unwrap();
    unsafe { sys_alloc(layout) }
}

/// Free a buffer previously returned by `alloc`.
#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let layout = Layout::from_size_align(len, 1).unwrap();
    unsafe { sys_dealloc(ptr, layout) }
}

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
    // Invalid UTF-8 is treated leniently: lossy decode keeps the PoC robust
    // without diverging from JS, which also never throws here.
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
