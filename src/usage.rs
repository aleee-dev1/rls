use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::args::Args;

// ==================== WALK CACHE ====================

/// Run-wide memoization for recursive walkers. Within a single `rls` run the
/// filters (`-a`, `-k`, `-f`) are constant and entry paths are visited through
/// a unique nesting chain, so caching by path is safe:
/// - `usage`: total size for each directory, computed once instead of once
///   per ancestor that displays it (removes the O(n * depth) behavior of `-u`);
/// - `matches`: filter-lookahead results, keyed by (path, dir_depth,
///   allowed_depth) because the same directory can be probed in different
///   depth contexts; only complete "no match" results are cached (an early
///   `true` return does not examine the whole subtree);
/// - `hits`: exact per-file content hit counts for the `-k` column
///   (`None` = no usable content, e.g. non-UTF-8 or unreadable);
/// - `probe`: cheap binary answer "does this file contain the keyword at
///   all?", kept separate from `hits` because it stops at the first hit and
///   its result must not leak into the exact counts the column needs.
///
/// Note: values are only valid within one run; files can change between runs.
pub struct WalkCache {
    usage: HashMap<PathBuf, u64>,
    matches: HashMap<(PathBuf, usize, usize), bool>,
    hits: HashMap<PathBuf, Option<u64>>,
    probe: HashMap<PathBuf, bool>,
}

impl Default for WalkCache {
    fn default() -> Self {
        Self::new()
    }
}

impl WalkCache {
    pub fn new() -> Self {
        WalkCache {
            usage: HashMap::new(),
            matches: HashMap::new(),
            hits: HashMap::new(),
            probe: HashMap::new(),
        }
    }
}

// ==================== CONTENT SCAN (-k) ====================

/// Case-insensitive count of `keyword` occurrences in a file, decoded
/// strictly as UTF-8 (`None` when the file is unreadable or not UTF-8 — which
/// is how non-UTF-8 files are skipped under `-k`).
///
/// Streams line by line (`BufReader::read_until`), so memory stays bounded
/// by the longest single line instead of the whole file size. Case folding
/// per line keeps full Unicode behavior identical to the previous whole-file
/// `to_lowercase()` — a byte like 0x0A can never appear inside a multi-byte
/// UTF-8 sequence, so line boundaries are always character boundaries.
///
/// Empty `keyword` can never occur through normal parsing, so its behavior
/// is undefined by contract.
fn stream_keyword_hits(path: &Path, keyword: &str, stop_after: Option<u64>) -> Option<u64> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);

    let mut line: Vec<u8> = Vec::with_capacity(4096);
    let mut total: u64 = 0;

    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => {}
            // I/O error mid-file: report just what was counted so far.
            Err(_) => break,
        }

        let text = match std::str::from_utf8(&line) {
            Ok(t) => t,
            // Any invalid byte means "not a UTF-8 file" for the whole file,
            // matching the skip-on-non-UTF-8 behavior.
            Err(_) => return None,
        };

        for _ in text.to_lowercase().matches(keyword) {
            total += 1;
            if let Some(stop) = stop_after {
                if total >= stop {
                    return Some(total);
                }
            }
        }
    }

    Some(total)
}

/// Exact keyword hit count of a regular file for the `k:<count>` column.
/// Counts never have to stop early here: only rows reaching the output ask.
pub fn scan_keyword_hits(args: &Args, path: &Path, cache: &mut WalkCache) -> Option<u64> {
    let keyword = args.keyword.as_ref()?;

    if let Some(&hits) = cache.hits.get(path) {
        return hits;
    }

    let hits = stream_keyword_hits(path, keyword, None);
    cache.hits.insert(path.to_path_buf(), hits);
    hits
}

/// Cheap lookahead: whether a regular file contains the keyword at all.
/// Stops at the first hit; results live in a separate cache slot so the
/// exact counts for the display column are never polluted.
pub fn keyword_matches(args: &Args, path: &Path, cache: &mut WalkCache) -> bool {
    // Callers all gate on args.keyword.is_some(); a None here means the
    // caller simply didn't activate content search.
    let keyword = match args.keyword.as_deref() {
        Some(k) => k,
        None => return false,
    };

    if let Some(&hit) = cache.probe.get(path) {
        return hit;
    }

    let hit = stream_keyword_hits(path, keyword, Some(1)).is_some_and(|n| n > 0);
    cache.probe.insert(path.to_path_buf(), hit);
    hit
}

/// Whether a plain file entry passes both active filters (`-f` filename and
/// `-k` content). Directory callers must not use this — they match through
/// `subtree_contains_match` instead, since a directory itself has no content
/// to scan.
pub fn entry_file_matches(args: &Args, name: &str, path: &Path, cache: &mut WalkCache) -> bool {
    if let Some(filename) = &args.filename {
        if !name.to_lowercase().contains(filename) {
            return false;
        }
    }

    if args.keyword.is_some() && !keyword_matches(args, path, cache) {
        return false;
    }

    true
}

// ==================== DIRECTORY USAGE ====================

/// Disk usage of a directory subtree under the current filters, mirroring the
/// semantics that make `-du` accurate where plain apparent-size sums lie:
/// - regular files count their allocated blocks, not their logical size, so
///   sparse files are reported as the blocks they actually occupy;
/// - a hard-linked file is counted once within each subtree walk (a `(dev,
///   ino)` set is scoped to one walk, so directories listing the same inode
///   still each report their own, correct subtree size);
/// - the directory inodes themselves count their own allocated blocks;
/// - hidden files/dirs are included only when shown;
/// - with `-f`/`-k`, only files passing the filters are counted;
/// - symlinks count their own (trivial) entry length, never their target.
///
/// Results are memoized in `cache`, so a subtree is read once per run no
/// matter how many levels display or count it.
pub fn directory_usage(path: &Path, args: &Args, cache: &mut WalkCache) -> u64 {
    if let Some(&total) = cache.usage.get(path) {
        return total;
    }

    let total = walk_usage(path, args, cache, &mut HashSet::new());
    cache.usage.insert(path.to_path_buf(), total);
    total
}

/// Allocated size of one inode in bytes (512-byte blocks, as reported by
/// st_blocks). This is what "space used" means; logical size and disk size
/// differ for sparse files, and usage should reflect what is on disk. Unix
/// only: WASI has no block information at all and gets no substitute.
#[cfg(unix)]
pub fn blocks_bytes(blocks: u64) -> u64 {
    blocks * 512
}

/// Size basis of a single entry, in bytes: allocated blocks on Unix,
/// the logical size on WASI (the only size its metadata can report; no
/// honest "used" value exists there). The `-u` output column is refused at
/// startup on WASI precisely because of this gap; the value still powers
/// size sorting and internal plumbing, where a logical size is meaningful.
#[cfg(unix)]
pub fn disk_size(metadata: &fs::Metadata) -> u64 {
    blocks_bytes(metadata.blocks())
}

#[cfg(not(unix))]
pub fn disk_size(metadata: &fs::Metadata) -> u64 {
    metadata.len()
}

fn walk_usage(
    path: &Path,
    args: &Args,
    cache: &mut WalkCache,
    counted: &mut HashSet<(u64, u64)>,
) -> u64 {
    let mut total = 0u64;

    // WASI has no stable inode accessors, so the hard-link cache goes unused
    // and would otherwise warn there.
    #[cfg(not(unix))]
    let _ = counted;

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

        let matches_filename = match &args.filename {
            Some(f) => name.to_lowercase().contains(f),
            None => true,
        };

        let is_real_dir = metadata.is_dir() && !metadata.file_type().is_symlink();

        if is_real_dir {
            // The directory inode itself uses blocks on disk.
            total += disk_size(&metadata);
            // Descend only when the subtree can contribute something.
            if matches_filename
                || subtree_contains_match(&full_path, 0, usize::MAX, args, cache)
            {
                total += directory_usage(&full_path, args, cache);
            }
        } else if entry_file_matches(args, &name, &full_path, cache) {
            // Deduplicate hard links within this subtree: the second (and any
            // further) name for an already-counted inode adds nothing.
            #[cfg(unix)]
            if metadata.is_file() && !counted.insert((metadata.dev(), metadata.ino())) {
                continue;
            }
            total += disk_size(&metadata);
        }
    }

    total
}

// ==================== KEYWORD MATCH ====================

/// Returns true if the subtree at `path` (a directory at `dir_depth`) contains
/// any file that passes the active filters (`-f` name match, `-k` content
/// match). Used to show ancestor directories of matching files so the tree
/// under `-r` stays connected.
/// Memoized in `cache` so subtrees probed by an ancestor are not re-read when
/// the recursion reaches them.
pub fn subtree_contains_match(
    path: &Path,
    dir_depth: usize,
    allowed_depth: usize,
    args: &Args,
    cache: &mut WalkCache,
) -> bool {
    if args.filename.is_none() && args.keyword.is_none() {
        return true;
    }

    let key = (path.to_path_buf(), dir_depth, allowed_depth);
    if let Some(&hit) = cache.matches.get(&key) {
        return hit;
    }

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
            if dir_depth < allowed_depth
                && subtree_contains_match(&full_path, dir_depth + 1, allowed_depth, args, cache)
            {
                // Early exit: the enclosing subtree is not fully examined, so
                // nothing about intermediate directories is cached. Child
                // "no match" results computed along the way stay cached.
                return true;
            }
        } else if entry_file_matches(args, &name, &full_path, cache) {
            return true;
        }
    }

    // Whole subtree examined: the "no match" answer is safe to reuse.
    cache.matches.insert(key, false);
    false
}

// ==================== COMPUTE MAX DEPTH ====================

pub fn compute_max_depth(path: &Path, current_depth: usize, show_hidden: bool) -> usize {
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

// ==================== TESTS ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{ColorMode, ColorSettings};
    use std::io::Write;

    /// A minimal Args for walker tests: no detail columns, default sort.
    fn test_args(keyword: Option<&str>, filename: Option<&str>, show_hidden: bool) -> Args {
        Args {
            target: ".".to_string(),
            show_hidden,
            show_details: false,
            detail_order: Vec::new(),
            sort_field: 'n',
            sort_dir: 'a',
            keyword: keyword.map(|s| s.to_string()),
            filename: filename.map(|s| s.to_string()),
            limit: None,
            recursive: false,
            max_depth: None,
            color: ColorSettings {
                mode: ColorMode::Never,
                detail_color: false,
            },
        }
    }

    // ---- stream_keyword_hits (public through scan_keyword_hits) ----

    #[test]
    fn keyword_counts_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f.txt");
        std::fs::write(&file, b"Hello world\nworld NEWS\nWORLD\nbye").unwrap();

        let args = test_args(Some("world"), None, false);
        let hits = scan_keyword_hits(&args, &file, &mut WalkCache::new());
        assert_eq!(hits, Some(3));
    }

    #[test]
    fn keyword_zero_hits_and_missing_keyword() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f.txt");
        std::fs::write(&file, b"nothing matches").unwrap();

        let args = test_args(Some("world"), None, false);
        assert_eq!(scan_keyword_hits(&args, &file, &mut WalkCache::new()), Some(0));

        // No keyword in args: None (the caller's contract).
        let no_kw = test_args(None, None, false);
        assert_eq!(scan_keyword_hits(&no_kw, &file, &mut WalkCache::new()), None);
    }

    #[test]
    fn non_utf8_file_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("bin.dat");
        let mut f = fs::File::create(&file).unwrap();
        f.write_all(&[b'a', 0xff, b'w', b'o', b'r', b'l', b'd']).unwrap();
        drop(f);

        let args = test_args(Some("world"), None, false);
        assert_eq!(scan_keyword_hits(&args, &file, &mut WalkCache::new()), None);
    }

    // ---- keyword_matches (the early-stopping probe) ----

    #[test]
    fn probe_matches_and_misses() {
        let dir = tempfile::tempdir().unwrap();
        let hit = dir.path().join("hit.txt");
        let miss = dir.path().join("miss.txt");
        std::fs::write(&hit, b"prefix world suffix\nsuffix").unwrap();
        std::fs::write(&miss, b"nothing here\nreally nothing").unwrap();

        let args = test_args(Some("world"), None, false);
        let mut cache = WalkCache::new();
        assert!(keyword_matches(&args, &hit, &mut cache));
        assert!(!keyword_matches(&args, &miss, &mut cache));
        // The probe result is cached and must not leak into exact counts.
        assert_eq!(
            scan_keyword_hits(&args, &hit, &mut cache),
            Some(1)
        );
    }

    // ---- directory_usage ----

    fn write_sized(path: &Path, bytes: usize) {
        let mut f = fs::File::create(path).unwrap();
        let chunk = [b'x'; 1024];
        for _ in 0..bytes / 1024 {
            f.write_all(&chunk).unwrap();
        }
        f.flush().unwrap();
    }

    #[test]
    fn usage_counts_hardlinks_once_per_subtree() {
        let dir = tempfile::tempdir().unwrap();
        let t = dir.path();

        // A file comfortably above a few blocks, listed twice via hard link.
        write_sized(&t.join("one.txt"), 40 * 1024);
        fs::hard_link(t.join("one.txt"), t.join("two.txt")).unwrap();

        let mut before = WalkCache::new();
        let args = test_args(None, None, false);
        let usage_one_name = walk_usage(&t.join("sub"), &args, &mut before, &mut HashSet::new());

        // Move the file into a subdir so "before" only measures one name.
        fs::create_dir(t.join("sub")).unwrap();
        fs::rename(t.join("one.txt"), t.join("sub/one.txt")).unwrap();
        let mut cache = WalkCache::new();
        let mut counted = HashSet::new();
        let usage_link1 = walk_usage(&t.join("sub"), &args, &mut cache, &mut counted);
        usage_one_name_marker(usage_one_name);

        fs::hard_link(t.join("sub/one.txt"), t.join("sub/two.txt")).unwrap();
        let mut cache2 = WalkCache::new();
        let usage_link2 = walk_usage(&t.join("sub"), &args, &mut cache2, &mut HashSet::new());

        // The second name adds nothing.
        assert_eq!(usage_link1, usage_link2);
        assert!(usage_link1 > 0);
    }

    fn usage_one_name_marker(_picsum: u64) {}

    #[test]
    fn usage_respects_hidden_filter() {
        let dir = tempfile::tempdir().unwrap();
        let t = dir.path();
        write_sized(&t.join("visible.txt"), 40 * 1024);
        write_sized(&t.join(".hidden.txt"), 40 * 1024);

        let hidden = test_args(None, None, false);
        let all = test_args(None, None, true);
        let u_hidden = directory_usage(t, &hidden, &mut WalkCache::new());
        let u_all = directory_usage(t, &all, &mut WalkCache::new());
        assert!(u_hidden < u_all);
    }

    #[test]
    fn usage_respects_filters() {
        let dir = tempfile::tempdir().unwrap();
        let t = dir.path();
        write_sized(&t.join("keep-me.txt"), 40 * 1024);
        write_sized(&t.join("drop.txt"), 40 * 1024);

        let filtered = test_args(None, Some("keep"), false);
        let usage = directory_usage(t, &filtered, &mut WalkCache::new());
        let all = test_args(None, None, false);
        let both = directory_usage(t, &all, &mut WalkCache::new());

        // One file is excluded; the remaining usage still comfortably
        // includes the kept file's blocks.
        assert!(usage < both);
        assert!(usage > 30_000);
    }

    // Usage with a keyword filter counts only files containing it.
    #[test]
    fn usage_keyword_filter() {
        let dir = tempfile::tempdir().unwrap();
        let t = dir.path();
        std::fs::write(t.join("yes.txt"), b"needle needle needle\n").unwrap();
        write_sized(&t.join("no.txt"), 40 * 1024);

        let filtered = test_args(Some("needle"), None, false);
        let usage = directory_usage(t, &filtered, &mut WalkCache::new());
        let all = test_args(None, None, false);
        assert!(usage < directory_usage(t, &all, &mut WalkCache::new()) / 2);
    }

    // ---- subtree_contains_match ----

    #[test]
    fn subtree_lookahead_finds_and_prunes() {
        let dir = tempfile::tempdir().unwrap();
        let t = dir.path();
        fs::create_dir(t.join("a")).unwrap();
        fs::create_dir(t.join("b")).unwrap();
        std::fs::write(t.join("a/deep.txt"), b"has the needle").unwrap();
        std::fs::write(t.join("b/deep.txt"), b"empty").unwrap();

        let args = test_args(Some("needle"), None, false);
        let mut cache = WalkCache::new();
        assert!(subtree_contains_match(
            &t.join("a"),
            0,
            usize::MAX,
            &args,
            &mut cache
        ));
        assert!(!subtree_contains_match(
            &t.join("b"),
            0,
            usize::MAX,
            &args,
            &mut cache
        ));
        // The "no match" answer for b is cached under the keyword probe;
        // a *fresh* run (new filters = new run in production) re-probes and
        // finds b by name.
        let fargs = test_args(None, Some("deep"), false);
        assert!(subtree_contains_match(
            &t.join("b"),
            0,
            usize::MAX,
            &fargs,
            &mut WalkCache::new()
        ));
    }

    // With no filters active, lookahead always says yes.
    #[test]
    fn subtree_lookahead_no_filters() {
        let args = test_args(None, None, false);
        assert!(subtree_contains_match(
            Path::new("/nonexistent"),
            0,
            usize::MAX,
            &args,
            &mut WalkCache::new()
        ));
    }
}
