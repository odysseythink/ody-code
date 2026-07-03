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

// ---------------------------------------------------------------------------
// Glob: glob matching via globset
// ---------------------------------------------------------------------------

use globset::GlobBuilder;

const GLOB_ERROR: u32 = u32::MAX;

/// Match a glob pattern against a value.
///
/// `options` is a UTF-8 string: "true" for case-insensitive, anything else for
/// case-sensitive. Returns 1 on match, 0 on no-match, and GLOB_ERROR when the
/// pattern cannot be handled by the Rust subset (caller should fall back to
/// picomatch).
#[no_mangle]
pub extern "C" fn glob_match(
    value_ptr: *const u8,
    value_len: usize,
    pattern_ptr: *const u8,
    pattern_len: usize,
    opts_ptr: *const u8,
    opts_len: usize,
) -> u32 {
    let value = match unsafe { decode_utf8(value_ptr, value_len) } {
        Ok(s) => s,
        Err(_) => return GLOB_ERROR,
    };
    let pattern = match unsafe { decode_utf8(pattern_ptr, pattern_len) } {
        Ok(s) => s,
        Err(_) => return GLOB_ERROR,
    };
    let opts = match unsafe { decode_utf8(opts_ptr, opts_len) } {
        Ok(s) => s,
        Err(_) => return GLOB_ERROR,
    };
    let nocase = opts == "true";
    glob_match_impl(&value, &pattern, nocase)
}

fn glob_match_impl(value: &str, pattern: &str, nocase: bool) -> u32 {
    let patterns = match expand_braces(pattern) {
        Some(ps) => ps,
        None => return GLOB_ERROR,
    };

    for p in patterns {
        let glob = match GlobBuilder::new(&p)
            .literal_separator(true)
            .backslash_escape(true)
            .case_insensitive(nocase)
            .build()
        {
            Ok(g) => g,
            Err(_) => return GLOB_ERROR,
        };
        if glob.compile_matcher().is_match(value) {
            return 1;
        }
    }
    0
}

/// One-level brace expansion. Returns None if the pattern contains braces that
/// cannot be expanded safely (nested braces, braces inside unclosed character
/// classes, etc.), signalling a fall-back to picomatch.
fn expand_braces(pattern: &str) -> Option<Vec<String>> {
    let bytes = pattern.as_bytes();
    let mut bracket_depth: i32 = 0;
    let mut brace_start: Option<usize> = None;

    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'[' => bracket_depth += 1,
            b']' => bracket_depth = (bracket_depth - 1).max(0),
            b'{' if bracket_depth == 0 => {
                if brace_start.is_some() {
                    return None;
                }
                brace_start = Some(i);
            }
            b'}' if bracket_depth == 0 => {
                let start = brace_start?;
                let inner = &pattern[start + 1..i];
                if inner.is_empty() || inner.contains('{') || inner.contains('}') {
                    return None;
                }
                let prefix = &pattern[..start];
                let suffix = &pattern[i + 1..];
                let choices: Vec<&str> = inner.split(',').collect();
                return Some(
                    choices
                        .iter()
                        .map(|c| format!("{}{}{}", prefix, c, suffix))
                        .collect(),
                );
            }
            _ => {}
        }
    }

    if brace_start.is_some() {
        return None;
    }
    Some(vec![pattern.to_string()])
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

#[cfg(test)]
mod glob_tests {
    use super::*;

    #[test]
    fn glob_match_star() {
        assert_eq!(call_glob("main.ts", "*.ts", "false"), 1);
        assert_eq!(call_glob("src/main.ts", "*.ts", "false"), 0);
    }

    #[test]
    fn glob_match_double_star() {
        assert_eq!(call_glob("src/deep/main.ts", "src/**/*.ts", "false"), 1);
        assert_eq!(call_glob("main.ts", "src/**/*.ts", "false"), 0);
    }

    #[test]
    fn glob_match_brace() {
        assert_eq!(call_glob("a/b.ts", "a/{b,c}.ts", "false"), 1);
        assert_eq!(call_glob("a/c.ts", "a/{b,c}.ts", "false"), 1);
        assert_eq!(call_glob("a/d.ts", "a/{b,c}.ts", "false"), 0);
    }

    #[test]
    fn glob_match_nocase() {
        assert_eq!(call_glob("MAIN.TS", "*.ts", "true"), 1);
        assert_eq!(call_glob("MAIN.TS", "*.ts", "false"), 0);
    }

    #[test]
    fn glob_match_escaped_special_and_question() {
        assert_eq!(call_glob("a*b", "a\\*b", "false"), 1);
        assert_eq!(call_glob("aXb", "a?b", "false"), 1);
        assert_eq!(call_glob("a/b", "a?b", "false"), 0);
    }

    #[test]
    fn glob_match_character_class() {
        assert_eq!(call_glob("abc", "a[bc]c", "false"), 1);
        assert_eq!(call_glob("adc", "a[bc]c", "false"), 0);
    }

    #[test]
    fn glob_match_unsupported_returns_error() {
        assert_eq!(call_glob("a/c.ts", "a/{b,{c,d}}.ts", "false"), GLOB_ERROR);
    }

    fn call_glob(value: &str, pattern: &str, opts: &str) -> u32 {
        let v = value.as_bytes();
        let p = pattern.as_bytes();
        let o = opts.as_bytes();
        glob_match(
            v.as_ptr(),
            v.len(),
            p.as_ptr(),
            p.len(),
            o.as_ptr(),
            o.len(),
        )
    }
}
