//! Shared raw ABI helpers for Wasm exports.
//!
//! Convention:
//!   - `alloc(len)` returns a pointer to `len` zero-initialized bytes.
//!   - `dealloc(ptr, len)` frees a buffer previously returned by `alloc`.
//!   - Functions returning strings use `alloc_cstring`, which allocates `len+1`
//!     bytes and writes a NUL terminator. The JS side reads until NUL and then
//!     calls `dealloc(ptr, decoded_len + 1)`.

use std::alloc::{alloc as sys_alloc, dealloc as sys_dealloc, Layout};

/// Error codes returned to JS across the Wasm boundary.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmError {
    Ok = 0,
    UnknownEncoding = 1,
    InvalidUtf8 = 2,
    AllocFailed = 3,
}

/// Allocate `len` bytes in Wasm linear memory.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
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

/// Decode UTF-8 bytes from Wasm linear memory.
///
/// # Safety
/// `ptr` must point to `len` readable bytes or be null when `len == 0`.
pub unsafe fn decode_utf8(ptr: *const u8, len: usize) -> Result<String, WasmError> {
    if ptr.is_null() || len == 0 {
        return Ok(String::new());
    }
    let bytes = std::slice::from_raw_parts(ptr, len);
    String::from_utf8(bytes.to_vec()).map_err(|_| WasmError::InvalidUtf8)
}

/// Allocate a NUL-terminated buffer containing `s` as UTF-8.
/// Returns null for empty strings; the JS side treats null as "".
pub fn alloc_cstring(s: &str) -> *mut u8 {
    if s.is_empty() {
        return std::ptr::null_mut();
    }
    let bytes = s.as_bytes();
    let len = bytes.len();
    let ptr = alloc(len + 1);
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, len);
        ptr.add(len).write(0);
    }
    ptr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_dealloc_roundtrip() {
        let ptr = alloc(5);
        assert!(!ptr.is_null());
        unsafe { std::ptr::write_bytes(ptr, 0xAB, 5); }
        dealloc(ptr, 5);
    }

    #[test]
    fn decode_utf8_empty_and_null() {
        assert_eq!(unsafe { decode_utf8(std::ptr::null(), 0) }.unwrap(), "");
    }

    #[test]
    fn alloc_cstring_roundtrip() {
        let s = "hello 世界";
        let ptr = alloc_cstring(s);
        assert!(!ptr.is_null());
        unsafe {
            let bytes = std::slice::from_raw_parts(ptr, s.len() + 1);
            assert_eq!(&bytes[..s.len()], s.as_bytes());
            assert_eq!(bytes[s.len()], 0);
            dealloc(ptr, s.len() + 1);
        }
    }

    #[test]
    fn alloc_cstring_empty_returns_null() {
        assert!(alloc_cstring("").is_null());
    }
}
