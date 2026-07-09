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
        ErrorMode::Strict => String::from_utf8(data.to_vec()).map_err(|_| DecodeError {
            encoding: "utf-8".to_string(),
        }),
        ErrorMode::Replace => Ok(String::from_utf8_lossy(data).to_string()),
        ErrorMode::Ignore => Ok(decode_utf8_ignore(data)),
    }
}

fn is_utf8_continuation(byte: u8) -> bool {
    (0x80..=0xbf).contains(&byte)
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
        if (0xc2..=0xdf).contains(&b0) {
            if i + 1 < data.len() && is_utf8_continuation(data[i + 1]) {
                let cp = u32::from(b0 & 0x1f) << 6 | u32::from(data[i + 1] & 0x3f);
                output.push(char::from_u32(cp).unwrap());
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if (0xe0..=0xef).contains(&b0) {
            if i + 2 < data.len() {
                let b1 = data[i + 1];
                let b2 = data[i + 2];
                let valid_second = match b0 {
                    0xe0 => (0xa0..=0xbf).contains(&b1),
                    0xed => (0x80..=0x9f).contains(&b1),
                    _ => is_utf8_continuation(b1),
                };
                if valid_second && is_utf8_continuation(b2) {
                    let cp = u32::from(b0 & 0x0f) << 12
                        | u32::from(b1 & 0x3f) << 6
                        | u32::from(b2 & 0x3f);
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
        if (0xf0..=0xf4).contains(&b0) {
            if i + 3 < data.len() {
                let b1 = data[i + 1];
                let b2 = data[i + 2];
                let b3 = data[i + 3];
                let valid_second = match b0 {
                    0xf0 => (0x90..=0xbf).contains(&b1),
                    0xf4 => (0x80..=0x8f).contains(&b1),
                    _ => is_utf8_continuation(b1),
                };
                if valid_second && is_utf8_continuation(b2) && is_utf8_continuation(b3) {
                    let cp = u32::from(b0 & 0x07) << 18
                        | u32::from(b1 & 0x3f) << 12
                        | u32::from(b2 & 0x3f) << 6
                        | u32::from(b3 & 0x3f);
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
                Err(DecodeError {
                    encoding: "utf-16le".to_string(),
                })
            } else {
                Ok(cow.to_string())
            }
        }
        ErrorMode::Replace => {
            let cow = encoding_rs::UTF_16LE.decode_without_bom_handling(data).0;
            // encoding_rs may output raw surrogates (U+D800–U+DFFF) for lone
            // surrogates. TS TextDecoder replaces them with U+FFFD.
            Ok(replace_surrogates_with_fffd(&cow))
        }
        ErrorMode::Ignore => Ok(decode_utf16le_ignore(data)),
    }
}

/// Replace any lone surrogate code points (U+D800–U+DFFF) with U+FFFD.
/// encoding_rs may preserve raw surrogates; TS TextDecoder replaces them.
fn replace_surrogates_with_fffd(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            let cp = c as u32;
            if (0xd800..=0xdfff).contains(&cp) {
                '\u{fffd}'
            } else {
                c
            }
        })
        .collect()
}

fn decode_utf16le_ignore(data: &[u8]) -> String {
    let mut output = String::new();
    let mut i = 0;
    while i + 1 < data.len() {
        let first = data[i] as u32 | ((data[i + 1] as u32) << 8);
        if (0xd800..=0xdbff).contains(&first) {
            if i + 3 < data.len() {
                let low = data[i + 2] as u32 | ((data[i + 3] as u32) << 8);
                if (0xdc00..=0xdfff).contains(&low) {
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
        if (0xdc00..=0xdfff).contains(&first) {
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
        // Invalid byte 0xff followed by a valid U+FFFD encoded in UTF-8 (0xef 0xbf 0xbd).
        let data = b"\xff\xef\xbf\xbd hello";
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
