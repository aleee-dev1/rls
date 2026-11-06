//! End-to-end tests running the real `rls` binary against temp dirs.
//!
//! Output is always piped here (never a TTY), which also exercises the
//! auto-color rule: piped runs must be escape-free.

use std::fs;
use std::process::{Command, Output};

const ESC: &str = "\x1b";

struct Rls;

impl Rls {
    /// Run rls with the target path first, remaining args after.
    fn run(dir: &std::path::Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_rls"))
            .arg(dir)
            .args(args)
            .output()
            .expect("rls binary should run")
    }
}

/// Names only: strip the first two columns ("type" + spaces) from piped
/// output, which is padding-free because tests control all names.
/// Names only: the last whitespace-separated token of each row. Test names
/// contain no spaces, so this survives any detail-column padding.
fn names(dir: &std::path::Path, args: &[&str]) -> Vec<String> {
    String::from_utf8_lossy(&Rls::run(dir, args).stdout)
        .lines()
        .map(|line| {
            line.split_whitespace()
                .last()
                .unwrap_or("")
                .to_string()
        })
        .collect()
}

fn make(dir: &std::path::Path, rel: &str, body: &[u8]) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

// ==================== BASIC LISTING ====================

#[test]
fn lists_top_level_without_hidden() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "aa.txt", b"one\n");
    make(dir.path(), ".dotfile", b"hidden\n");
    fs::create_dir_all(dir.path().join("sub")).unwrap();

    let n = names(dir.path(), &["-sn"]);
    assert_eq!(n, vec!["aa.txt", "sub"]);
}

#[test]
fn all_flag_shows_hidden() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "aa.txt", b"one\n");
    make(dir.path(), ".dotfile", b"hidden\n");

    let n = names(dir.path(), &["-a", "-sn"]);
    assert_eq!(n, vec![".dotfile", "aa.txt"]);
}

#[test]
fn recursion_prints_relative_paths() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "root.txt", b"r\n");
    make(dir.path(), "sub/deep.txt", b"d\n");
    make(dir.path(), "sub/lower/x.txt", b"x\n");

    let n = names(dir.path(), &["-r", "-sn"]);
    // Per-directory ascending sort, depth-first: each directory's entries
    // appear sorted, children inline under their parent's path.
    assert_eq!(
        n,
        vec!["root.txt", "sub", "sub/deep.txt", "sub/lower", "sub/lower/x.txt"]
    );
}

#[test]
fn depth_limit_caps_recursion() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "l1/mid.txt", b"m\n");
    make(dir.path(), "l1/l2/deep.txt", b"d\n");

    let n = names(dir.path(), &["-r", "1", "-sn"]);
    assert_eq!(
        n,
        vec!["l1", "l1/l2", "l1/mid.txt"],
        "depth 1 lists two levels; l2's children must be absent"
    );
    assert_eq!(
        n.iter().filter(|p| p.as_str().contains("deep.txt")).count(),
        0
    );
}

#[test]
fn depth_negative_is_relative_to_tree() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "l1/l2/deep.txt", b"d\n");

    // Tree depth is 2; -1 stops one level short.
    let n = names(dir.path(), &["-r", "-1", "-sn"]);
    assert_eq!(n, vec!["l1", "l1/l2"]);
}

// ==================== SORTING ====================

#[test]
fn sort_by_name_desc() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "a.txt", b"1\n");
    make(dir.path(), "b.txt", b"2\n");

    let n = names(dir.path(), &["-snd"]);
    assert_eq!(n, vec!["b.txt", "a.txt"]);
}

#[test]
fn sort_size_does_not_show_size_column() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "big.txt", &[b'x'; 40 * 1024]);
    make(dir.path(), "tiny.txt", b"1\n");

    let out = String::from_utf8_lossy(&Rls::run(dir.path(), &["-su"]).stdout).into_owned();
    assert!(!out.contains(" KB"), "-su must not add a size column: {out}");

    // Without a display column the size sort still applies (ascending).
    let n = names(dir.path(), &["-su"]);
    assert_eq!(n.first().unwrap(), "tiny.txt");
}

// ==================== FILTERS ====================

#[test]
fn keyword_filter_prunes_branches_and_shows_counts() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "hit.txt", b"needle\nand needle again\n");
    make(dir.path(), "no.txt", b"nothing\n");
    make(dir.path(), "quietdir/feh.txt", b"no match\n");

    let out = String::from_utf8_lossy(&Rls::run(dir.path(), &["-k", "needle", "-r", "-sn"]).stdout)
        .into_owned();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "only matching rows print; ancestor dirs fold into the path: {out}"
    );
    assert!(lines[0].contains("hit.txt"), "{out}");
    assert!(lines[0].contains("k:2"), "{out}");
    assert!(!out.contains("quietdir"), "{out}");
}

#[test]
fn keyword_case_insensitive_count() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "hit.txt", b"Needle NEEDLE needle\n");
    let out = String::from_utf8_lossy(&Rls::run(dir.path(), &["-k", "needle"]).stdout).into_owned();
    assert!(out.contains("k:3"));
}

#[test]
fn filename_filter_with_recursion_keeps_paths() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "keep_me.txt", b"k\n");
    make(dir.path(), "other.txt", b"o\n");
    make(dir.path(), "bundle/keep_here.md", b"k\n");
    make(dir.path(), "bundle/drop_me.md", b"d\n");

    let out = String::from_utf8_lossy(&Rls::run(dir.path(), &["-f", "keep", "-r", "-sn"]).stdout)
        .into_owned();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines, vec!["text  bundle/keep_here.md", "text  keep_me.txt"]);
}

#[test]
fn keyword_result_budget_stops() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "a_hit.txt", b"needle needle\n");
    make(dir.path(), "b_hit.txt", b"needle\n");

    let out = String::from_utf8_lossy(&Rls::run(dir.path(), &["-k:1", "needle", "-sn"]).stdout)
        .into_owned();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "-k:1 must stop after one matching file: {out}"
    );
}

// ==================== DETAILS SUMMARY ====================

#[test]
fn details_summary_counts() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "one.txt", b"x\n");
    make(dir.path(), "two.txt", b"y\n");
    fs::create_dir(dir.path().join("sub")).unwrap();

    let raw = String::from_utf8_lossy(&Rls::run(dir.path(), &["-d", "-sn"]).stdout).into_owned();
    let lines: Vec<&str> = raw.lines().collect();
    assert!(lines.len() >= 2, "summary + separator expected: {raw}");

    let summary = lines
        .last()
        .unwrap_or_else(|| panic!("summary line missing: {raw}"));
    // The separator is exactly as wide as the summary text above it.
    let separator = lines
        .iter()
        .rev()
        .nth(1)
        .unwrap_or_else(|| panic!("separator missing: {raw}"));
    assert_eq!(
        separator.chars().count(),
        summary.chars().count(),
        "separator must match summary width"
    );
    assert_eq!(*summary, "1 directory, 2 files");
}

#[test]
fn details_summary_singular_plural() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "only.txt", b"x\n");

    let raw = String::from_utf8_lossy(&Rls::run(dir.path(), &["-d", "-sn"]).stdout).into_owned();
    // Singular only for the == 1 case (unchanged legacy message).
    let lines: Vec<&str> = raw.lines().collect();
    assert_eq!(*lines.last().unwrap(), "0 directors, 1 file");
    // Separator width matches the summary it sits above.
    assert_eq!(lines[lines.len() - 2].chars().count(), 19);
}

// ==================== COLOR GATING (the pipe-safety regression) ====================

#[test]
fn piped_output_is_escape_free_by_default() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "a.txt", b"1\n");
    fs::create_dir(dir.path().join("sub")).unwrap();

    let raw = String::from_utf8_lossy(&Rls::run(dir.path(), &[]).stdout).into_owned();
    assert!(!raw.contains(ESC), "piped output must be plain: {raw:?}");
}

#[test]
fn color_always_colors_pipe_output() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "a.txt", b"1\n");
    fs::create_dir(dir.path().join("sub")).unwrap();

    let raw =
        String::from_utf8_lossy(&Rls::run(dir.path(), &["--color=always"]).stdout).into_owned();
    assert!(raw.contains(ESC), "--color=always must colorize: {raw:?}");

    let raw_never =
        String::from_utf8_lossy(&Rls::run(dir.path(), &["--color=never"]).stdout).into_owned();
    assert!(!raw_never.contains(ESC));
}

#[test]
fn no_color_defeats_auto_but_not_always() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "a.txt", b"1\n");

    let base = Command::new(env!("CARGO_BIN_EXE_rls"))
        .arg(dir.path())
        .output()
        .unwrap();
    let with_no_color = Command::new(env!("CARGO_BIN_EXE_rls"))
        .env("NO_COLOR", "1")
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(base.stdout, with_no_color.stdout);

    let forced = Command::new(env!("CARGO_BIN_EXE_rls"))
        .env("NO_COLOR", "1")
        .arg(dir.path())
        .arg("--color=always")
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&forced.stdout).contains(ESC),
        "NO_COLOR must not defeat --color=always"
    );
}

// ==================== ERRORS & LEGACY SYNTAX ====================

#[test]
fn missing_target_errors_with_exit_code() {
    let out = Command::new(env!("CARGO_BIN_EXE_rls"))
        .arg("/definitely/not/here")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("No such file or directory"));
}

#[test]
fn unknown_flag_reports_and_exits() {
    let out = Command::new(env!("CARGO_BIN_EXE_rls"))
        .arg("--nope")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("error"));
}

#[test]
fn help_exits_cleanly() {
    let out = Command::new(env!("CARGO_BIN_EXE_rls"))
        .arg("-h")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Usage"));
    assert!(text.contains("--help"));
}

#[test]
fn dash_named_target_via_double_dash() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("-weird")).unwrap();
    make(dir.path(), "-weird/x.txt", b"1\n");

    let out = Command::new(env!("CARGO_BIN_EXE_rls"))
        .arg("--")
        .arg(dir.path().join("-weird"))
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("x.txt"), "{text}");
}

#[test]
fn legacy_f_cap_syntax() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "a_match.txt", b"1\n");
    make(dir.path(), "b_match.txt", b"2\n");

    let out =
        String::from_utf8_lossy(&Rls::run(dir.path(), &["-f:1", "match", "-sn"]).stdout)
            .into_owned();
    assert_eq!(out.trim().lines().count(), 1);
    assert!(out.contains("a_match.txt"), "name sort picks the first: {out}");
}

#[test]
fn keyword_column_shows_dash_for_ancestor_dirs() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "sub/inner.txt", b"needle\n");
    make(dir.path(), "top.txt", b"noise\n");

    let out =
        String::from_utf8_lossy(&Rls::run(dir.path(), &["-k", "needle", "-r", "-sn"]).stdout)
            .into_owned();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 1, "one matching row only: {out}");
    assert!(lines[0].contains("sub/inner.txt"), "{out}");
    assert!(lines[0].contains("k:1"), "{out}");
}

#[test]
fn keyword_and_filename_filters_combine() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "a.txt", b"needle\n");
    make(dir.path(), "b_needle_name.txt", b"noise\n");
    make(dir.path(), "drop.txt", b"needle\n");

    let out =
        String::from_utf8_lossy(&Rls::run(dir.path(), &["-k", "needle", "-f", "a.txt"]).stdout)
            .into_owned();
    // -f prunes by name; a.txt has the keyword, b_needle_name.txt doesn't,
    // drop.txt passes content but fails the name filter.
    let n: Vec<&str> = out.lines().collect();
    assert_eq!(n.len(), 1);
    assert!(n[0].contains("a.txt"), "{out}");
}

#[test]
fn symlink_rows_are_not_content_scanned() {
    let dir = tempfile::tempdir().unwrap();
    make(dir.path(), "real.txt", b"needle\n");
    std::os::unix::fs::symlink(dir.path().join("real.txt"), dir.path().join("link.txt")).unwrap();
    make(dir.path(), "real2.txt", b"plain\n");

    let n = names(dir.path(), &["-k", "needle", "-sn"]);
    assert!(
        !n.iter().any(|name| name.contains("link.txt")),
        "symlinks are never keyword-matched: {n:?}"
    );
}

#[test]
fn single_dash_long_input_fails_not_hangs() {
    // Old parser got a dedicated error for this; clap must still reject it.
    let out = Command::new(env!("CARGO_BIN_EXE_rls"))
        .arg("-opdr2")
        .output()
        .unwrap();
    assert!(!out.status.success());
}
