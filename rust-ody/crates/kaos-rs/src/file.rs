use std::fmt;
use std::io;

use base64::Engine;

use crate::text::{decode_text_with_errors, DecodeError, ErrorMode};

/// Error type for KAOS file I/O operations.
#[derive(Debug)]
pub enum KaosIoError {
    Io(io::Error),
    Decode(DecodeError),
}

impl fmt::Display for KaosIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KaosIoError::Io(e) => write!(f, "io error: {}", e),
            KaosIoError::Decode(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for KaosIoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            KaosIoError::Io(e) => Some(e),
            KaosIoError::Decode(e) => Some(e),
        }
    }
}

impl From<io::Error> for KaosIoError {
    fn from(e: io::Error) -> Self {
        KaosIoError::Io(e)
    }
}

impl From<DecodeError> for KaosIoError {
    fn from(e: DecodeError) -> Self {
        KaosIoError::Decode(e)
    }
}

/// Read up to `n` bytes from `path` (all bytes if `n` is None).
pub async fn read_bytes(path: &str, n: Option<u64>) -> Result<Vec<u8>, io::Error> {
    if let Some(limit) = n {
        use tokio::io::AsyncReadExt;
        let f = tokio::fs::File::open(path).await?;
        let mut buf = Vec::with_capacity(limit as usize);
        let mut taken = f.take(limit);
        tokio::io::copy(&mut taken, &mut buf).await?;
        Ok(buf)
    } else {
        tokio::fs::read(path).await
    }
}

/// Read file as text with encoding and error mode control.
pub async fn read_text(
    path: &str,
    encoding: Option<&str>,
    errors: Option<ErrorMode>,
) -> Result<String, KaosIoError> {
    let data = tokio::fs::read(path).await?;
    let enc = encoding.unwrap_or("utf-8");
    let mode = errors.unwrap_or(ErrorMode::Strict);
    Ok(decode_text_with_errors(&data, enc, mode)?)
}

/// Yield lines from the file at `path` one by one.
///
/// Lines are yielded with trailing newline preserved, matching TS behaviour:
///   "a\nb\n" → ["a\n", "b"]  (empty final line dropped)
///   "a\nb"   → ["a\n", "b"]
pub async fn read_lines(
    path: &str,
    encoding: Option<&str>,
    errors: Option<ErrorMode>,
) -> Result<Vec<String>, KaosIoError> {
    let content = read_text(path, encoding, errors).await?;
    let lines: Vec<&str> = content.split('\n').collect();
    let mut out = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        if i < lines.len() - 1 {
            out.push(format!("{}\n", line));
        } else if !line.is_empty() {
            out.push(line.to_string());
        }
    }
    Ok(out)
}

/// Write raw bytes to `path`, returning the number of bytes written.
pub async fn write_bytes(path: &str, data: &[u8]) -> Result<u64, io::Error> {
    tokio::fs::write(path, data).await?;
    Ok(data.len() as u64)
}

/// Write text to `path`, returning the number of characters written.
/// `mode`: "w" (truncate) or "a" (append).
pub async fn write_text(
    path: &str,
    data: &str,
    mode: Option<&str>,
    encoding: Option<&str>,
) -> Result<usize, io::Error> {
    let enc = encoding.unwrap_or("utf-8");
    let m = mode.unwrap_or("w");

    let bytes = encode_text(data, enc)?;

    if m == "a" {
        use tokio::io::AsyncWriteExt;
        let mut f = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        f.write_all(&bytes).await?;
    } else {
        tokio::fs::write(path, &bytes).await?;
    }
    Ok(data.len())
}

fn encode_text(data: &str, encoding: &str) -> Result<Vec<u8>, io::Error> {
    let label = encoding.to_ascii_lowercase();
    match label.as_str() {
        "utf-8" | "utf8" => Ok(data.as_bytes().to_vec()),
        "ascii" => Ok(data
            .chars()
            .map(|c| if (c as u32) <= 0x7f { c as u8 } else { b'?' })
            .collect()),
        "latin1" | "binary" | "iso-8859-1" => Ok(data
            .chars()
            .map(|c| if (c as u32) <= 0xff { c as u8 } else { b'?' })
            .collect()),
        "utf-16le" | "utf16le" | "ucs2" | "ucs-2" => {
            let mut buf = Vec::with_capacity(data.encode_utf16().count() * 2);
            for unit in data.encode_utf16() {
                buf.extend_from_slice(&unit.to_le_bytes());
            }
            Ok(buf)
        }
        "hex" => hex::decode(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e)),
        "base64" => base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e)),
        _ => {
            // Unknown encodings fall back to a lossy byte-per-character mapping
            // so callers still get deterministic bytes.
            Ok(data.chars().map(|c| c as u8).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_temp(content: &[u8]) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        tokio::fs::write(&path, content).await.unwrap();
        (dir, path.to_string_lossy().to_string())
    }

    #[tokio::test]
    async fn read_bytes_full() {
        let (_dir, path) = setup_temp(b"hello world").await;
        let result = read_bytes(&path, None).await.unwrap();
        assert_eq!(result, b"hello world");
    }

    #[tokio::test]
    async fn read_bytes_partial() {
        let (_dir, path) = setup_temp(b"hello world").await;
        let result = read_bytes(&path, Some(5)).await.unwrap();
        assert_eq!(result, b"hello");
    }

    #[tokio::test]
    async fn read_bytes_partial_exceeds_file() {
        let (_dir, path) = setup_temp(b"hi").await;
        let result = read_bytes(&path, Some(100)).await.unwrap();
        assert_eq!(result, b"hi");
    }

    #[tokio::test]
    async fn read_text_strict_success() {
        let (_dir, path) = setup_temp("hello".as_bytes()).await;
        let result = read_text(&path, None, None).await.unwrap();
        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn read_text_strict_rejects_invalid() {
        let (_dir, path) = setup_temp(b"hello \xff world").await;
        let result = read_text(&path, None, Some(ErrorMode::Strict)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn read_text_replace_substitutes() {
        let (_dir, path) = setup_temp(b"hello \xff world").await;
        let result = read_text(&path, None, Some(ErrorMode::Replace))
            .await
            .unwrap();
        assert_eq!(result, "hello \u{fffd} world");
    }

    #[tokio::test]
    async fn read_text_ignore_drops_invalid() {
        let (_dir, path) = setup_temp(b"\xff\xef\xbf\xbd hello").await;
        let result = read_text(&path, None, Some(ErrorMode::Ignore))
            .await
            .unwrap();
        assert_eq!(result, "\u{fffd} hello");
    }

    #[tokio::test]
    async fn read_lines_trailing_newline() {
        let (_dir, path) = setup_temp("a\nb\n".as_bytes()).await;
        let lines = read_lines(&path, None, None).await.unwrap();
        assert_eq!(lines, vec!["a\n", "b\n"]);
    }

    #[tokio::test]
    async fn read_lines_no_trailing_newline() {
        let (_dir, path) = setup_temp("a\nb".as_bytes()).await;
        let lines = read_lines(&path, None, None).await.unwrap();
        assert_eq!(lines, vec!["a\n", "b"]);
    }

    #[tokio::test]
    async fn write_bytes_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        let path_str = path.to_string_lossy().to_string();

        let n = write_bytes(&path_str, b"hello").await.unwrap();
        assert_eq!(n, 5);

        let content = tokio::fs::read(&path).await.unwrap();
        assert_eq!(content, b"hello");
    }

    #[tokio::test]
    async fn write_text_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        let path_str = path.to_string_lossy().to_string();

        let n = write_text(&path_str, "hello", None, None).await.unwrap();
        assert_eq!(n, 5);

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn write_text_append() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        let path_str = path.to_string_lossy().to_string();

        write_text(&path_str, "hello", None, None).await.unwrap();
        let n = write_text(&path_str, " world", Some("a"), None)
            .await
            .unwrap();
        assert_eq!(n, 6);

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn read_text_utf16le_replace() {
        // U+D800 (lone surrogate) + 'A' in UTF-16LE
        let data = [0x00u8, 0xd8, 0x41, 0x00];
        let (_dir, path) = setup_temp(&data).await;
        let result = read_text(&path, Some("utf-16le"), Some(ErrorMode::Replace))
            .await
            .unwrap();
        assert_eq!(result, "\u{fffd}A");
    }

    #[tokio::test]
    async fn write_text_utf16le_encodes_bomlessly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.bin");
        let path_str = path.to_string_lossy().to_string();

        let n = write_text(&path_str, "hello", None, Some("utf-16le"))
            .await
            .unwrap();
        assert_eq!(n, 5);

        let content = tokio::fs::read(&path).await.unwrap();
        assert_eq!(
            content,
            vec![0x68, 0x00, 0x65, 0x00, 0x6c, 0x00, 0x6c, 0x00, 0x6f, 0x00]
        );
    }

    #[tokio::test]
    async fn write_text_ascii_replaces_out_of_range() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.bin");
        let path_str = path.to_string_lossy().to_string();

        write_text(&path_str, "Aé", None, Some("ascii"))
            .await
            .unwrap();
        let content = tokio::fs::read(&path).await.unwrap();
        assert_eq!(content, vec![b'A', b'?']);
    }

    #[tokio::test]
    async fn write_text_hex_decodes_to_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.bin");
        let path_str = path.to_string_lossy().to_string();

        let n = write_text(&path_str, "48656c6c6f", None, Some("hex"))
            .await
            .unwrap();
        assert_eq!(n, 10);

        let content = tokio::fs::read(&path).await.unwrap();
        assert_eq!(content, b"Hello");
    }

    #[tokio::test]
    async fn write_text_base64_decodes_to_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.bin");
        let path_str = path.to_string_lossy().to_string();

        write_text(&path_str, "aGVsbG8=", None, Some("base64"))
            .await
            .unwrap();
        let content = tokio::fs::read(&path).await.unwrap();
        assert_eq!(content, b"hello");
    }
}
