use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use sha2::{Digest, Sha256};
use thiserror::Error;

const RG_VERSION: &str = "15.0.0";
const RG_BASE_URL: &str = "https://code.kimi.com/kimi-code/rg";
const DOWNLOAD_TIMEOUT_MS: u64 = 600_000;

static RG_ARCHIVE_SHA256: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

fn rg_archive_sha256() -> &'static HashMap<&'static str, &'static str> {
    RG_ARCHIVE_SHA256.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert(
            "ripgrep-15.0.0-aarch64-apple-darwin.tar.gz",
            "98bb2e61e7277ba0ea72d2ae2592497fd8d2940934a16b122448d302a6637e3b",
        );
        m.insert(
            "ripgrep-15.0.0-aarch64-pc-windows-msvc.zip",
            "572709c8770cb7f9385d725cb06d2bcd9537ec24d4dd17b1be1d65a876f8b591",
        );
        m.insert(
            "ripgrep-15.0.0-aarch64-unknown-linux-gnu.tar.gz",
            "15f8cc2fab12d88491c54d49f38589922a9d6a7353c29b0a0856727bcdf80754",
        );
        m.insert(
            "ripgrep-15.0.0-x86_64-apple-darwin.tar.gz",
            "44128c733d127ddbda461e01225a68b5f9997cfe7635242a797f645ca674a71a",
        );
        m.insert(
            "ripgrep-15.0.0-x86_64-pc-windows-msvc.zip",
            "21a98bf42c4da97ca543c010e764cc6dec8b9b7538d05f8d21874016385e0860",
        );
        m.insert(
            "ripgrep-15.0.0-x86_64-unknown-linux-musl.tar.gz",
            "253ad0fd5fef0d64cba56c70dccdacc1916d4ed70ad057cc525fcdb0c3bbd2a7",
        );
        m
    })
}

#[derive(Debug, Error, Clone)]
pub enum RgError {
    #[error("unsupported platform/arch for ripgrep download: {platform}/{arch}")]
    Unsupported { platform: String, arch: String },
    #[error("no pinned SHA-256 is configured for ripgrep archive {0}")]
    NoChecksum(String),
    #[error("download failed: HTTP {status} {status_text}")]
    Http { status: u16, status_text: String },
    #[error("network error: {0}")]
    Network(#[from] Arc<reqwest::Error>),
    #[error("io error: {0}")]
    Io(#[from] Arc<io::Error>),
    #[error("zip error: {0}")]
    Zip(#[from] Arc<zip::result::ZipError>),
    #[error("checksum mismatch for {name}: expected {expected}, got {actual}")]
    Checksum {
        name: String,
        expected: String,
        actual: String,
    },
    #[error("ripgrep archive did not contain expected binary: {0}")]
    MissingBinary(String),
    #[error("cancelled")]
    Cancelled,
}

impl From<io::Error> for RgError {
    fn from(e: io::Error) -> Self {
        RgError::Io(Arc::new(e))
    }
}

impl From<reqwest::Error> for RgError {
    fn from(e: reqwest::Error) -> Self {
        RgError::Network(Arc::new(e))
    }
}

impl From<zip::result::ZipError> for RgError {
    fn from(e: zip::result::ZipError) -> Self {
        RgError::Zip(Arc::new(e))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RgResolutionSource {
    SystemPath,
    Vendor,
    ShareBinCached,
    ShareBinDownloaded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgResolution {
    pub path: PathBuf,
    pub source: RgResolutionSource,
}

pub struct EnsureRgOptions {
    pub share_dir: Option<PathBuf>,
    pub cancel: Option<tokio::sync::watch::Receiver<bool>>,
}

pub async fn ensure_rg_path(options: EnsureRgOptions) -> Result<RgResolution, RgError> {
    if let Some(c) = options.cancel.as_ref() {
        if *c.borrow() {
            return Err(RgError::Cancelled);
        }
    }
    let share_dir = options.share_dir.unwrap_or_else(get_share_dir);
    if let Some(existing) = find_existing_rg(&share_dir, None).await {
        return Ok(existing);
    }
    if let Some(c) = options.cancel.as_ref() {
        if *c.borrow() {
            return Err(RgError::Cancelled);
        }
    }
    download_rg_with_lock(share_dir, options.cancel).await
}

pub async fn find_existing_rg(
    share_dir: impl AsRef<Path>,
    path_env: Option<&str>,
) -> Option<RgResolution> {
    let path_env = path_env
        .map(String::from)
        .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default());
    let bin_name = rg_binary_name();
    let sep = if cfg!(windows) { ';' } else { ':' };
    for dir in path_env.split(sep) {
        if dir.is_empty() {
            continue;
        }
        let candidate = Path::new(dir).join(bin_name);
        if is_executable_file(&candidate).await {
            return Some(RgResolution {
                path: candidate,
                source: RgResolutionSource::SystemPath,
            });
        }
    }
    if let Some(vendor) = get_vendor_rg_path(bin_name) {
        if is_executable_file(&vendor).await {
            return Some(RgResolution {
                path: vendor,
                source: RgResolutionSource::Vendor,
            });
        }
    }
    let cache = share_dir.as_ref().join("bin").join(bin_name);
    if is_executable_file(&cache).await {
        return Some(RgResolution {
            path: cache,
            source: RgResolutionSource::ShareBinCached,
        });
    }
    None
}

pub fn detect_target() -> Option<String> {
    detect_target_for(std::env::consts::ARCH, std::env::consts::OS)
}

pub fn detect_target_for(arch: &str, platform: &str) -> Option<String> {
    let arch = match arch {
        "x86_64" => "x86_64",
        "aarch64" | "arm64" => "aarch64",
        _ => return None,
    };
    match platform {
        "macos" | "darwin" => Some(format!("{arch}-apple-darwin")),
        "linux" => Some(if arch == "x86_64" {
            "x86_64-unknown-linux-musl".to_string()
        } else {
            "aarch64-unknown-linux-gnu".to_string()
        }),
        "windows" => Some(format!("{arch}-pc-windows-msvc")),
        _ => None,
    }
}

pub fn rg_unavailable_message(cause: &RgError) -> String {
    let share_bin = get_share_dir().join("bin").join(rg_binary_name());
    format!(
        "ripgrep (rg) is not available and the automatic bootstrap failed.\n\n\
         Error: {cause}\n\n\
         Fix options:\n\
           macOS:   brew install ripgrep\n\
           Ubuntu:  sudo apt-get install ripgrep\n\
           Other:   https://github.com/BurntSushi/ripgrep#installation\n\n\
         Alternatively, drop a static rg binary at {}",
        share_bin.display()
    )
}

pub async fn verify_archive_checksum(
    archive_path: impl AsRef<Path>,
    archive_name: &str,
    expected_sha256: &str,
) -> Result<(), RgError> {
    let bytes = tokio::fs::read(archive_path).await?;
    let actual = hex::encode(Sha256::digest(&bytes));
    if actual != expected_sha256 {
        Err(RgError::Checksum {
            name: archive_name.to_string(),
            expected: expected_sha256.to_string(),
            actual,
        })
    } else {
        Ok(())
    }
}

pub fn extract_rg_from_tar_gz(
    archive_path: impl AsRef<Path>,
    extract_dir: impl AsRef<Path>,
) -> Result<PathBuf, RgError> {
    let bytes = std::fs::read(archive_path)?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(std::io::Cursor::new(bytes)));
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let name = path.to_string_lossy();
        if name.ends_with("/rg") || name.ends_with("\\rg") {
            entry.unpack_in(&extract_dir)?;
            return Ok(extract_dir.as_ref().join(path));
        }
    }
    Err(RgError::MissingBinary("rg".to_string()))
}

pub fn extract_rg_from_zip(
    archive_path: impl AsRef<Path>,
    extract_dir: impl AsRef<Path>,
) -> Result<PathBuf, RgError> {
    let bytes = std::fs::read(archive_path)?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name();
        let basename = Path::new(name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if basename == "rg" || basename == "rg.exe" {
            let extracted = extract_dir.as_ref().join(name);
            if let Some(parent) = extracted.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&extracted)?;
            std::io::copy(&mut entry, &mut out)?;
            return Ok(extracted);
        }
    }
    Err(RgError::MissingBinary("rg or rg.exe".to_string()))
}

fn rg_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "rg.exe"
    } else {
        "rg"
    }
}

fn get_vendor_rg_path(_bin_name: &str) -> Option<PathBuf> {
    None
}

pub fn get_share_dir() -> PathBuf {
    if let Ok(override_dir) = std::env::var("ODY_CODE_HOME") {
        if !override_dir.is_empty() {
            return PathBuf::from(override_dir);
        }
    }
    dirs::home_dir()
        .map(|h| h.join(".ody-code"))
        .unwrap_or_else(|| PathBuf::from(".ody-code"))
}

async fn is_executable_file(p: impl AsRef<Path>) -> bool {
    tokio::fs::metadata(p.as_ref())
        .await
        .map(|m| m.is_file())
        .unwrap_or(false)
}

struct SharedDownload {
    result_rx: tokio::sync::watch::Receiver<Option<Result<RgResolution, RgError>>>,
}

static DOWNLOAD: OnceLock<tokio::sync::Mutex<Option<SharedDownload>>> = OnceLock::new();

async fn download_rg_with_lock(
    share_dir: PathBuf,
    mut cancel: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<RgResolution, RgError> {
    let lock = DOWNLOAD.get_or_init(|| tokio::sync::Mutex::new(None));
    let mut rx = {
        let mut guard = lock.lock().await;
        if let Some(shared) = guard.as_ref() {
            shared.result_rx.clone()
        } else {
            let (tx, rx) = tokio::sync::watch::channel(None);
            let share_dir2 = share_dir.clone();
            tokio::spawn(async move {
                let res = async {
                    if let Some(existing) = find_existing_rg(&share_dir2, None).await {
                        return Ok(existing);
                    }
                    let path = download_and_install_rg(share_dir2).await?;
                    Ok(RgResolution {
                        path,
                        source: RgResolutionSource::ShareBinDownloaded,
                    })
                }
                .await;
                let _ = tx.send(Some(res));
            });
            *guard = Some(SharedDownload {
                result_rx: rx.clone(),
            });
            rx
        }
    };

    loop {
        if let Some(res) = rx.borrow().clone() {
            return res;
        }
        tokio::select! {
            _ = rx.changed() => {}
            _ = cancel_signal(cancel.as_mut()) => return Err(RgError::Cancelled),
        }
    }
}

async fn cancel_signal(cancel: Option<&mut tokio::sync::watch::Receiver<bool>>) {
    if let Some(c) = cancel {
        let _ = c.changed().await;
    } else {
        std::future::pending().await
    }
}

async fn download_and_install_rg(share_dir: PathBuf) -> Result<PathBuf, RgError> {
    let target = detect_target().ok_or_else(|| RgError::Unsupported {
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    })?;

    let is_windows = target.contains("windows");
    let archive_ext = if is_windows { "zip" } else { "tar.gz" };
    let archive_name = format!("ripgrep-{RG_VERSION}-{target}.{archive_ext}");
    let expected_sha256 = rg_archive_sha256()
        .get(archive_name.as_str())
        .ok_or_else(|| RgError::NoChecksum(archive_name.clone()))?
        .to_string();
    let url = format!("{RG_BASE_URL}/{archive_name}");

    let bin_dir = share_dir.join("bin");
    tokio::fs::create_dir_all(&bin_dir).await?;
    let destination = bin_dir.join(rg_binary_name());

    let tmp = tempfile::tempdir()?;
    let archive_path = tmp.path().join(&archive_name);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(DOWNLOAD_TIMEOUT_MS))
        .build()?;
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        return Err(RgError::Http {
            status: resp.status().as_u16(),
            status_text: resp
                .status()
                .canonical_reason()
                .unwrap_or("Unknown")
                .to_string(),
        });
    }
    let bytes = resp.bytes().await?;
    tokio::fs::write(&archive_path, &bytes).await?;
    verify_archive_checksum(&archive_path, &archive_name, &expected_sha256).await?;

    if is_windows {
        let extracted = extract_rg_from_zip(&archive_path, tmp.path())?;
        tokio::fs::copy(&extracted, &destination).await?;
    } else {
        let extracted = extract_rg_from_tar_gz(&archive_path, tmp.path())?;
        tokio::fs::copy(&extracted, &destination).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o755))
                .await?;
        }
    }

    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[tokio::test]
    async fn find_existing_rg_returns_none_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let result = find_existing_rg(tmp.path(), Some("")).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn find_existing_rg_prefers_system_path() {
        let tmp = tempfile::tempdir().unwrap();
        let path_dir = tmp.path().join("path");
        fs::create_dir_all(&path_dir).unwrap();
        let on_path = path_dir.join(rg_binary_name());
        fs::write(&on_path, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&on_path, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let cache = tmp.path().join("bin").join(rg_binary_name());
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        fs::write(&cache, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&cache, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let path_env = path_dir.to_string_lossy().to_string();
        let result = find_existing_rg(tmp.path(), Some(&path_env)).await.unwrap();
        assert_eq!(result.source, RgResolutionSource::SystemPath);
        assert_eq!(result.path, on_path);
    }

    #[tokio::test]
    async fn find_existing_rg_falls_back_to_share_bin() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("bin").join(rg_binary_name());
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        fs::write(&cache, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&cache, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let result = find_existing_rg(tmp.path(), Some("")).await.unwrap();
        assert_eq!(result.source, RgResolutionSource::ShareBinCached);
        assert_eq!(result.path, cache);
    }

    #[test]
    fn detect_target_for_mappings() {
        assert_eq!(
            detect_target_for("aarch64", "darwin"),
            Some("aarch64-apple-darwin".to_string())
        );
        assert_eq!(
            detect_target_for("x86_64", "darwin"),
            Some("x86_64-apple-darwin".to_string())
        );
        assert_eq!(
            detect_target_for("x86_64", "linux"),
            Some("x86_64-unknown-linux-musl".to_string())
        );
        assert_eq!(
            detect_target_for("aarch64", "linux"),
            Some("aarch64-unknown-linux-gnu".to_string())
        );
        assert_eq!(
            detect_target_for("x86_64", "windows"),
            Some("x86_64-pc-windows-msvc".to_string())
        );
        assert_eq!(detect_target_for("mips", "linux"), None);
    }

    #[test]
    fn rg_unavailable_message_contains_cause_and_hints() {
        let msg = rg_unavailable_message(&RgError::Cancelled);
        assert!(msg.contains("ripgrep (rg) is not available"));
        assert!(msg.contains("brew install ripgrep"));
        assert!(msg.contains("https://github.com/BurntSushi/ripgrep"));
    }

    #[tokio::test]
    async fn verify_checksum_accepts_matching_digest() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("archive.tar.gz");
        let payload = b"trusted archive bytes";
        fs::write(&path, payload).unwrap();
        let expected = hex::encode(Sha256::digest(payload));
        verify_archive_checksum(&path, "archive.tar.gz", &expected)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn verify_checksum_rejects_mismatched_digest() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("archive.tar.gz");
        fs::write(&path, "tampered archive bytes").unwrap();
        let err = verify_archive_checksum(&path, "archive.tar.gz", &"0".repeat(64))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("checksum mismatch"));
    }

    fn build_tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let enc = flate2::write::GzEncoder::new(&mut buf, flate2::Compression::default());
            let mut tar = tar::Builder::new(enc);
            for (name, content) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_path(name).unwrap();
                header.set_size(content.len() as u64);
                header.set_cksum();
                tar.append(&header, *content).unwrap();
            }
            tar.finish().unwrap();
        }
        buf
    }

    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default();
            for (name, content) in entries {
                zip.start_file(*name, options).unwrap();
                zip.write_all(content).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn extract_tar_gz_finds_rg_binary() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("rg.tar.gz");
        let payload = b"#!/bin/sh\necho ripgrep 15.0.0\n";
        let tar_bytes = build_tar_gz(&[("ripgrep-15.0.0-x86_64-apple-darwin/rg", payload)]);
        fs::write(&archive, tar_bytes).unwrap();

        let extract_dir = tmp.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();
        let extracted = extract_rg_from_tar_gz(&archive, &extract_dir).unwrap();
        assert!(extracted.to_string_lossy().contains("/rg"));
        assert_eq!(fs::read(&extracted).unwrap(), payload);
    }

    #[test]
    fn extract_tar_gz_missing_rg_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("rg.tar.gz");
        let tar_bytes = build_tar_gz(&[("README.md", b"readme")]);
        fs::write(&archive, tar_bytes).unwrap();

        let extract_dir = tmp.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();
        let err = extract_rg_from_tar_gz(&archive, &extract_dir).unwrap_err();
        assert!(err.to_string().contains("did not contain expected binary"));
    }

    #[test]
    fn extract_zip_finds_rg_exe() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("rg.zip");
        let payload = b"MZfake-pe-bytes";
        let zip_bytes = build_zip(&[("ripgrep-15.0.0-x86_64-pc-windows-msvc/rg.exe", payload)]);
        fs::write(&archive, zip_bytes).unwrap();

        let extract_dir = tmp.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();
        let extracted = extract_rg_from_zip(&archive, &extract_dir).unwrap();
        assert!(extracted.to_string_lossy().contains("rg.exe"));
        assert_eq!(fs::read(&extracted).unwrap(), payload);
    }

    #[test]
    fn extract_zip_missing_rg_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("rg.zip");
        let zip_bytes = build_zip(&[("README.md", b"readme")]);
        fs::write(&archive, zip_bytes).unwrap();

        let extract_dir = tmp.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();
        let err = extract_rg_from_zip(&archive, &extract_dir).unwrap_err();
        assert!(err.to_string().contains("did not contain expected binary"));
    }

    #[tokio::test]
    async fn entry_cancellation_returns_cancelled() {
        let tmp = tempfile::tempdir().unwrap();
        let (_tx, rx) = tokio::sync::watch::channel(true);
        let err = ensure_rg_path(EnsureRgOptions {
            share_dir: Some(tmp.path().to_path_buf()),
            cancel: Some(rx),
        })
        .await
        .unwrap_err();
        assert!(matches!(err, RgError::Cancelled));
    }

    #[tokio::test]
    #[ignore = "requires network access to CDN"]
    async fn bootstrap_download_installs_rg() {
        let tmp = tempfile::tempdir().unwrap();
        let resolution = ensure_rg_path(EnsureRgOptions {
            share_dir: Some(tmp.path().to_path_buf()),
            cancel: None,
        })
        .await
        .unwrap();
        assert!(resolution.path.exists());
        assert_eq!(resolution.source, RgResolutionSource::ShareBinDownloaded);
        let output = std::process::Command::new(&resolution.path)
            .arg("--version")
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&output.stdout).contains("ripgrep"));
    }
}
