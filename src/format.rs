use std::cell::RefCell;
use std::collections::HashMap;

use crate::colors::{COL_CHMOD, COL_READ, COL_RESET, COL_SIZE, COL_WRITE};

// ==================== PERMISSIONS ====================

pub fn format_permissions(mode: u32, is_dir: bool, is_symlink: bool) -> String {
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
pub fn wrap_detail(code: &str, plain: &str) -> String {
    format!("{}{}{}", code, plain, COL_RESET)
}

/// Colors a permission string char by char: r blue, w red, x green,
/// d/l (type marker) blue, dashes grey.
pub fn colored_permissions(mode: u32, is_dir: bool, is_symlink: bool) -> String {
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

thread_local! {
    /// Memoized name lookups. `getpwuid`/`getgrgid` go through NSS, which on
    /// network backends (SSSD/LDAP) can cost a round-trip per file; each
    /// uid/gid is resolved at most once per run.
    static USER_CACHE: RefCell<HashMap<u32, String>> = RefCell::new(HashMap::new());
    static GROUP_CACHE: RefCell<HashMap<u32, String>> = RefCell::new(HashMap::new());
}

pub fn get_owner_group(uid: u32, gid: u32) -> String {
    format!("{}:{}", lookup_user_name(uid), lookup_group_name(gid))
}

fn lookup_user_name(uid: u32) -> String {
    USER_CACHE.with(|cache| {
        if let Some(name) = cache.borrow().get(&uid) {
            return name.clone();
        }

        let name = unsafe {
            let pwd = getpwuid(uid);
            if !pwd.is_null() && !(*pwd).pw_name.is_null() {
                std::ffi::CStr::from_ptr((*pwd).pw_name)
                    .to_string_lossy()
                    .into_owned()
            } else {
                uid.to_string()
            }
        };

        cache.borrow_mut().insert(uid, name.clone());
        name
    })
}

fn lookup_group_name(gid: u32) -> String {
    GROUP_CACHE.with(|cache| {
        if let Some(name) = cache.borrow().get(&gid) {
            return name.clone();
        }

        let name = unsafe {
            let grp = getgrgid(gid);
            if !grp.is_null() && !(*grp).gr_name.is_null() {
                std::ffi::CStr::from_ptr((*grp).gr_name)
                    .to_string_lossy()
                    .into_owned()
            } else {
                gid.to_string()
            }
        };

        cache.borrow_mut().insert(gid, name.clone());
        name
    })
}

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

// ==================== SIZE ====================

/// 1024-based units with ls-style labels ("KB" here is 1024 bytes, like
/// `ls -h` default; `--si`-style 1000-based sizes are a different flag that
/// doesn't exist yet). Used by the `-u` column and the `-d` total.
pub fn format_size(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0usize;

    while size >= 1024.0 && unit < units.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} {}", size, units[unit])
    }
}

// ==================== DATETIME ====================
// Formatted dates disappeared from the output when sorting stopped forcing
// date columns; if a `--date` style column comes back, bring chrono back
// with it and delete this note.

// ==================== TESTS ====================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- format_size (1024-based, ls-style labels) ----

    #[test]
    fn size_bytes_stay_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1), "1 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn size_units_are_1024_based() {
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
        assert_eq!(format_size(1073741824), "1.0 GB");
        assert_eq!(format_size(1_099_511_627_776), "1.0 TB");
    }

    #[test]
    fn size_caps_at_largest_unit() {
        // 1024 TB stays "1024.0 TB" — no PB unit, no infinity loops.
        assert_eq!(format_size(1024 * 1_099_511_627_776), "1024.0 TB");
    }

    // ---- format_permissions ----

    #[test]
    fn perms_plain_file() {
        // 1 type marker + 9 permission bits, all dashes for mode 000.
        assert_eq!(format_permissions(0o644, false, false), "-rw-r--r--");
        assert_eq!(format_permissions(0o000, false, false), "----------");
    }

    #[test]
    fn perms_directory_and_symlink() {
        assert_eq!(format_permissions(0o755, true, false), "drwxr-xr-x");
        // A symlink keeps its own mode bits; only the type marker is 'l'.
        assert_eq!(format_permissions(0o644, false, true), "lrw-r--r--");
    }

    // every bit is rendered independently
    #[test]
    fn perms_set_bits() {
        assert_eq!(format_permissions(0o777, false, false), "-rwxrwxrwx");
        assert_eq!(format_permissions(0o111, false, false), "---x--x--x");
        assert_eq!(format_permissions(0o222, false, false), "--w--w--w-");
        assert_eq!(format_permissions(0o444, false, false), "-r--r--r--");
    }

    // ---- colored_permissions ----

    #[test]
    fn perms_color_wraps_each_char() {
        // 755 exercises every palette entry: type marker blue, r blue,
        // w red, x green, dashes grey.
        let out = colored_permissions(0o755, false, false);
        // 10 characters, each wrapped in color + reset
        assert_eq!(out.matches(COL_RESET).count(), 10);
        assert!(out.contains(COL_READ));
        assert!(out.contains(COL_WRITE));
        assert!(out.contains(COL_CHMOD));
        assert!(out.contains(COL_SIZE));
    }
}
