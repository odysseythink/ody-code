use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

pub const MEDIA_SNIFF_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Text,
    Image,
    Video,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileType {
    pub kind: FileKind,
    pub mime_type: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

static IMAGE_MIME_BY_SUFFIX: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(".png", "image/png");
    m.insert(".jpg", "image/jpeg");
    m.insert(".jpeg", "image/jpeg");
    m.insert(".gif", "image/gif");
    m.insert(".bmp", "image/bmp");
    m.insert(".tif", "image/tiff");
    m.insert(".tiff", "image/tiff");
    m.insert(".webp", "image/webp");
    m.insert(".ico", "image/x-icon");
    m.insert(".heic", "image/heic");
    m.insert(".heif", "image/heif");
    m.insert(".avif", "image/avif");
    m.insert(".svgz", "image/svg+xml");
    m
});

static VIDEO_MIME_BY_SUFFIX: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(".mp4", "video/mp4");
    m.insert(".mpg", "video/mpeg");
    m.insert(".mpeg", "video/mpeg");
    m.insert(".mkv", "video/x-matroska");
    m.insert(".avi", "video/x-msvideo");
    m.insert(".mov", "video/quicktime");
    m.insert(".ogv", "video/ogg");
    m.insert(".wmv", "video/x-ms-wmv");
    m.insert(".webm", "video/webm");
    m.insert(".m4v", "video/x-m4v");
    m.insert(".flv", "video/x-flv");
    m.insert(".3gp", "video/3gpp");
    m.insert(".3g2", "video/3gpp2");
    m
});

static TEXT_MIME_BY_SUFFIX: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(".svg", "image/svg+xml");
    m
});

static NON_TEXT_SUFFIXES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s = HashSet::new();
    for ext in [
        ".icns", ".psd", ".ai", ".eps", ".pdf", ".doc", ".docx", ".dot", ".dotx", ".rtf", ".odt",
        ".xls", ".xlsx", ".xlsm", ".xlt", ".xltx", ".xltm", ".ods", ".ppt", ".pptx", ".pptm",
        ".pps", ".ppsx", ".odp", ".pages", ".numbers", ".key", ".zip", ".rar", ".7z", ".tar",
        ".gz", ".tgz", ".bz2", ".xz", ".zst", ".lz", ".lz4", ".br", ".cab", ".ar", ".deb", ".rpm",
        ".mp3", ".wav", ".flac", ".ogg", ".oga", ".opus", ".aac", ".m4a", ".wma", ".ttf", ".otf",
        ".woff", ".woff2", ".exe", ".dll", ".so", ".dylib", ".bin", ".apk", ".ipa", ".jar",
        ".class", ".pyc", ".pyo", ".wasm", ".dmg", ".iso", ".img", ".sqlite", ".sqlite3", ".db",
        ".db3",
    ] {
        s.insert(ext);
    }
    s
});

const ASF_HEADER: &[u8] = &[
    0x30, 0x26, 0xb2, 0x75, 0x8e, 0x66, 0xcf, 0x11, 0xa6, 0xd9, 0x00, 0xaa, 0x00, 0x62, 0xce, 0x6c,
];

static FTYP_IMAGE_BRANDS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("avif", "image/avif");
    m.insert("avis", "image/avif");
    m.insert("heic", "image/heic");
    m.insert("heif", "image/heif");
    m.insert("heix", "image/heif");
    m.insert("hevc", "image/heic");
    m.insert("mif1", "image/heif");
    m.insert("msf1", "image/heif");
    m
});

static FTYP_VIDEO_BRANDS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("isom", "video/mp4");
    m.insert("iso2", "video/mp4");
    m.insert("iso5", "video/mp4");
    m.insert("mp41", "video/mp4");
    m.insert("mp42", "video/mp4");
    m.insert("avc1", "video/mp4");
    m.insert("mp4v", "video/mp4");
    m.insert("m4v", "video/x-m4v");
    m.insert("qt", "video/quicktime");
    m.insert("3gp4", "video/3gpp");
    m.insert("3gp5", "video/3gpp");
    m.insert("3gp6", "video/3gpp");
    m.insert("3gp7", "video/3gpp");
    m.insert("3g2", "video/3gpp2");
    m
});

fn starts_with(buf: &[u8], prefix: &[u8]) -> bool {
    buf.len() >= prefix.len() && buf[..prefix.len()] == *prefix
}

fn sniff_ftyp_brand(header: &[u8]) -> Option<String> {
    if header.len() < 12 {
        return None;
    }
    if &header[4..8] != b"ftyp" {
        return None;
    }
    let raw = String::from_utf8_lossy(&header[8..12]);
    Some(raw.to_lowercase().trim().trim_end_matches('\0').to_string())
}

pub fn sniff_media_from_magic(data: &[u8]) -> Option<FileType> {
    let header = if data.len() > MEDIA_SNIFF_BYTES {
        &data[..MEDIA_SNIFF_BYTES]
    } else {
        data
    };

    if starts_with(header, &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]) {
        return Some(FileType {
            kind: FileKind::Image,
            mime_type: "image/png",
        });
    }
    if starts_with(header, &[0xff, 0xd8, 0xff]) {
        return Some(FileType {
            kind: FileKind::Image,
            mime_type: "image/jpeg",
        });
    }
    if starts_with(header, b"GIF87a") || starts_with(header, b"GIF89a") {
        return Some(FileType {
            kind: FileKind::Image,
            mime_type: "image/gif",
        });
    }
    if starts_with(header, b"BM") {
        return Some(FileType {
            kind: FileKind::Image,
            mime_type: "image/bmp",
        });
    }
    if starts_with(header, &[0x49, 0x49, 0x2a, 0x00])
        || starts_with(header, &[0x4d, 0x4d, 0x00, 0x2a])
    {
        return Some(FileType {
            kind: FileKind::Image,
            mime_type: "image/tiff",
        });
    }
    if starts_with(header, &[0x00, 0x00, 0x01, 0x00]) {
        return Some(FileType {
            kind: FileKind::Image,
            mime_type: "image/x-icon",
        });
    }
    if starts_with(header, b"RIFF") && header.len() >= 12 {
        let chunk = &header[8..12];
        if chunk == b"WEBP" {
            return Some(FileType {
                kind: FileKind::Image,
                mime_type: "image/webp",
            });
        }
        if chunk == b"AVI " {
            return Some(FileType {
                kind: FileKind::Video,
                mime_type: "video/x-msvideo",
            });
        }
    }
    if starts_with(header, b"FLV") {
        return Some(FileType {
            kind: FileKind::Video,
            mime_type: "video/x-flv",
        });
    }
    if starts_with(header, ASF_HEADER) {
        return Some(FileType {
            kind: FileKind::Video,
            mime_type: "video/x-ms-wmv",
        });
    }
    if starts_with(header, &[0x1a, 0x45, 0xdf, 0xa3]) {
        let lowered = String::from_utf8_lossy(header).to_lowercase();
        if lowered.contains("webm") {
            return Some(FileType {
                kind: FileKind::Video,
                mime_type: "video/webm",
            });
        }
        if lowered.contains("matroska") {
            return Some(FileType {
                kind: FileKind::Video,
                mime_type: "video/x-matroska",
            });
        }
    }
    if let Some(brand) = sniff_ftyp_brand(header) {
        if !brand.is_empty() {
            if let Some(mime) = FTYP_IMAGE_BRANDS.get(brand.as_str()) {
                return Some(FileType {
                    kind: FileKind::Image,
                    mime_type: *mime,
                });
            }
            if let Some(mime) = FTYP_VIDEO_BRANDS.get(brand.as_str()) {
                return Some(FileType {
                    kind: FileKind::Video,
                    mime_type: *mime,
                });
            }
        }
    }
    None
}

pub fn sniff_image_dimensions(data: &[u8]) -> Option<ImageDimensions> {
    // PNG — IHDR width/height are big-endian uint32 at offsets 16 and 20.
    if starts_with(data, &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]) && data.len() >= 24 {
        return Some(ImageDimensions {
            width: u32::from_be_bytes(data[16..20].try_into().unwrap()),
            height: u32::from_be_bytes(data[20..24].try_into().unwrap()),
        });
    }

    // GIF — logical-screen width/height are little-endian uint16 at 6 and 8.
    if (starts_with(data, b"GIF87a") || starts_with(data, b"GIF89a")) && data.len() >= 10 {
        return Some(ImageDimensions {
            width: u16::from_le_bytes(data[6..8].try_into().unwrap()) as u32,
            height: u16::from_le_bytes(data[8..10].try_into().unwrap()) as u32,
        });
    }

    // BMP — DIB header width/height are little-endian int32 at 18 and 22.
    if starts_with(data, b"BM") && data.len() >= 26 {
        let height = i32::from_le_bytes(data[22..26].try_into().unwrap());
        return Some(ImageDimensions {
            width: i32::from_le_bytes(data[18..22].try_into().unwrap()) as u32,
            height: height.unsigned_abs(),
        });
    }

    // WEBP — RIFF container; VP8/VP8L/VP8X each store dimensions differently.
    if starts_with(data, b"RIFF") && data.len() >= 30 {
        let four_cc = std::str::from_utf8(&data[12..16]).unwrap_or("");
        if four_cc == "VP8 " {
            return Some(ImageDimensions {
                width: (u16::from_le_bytes(data[26..28].try_into().unwrap()) & 0x3fff) as u32,
                height: (u16::from_le_bytes(data[28..30].try_into().unwrap()) & 0x3fff) as u32,
            });
        }
        if four_cc == "VP8L" && data.len() >= 25 {
            let bits = u32::from_le_bytes(data[21..25].try_into().unwrap());
            return Some(ImageDimensions {
                width: (bits & 0x3fff) + 1,
                height: ((bits >> 14) & 0x3fff) + 1,
            });
        }
        if four_cc == "VP8X" {
            let width =
                1 + (data[24] as u32 | ((data[25] as u32) << 8) | ((data[26] as u32) << 16));
            let height =
                1 + (data[27] as u32 | ((data[28] as u32) << 8) | ((data[29] as u32) << 16));
            return Some(ImageDimensions { width, height });
        }
    }

    // JPEG — scan SOFn segments.
    if starts_with(data, &[0xff, 0xd8]) {
        let mut offset = 2;
        while offset + 9 < data.len() {
            if data[offset] != 0xff {
                offset += 1;
                continue;
            }
            let marker = data[offset + 1];
            if marker >= 0xc0
                && marker <= 0xcf
                && marker != 0xc4
                && marker != 0xc8
                && marker != 0xcc
            {
                return Some(ImageDimensions {
                    height: u16::from_be_bytes(data[offset + 5..offset + 7].try_into().unwrap())
                        as u32,
                    width: u16::from_be_bytes(data[offset + 7..offset + 9].try_into().unwrap())
                        as u32,
                });
            }
            if marker == 0xd8 || marker == 0xd9 || (marker >= 0xd0 && marker <= 0xd7) {
                offset += 2;
                continue;
            }
            let segment_length =
                u16::from_be_bytes(data[offset + 2..offset + 4].try_into().unwrap());
            if segment_length < 2 {
                break;
            }
            offset += 2 + segment_length as usize;
        }
    }

    None
}

fn get_suffix(path: &str) -> String {
    let idx = match path.rfind('.') {
        Some(i) => i,
        None => return "".to_string(),
    };
    let last_sep = path.rfind(['/', '\\']).unwrap_or(0);
    if idx <= last_sep {
        return "".to_string();
    }
    path[idx..].to_lowercase()
}

pub fn detect_file_type(path: &str, header: Option<&[u8]>) -> FileType {
    let suffix = get_suffix(path);
    let mut media_hint: Option<FileType> = None;
    if let Some(mime) = TEXT_MIME_BY_SUFFIX.get(suffix.as_str()) {
        media_hint = Some(FileType {
            kind: FileKind::Text,
            mime_type: *mime,
        });
    } else if let Some(mime) = IMAGE_MIME_BY_SUFFIX.get(suffix.as_str()) {
        media_hint = Some(FileType {
            kind: FileKind::Image,
            mime_type: *mime,
        });
    } else if let Some(mime) = VIDEO_MIME_BY_SUFFIX.get(suffix.as_str()) {
        media_hint = Some(FileType {
            kind: FileKind::Video,
            mime_type: *mime,
        });
    }

    if let Some(buf) = header {
        if let Some(sniffed) = sniff_media_from_magic(buf) {
            if let Some(hint) = media_hint {
                if sniffed.kind != hint.kind {
                    return FileType {
                        kind: FileKind::Unknown,
                        mime_type: "",
                    };
                }
                return hint;
            }
            return sniffed;
        }
        if buf.contains(&0x00) {
            return FileType {
                kind: FileKind::Unknown,
                mime_type: "",
            };
        }
    }

    if let Some(hint) = media_hint {
        return hint;
    }
    if NON_TEXT_SUFFIXES.contains(suffix.as_str()) {
        return FileType {
            kind: FileKind::Unknown,
            mime_type: "",
        };
    }
    FileType {
        kind: FileKind::Text,
        mime_type: "text/plain",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_sniff_bytes_is_512() {
        assert_eq!(MEDIA_SNIFF_BYTES, 512);
    }

    #[test]
    fn sniff_png_magic() {
        let header = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0];
        assert_eq!(
            sniff_media_from_magic(&header),
            Some(FileType {
                kind: FileKind::Image,
                mime_type: "image/png",
            })
        );
    }

    #[test]
    fn sniff_jpeg_magic() {
        let header = vec![0xff, 0xd8, 0xff, 0xe0, 0, 0];
        assert_eq!(
            sniff_media_from_magic(&header),
            Some(FileType {
                kind: FileKind::Image,
                mime_type: "image/jpeg",
            })
        );
    }

    #[test]
    fn sniff_gif_magic() {
        assert_eq!(
            sniff_media_from_magic(b"GIF87a\0\0"),
            Some(FileType {
                kind: FileKind::Image,
                mime_type: "image/gif",
            })
        );
        assert_eq!(
            sniff_media_from_magic(b"GIF89a\0\0"),
            Some(FileType {
                kind: FileKind::Image,
                mime_type: "image/gif",
            })
        );
    }

    #[test]
    fn sniff_webp_magic() {
        let header = [b'R', b'I', b'F', b'F', 0, 0, 0, 0, b'W', b'E', b'B', b'P'];
        assert_eq!(
            sniff_media_from_magic(&header),
            Some(FileType {
                kind: FileKind::Image,
                mime_type: "image/webp",
            })
        );
    }

    #[test]
    fn sniff_avif_ftyp() {
        let mut header = vec![0, 0, 0, 0x20];
        header.extend_from_slice(b"ftyp");
        header.extend_from_slice(b"avif");
        header.resize(32, 0);
        assert_eq!(
            sniff_media_from_magic(&header),
            Some(FileType {
                kind: FileKind::Image,
                mime_type: "image/avif",
            })
        );
    }

    #[test]
    fn sniff_mp4_ftyp() {
        let mut header = vec![0, 0, 0, 0x18];
        header.extend_from_slice(b"ftyp");
        header.extend_from_slice(b"mp42");
        header.extend_from_slice(&[0, 0, 0, 0]);
        header.extend_from_slice(b"mp42isom");
        let result = sniff_media_from_magic(&header).unwrap();
        assert_eq!(result.kind, FileKind::Video);
        assert_eq!(result.mime_type, "video/mp4");
    }

    #[test]
    fn sniff_matroska_and_webm() {
        let ebml = vec![0x1a, 0x45, 0xdf, 0xa3];
        let matroska = [ebml.clone(), b".matroska."[..].to_vec()].concat();
        assert_eq!(
            sniff_media_from_magic(&matroska),
            Some(FileType {
                kind: FileKind::Video,
                mime_type: "video/x-matroska",
            })
        );
        let webm = [ebml, b".webm."[..].to_vec()].concat();
        assert_eq!(
            sniff_media_from_magic(&webm),
            Some(FileType {
                kind: FileKind::Video,
                mime_type: "video/webm",
            })
        );
    }

    #[test]
    fn sniff_avi_riff() {
        let header = [b'R', b'I', b'F', b'F', 0, 0, 0, 0, b'A', b'V', b'I', b' '];
        assert_eq!(
            sniff_media_from_magic(&header),
            Some(FileType {
                kind: FileKind::Video,
                mime_type: "video/x-msvideo",
            })
        );
    }

    #[test]
    fn detect_by_extension_case_insensitive() {
        assert_eq!(
            detect_file_type("foo.PNG", None),
            FileType {
                kind: FileKind::Image,
                mime_type: "image/png",
            }
        );
        assert_eq!(
            detect_file_type("foo.mkv", None),
            FileType {
                kind: FileKind::Video,
                mime_type: "video/x-matroska",
            }
        );
    }

    #[test]
    fn svg_is_text_despite_image_mime() {
        let result = detect_file_type("icon.svg", None);
        assert_eq!(result.kind, FileKind::Text);
        assert_eq!(result.mime_type, "image/svg+xml");
    }

    #[test]
    fn nul_byte_header_makes_unknown() {
        let header = [b"partial"[..].to_vec(), vec![0x00, 0x00]].concat();
        assert_eq!(
            detect_file_type("notes.txt", Some(&header)).kind,
            FileKind::Unknown
        );
    }

    #[test]
    fn extension_sniff_kind_mismatch_returns_unknown() {
        let jpeg_header = vec![0xff, 0xd8, 0xff, 0xe0];
        assert_eq!(
            detect_file_type("mismatch.mp4", Some(&jpeg_header)).kind,
            FileKind::Unknown
        );
    }

    #[test]
    fn non_text_suffix_is_unknown() {
        assert_eq!(
            detect_file_type("archive.zip", None).kind,
            FileKind::Unknown
        );
    }

    #[test]
    fn unknown_suffix_falls_back_to_text_plain() {
        let result = detect_file_type("README", None);
        assert_eq!(result.kind, FileKind::Text);
        assert_eq!(result.mime_type, "text/plain");
    }

    #[test]
    fn typescript_suffixes_are_text() {
        assert_eq!(detect_file_type("app.ts", None).kind, FileKind::Text);
        assert_eq!(detect_file_type("component.tsx", None).kind, FileKind::Text);
        assert_eq!(detect_file_type("module.mts", None).kind, FileKind::Text);
        assert_eq!(detect_file_type("common.cts", None).kind, FileKind::Text);
    }

    #[test]
    fn dotfile_has_no_suffix_and_is_text() {
        assert_eq!(detect_file_type(".env", None).kind, FileKind::Text);
    }

    #[test]
    fn tar_gz_suffix_is_unknown() {
        assert_eq!(
            detect_file_type("archive.tar.gz", None).kind,
            FileKind::Unknown
        );
    }

    #[test]
    fn png_dimensions() {
        let mut buf = vec![0; 24];
        buf[..8].copy_from_slice(&[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
        buf[12..16].copy_from_slice(b"IHDR");
        buf[16..20].copy_from_slice(&800u32.to_be_bytes());
        buf[20..24].copy_from_slice(&600u32.to_be_bytes());
        assert_eq!(
            sniff_image_dimensions(&buf),
            Some(ImageDimensions {
                width: 800,
                height: 600
            })
        );
    }

    #[test]
    fn gif_dimensions() {
        let mut buf = vec![0; 10];
        buf[..6].copy_from_slice(b"GIF89a");
        buf[6..8].copy_from_slice(&320u16.to_le_bytes());
        buf[8..10].copy_from_slice(&240u16.to_le_bytes());
        assert_eq!(
            sniff_image_dimensions(&buf),
            Some(ImageDimensions {
                width: 320,
                height: 240
            })
        );
    }

    #[test]
    fn bmp_top_down_dimensions() {
        let mut buf = vec![0; 26];
        buf[..2].copy_from_slice(b"BM");
        buf[18..22].copy_from_slice(&640u32.to_le_bytes());
        buf[22..26].copy_from_slice(&(-480i32).to_le_bytes());
        assert_eq!(
            sniff_image_dimensions(&buf),
            Some(ImageDimensions {
                width: 640,
                height: 480
            })
        );
    }

    #[test]
    fn jpeg_dimensions_non_square() {
        let soi = vec![0xff, 0xd8];
        let app0 = vec![0xff, 0xe0, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00];
        let mut sof0 = vec![0; 19];
        sof0[0] = 0xff;
        sof0[1] = 0xc0;
        sof0[2..4].copy_from_slice(&17u16.to_be_bytes());
        sof0[4] = 8;
        sof0[5..7].copy_from_slice(&700u16.to_be_bytes());
        sof0[7..9].copy_from_slice(&100u16.to_be_bytes());
        let buf = [soi, app0, sof0].concat();
        assert_eq!(
            sniff_image_dimensions(&buf),
            Some(ImageDimensions {
                width: 100,
                height: 700
            })
        );
    }

    #[test]
    fn truncated_input_returns_none() {
        assert_eq!(sniff_image_dimensions(&[0x89, 0x50, 0x4e, 0x47]), None);
        assert_eq!(sniff_image_dimensions(b"not an image"), None);
    }
}
