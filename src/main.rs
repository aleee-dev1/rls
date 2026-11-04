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

// Detail column colors used with -c: -r blue, -w red, -x green, directory/link
// marker blue, dashes grey, usage yellow, owner purple, dates grey.
const COL_READ: &str = "\x1b[34m";
const COL_WRITE: &str = "\x1b[31m";
const COL_CHMOD: &str = "\x1b[32m";
const COL_USAGE: &str = "\x1b[33m";
const COL_OWNER: &str = "\x1b[35m";

// ==================== ITEM ====================

#[derive(Clone)]
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

// ==================== DETAIL COLUMN COLORS ====================

/// Wraps a detail value in a single color (used with `-c` off, where all
/// detail columns are plain grey, or simple colored columns).
fn wrap_detail(code: &str, plain: &str) -> String {
    format!("{}{}{}", code, plain, COL_RESET)
}

/// Colors a permission string char by char: r blue, w red, x green,
/// d/l (type marker) blue, dashes grey.
fn colored_permissions(mode: u32, is_dir: bool, is_symlink: bool) -> String {
    let plain = format_permissions(mode, is_dir, is_symlink);
    let mut out = String::new();
    for c in plain.chars() {
        let code = match c {
            'r' => COL_READ,
            'w' => COL_WRITE,
            'x' => COL_CHMOD,
            'd' | 'l' => COL_READ,
            _ => COL_SIZE,
        };
        out.push_str(code);
        out.push(c);
        out.push_str(COL_RESET);
    }
    out
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

// ==================== DIRECTORY USAGE ====================

/// Total usage of a directory subtree under the current filters:
/// - hidden files/dirs (`-a`/`-H`) are included only when they are shown;
/// - with `-k`, only files matching the keyword are counted;
/// - symlinks count as their own size.
/// Plain files simply report their own size.
fn directory_usage(path: &Path, args: &Args) -> u64 {
    let mut total = 0u64;

    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return 0,
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !args.show_hidden && name.starts_with('.') {
            continue;
        }

        let full_path = entry.path();
        let metadata = match fs::symlink_metadata(&full_path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let matches_keyword = match &args.keyword {
            Some(k) => name.to_lowercase().contains(k.as_str()),
            None => true,
        };

        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            // Descend only when the subtree can contribute something.
            if matches_keyword || subtree_contains_match(&full_path, 0, usize::MAX, args) {
                total += directory_usage(&full_path, args);
            }
        } else if matches_keyword {
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
-o, --owner           Show owner and group (user:group)\n    \
-c, --color           Colorize detail columns (r blue, w red, x green,\n                          usage yellow, owner purple, dates grey)\n                          Detail columns appear before names, aligned\n                          across the whole tree, in flag order; tree lines\n                          hang under the parent name's first character\
\n    \
-d, --details         Show directory summary at the end\n    \
-r, --recursive       Recursively list subdirectories (tree view)\n    \
-k <keyword>          Filter files matching keyword (case-insensitive)\n    \
-s<field>[a|d]        Sort by field (n: name, c: created, m: modified, u: size)\n                          Direction: a (ascending, default), d (descending)\n                          Example: -sn, -scd, -su"
    );
}

struct Args {
    target: String,
    show_hidden: bool,
    show_details: bool,
    detail_order: Vec<DisplayMode>,
    sort_field: char,
    sort_dir: char,
    keyword: Option<String>,
    recursive: bool,
    max_depth: Option<i32>,
    colored: bool,
}

fn parse_args() -> Args {
    let mut target: Option<String> = None;
    let mut show_hidden = false;
    let mut show_details = false;
    let mut sort_field = 'c';
    let mut sort_dir = 'a';
    let mut keyword: Option<String> = None;
    let mut recursive = false;
    let mut max_depth: Option<i32> = None;
    let mut colored = false;

    // Detail columns are recorded in the order their flags appear on the
    // command line, so `-pou` renders permissions, owner, usage.
    let mut detail_order: Vec<DisplayMode> = Vec::new();
    macro_rules! push_col {
        ($m:expr) => {
            if !detail_order.contains(&$m) {
                detail_order.push($m);
            }
        };
    }

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
            push_col!(DisplayMode::Usage);
        } else if arg == "-p" || arg == "--permissions" || arg == "--perm" {
            push_col!(DisplayMode::Permissions);
        } else if arg == "-o" || arg == "--owner" {
            push_col!(DisplayMode::Owner);
        } else if arg == "-c" || arg == "--color" || arg == "--colour" {
            colored = true;
        } else if arg == "-d" || arg == "--details" {
            show_details = true;
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
            match sort_field {
                'c' => push_col!(DisplayMode::Created),
                'm' => push_col!(DisplayMode::Modified),
                'u' => push_col!(DisplayMode::Usage),
                _ => {}
            }
        } else if arg.starts_with("-s") {
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
            match sort_field {
                'c' => push_col!(DisplayMode::Created),
                'm' => push_col!(DisplayMode::Modified),
                'u' => push_col!(DisplayMode::Usage),
                _ => {}
            }
        } else if arg.starts_with('-') {
            let rest = &arg[1..];
            let mut valid = true;
            for c in rest.chars() {
                match c {
                    'a' | 'H' => show_hidden = true,
                    'u' => push_col!(DisplayMode::Usage),
                    'p' => push_col!(DisplayMode::Permissions),
                    'o' => push_col!(DisplayMode::Owner),
                    'd' => show_details = true,
                    'r' => recursive = true,
                    'c' => colored = true,
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
        show_details,
        detail_order,
        sort_field,
        sort_dir,
        keyword,
        recursive,
        max_depth,
        colored,
    }
}

// ==================== KEYWORD MATCH ====================

/// Returns true if the subtree at `path` (a directory at `dir_depth`) contains
/// any file whose name matches the keyword. Used to show ancestor directories
/// of matching files so the tree under `-r -k` stays connected.
fn subtree_contains_match(path: &Path, dir_depth: usize, allowed_depth: usize, args: &Args) -> bool {
    let keyword = match &args.keyword {
        Some(k) => k,
        None => return true,
    };

    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return false,
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !args.show_hidden && name.starts_with('.') {
            continue;
        }

        let full_path = entry.path();
        let metadata = match fs::symlink_metadata(&full_path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            if dir_depth < allowed_depth && subtree_contains_match(&full_path, dir_depth + 1, allowed_depth, args) {
                return true;
            }
        } else if name.to_lowercase().contains(keyword) {
            return true;
        }
    }

    false
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
    Owner,
    Created,
    Modified,
}

// ==================== DETAILS STATS ====================

struct DetailsStats {
    dir_count: usize,
    file_count: usize,
    total_size: u64,
}

// ==================== PROCESS DIRECTORY ====================

/// One collected output row: the item itself, the tree prefix to draw inside
/// the name zone, and the plain (uncolored) values of each detail column.
struct Row {
    item: Item,
    tree_prefix: String,
    plains: Vec<String>,
}

/// Plain (uncolored) text value of a detail column for an item. Widths are
/// computed from these.
fn detail_plain(mode: &DisplayMode, item: &Item) -> String {
    match mode {
        DisplayMode::Type => item.file_type.clone(),
        DisplayMode::Usage => format_size(item.size),
        DisplayMode::Permissions => format_permissions(item.mode, item.is_dir, item.is_symlink),
        DisplayMode::Owner => get_owner_group(item.uid, item.gid),
        DisplayMode::Created => format_datetime(item.created),
        DisplayMode::Modified => format_datetime(item.modified),
    }
}

/// Detailed display value of a detail column: grey when `-c` is off, or the
/// per-column `-c` color (usage yellow, owner purple; permissions get their
/// char-by-char coloring; type and dates stay grey).
fn detail_display(mode: &DisplayMode, plain: &str, item: &Item, colored: bool) -> String {
    match mode {
        DisplayMode::Usage => wrap_detail(
            if colored { COL_USAGE } else { COL_SIZE },
            plain,
        ),
        DisplayMode::Permissions => {
            if colored {
                colored_permissions(item.mode, item.is_dir, item.is_symlink)
            } else {
                wrap_detail(COL_SIZE, plain)
            }
        }
        DisplayMode::Owner => wrap_detail(
            if colored { COL_OWNER } else { COL_SIZE },
            plain,
        ),
        _ => wrap_detail(COL_SIZE, plain),
    }
}

fn process_directory(
    path: &Path,
    depth: usize,
    allowed_depth: usize,
    args: &Args,
    detail_modes: &[DisplayMode],
    stats: &mut DetailsStats,
    prefix: &str,
    rows: &mut Vec<Row>,
) {
    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return,
    };

    let need_size = args.detail_order.iter().any(|m| matches!(m, DisplayMode::Usage));
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

        let size = if is_dir && need_size {
            directory_usage(&full_path, args)
        } else if is_dir {
            0
        } else {
            direct_size
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
            let show = match &args.keyword {
                Some(k) => {
                    if item.name.to_lowercase().contains(k) {
                        true
                    } else if args.recursive && item.is_dir && !item.is_symlink {
                        // Keep ancestor directories of matching files so the
                        // tree under `-r -k` stays connected (like `tree -P`).
                        subtree_contains_match(&item.full_path, depth, allowed_depth, args)
                    } else {
                        false
                    }
                }
                None => true,
            };
            if show {
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
            // A directory whose subtree will be listed already contributes its
            // files at deeper levels; adding its own apparent size here would
            // double count. Only unexpanded (or depth-capped) directories
            // contribute their contents to the total.
            let will_recurse = args.recursive && !item.is_symlink && depth < allowed_depth;
            if !will_recurse {
                stats.total_size += directory_usage(&item.full_path, args);
            }
        } else {
            stats.file_count += 1;
            stats.total_size += item.size;
        }
    }

    let last_printed_idx = matching_indices.last().copied();

    for (row_idx, item) in items.iter().enumerate() {
        let matches_keyword = matching_indices.contains(&row_idx);
        let is_last_slot = last_printed_idx == Some(row_idx);
        // Top-level entries hang directly from the target directory, so no
        // connector is drawn for them; only their children get tree lines.
        let connector: &str = if depth == 0 {
            ""
        } else if is_last_slot {
            "└── "
        } else {
            "├── "
        };

        if matches_keyword {
            // Collect the row; columns and tree lines are rendered once all
            // rows are known, with widths shared across the whole tree.
            rows.push(Row {
                item: item.clone(),
                tree_prefix: format!("{}{}", prefix, connector),
                plains: detail_modes.iter().map(|m| detail_plain(m, item)).collect(),
            });
        }

        if args.recursive && matches_keyword && item.is_dir && !item.is_symlink && depth < allowed_depth {
            // The tree lives inside the name zone: a child's connector must
            // land exactly where this item's name begins, so the classic
            // 4-column slot logic applies; no column widths are involved.
            let child_prefix = if depth == 0 {
                prefix.to_string()
            } else if is_last_slot {
                format!("{}    ", prefix)
            } else {
                format!("{}│   ", prefix)
            };
            process_directory(
                &item.full_path,
                depth + 1,
                allowed_depth,
                args,
                detail_modes,
                stats,
                &child_prefix,
                rows,
            );
        }
    }
}

// ==================== MAIN ====================

fn main() {
    let args = parse_args();

    let show_size = args.detail_order.contains(&DisplayMode::Usage);

    // The file type column stays first by default; the remaining columns
    // follow the order in which their flags were given.
    let mut detail_modes: Vec<DisplayMode> = Vec::new();
    detail_modes.push(DisplayMode::Type);
    for mode in &args.detail_order {
        if !detail_modes.contains(mode) {
            detail_modes.push(*mode);
        }
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

    let mut stats = DetailsStats {
        dir_count: 0,
        file_count: 0,
        total_size: 0,
    };

    // Collect every visible row first so detail column widths can be shared
    // across the whole tree, then render.
    let mut rows: Vec<Row> = Vec::new();
    process_directory(target_path, 0, allowed_depth, &args, &detail_modes, &mut stats, "", &mut rows);

    let mut widths: Vec<usize> = vec![0; detail_modes.len()];
    for row in &rows {
        for (ci, plain) in row.plains.iter().enumerate() {
            widths[ci] = widths[ci].max(plain.chars().count());
        }
    }

    for row in &rows {
        let mut line = String::new();
        // Detail columns at line start, in flag order, space separated and
        // padded so they align across the entire listing.
        for (ci, mode) in detail_modes.iter().enumerate() {
            let plain = &row.plains[ci];
            let disp = detail_display(mode, plain, &row.item, args.colored);
            let pad = widths[ci].saturating_sub(plain.chars().count());

            if matches!(mode, DisplayMode::Usage) {
                line.push_str(&" ".repeat(pad));
                line.push_str(&disp);
            } else {
                line.push_str(&disp);
                line.push_str(&" ".repeat(pad));
            }

            if ci != detail_modes.len() - 1 {
                line.push(' ');
            }
        }
        line.push_str("  ");
        // Tree lines live inside the name zone: first-level items start
        // bare, nested ones hang under their parent's name character.
        line.push_str(&row.tree_prefix);
        let color = get_color(row.item.is_dir, row.item.is_symlink, row.item.mode);
        line.push_str(color);
        line.push_str(&row.item.name);
        line.push_str(COL_RESET);

        println!("{}", line);
    }

    if args.show_details {
        let mut details = format!(
            "{} director{}, {} file{}",
            stats.dir_count,
            if stats.dir_count == 1 { "y" } else { "s" },
            stats.file_count,
            if stats.file_count == 1 { "" } else { "s" }
        );

        if show_size {
            details.push_str(&format!(", total {} used", format_size(stats.total_size)));
        }

        // Draw a separator above the details line, exactly as wide as it is.
        println!("{}", "─".repeat(details.chars().count()));
        println!("{}", details);
    }
}