use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

// ==================== ITEM ====================

#[derive(Clone)]
pub struct Item {
    pub name: String,
    pub full_path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub file_type: String,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub created: SystemTime,
    pub modified: SystemTime,
    pub size: u64,
    /// Only populated when `-k` (content keyword) is active and the entry is
    /// a plain regular file: Some(count) when the file is UTF-8, where the
    /// file may still contain zero hits; None for unavailable content
    /// (directories, symlinks, non-UTF-8 or unreadable files).
    pub keyword_hits: Option<u64>,
}

// ==================== SHEBANG & TYPE DETECTION ====================

pub fn detect_shebang(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;    let mut buf = [0u8; 128];
    let n = file.read(&mut buf).ok()?;
    if n < 2 || &buf[..2] != b"#!" {
        return None;
    }

    let end = buf[..n].iter().position(|&b| b == b'\n').unwrap_or(n);
    let line = std::str::from_utf8(&buf[2..end]).ok()?.trim();

    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let runner = if parts[0].ends_with("/env") || parts[0] == "env" {
        parts.iter().skip(1).find(|s| !s.starts_with('-')).copied()?
    } else {
        parts[0]
    };

    let name = Path::new(runner)
        .file_name()?
        .to_str()?
        .to_lowercase();

    if name.starts_with("python") {
        Some("python".to_string())
    } else if name.starts_with("node") {
        Some("node".to_string())
    } else if name.starts_with("bash") {
        Some("bash".to_string())
    } else if name.starts_with("zsh") {
        Some("zsh".to_string())
    } else if name.starts_with("sh") {
        Some("sh".to_string())
    } else if name.starts_with("ruby") {
        Some("ruby".to_string())
    } else if name.starts_with("perl") {
        Some("perl".to_string())
    } else {
        Some(name)
    }
}

/// `probe_shebang` must only be true for entries already known to be plain
/// regular files: `detect_shebang` opens and reads them, and opening special
/// files (FIFOs, sockets, devices) can block forever or trigger side effects.
pub fn determine_file_type(
    name: &str,
    is_dir: bool,
    is_symlink: bool,
    probe_shebang: bool,
    full_path: &Path,
) -> String {
    if is_symlink {
        return "symlink".to_string();
    }
    if is_dir {
        return "dir".to_string();
    }

    if probe_shebang {
        if let Some(runner) = detect_shebang(full_path) {
            return runner;
        }
    }

    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    if let Some(ext) = ext {
        match ext.as_str() {
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" | "ico" | "tiff" | "avif" => {
                return "image".to_string();
            }
            "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" => {
                return "video".to_string();
            }
            "mp3" | "m4a" | "wav" | "flac" | "aac" | "ogg" | "wma" | "opus" => {
                return "audio".to_string();
            }
            "zip" | "xz" | "gz" | "tar" | "bz2" | "7z" | "rar" | "zst" | "tgz" => {
                return "archive".to_string();
            }
            "txt" | "md" | "log" | "rst" | "csv" | "tsv" => {
                return "text".to_string();
            }
            "rs" => return "rust".to_string(),
            "js" | "mjs" | "cjs" => return "javascript".to_string(),
            "ts" | "mts" | "cts" => return "typescript".to_string(),
            "py" | "pyw" => return "python".to_string(),
            "sh" | "bash" | "zsh" => return "bash".to_string(),
            "c" | "h" => return "c".to_string(),
            "cpp" | "hpp" | "cc" | "cxx" => return "c++".to_string(),
            "go" => return "go".to_string(),
            "java" => return "java".to_string(),
            "kt" | "kts" => return "kotlin".to_string(),
            "rb" => return "ruby".to_string(),
            "php" => return "php".to_string(),
            "html" | "htm" => return "html".to_string(),
            "css" | "scss" | "sass" | "less" => return "css".to_string(),
            "json" => return "json".to_string(),
            "toml" => return "toml".to_string(),
            "yaml" | "yml" => return "yaml".to_string(),
            "xml" => return "xml".to_string(),
            "sql" => return "sql".to_string(),
            "asm" | "s" => return "assembly".to_string(),
            "lua" => return "lua".to_string(),
            "r" => return "r".to_string(),
            "swift" => return "swift".to_string(),
            "zig" => return "zig".to_string(),
            "nim" => return "nim".to_string(),
            "dart" => return "dart".to_string(),
            "ex" | "exs" => return "elixir".to_string(),
            "hs" => return "haskell".to_string(),
            "scala" => return "scala".to_string(),
            "clj" => return "clojure".to_string(),
            _ => {}
        }
    }

    "file".to_string()
}
