# Part 1: Rust/Wasm Foundation

Scope: 在 `rust-ody` crate 中新增 Phase 1-A 所需的 Rust 依赖，并建立所有 Wasm 导出函数共享的 raw ABI 内存与字符串工具。

---

### Task 1: 添加 Rust 依赖

**Depends on:** none

**Files:**
- Modify: `rust-ody/Cargo.toml:1-17`

**Goal:** 引入 `tiktoken-rs`、`similar`、`globset` 三个 crate，使 Phase 1-A 的三个计算热点都能在 Rust 侧实现。

**Step-by-step:**

- [ ] 在 `rust-ody/Cargo.toml` 的 `[dependencies]` 段添加：

```toml
[package]
name = "ody-rust"
version = "0.1.0"
edition = "2021"
description = "PoC: hot-path logic in Rust compiled to Wasm for ody-code"
license = "MIT"

[lib]
# cdylib -> produces a standalone .wasm with no wasm-bindgen glue.
crate-type = ["cdylib"]

[dependencies]
tiktoken-rs = "0.5.5"
similar = "2.6.0"
globset = "0.4.14"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

- [ ] 运行依赖解析与 native 检查：

```bash
cd rust-ody && cargo check --quiet
```

- [ ] **Manual verification:** 命令成功退出（exit code 0），没有 unresolved import 或版本冲突报错。

- [ ] Commit: `chore(rust-ody): add tiktoken-rs, similar, globset dependencies`

---

### Task 2: 共享 raw ABI 与字符串辅助函数

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/src/abi.rs:1-120`
- Modify: `rust-ody/src/lib.rs:1-91`

**Goal:** 把 `alloc`/`dealloc` 与字符串编解码逻辑抽到共享模块，后续 `count_tokens`、`compute_diff`、`glob_match` 统一使用同一套 ABI 约定。

**Step-by-step:**

- [ ] 先写失败的单元测试。创建 `rust-ody/src/abi.rs` 并写入测试骨架（此时 `alloc_cstring`/`decode_utf8` 还未实现，编译失败）：

```rust
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
```

- [ ] 运行测试并确认**失败**：

```bash
cd rust-ody && cargo test --quiet
```

Expected failure: `error[E0425]: cannot find function 'alloc_cstring' in this scope` 或类似编译错误。

- [ ] 实现 `rust-ody/src/abi.rs`：

```rust
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
```

- [ ] 修改 `rust-ody/src/lib.rs`，保留 PoC 的 `estimate_tokens` 并暴露共享 ABI：

```rust
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
```

- [ ] 运行 native 单元测试并确认**通过**：

```bash
cd rust-ody && cargo test --quiet
```

Expected output:

```text	running 4 tests
test abi::tests::alloc_cstring_empty_returns_null ... ok
test abi::tests::alloc_cstring_roundtrip ... ok
test abi::tests::alloc_dealloc_roundtrip ... ok
test abi::tests::decode_utf8_empty_and_null ... ok

running 1 test
test tests::matches_ts_heuristic ... ok
```

- [ ] 运行 Wasm release 构建并确认体积：

```bash
cd rust-ody && cargo build --release --target wasm32-unknown-unknown
ls -lh target/wasm32-unknown-unknown/release/ody_rust.wasm
```

- [ ] **Manual verification:** 构建成功退出；记录 `.wasm` 大小。若此时（尚未加入 tokenizer rank 数据）大小已 >2MB，立即暂停并回到设计重新审视 embedding 策略。

- [ ] Commit: `feat(rust-ody): shared raw ABI helpers for wasm exports`

---

## Local Self-Review

- [ ] 1. Spec-coverage: Part 1 覆盖 "统一 raw ABI"、"string 分配/释放约定"、"依赖引入" 与 "体积初检"。
- [ ] 2. Placeholder scan: `abi.rs` 中无 TODO/TBD；所有函数给出完整实现与 unsafe 契约注释。
- [ ] 3. No phantom tasks: Task 1 修改 Cargo.toml 并验证解析；Task 2 创建 `abi.rs` 与测试，并通过 native + Wasm 构建。
- [ ] 4. Dependency soundness: Task 2 仅依赖 Task 1 添加的 crate；Task 1 无前置依赖。
- [ ] 5. Caller & build soundness: `lib.rs` 仍导出 `estimate_tokens` 且测试通过；Wasm 构建成功。本阶段未变更任何 TS 签名。
- [ ] 6. Test-the-risk: 测试覆盖空字符串/null 指针、非空字符串的 NUL 终止、分配/释放配对；这些都是 ABI 边界最容易出错的点。
- [ ] 7. Type consistency: `WasmError` 枚举与 `alloc_cstring` 签名在后续 Part 3/4/5 中复用，命名已固定。
