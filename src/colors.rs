// ==================== COLORS ====================

pub const COL_DIR: &str = "\x1b[1;34m";
pub const COL_EXEC: &str = "\x1b[32m";
pub const COL_FILE: &str = "\x1b[0m";
pub const COL_SYMLINK: &str = "\x1b[36m";
pub const COL_SIZE: &str = "\x1b[90m";
pub const COL_RESET: &str = "\x1b[0m";

// Detail column colors used with -c: -r blue, -w red, -x green, directory/link
// marker blue, dashes grey, usage yellow, owner purple, dates grey.
pub const COL_READ: &str = "\x1b[34m";
pub const COL_WRITE: &str = "\x1b[31m";
pub const COL_CHMOD: &str = "\x1b[32m";
pub const COL_USAGE: &str = "\x1b[33m";
pub const COL_OWNER: &str = "\x1b[35m";

// ==================== COLOR ====================

pub fn get_color(is_dir: bool, is_symlink: bool, mode: u32) -> &'static str {
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
