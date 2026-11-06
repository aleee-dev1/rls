use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::args::{Args, DisplayMode};
use crate::colors::{COL_OWNER, COL_SIZE, COL_USAGE};
use crate::format::{
    colored_permissions, format_permissions, format_size, get_owner_group, wrap_detail,
};
use crate::item::{determine_file_type, Item};
use crate::usage::{
    blocks_bytes, directory_usage, scan_keyword_hits, subtree_contains_match, WalkCache,
};

// ==================== DETAILS STATS ====================

pub struct DetailsStats {
    pub dir_count: usize,
    pub file_count: usize,
    pub total_size: u64,
}

// ==================== PROCESS DIRECTORY ====================

/// One collected output row: the item itself, its path prefix relative to the
/// target directory (empty for top-level entries, `a/b/` for nested ones), and
/// the plain (uncolored) values of each detail column.
pub struct Row {
    pub item: Item,
    pub path_prefix: String,
    pub plains: Vec<String>,
}

/// Plain (uncolored) text value of a detail column for an item. Widths are
/// computed from these.
pub fn detail_plain(mode: &DisplayMode, item: &Item) -> String {
    match mode {
        DisplayMode::Type => item.file_type.clone(),
        DisplayMode::Usage => format_size(item.size),
        DisplayMode::Permissions => format_permissions(item.mode, item.is_dir, item.is_symlink),
        DisplayMode::Owner => get_owner_group(item.uid, item.gid),
        // Count of `-k` content hits; ancestor directories (no content)
        // render as a dash instead of a count.
        DisplayMode::KeywordHits => match item.keyword_hits {
            Some(hits) => format!("k:{}", hits),
            None => "-".to_string(),
        },
    }
}

/// Detailed display value of a detail column. Plain (escape-free) when color
/// is disabled for the stream; when enabled, grey unless `-c` asks for the
/// per-column palette (usage yellow, owner purple; permissions get their
/// char-by-char coloring; type, dates and keyword hits stay grey).
pub fn detail_display(
    mode: &DisplayMode,
    plain: &str,
    item: &Item,
    color_enabled: bool,
    detail_color: bool,
) -> String {
    if !color_enabled {
        return plain.to_string();
    }
    match mode {
        DisplayMode::Usage => wrap_detail(
            if detail_color { COL_USAGE } else { COL_SIZE },
            plain,
        ),
        DisplayMode::Permissions => {
            if detail_color {
                colored_permissions(item.mode, item.is_dir, item.is_symlink)
            } else {
                wrap_detail(COL_SIZE, plain)
            }
        }
        DisplayMode::Owner => wrap_detail(
            if detail_color { COL_OWNER } else { COL_SIZE },
            plain,
        ),
        _ => wrap_detail(COL_SIZE, plain),
    }
}

/// Combined filter evaluation for one entry in output order. Content scans
/// (`-k`) happen lazily here — only evaluated entries get read — and the hit
/// count is stored in `item.keyword_hits` for the k:<count> column.
/// Returns:
/// - `matched`: whether work should happen for the row — both filters pass,
///   or (directories only) the subtree contains matches so recursion must
///   descend to reach them;
/// - `self_match`: whether the entry matches on its own (every file, or a
///   directory matching by name under `-f`). Only `self_match` rows are
///   rendered and consume the result budget; ancestor-connectivity
///   directories are recursed into but not listed.
fn evaluate_match(
    args: &Args,
    item: &mut Item,
    depth: usize,
    allowed_depth: usize,
    cache: &mut WalkCache,
) -> (bool, bool) {
    let connects = args.recursive && item.is_dir && !item.is_symlink;
    let mut matched = true;
    let mut self_match = true;

    // Filename filter (-f): names; directories may also qualify through
    // their subtree under -r.
    if let Some(filename) = &args.filename {
        let name_ok = item.name.to_lowercase().contains(filename);
        let dir_fallback = connects
            && subtree_contains_match(&item.full_path, depth, allowed_depth, args, cache);
        let ok = if item.is_dir { name_ok || dir_fallback } else { name_ok };
        matched &= ok;
        self_match &= name_ok;
    }

    // Content keyword filter (-k): regular files are scanned lazily;
    // directories connect only through their subtree (free of budget); a
    // symlink is never content-scanned, hence never matches by itself.
    if args.keyword.is_some() {
        if item.is_dir {
            let subtree_ok =
                subtree_contains_match(&item.full_path, depth, allowed_depth, args, cache);
            matched &= connects && subtree_ok;
            self_match &= false;
        } else if item.is_symlink {
            matched = false;
            self_match = false;
        } else {
            let hits = scan_keyword_hits(args, &item.full_path, cache);
            item.keyword_hits = hits;
            let ok = hits.is_some_and(|hits| hits > 0);
            matched &= ok;
            self_match &= ok;
        }
    }

    (matched, self_match)
}

// The signature is wide by design: every recursion level shares the run's
// config, caches and accumulators instead of reopening them per level.
#[allow(clippy::too_many_arguments)]
pub fn process_directory(
    path: &Path,
    depth: usize,
    allowed_depth: usize,
    args: &Args,
    detail_modes: &[DisplayMode],
    stats: &mut DetailsStats,
    prefix: &str,
    rows: &mut Vec<Row>,
    cache: &mut WalkCache,
    remaining: &mut usize,
) {
    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => {
            // Only the explicitly requested directory reports failures;
            // permission-error noise from deeper walk levels (that never
            // appear in the listing anyway) would be overwhelming.
            if depth == 0 {
                eprintln!("rls: cannot open '{}'", path.display());
            }
            return;
        }
    };

    // Sizes feed both the usage column and the `-su` sort. Sorting by size
    // must not force the column on, but it still needs every directory's
    // subtree usage computed — hence the second condition.
    let need_size = args.detail_order.iter().any(|m| matches!(m, DisplayMode::Usage))
        || args.sort_field == 'u';
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
        // Opening a probe file must be reserved to plain regular files
        // (FIFOs etc. can hang `open`/`read` forever), and shebang scanning
        // is about *runnable* scripts, so only executable files are opened.
        // Everything else is typed by extension below.
        let mode_pre = metadata.permissions().mode();
        let probe_shebang = !is_symlink && !is_dir && metadata.is_file() && mode_pre & 0o111 != 0;
        let file_type = determine_file_type(&name, is_dir, is_symlink, probe_shebang, &full_path);
        let mode = mode_pre;
        let uid = metadata.uid();
        let gid = metadata.gid();
        // Content keyword (`-k`) scans are deferred to match evaluation: only
        // entries that reach the output (or stop the budget) get read.
        let keyword_hits = None;
        // Filesystems without birth-time support (tmpfs, older ext4, some
        // network filesystems) return Err here; falling back to the mtime
        // beats displaying 01/01/1970.
        let created = metadata.created().or_else(|_| metadata.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        // "Usage" semantics: allocated blocks, not logical size, so sparse
        // files report what they actually occupy on disk. Symlinks are the
        // one exception — their inode size is the target path length, which
        // is what ls-style listings traditionally show and what st_blocks
        // reports as zero anyway.
        let direct_size = if is_symlink { metadata.len() } else { blocks_bytes(metadata.blocks()) };

        let size = if is_dir && need_size {
            directory_usage(&full_path, args, cache)
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
            keyword_hits,
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

    // Single lazy pass: each item is evaluated in sorted order (the output
    // order), which lets `-k`/`-f` result budgets stop work as early as
    // possible. First the count for `-d`, then the row itself, then the
    // recursion — matching the previous stats+row loop split.
    for item in items.iter_mut() {
        // Budget spent: no further rows or recursion anywhere.
        if *remaining == 0 {
            break;
        }

        let (matched, self_match) = evaluate_match(args, item, depth, allowed_depth, cache);

        if matched {
            // Ancestor directories that match only through their subtree are
            // walked but not rendered: entries print as relative paths
            // (`bashes/deauth/radar.json`), so the parent folders are already
            // part of the matched line — a separate dir row would repeat them.
            // Recursion below still descends into them to reach the matches.
            if self_match {
                // Stats part: a directory whose subtree will be listed already
                // contributes its files at deeper levels; adding its own apparent
                // size here would double count. Only unexpanded (or depth-capped)
                // directories contribute their contents to the total. The walk is
                // skipped entirely unless the details summary will be printed and
                // the total needs a size column (otherwise it dominated every
                // run: `rls /home/ali` took ~0.5s walking subtrees that were
                // never reported).
                if item.is_dir {
                    stats.dir_count += 1;
                    let will_recurse = args.recursive && !item.is_symlink && depth < allowed_depth;
                    if !will_recurse && args.show_details && need_size {
                        stats.total_size += directory_usage(&item.full_path, args, cache);
                    }
                } else {
                    stats.file_count += 1;
                    stats.total_size += item.size;
                }

                // Collect the row; columns and paths are rendered once all rows
                // are known, with widths shared across the whole tree. Nested
                // entries carry their path prefix relative to the target
                // directory (e.g. `src/list/row.rs`), so paths can be copied
                // straight into later commands; top-level entries stay bare
                // names.
                rows.push(Row {
                    item: item.clone(),
                    path_prefix: prefix.to_string(),
                    plains: detail_modes.iter().map(|m| detail_plain(m, item)).collect(),
                });

                // Files and by-name matches consume the result budget.
                if args.limit.is_some() {
                    *remaining = remaining.saturating_sub(1);
                }
            }

            if args.recursive && item.is_dir && !item.is_symlink && depth < allowed_depth && *remaining > 0 {
                // Children push the parent onto their path prefix, nesting
                // the whole ancestor chain (`a/b/file.txt`), not glyphs.
                let child_prefix = format!("{}{}/", prefix, item.name);
                process_directory(
                    &item.full_path,
                    depth + 1,
                    allowed_depth,
                    args,
                    detail_modes,
                    stats,
                    &child_prefix,
                    rows,
                    cache,
                    remaining,
                );
            }
        }
    }
}
