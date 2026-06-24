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

// ---------------------------------------------------------------------------
// Diff: compute unified diff + format git diff via similar
// ---------------------------------------------------------------------------

use similar::TextDiff;

/// Compute a unified diff between two UTF-8 texts.
/// Returns a NUL-terminated string pointer (caller must dealloc with decoded_len + 1).
#[no_mangle]
pub extern "C" fn compute_diff(
    old_ptr: *const u8,
    old_len: usize,
    new_ptr: *const u8,
    new_len: usize,
) -> *mut u8 {
    let old_text = match unsafe { decode_utf8(old_ptr, old_len) } {
        Ok(s) => s,
        Err(_) => return alloc_cstring(""),
    };
    let new_text = match unsafe { decode_utf8(new_ptr, new_len) } {
        Ok(s) => s,
        Err(_) => return alloc_cstring(""),
    };

    if old_text.is_empty() && new_text.is_empty() {
        return alloc_cstring("");
    }

    let diff = TextDiff::from_lines(&old_text, &new_text);
    let unified = diff
        .unified_diff()
        .context_radius(3)
        .header("old", "new")
        .to_string();
    alloc_cstring(&unified)
}

/// Minimal git-diff cleaner: strip trailing whitespace, drop empty hunks,
/// preserve trailing newline. On parse failure returns the raw input unchanged.
#[no_mangle]
pub extern "C" fn format_git_diff(raw_ptr: *const u8, raw_len: usize) -> *mut u8 {
    let raw = match unsafe { decode_utf8(raw_ptr, raw_len) } {
        Ok(s) => s,
        Err(_) => return alloc_cstring(""),
    };
    let formatted = format_git_diff_impl(&raw);
    alloc_cstring(&formatted)
}

fn format_git_diff_impl(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim_end();
        if line.starts_with("@@ ") {
            let header_idx = i;
            i += 1;
            let mut body: Vec<String> = Vec::new();
            let mut has_change = false;
            while i < lines.len() {
                let l = lines[i].trim_end();
                if l.starts_with("@@ ")
                    || l.starts_with("diff --git")
                    || l.starts_with("--- ")
                    || l.starts_with("+++ ")
                {
                    break;
                }
                if l.starts_with('+') || l.starts_with('-') {
                    has_change = true;
                }
                body.push(l.to_string());
                i += 1;
            }
            if has_change {
                out.push(lines[header_idx].trim_end().to_string());
                out.extend(body);
            }
        } else {
            out.push(line.to_string());
            i += 1;
        }
    }

    let mut result = out.join("\n");
    if raw.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }
    result
}

#[cfg(test)]
mod estimate_tests {
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

#[cfg(test)]
mod diff_tests {
    use super::*;

    #[test]
    fn compute_diff_basic() {
        let out = call_compute_diff("a\nb", "a\nc\nb");
        assert!(out.contains("@@"));
        assert!(out.contains("+c"));
        assert!(out.contains("--- old"));
        assert!(out.contains("+++ new"));
    }

    #[test]
    fn compute_diff_empty_inputs() {
        assert_eq!(call_compute_diff("", ""), "");
    }

    #[test]
    fn format_git_diff_strips_trailing_whitespace() {
        let raw = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n ";
        let out = call_format_git_diff(raw);
        assert!(!out.ends_with(' '));
        assert!(out.contains("diff --git"));
    }

    #[test]
    fn format_git_diff_drops_empty_hunk() {
        let raw = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n context\n context\n";
        let out = call_format_git_diff(raw);
        assert!(!out.contains("@@"));
    }

    fn call_compute_diff(old: &str, new: &str) -> String {
        let old_b = old.as_bytes();
        let new_b = new.as_bytes();
        let ptr = compute_diff(old_b.as_ptr(), old_b.len(), new_b.as_ptr(), new_b.len());
        read_cstring(ptr)
    }

    fn call_format_git_diff(raw: &str) -> String {
        let b = raw.as_bytes();
        let ptr = format_git_diff(b.as_ptr(), b.len());
        read_cstring(ptr)
    }

    fn read_cstring(ptr: *mut u8) -> String {
        if ptr.is_null() {
            return String::new();
        }
        unsafe {
            let view = std::slice::from_raw_parts(ptr, 1024);
            let mut len = 0;
            while view[len] != 0 {
                len += 1;
            }
            let s = String::from_utf8_lossy(&view[..len]).to_string();
            dealloc(ptr, len + 1);
            s
        }
    }
}
