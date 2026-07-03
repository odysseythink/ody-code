# Part 2 — Shared Pure Helpers

本 Part 交付 `decodeTextWithErrors`、`globPatternToRegex` 与 `BufferedReadable` 三个共享 helper 的 Rust 实现，每个都通过单元测试钉死与 TS 的语义等价性。

---

### Task 4: Port decodeTextWithErrors

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/kaos-rs/src/text.rs`

**Steps:**
- [ ] Write the failing tests first in `src/text.rs`:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn strict_rejects_invalid_utf8() {
          let data = b"hello \xff world";
          assert!(decode_text_with_errors(data, "utf-8", ErrorMode::Strict).is_err());
      }

      #[test]
      fn replace_substitutes_invalid_utf8_with_replacement_char() {
          let data = b"hello \xff world";
          let result = decode_text_with_errors(data, "utf-8", ErrorMode::Replace).unwrap();
          assert_eq!(result, "hello \u{fffd} world");
      }

      #[test]
      fn ignore_drops_invalid_utf8_but_preserves_valid_replacement_char() {
          // Invalid sequence followed by a valid U+FFFD encoded in UTF-8.
          let data = b"\xff\xbf\xbd hello";
          let result = decode_text_with_errors(data, "utf-8", ErrorMode::Ignore).unwrap();
          assert_eq!(result, "\u{fffd} hello");
      }

      #[test]
      fn ignore_drops_invalid_utf16le_surrogate() {
          // High surrogate alone (0xd800 little-endian) is invalid.
          let data = [0x00, 0xd8, 0x41, 0x00]; // U+D800, 'A'
          let result = decode_text_with_errors(&data, "utf-16le", ErrorMode::Ignore).unwrap();
          assert_eq!(result, "A");
      }

      #[test]
      fn replace_substitutes_invalid_utf16le() {
          let data = [0x00, 0xd8, 0x41, 0x00];
          let result = decode_text_with_errors(&data, "utf-16le", ErrorMode::Replace).unwrap();
          assert_eq!(result, "\u{fffd}A");
      }

      #[test]
      fn strict_rejects_invalid_utf16le() {
          let data = [0x00, 0xd8, 0x41, 0x00];
          assert!(decode_text_with_errors(&data, "utf-16le", ErrorMode::Strict).is_err());
      }
  }
  ```
  Run:
  ```bash
  cd rust-ody && cargo test -p kaos-rs --lib text::tests
  ```
  Expected: compilation fails because `decode_text_with_errors` and `ErrorMode` do not exist.
- [ ] Implement `src/text.rs`:
  ```rust
  use std::fmt;

  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum ErrorMode {
      Strict,
      Replace,
      Ignore,
  }

  #[derive(Debug)]
  pub struct DecodeError {
      encoding: String,
  }

  impl fmt::Display for DecodeError {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
          write!(f, "decode error for encoding {}", self.encoding)
      }
  }

  impl std::error::Error for DecodeError {}

  /// Decode bytes into a string with Python-compatible `errors` handling.
  ///
  /// - `strict` (default): throw on invalid sequences.
  /// - `replace`: substitute each invalid sequence with U+FFFD.
  /// - `ignore`: drop invalid input sequences while preserving valid U+FFFD characters.
  pub fn decode_text_with_errors(
      data: &[u8],
      encoding: &str,
      mode: ErrorMode,
  ) -> Result<String, DecodeError> {
      let label = encoding.to_ascii_lowercase();
      match label.as_str() {
          "utf-8" | "utf8" => decode_utf8(data, mode),
          "utf-16le" | "utf16le" | "ucs2" | "ucs-2" => decode_utf16le(data, mode),
          _ => {
              // Non-UTF encodings are treated as lossless byte↔character mappings;
              // `errors` has no effect. Use `encoding_rs` only if later needed.
              Ok(data.iter().map(|&b| b as char).collect())
          }
      }
  }

  fn decode_utf8(data: &[u8], mode: ErrorMode) -> Result<String, DecodeError> {
      match mode {
          ErrorMode::Strict => String::from_utf8(data.to_vec())
              .map_err(|_| DecodeError { encoding: "utf-8".to_string() }),
          ErrorMode::Replace => Ok(String::from_utf8_lossy(data).to_string()),
          ErrorMode::Ignore => Ok(decode_utf8_ignore(data)),
      }
  }

  fn is_utf8_continuation(byte: u8) -> bool {
      byte >= 0x80 && byte <= 0xbf
  }

  fn decode_utf8_ignore(data: &[u8]) -> String {
      let mut output = String::new();
      let mut i = 0;
      while i < data.len() {
          let b0 = data[i];
          if b0 <= 0x7f {
              output.push(b0 as char);
              i += 1;
              continue;
          }
          if b0 >= 0xc2 && b0 <= 0xdf {
              if i + 1 < data.len() && is_utf8_continuation(data[i + 1]) {
                  let cp = ((b0 & 0x1f) << 6) | (data[i + 1] & 0x3f);
                  output.push(char::from_u32(cp).unwrap());
                  i += 2;
                  continue;
              }
              i += 1;
              continue;
          }
          if b0 >= 0xe0 && b0 <= 0xef {
              if i + 2 < data.len() {
                  let b1 = data[i + 1];
                  let b2 = data[i + 2];
                  let valid_second = match b0 {
                      0xe0 => b1 >= 0xa0 && b1 <= 0xbf,
                      0xed => b1 >= 0x80 && b1 <= 0x9f,
                      _ => is_utf8_continuation(b1),
                  };
                  if valid_second && is_utf8_continuation(b2) {
                      let cp = ((b0 & 0x0f) << 12) | ((b1 & 0x3f) << 6) | (b2 & 0x3f);
                      if let Some(c) = char::from_u32(cp) {
                          output.push(c);
                      }
                      i += 3;
                      continue;
                  }
              }
              i += 1;
              continue;
          }
          if b0 >= 0xf0 && b0 <= 0xf4 {
              if i + 3 < data.len() {
                  let b1 = data[i + 1];
                  let b2 = data[i + 2];
                  let b3 = data[i + 3];
                  let valid_second = match b0 {
                      0xf0 => b1 >= 0x90 && b1 <= 0xbf,
                      0xf4 => b1 >= 0x80 && b1 <= 0x8f,
                      _ => is_utf8_continuation(b1),
                  };
                  if valid_second && is_utf8_continuation(b2) && is_utf8_continuation(b3) {
                      let cp = ((b0 & 0x07) << 18)
                          | ((b1 & 0x3f) << 12)
                          | ((b2 & 0x3f) << 6)
                          | (b3 & 0x3f);
                      if let Some(c) = char::from_u32(cp) {
                          output.push(c);
                      }
                      i += 4;
                      continue;
                  }
              }
              i += 1;
              continue;
          }
          i += 1;
      }
      output
  }

  fn decode_utf16le(data: &[u8], mode: ErrorMode) -> Result<String, DecodeError> {
      match mode {
          ErrorMode::Strict => {
              let (cow, had_errors) = encoding_rs::UTF_16LE.decode_without_bom_handling(data);
              if had_errors {
                  Err(DecodeError { encoding: "utf-16le".to_string() })
              } else {
                  Ok(cow.to_string())
              }
          }
          ErrorMode::Replace => Ok(encoding_rs::UTF_16LE.decode_without_bom_handling(data).0.to_string()),
          ErrorMode::Ignore => Ok(decode_utf16le_ignore(data)),
      }
  }

  fn decode_utf16le_ignore(data: &[u8]) -> String {
      let mut output = String::new();
      let mut i = 0;
      while i + 1 < data.len() {
          let first = data[i] as u32 | ((data[i + 1] as u32) << 8);
          if first >= 0xd800 && first <= 0xdbff {
              if i + 3 < data.len() {
                  let low = data[i + 2] as u32 | ((data[i + 3] as u32) << 8);
                  if low >= 0xdc00 && low <= 0xdfff {
                      let cp = 0x10000 + ((first - 0xd800) << 10) + (low - 0xdc00);
                      if let Some(c) = char::from_u32(cp) {
                          output.push(c);
                      }
                      i += 4;
                      continue;
                  }
              }
              i += 2;
              continue;
          }
          if first >= 0xdc00 && first <= 0xdfff {
              i += 2;
              continue;
          }
          if let Some(c) = char::from_u32(first) {
              output.push(c);
          }
          i += 2;
      }
      output
  }
  ```
  Add to `Cargo.toml` under `[dependencies]`:
  ```toml
  encoding_rs = "0.8"
  ```
- [ ] Run tests:
  ```bash
  cd rust-ody && cargo test -p kaos-rs --lib text::tests
  ```
  Expected: all decode tests pass.
- [ ] Commit: `feat(kaos-rs): decodeTextWithErrors with strict/replace/ignore`.

---

### Task 5: Port globPatternToRegex

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/kaos-rs/src/glob.rs`

**Steps:**
- [ ] Write the failing tests first in `src/glob.rs`:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      fn matches(pattern: &str, case_sensitive: bool, input: &str) -> bool {
          glob_pattern_to_regex(pattern, case_sensitive).is_match(input)
      }

      #[test]
      fn star_matches_any_chars_except_slash() {
          assert!(matches("*.txt", true, "a.txt"));
          assert!(!matches("*.txt", true, "a/b.txt"));
      }

      #[test]
      fn question_matches_single_char() {
          assert!(matches("file?.log", true, "file1.log"));
          assert!(!matches("file?.log", true, "file12.log"));
      }

      #[test]
      fn char_class_negation_with_bang() {
          assert!(matches("[!a].txt", true, "b.txt"));
          assert!(!matches("[!a].txt", true, "a.txt"));
      }

      #[test]
      fn char_class_literal_caret_is_escaped() {
          assert!(matches("[a^].txt", true, "^.txt"));
          assert!(matches("[a^].txt", true, "a.txt"));
      }

      #[test]
      fn backslash_escapes_metachar() {
          assert!(matches("file\\*.txt", true, "file*.txt"));
          assert!(!matches("file\\*.txt", true, "fileA.txt"));
      }

      #[test]
      fn case_insensitive_flag_works() {
          assert!(matches("*.TXT", false, "a.txt"));
          assert!(!matches("*.TXT", true, "a.txt"));
      }
  }
  ```
  Run:
  ```bash
  cd rust-ody && cargo test -p kaos-rs --lib glob::tests
  ```
  Expected: compilation fails because `glob_pattern_to_regex` does not exist.
- [ ] Implement `src/glob.rs`:
  ```rust
  use regex::Regex;

  /// Convert a glob pattern segment (e.g. `"*.txt"`, `"file?.log"`) into a RegExp.
  /// Mirrors Python pathlib behavior: includes dotfiles, case-sensitive by default.
  pub fn glob_pattern_to_regex(pattern: &str, case_sensitive: bool) -> Regex {
      let mut regex = String::from('^');
      let chars: Vec<char> = pattern.chars().collect();
      let mut i = 0;
      while i < chars.len() {
          let ch = chars[i];
          match ch {
              '*' => regex.push_str("[^/]*"),
              '?' => regex.push_str("[^/]"),
              '[' => {
                  let mut j = i + 1;
                  let mut found = false;
                  while j < chars.len() {
                      if chars[j] == ']' {
                          found = true;
                          break;
                      }
                      j += 1;
                  }
                  if !found {
                      regex.push_str("\\[");
                  } else {
                      let mut char_class: String = chars[i + 1..j].iter().collect();
                      // Escape backslashes inside the class so a trailing backslash
                      // does not accidentally escape the closing `]`.
                      char_class = char_class.replace('\\', "\\\\");
                      if char_class.starts_with('!') {
                          char_class = format!("^{}", &char_class[1..]);
                      } else if char_class.starts_with('^') {
                          char_class = format!("\\{}", char_class);
                      }
                      regex.push('[');
                      regex.push_str(&char_class);
                      regex.push(']');
                      i = j;
                  }
              }
              '\\' => {
                  if i + 1 < chars.len() {
                      let next = chars[i + 1];
                      regex.push_str(&regex_escape_char(next));
                      i += 1;
                  } else {
                      regex.push_str("\\\\");
                  }
              }
              _ => regex.push_str(&regex_escape_char(ch)),
          }
          i += 1;
      }
      regex.push('$');
      let flags = if case_sensitive { "" } else { "(?i)" };
      Regex::new(&format!("{}{}", flags, regex)).expect("generated regex is valid")
  }

  fn regex_escape_char(ch: char) -> String {
      let s = ch.to_string();
      regex::escape(&s)
  }
  ```
  Note: `regex` is already a workspace dependency of `ody-host`, but `kaos-rs` does not inherit it. Add to `Cargo.toml` under `[dependencies]`:
  ```toml
  regex = "1"
  ```
- [ ] Run tests:
  ```bash
  cd rust-ody && cargo test -p kaos-rs --lib glob::tests
  ```
  Expected: all glob tests pass.
- [ ] Commit: `feat(kaos-rs): globPatternToRegex`.

---

### Task 6: Port BufferedReadable buffering semantics

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/kaos-rs/src/buffered.rs`

**Steps:**
- [ ] Update `rust-ody/crates/kaos-rs/Cargo.toml` to add `tokio`:
  ```toml
  [dependencies]
  dirs = "5"
  encoding_rs = "0.8"
  path-clean = "1"
  regex = "1"
  thiserror = "1"
  tokio = { workspace = true }
  which = "6"
  ```
- [ ] Write the failing tests first in `src/buffered.rs`:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use tokio::io::{AsyncReadExt, Cursor};

      #[tokio::test]
      async fn buffers_all_data_and_allows_read_after_source_ends() {
          let data = b"hello world";
          let source = Cursor::new(data.to_vec());
          let mut buffered = BufferedReadable::new(source);

          // Wait until the source is fully drained into the internal buffer.
          let mut all = Vec::new();
          buffered.read_to_end(&mut all).await.unwrap();
          assert_eq!(all, data);
          assert!(buffered.is_ended());
      }

      #[tokio::test]
      async fn partial_reads_then_wait_then_remaining() {
          let data = b"abcdefghij";
          let source = Cursor::new(data.to_vec());
          let mut buffered = BufferedReadable::new(source);

          let mut first = [0u8; 3];
          buffered.read_exact(&mut first).await.unwrap();
          assert_eq!(&first, b"abc");

          let mut rest = Vec::new();
          buffered.read_to_end(&mut rest).await.unwrap();
          assert_eq!(rest, b"defghij");
      }
  }
  ```
  Run:
  ```bash
  cd rust-ody && cargo test -p kaos-rs --lib buffered::tests
  ```
  Expected: compilation fails because `BufferedReadable` does not exist.
- [ ] Implement `src/buffered.rs`:
  ```rust
  use std::io;
  use std::pin::Pin;
  use std::task::{Context, Poll};

  use tokio::io::{AsyncRead, ReadBuf};

  /// A wrapper around an async reader that preserves backpressure while still
  /// allowing consumers to read buffered output after the source has ended.
  pub struct BufferedReadable<R> {
      inner: R,
      buffer: Vec<u8>,
      ended: bool,
  }

  impl<R> BufferedReadable<R> {
      pub fn new(inner: R) -> Self {
          Self {
              inner,
              buffer: Vec::with_capacity(128 * 1024),
              ended: false,
          }
      }

      pub fn is_ended(&self) -> bool {
          self.ended
      }
  }

  impl<R: AsyncRead + Unpin> AsyncRead for BufferedReadable<R> {
      fn poll_read(
          mut self: Pin<&mut Self>,
          cx: &mut Context<'_>,
          buf: &mut ReadBuf<'_>,
      ) -> Poll<io::Result<()>> {
          loop {
              if !self.buffer.is_empty() {
                  let n = std::cmp::min(buf.remaining(), self.buffer.len());
                  buf.put_slice(&self.buffer[..n]);
                  self.buffer.drain(..n);
                  return Poll::Ready(Ok(()));
              }
              if self.ended {
                  return Poll::Ready(Ok(()));
              }

              let mut temp = [0u8; 4096];
              let mut temp_buf = ReadBuf::new(&mut temp);
              match Pin::new(&mut self.inner).poll_read(cx, &mut temp_buf) {
                  Poll::Pending => {
                      if self.buffer.is_empty() {
                          return Poll::Pending;
                      }
                  }
                  Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                  Poll::Ready(Ok(())) => {
                      let n = temp_buf.filled().len();
                      if n == 0 {
                          self.ended = true;
                      } else {
                          self.buffer.extend_from_slice(&temp[..n]);
                      }
                  }
              }
          }
      }
  }
  ```
- [ ] Run tests:
  ```bash
  cd rust-ody && cargo test -p kaos-rs --lib buffered::tests
  ```
  Expected: all buffered tests pass.
- [ ] Run full crate tests to ensure tokio addition did not break workspace:
  ```bash
  cd rust-ody && cargo test -p kaos-rs && cargo check --workspace
  ```
  Expected: `kaos-rs` tests pass and workspace still compiles.
- [ ] Commit: `feat(kaos-rs): BufferedReadable async buffering semantics`.

---

## Local Self-Review

- [ ] 1. Spec-coverage: 4.1.0.5 `decodeTextWithErrors` → T4; 4.1.0.5 `globPatternToRegex` → T5; 4.1.0.5 `BufferedReadable` → T6。无 GAP。
- [ ] 2. Placeholder scan: 本 Part 无 TODO/TBD；非 UTF 编码 fallback 已在注释说明为“lossless byte↔char”。
- [ ] 3. No phantom tasks: T4/T5/T6 各产出带测试的模块与 Cargo.toml 依赖更新。
- [ ] 4. Dependency soundness: T4/T5/T6 均只依赖 T1；彼此无依赖；无反向依赖。
- [ ] 5. Caller & build soundness: T6 新增 `tokio` workspace 依赖到 `kaos-rs`，同一 Task 以 `cargo check --workspace` 验证；无 TS 共享签名变更。
- [ ] 6. Test-the-risk: T4 断言 strict/replace/ignore 三种模式与 valid U+FFFD 保留；T5 断言字符类 `!`/`^` 转义、大小写开关、反斜杠转义；T6 断言 source 结束后仍可完整读取。
- [ ] 7. Type一致性: helper 函数签名（`decode_text_with_errors`、`glob_pattern_to_regex`、`BufferedReadable`）与 TS 对应函数语义对齐；Part 3 的 fixture 直接调用这些符号。
