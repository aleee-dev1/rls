use std::env;
use std::fs;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Local};

// ==================== COLORS ====================

const COL_DIR: &str = "\x1b[1;34m";
const COL_EXEC: &str = "\x1b[32m";
const COL_FILE: &str = "\x1b[0m";
const COL_SYMLINK: &str = "\x1b[36m";
const COL_SIZE: &str = "\x1b[90m";
const COL_RESET: &str = "\x1b[0m";

// ==================== ITEM ====================

struct Item {
    name: String,
    full_path: PathBuf,
    is_dir: bool,
    is_symlink: bool,
    file_type: String,
    mode: u32,
    uid: u32,
    gid: u32,
    created: SystemTime,
    modified: SystemTime,
    size: u64,
}

// ==================== SHEBANG & TYPE DETECTION ====================

fn detect_shebang(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut buf = [0u8; 128];
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

fn determine_file_type(name: &str, is_dir: bool, is_symlink: bool, full_path: &Path) -> String {
    if is_symlink {
        return "symlink".to_string();
    }
    if is_dir {
        return "dir".to_string();
    }

    if let Some(runner) = detect_shebang(full_path) {
        return runner;
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

// ==================== PERMISSIONS ====================

fn format_permissions(mode: u32, is_dir: bool, is_symlink: bool) -> String {
    let file_type = if is_symlink {
        'l'
    } else if is_dir {
        'd'
    } else {
        '-'
    };
    let owner_r = if mode & 0o400 != 0 { 'r' } else { '-' };
    let owner_w = if mode & 0o200 != 0 { 'w' } else { '-' };
    let owner_x = if mode & 0o100 != 0 { 'x' } else { '-' };
    let group_r = if mode & 0o040 != 0 { 'r' } else { '-' };
    let group_w = if mode & 0o020 != 0 { 'w' } else { '-' };
    let group_x = if mode & 0o010 != 0 { 'x' } else { '-' };
    let other_r = if mode & 0o004 != 0 { 'r' } else { '-' };
    let other_w = if mode & 0o002 != 0 { 'w' } else { '-' };
    let other_x = if mode & 0o001 != 0 { 'x' } else { '-' };

    format!(
        "{}{}{}{}{}{}{}{}{}{}",
        file_type, owner_r, owner_w, owner_x, group_r, group_w, group_x, other_r, other_w, other_x
    )
}

fn format_rwx(r: bool, w: bool, x: bool) -> String {
    let mut s = String::new();
    if r {
        s.push('r');
    }
    if w {
        s.push('w');
    }
    if x {
        s.push('x');
    }
    if s.is_empty() {
        "-".to_string()
    } else {
        s
    }
}

// ==================== OWNER ====================

fn get_owner_group(uid: u32, gid: u32) -> String {
    extern "C" {
        fn getpwuid(uid: u32) -> *const Passwd;
        fn getgrgid(gid: u32) -> *const Group;
    }

    #[repr(C)]
    struct Passwd {
        pw_name: *const std::os::raw::c_char,
    }

    #[repr(C)]
    struct Group {
        gr_name: *const std::os::raw::c_char,
    }

    let user_name = unsafe {
        let pwd = getpwuid(uid);
        if !pwd.is_null() && !(*pwd).pw_name.is_null() {
            std::ffi::CStr::from_ptr((*pwd).pw_name)
                .to_string_lossy()
                .into_owned()
        } else {
            uid.to_string()
        }
    };

    let group_name = unsafe {
        let grp = getgrgid(gid);
        if !grp.is_null() && !(*grp).gr_name.is_null() {
            std::ffi::CStr::from_ptr((*grp).gr_name)
                .to_string_lossy()
                .into_owned()
        } else {
            gid.to_string()
        }
    };

    format!("{}:{}", user_name, group_name)
}

// ==================== SIZE ====================

fn format_size(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0usize;

    while size >= 800.0 && unit < units.len() - 1 {
        size /= 1000.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} {}", size, units[unit])
    }
}

// ==================== APPARENT SIZE ====================

fn apparent_size(path: &Path, is_dir: bool, direct_size: u64) -> u64 {
    if !is_dir {
        return direct_size;
    }

    let mut total = 0u64;

    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return 0,
    };

    for entry in entries.flatten() {
        let child_path = entry.path();

        let metadata = match fs::symlink_metadata(&child_path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if metadata.is_dir() {
            total += apparent_size(&child_path, true, 0);
        } else {
            total += metadata.len();
        }
    }

    total
}

// ==================== DATETIME ====================

fn format_datetime(t: SystemTime) -> String {
    let dt: DateTime<Local> = DateTime::from(t);
    dt.format("%d/%m/%Y %H:%M:%S").to_string()
}

// ==================== COLOR ====================

fn get_color(is_dir: bool, is_symlink: bool, mode: u32) -> &'static str {
    if is_symlink {
        return COL_SYMLINK;
    }

    if is_dir {
        return COL_DIR;
    }

    if mode & 0o111 != 0 {
        return COL_EXEC;
    }

    COL_FILE
}

// ==================== HELP & ARGS ====================

fn print_help() {
    println!(
        "rls - A modern file listing utility\n\n\
USAGE:\n    \
rls [OPTIONS] [DIRECTORY]\n\n\
OPTIONS:\n    \
-h, --help            Print this help information\n    \
-a, -H, --all         Show hidden files (starting with .)\n    \
-u, --usage           Show file/directory size\n    \
-p, --permissions     Show permission string (e.g. -rw-r--r--)\n    \
-pd, --perm-detail    Show detailed permissions (ow:rwx g:rx ot:rx)\n    \
-o, --owner           Show owner and group (user:group)\n    \
-d, --summary         Show directory summary at the end\n    \
-r, --recursive       Recursively list subdirectories\n    \
-k <keyword>          Filter files matching keyword (case-insensitive)\n    \
-s<field>[a|d]        Sort by field (n: name, c: created, m: modified, u: size)\n                          Direction: a (ascending, default), d (descending)\n                          Example: -sn, -scd, -su"
    );
}

struct Args {
    target: String,
    show_hidden: bool,
    show_usage: bool,
    show_summary: bool,
    show_permissions: bool,
    show_perm_detail: bool,
    show_owner: bool,
    sort_field: char,
    sort_dir: char,
    sort_specified: bool,
    keyword: Option<String>,
    recursive: bool,
    max_depth: Option<i32>,
}

fn parse_args() -> Args {
    let mut target: Option<String> = None;
    let mut show_hidden = false;
    let mut show_usage = false;
    let mut show_summary = false;
    let mut show_permissions = false;
    let mut show_perm_detail = false;
    let mut show_owner = false;
    let mut sort_field = 'c';
    let mut sort_dir = 'a';
    let mut sort_specified = false;
    let mut keyword: Option<String> = None;
    let mut recursive = false;
    let mut max_depth: Option<i32> = None;

    let raw: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;

    while i < raw.len() {
        let arg = &raw[i];

        if arg == "-h" || arg == "--help" {
            print_help();
            std::process::exit(0);
        } else if arg == "-a" || arg == "-H" || arg == "--all" || arg == "--hidden" {
            show_hidden = true;
        } else if arg == "-u" || arg == "--usage" || arg == "--size" {
            show_usage = true;
        } else if arg == "-p" || arg == "--permissions" || arg == "--perm" {
            show_permissions = true;
        } else if arg == "-pd" || arg == "--perm-detail" {
            show_perm_detail = true;
        } else if arg == "-o" || arg == "--owner" {
            show_owner = true;
        } else if arg == "-d" || arg == "--summary" {
            show_summary = true;
        } else if arg == "-r" || arg == "--recursive" {
            recursive = true;
            if i + 1 < raw.len() {
                if let Ok(val) = raw[i + 1].parse::<i32>() {
                    max_depth = Some(val);
                    i += 1;
                }
            }
        } else if arg.starts_with("--recursive=") {
            recursive = true;
            if let Ok(val) = arg["--recursive=".len()..].parse::<i32>() {
                max_depth = Some(val);
            }
        } else if arg == "-k" || arg == "--keyword" {
            i += 1;
            if i >= raw.len() {
                eprintln!("error: option '{}' requires an argument", arg);
                std::process::exit(1);
            }
            keyword = Some(raw[i].to_lowercase());
        } else if arg.starts_with("--keyword=") {
            let val = arg["--keyword=".len()..].trim();
            if val.is_empty() {
                eprintln!("error: option '--keyword' requires an argument");
                std::process::exit(1);
            }
            keyword = Some(val.to_lowercase());
        } else if arg == "-s" || arg == "--sort" {
            eprintln!("error: option '{}' requires a sort specifier (e.g. -sn, -sc, -smd)", arg);
            eprintln!("Fields: n (name), c (created), m (modified), u (usage)");
            eprintln!("Directions: a (ascending), d (descending)");
            std::process::exit(1);
        } else if arg.starts_with("--sort=") {
            let spec = &arg["--sort=".len()..];
            if spec.is_empty() {
                eprintln!("error: option '--sort' requires a sort specifier (e.g. --sort=n, --sort=cd)");
                std::process::exit(1);
            }
            sort_specified = true;
            let mut chars = spec.chars();
            let f = chars.next().unwrap();
            if !"ncmu".contains(f) {
                eprintln!("error: invalid sort field '{}'. Must be one of: n, c, m, u", f);
                std::process::exit(1);
            }
            sort_field = f;
            if let Some(d) = chars.next() {
                if d != 'a' && d != 'd' {
                    eprintln!("error: invalid sort direction '{}'. Must be 'a' (ascending) or 'd' (descending)", d);
                    std::process::exit(1);
                }
                sort_dir = d;
            }
        } else if arg.starts_with("-s") {
            sort_specified = true;
            let rest = &arg[2..];
            let mut chars = rest.chars();
            let f = match chars.next() {
                Some(c) => c,
                None => {
                    eprintln!("error: option '-s' requires a sort field (e.g. -sn, -sc, -su)");
                    std::process::exit(1);
                }
            };
            if !"ncmu".contains(f) {
                eprintln!("error: invalid sort field '{}'. Must be one of: n, c, m, u", f);
                std::process::exit(1);
            }
            sort_field = f;
            if let Some(d) = chars.next() {
                if d != 'a' && d != 'd' {
                    eprintln!("error: invalid sort direction '{}'. Must be 'a' (ascending) or 'd' (descending)", d);
                    std::process::exit(1);
                }
                sort_dir = d;
            }
        } else if arg.starts_with('-') {
            let rest = &arg[1..];
            if rest == "pd" {
                show_perm_detail = true;
            } else {
                let mut valid = true;
                for c in rest.chars() {
                    match c {
                        'a' | 'H' => show_hidden = true,
                        'u' => show_usage = true,
                        'p' => show_permissions = true,
                        'o' => show_owner = true,
                        'd' => show_summary = true,
                        'r' => recursive = true,
                        _ => {
                            valid = false;
                            break;
                        }
                    }
                }
                if !valid {
                    eprintln!("error: unrecognized flag '{}'\nTry 'rls --help' for more information.", arg);
                    std::process::exit(1);
                }
            }
        } else {
            if target.is_some() {
                eprintln!("error: unexpected argument '{}'", arg);
                std::process::exit(1);
            }
            target = Some(arg.clone());
        }

        i += 1;
    }

    Args {
        target: target.unwrap_or_else(|| ".".to_string()),
        show_hidden,
        show_usage,
        show_summary,
        show_permissions,
        show_perm_detail,
        show_owner,
        sort_field,
        sort_dir,
        sort_specified,
        keyword,
        recursive,
        max_depth,
    }
}

// ==================== COMPUTE MAX DEPTH ====================

fn compute_max_depth(path: &Path, current_depth: usize, show_hidden: bool) -> usize {
    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return current_depth,
    };

    let mut max_d = current_depth;

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !show_hidden && name.starts_with('.') {
            continue;
        }

        let full_path = entry.path();
        let metadata = match fs::symlink_metadata(&full_path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            let child_max = compute_max_depth(&full_path, current_depth + 1, show_hidden);
            if child_max > max_d {
                max_d = child_max;
            }
        }
    }

    max_d
}

// ==================== DISPLAY MODE ====================

#[derive(Clone, Copy, PartialEq)]
enum DisplayMode {
    Usage,
    Permissions,
    Type,
    PermOwner,
    PermGroup,
    PermOthers,
    Owner,
    Created,
    Modified,
}

// ==================== SUMMARY STATS ====================

struct SummaryStats {
    dir_count: usize,
    file_count: usize,
    total_size: u64,
}

// ==================== PROCESS DIRECTORY ====================

fn process_directory(
    path: &Path,
    depth: usize,
    allowed_depth: usize,
    args: &Args,
    detail_modes: &[DisplayMode],
    stats: &mut SummaryStats,
) {
    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return,
    };

    let need_size = args.show_usage || (args.sort_specified && args.sort_field == 'u');
    let mut items: Vec<Item> = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();

        if !args.show_hidden && name.starts_with('.') {
            continue;
        }

        let full_path: PathBuf = entry.path();

        let metadata = match fs::symlink_metadata(&full_path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let is_symlink = metadata.file_type().is_symlink();
        let is_dir = metadata.is_dir();
        let file_type = determine_file_type(&name, is_dir, is_symlink, &full_path);
        let mode = metadata.permissions().mode();
        let uid = metadata.uid();
        let gid = metadata.gid();
        let created = metadata.created().unwrap_or(SystemTime::UNIX_EPOCH);
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let direct_size = metadata.len();

        let size = if need_size {
            apparent_size(&full_path, is_dir, direct_size)
        } else {
            0
        };

        items.push(Item {
            name,
            full_path,
            is_dir,
            is_symlink,
            file_type,
            mode,
            uid,
            gid,
            created,
            modified,
            size,
        });
    }

    items.sort_by(|a, b| {
        let ord = match args.sort_field {
            'n' => a.name.cmp(&b.name),
            'm' => a.modified.cmp(&b.modified),
            'u' => a.size.cmp(&b.size),
            _ => a.created.cmp(&b.created),
        };

        if args.sort_dir == 'd' {
            ord.reverse()
        } else {
            ord
        }
    });

    let matching_indices: Vec<usize> = items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            let matches_keyword = match &args.keyword {
                Some(k) => item.name.to_lowercase().contains(k.as_str()),
                None => true,
            };
            if matches_keyword {
                Some(idx)
            } else {
                None
            }
        })
        .collect();

    for &idx in &matching_indices {
        let item = &items[idx];
        if item.is_dir {
            stats.dir_count += 1;
        } else {
            stats.file_count += 1;
            stats.total_size += item.size;
        }
    }

    let max_name_width = matching_indices
        .iter()
        .map(|&i| items[i].name.chars().count())
        .max()
        .unwrap_or(0);

    let detail_cols: Vec<(Vec<String>, usize, bool)> = detail_modes
        .iter()
        .map(|mode| {
            let (vals, is_right_aligned): (Vec<String>, bool) = match mode {
                DisplayMode::Usage => (
                    items.iter().map(|i| format_size(i.size)).collect(),
                    true,
                ),
                DisplayMode::Permissions => (
                    items
                        .iter()
                        .map(|i| format_permissions(i.mode, i.is_dir, i.is_symlink))
                        .collect(),
                    false,
                ),
                DisplayMode::Type => (
                    items.iter().map(|i| i.file_type.clone()).collect(),
                    false,
                ),
                DisplayMode::PermOwner => (
                    items
                        .iter()
                        .map(|i| {
                            format!(
                                "ow:{}",
                                format_rwx(
                                    i.mode & 0o400 != 0,
                                    i.mode & 0o200 != 0,
                                    i.mode & 0o100 != 0
                                )
                            )
                        })
                        .collect(),
                    false,
                ),
                DisplayMode::PermGroup => (
                    items
                        .iter()
                        .map(|i| {
                            format!(
                                "g:{}",
                                format_rwx(
                                    i.mode & 0o040 != 0,
                                    i.mode & 0o020 != 0,
                                    i.mode & 0o010 != 0
                                )
                            )
                        })
                        .collect(),
                    false,
                ),
                DisplayMode::PermOthers => (
                    items
                        .iter()
                        .map(|i| {
                            format!(
                                "ot:{}",
                                format_rwx(
                                    i.mode & 0o004 != 0,
                                    i.mode & 0o002 != 0,
                                    i.mode & 0o001 != 0
                                )
                            )
                        })
                        .collect(),
                    false,
                ),
                DisplayMode::Owner => (
                    items.iter().map(|i| get_owner_group(i.uid, i.gid)).collect(),
                    false,
                ),
                DisplayMode::Created => (
                    items.iter().map(|i| format_datetime(i.created)).collect(),
                    false,
                ),
                DisplayMode::Modified => (
                    items.iter().map(|i| format_datetime(i.modified)).collect(),
                    false,
                ),
            };
            let max_w = matching_indices
                .iter()
                .map(|&i| vals[i].chars().count())
                .max()
                .unwrap_or(0);
            (vals, max_w, is_right_aligned)
        })
        .collect();

    let indent = " ".repeat(depth * 2);

    for (row_idx, item) in items.iter().enumerate() {
        let matches_keyword = matching_indices.contains(&row_idx);

        if matches_keyword {
            let color = get_color(item.is_dir, item.is_symlink, item.mode);
            let name_len = item.name.chars().count();

            let col1_pad = if detail_cols.is_empty() {
                0
            } else {
                max_name_width - name_len
            };

            let mut line = format!("{}{}{}{}{}", indent, color, item.name, COL_RESET, " ".repeat(col1_pad));

            for (col_idx, (vals, max_w, is_right_aligned)) in detail_cols.iter().enumerate() {
                let val = &vals[row_idx];
                let val_len = val.chars().count();
                let pad = max_w - val_len;
                let is_last_col = col_idx == detail_cols.len() - 1;

                line.push_str("  ");

                if *is_right_aligned {
                    line.push_str(&format!("{}{}{}{}", COL_SIZE, " ".repeat(pad), val, COL_RESET));
                } else {
                    let col_pad = if is_last_col { 0 } else { pad };
                    line.push_str(&format!("{}{}{}{}", COL_SIZE, val, COL_RESET, " ".repeat(col_pad)));
                }
            }

            println!("{}", line);
        }

        if args.recursive && item.is_dir && !item.is_symlink && depth < allowed_depth {
            process_directory(&item.full_path, depth + 1, allowed_depth, args, detail_modes, stats);
        }
    }
}

// ==================== MAIN ====================

fn main() {
    let args = parse_args();

    let sort_detail = if args.sort_specified {
        match args.sort_field {
            'c' => Some(DisplayMode::Created),
            'm' => Some(DisplayMode::Modified),
            _ => None,
        }
    } else {
        None
    };

    let show_size = args.show_usage || (args.sort_specified && args.sort_field == 'u');

    let mut detail_modes: Vec<DisplayMode> = Vec::new();
    detail_modes.push(DisplayMode::Type);
    if show_size {
        detail_modes.push(DisplayMode::Usage);
    }
    if args.show_perm_detail {
        detail_modes.push(DisplayMode::PermOwner);
        detail_modes.push(DisplayMode::PermGroup);
        detail_modes.push(DisplayMode::PermOthers);
    } else if args.show_permissions {
        detail_modes.push(DisplayMode::Permissions);
    }
    if args.show_owner {
        detail_modes.push(DisplayMode::Owner);
    }
    if let Some(d) = sort_detail {
        detail_modes.push(d);
    }

    let target_path = Path::new(&args.target);

    if !target_path.exists() {
        eprintln!("error: No such file or directory");
        std::process::exit(1);
    }

    let allowed_depth = if !args.recursive {
        0
    } else {
        match args.max_depth {
            None => usize::MAX,
            Some(n) => {
                if n >= 0 {
                    n as usize
                } else {
                    let tree_max = compute_max_depth(target_path, 0, args.show_hidden);
                    let skip = (-n) as usize;
                    tree_max.saturating_sub(skip)
                }
            }
        }
    };

    let mut stats = SummaryStats {
        dir_count: 0,
        file_count: 0,
        total_size: 0,
    };

    process_directory(target_path, 0, allowed_depth, &args, &detail_modes, &mut stats);

    if args.show_summary {
        let mut summary = format!(
            "{} director{}, {} file{}",
            stats.dir_count,
            if stats.dir_count == 1 { "y" } else { "s" },
            stats.file_count,
            if stats.file_count == 1 { "" } else { "s" }
        );

        if show_size {
            summary.push_str(&format!(", total {} used", format_size(stats.total_size)));
        }

        println!("{}", summary);
    }
}