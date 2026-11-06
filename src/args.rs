use std::env;

use clap::error::ErrorKind;
use clap::{Arg, ArgAction, ArgMatches, Command, value_parser};

// ==================== DISPLAY MODE & COLOR ====================

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DisplayMode {
    Usage,
    Permissions,
    Type,
    Owner,
    KeywordHits,
}

/// When color reaches the terminal at all.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ColorMode {
    /// Foreground TTY and no NO_COLOR -> color; pipes -> plain.
    Auto,
    /// Force colored output, even into pipes.
    Always,
    /// Strip all escape codes, even on a TTY.
    Never,
}

/// Controls the entire output: names, types, detail columns.
///
/// `detail_color` (the `-c` flag) only switches the detail columns between
/// their per-column palette and plain grey *within* a color-enabled run;
/// when color is disabled everything is plain and `detail_color` is moot.
#[derive(Clone, Copy, PartialEq)]
pub struct ColorSettings {
    pub mode: ColorMode,
    pub detail_color: bool,
}

impl ColorSettings {
    /// Whether escape codes may be written to a stream.
    pub fn enabled(&self, stream_is_tty: bool) -> bool {
        match self.mode {
            ColorMode::Always => true,
            ColorMode::Never => false,
            // NO_COLOR spec: any non-empty value disables color.
            ColorMode::Auto => {
                stream_is_tty && env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
            }
        }
    }
}

pub struct Args {
    pub target: String,
    pub show_hidden: bool,
    pub show_details: bool,
    pub detail_order: Vec<DisplayMode>,
    pub sort_field: char,
    pub sort_dir: char,
    pub keyword: Option<String>,
    pub filename: Option<String>,
    /// `-k:<n>` / `-f:<n>`: stop after this many matched files.
    pub limit: Option<usize>,
    pub recursive: bool,
    pub max_depth: Option<i32>,
    pub color: ColorSettings,
}

// ==================== SORT SPEC ====================

/// Sort field and direction from a single `n|c|m|u` plus an optional `a|d`
/// spec (`-sn`, `-scd`, `--sort=ud`). None when the spec is malformed.
pub fn sort_spec(spec: &str) -> Option<(char, char)> {
    let mut chars = spec.chars();
    let f = chars.next()?;
    if !"ncmu".contains(f) {
        return None;
    }
    let mut dir = 'a';
    if let Some(d) = chars.next() {
        if d != 'a' && d != 'd' {
            return None;
        }
        dir = d;
    }
    Some((f, dir))
}

// ==================== ARGV NORMALIZATION ====================

/// Rewrites the legacy rls CLI shapes into plain clap arguments, so clap can
/// do all the real validation. Handles exactly the forms clap cannot express
/// on its own:
/// - `-r 2` / `-r -1`: a trailing integer after `-r` is the recursion depth,
///   not a directory name (an integer-named target must use `--`, e.g.
///   `rls -- 2`, like ls-style tools).
/// - `--recursive=2`: depth attached to the long flag.
/// - `-k:3` / `-f:2`: legacy per-option result caps.
///
/// Everything else — flag groups (`-opd`), the `--` marker, aliases,
/// unknown-flag errors — is left to clap untouched.
pub fn normalize(argv: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(argv.len());

    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].clone();
        let is_r = arg == "-r" || arg == "--recursive";
        let depth_attached = arg.starts_with("--recursive=");

        if is_r || depth_attached {
            out.push("--recursive".to_string());

            let depth: Option<&str> = if is_r {
                // Only a trailing token that parses as an integer is the
                // depth value; glued with '=' so clap never reads `-1` as
                // another flag of its own.
                argv.get(i + 1).and_then(|next| next.parse::<i32>().ok()).map(|_| argv[i + 1].as_str())
            } else {
                match &arg["--recursive=".len()..] {
                    "" => None,
                    d => Some(d),
                }
            };

            if let Some(d) = depth {
                out.push(format!("--depth={}", d));
                if is_r {
                    i += 1;
                }
            }
            i += 1;
            continue;
        }

        // Legacy per-option result caps: `-k:3 <keyword>` sets both the cap
        // and the keyword itself (the old grammar bundled them into one
        // shorthand token).
        if let Some(n) = arg.strip_prefix("-k:") {
            out.push(format!("--klimit={}", n));
            if !capture_legacy_value(&argv, &mut i, &arg, &mut out, "keyword") {
                continue;
            }
        } else if let Some(n) = arg.strip_prefix("-f:") {
            out.push(format!("--filimit={}", n));
            if !capture_legacy_value(&argv, &mut i, &arg, &mut out, "filename") {
                continue;
            }
        } else {
            out.push(arg);
        }
        i += 1;
    }

    out
}

/// Legacy `-k:3 <keyword>` / `-f:2 <name>`: the token AFTER the cap is the
/// filter value, exactly like the old parser's bare `-k`/`-f`. Returns true
/// when a value was captured (the main loop must skip past it).
fn capture_legacy_value(
    argv: &[String],
    i: &mut usize,
    cap_token: &str,
    out: &mut Vec<String>,
    option: &str,
) -> bool {
    let next = argv.get(*i + 1).cloned();
    match next {
        None => {
            eprintln!("error: option '{}' requires an argument", cap_token);
            std::process::exit(1);
        }
        Some(v) if v.starts_with('-') && v.len() > 1 => {
            eprintln!(
                "error: option '{}' requires an argument ('{}' looks like a flag)",
                cap_token, v
            );
            std::process::exit(1);
        }
        Some(v) => {
            out.push(format!("--{}={}", option, v));
            *i += 1;
            true
        }
    }
}

// ==================== COMMAND ====================

fn command() -> Command {
    Command::new("rls")
        .version(env!("CARGO_PKG_VERSION"))
        .about("A modern file listing utility")
        .arg(
            Arg::new("target")
                .value_name("DIRECTORY")
                .num_args(0..=1)
                .help("Directory to list (default: current directory)"),
        )
        .arg(
            Arg::new("all")
                .short('a')
                .long("all")
                .alias("hidden")
                .help("List hidden files (starting with .)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            // clap maps one short per arg; -H is the second historical
            // spelling of -a and needs its own arg entry.
            Arg::new("all_h")
                .short('H')
                .help("Alias of -a")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("usage")
                .short('u')
                .long("usage")
                .alias("size")
                .help("Show file/directory size")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("permissions")
                .short('p')
                .long("permissions")
                .alias("perm")
                .help("Show permission string (e.g. -rw-r--r--)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("owner")
                .short('o')
                .long("owner")
                .help("Show owner and group (user:group)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("color")
                .long("color")
                .value_name("WHEN")
                .require_equals(true)
                .value_parser(["auto", "always", "never"])
                .default_value("auto")
                .help(
                    "Colorize the whole listing: always, never, or auto (when \
                     stdout is a terminal and NO_COLOR is unset)",
                ),
        )
        .arg(
            Arg::new("dcolor")
                .short('c')
                .long("dcolor")
                .alias("colour")
                .help(
                    "Colorize detail columns (r/w/x blue/red/green, usage \
                     yellow, owner purple, dashes grey)",
                )
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("details")
                .short('d')
                .long("details")
                .help("Show directory summary at the end")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("recursive")
                .short('r')
                .long("recursive")
                .help("Recursively list subdirectories; nested entries print as relative paths")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("depth")
                .long("depth")
                .value_name("N")
                .require_equals(true)
                .value_parser(value_parser!(i32))
                .help("Limit recursion depth with -r (e.g. -r --depth=2); negative values stop N levels above the deepest")
                .hide(true),
        )
        .arg(
            Arg::new("filename")
                .short('f')
                .long("filename")
                .value_name("KEYWORD")
                .help("Filter files by filename (case-insensitive contains)")
                .value_parser(value_parser!(String)),
        )
        .arg(
            Arg::new("filimit")
                .long("filimit")
                .value_name("N")
                .require_equals(true)
                .value_parser(value_parser!(usize))
                .help("With -f, stop after N matches")
                .hide(true),
        )
        .arg(
            Arg::new("keyword")
                .short('k')
                .long("keyword")
                .value_name("KEYWORD")
                .help("Search file contents (case-insensitive); adds a k:<count> column")
                .value_parser(value_parser!(String)),
        )
        .arg(
            Arg::new("klimit")
                .long("klimit")
                .value_name("N")
                .require_equals(true)
                .value_parser(value_parser!(usize))
                .help("With -k, stop after N matches")
                .hide(true),
        )
        .arg(
            Arg::new("sort")
                .short('s')
                .long("sort")
                .value_name("SPEC")
                .help("Sort by field (n name, c created, m modified, u size), direction a|d after the field (default a)"),
        )
        .after_help(
            "FLAG GROUPS:\n    \
            Switch flags combine freely: -opd = -o -p -d\n    \
            -r may join a group, but a depth value must stay separate:\n    \
            -opd -r 2 works, -opdr 2 does not\n\n    \
            Value options (-k, -f, -s) never join a group\n\n    \
            LEGACY SHORT FORMS:\n    \
            -k:3 <keyword>    stop after 3 matching files with -k\n    \
            -f:2 <name>       stop after 2 filename matches with -f\n    \
            -r 2 / -r -1      recursion depth (positive or relative negative)",
        )
}

// ==================== PARSE ====================

/// Parse user args (without argv[0]) into Args, or exit the process on
/// malformed input (error message / help / version). The exit-on-error
/// behavior lives here by design; `sort_spec`, `normalize` and `args_from`
/// are the pure testable units.
pub fn parse_args() -> Args {
    let raw: Vec<String> = env::args().skip(1).collect();
    let raw = normalize(raw);
    args_from(raw)
}

/// Parse user args (without argv[0]) into Args, after `normalize` has run.
/// try_get_matches_from wants the program name in front, so `rls` is fed
/// back in here; behaviorally identical to parse_args for any argv that a
/// plain `env::args().skip(1)` produces.
pub(crate) fn args_from(argv: Vec<String>) -> Args {
    let matches = command()
        .try_get_matches_from(std::iter::once("rls".to_string()).chain(argv.iter().cloned()))
        .unwrap_or_else(|e| {
            let _ = e.print(); // clap already targets stderr and picks a nonzero exit code
            std::process::exit(match e.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => 0,
                _ => 2,
            })
        });

    build_args(&matches, &argv)
}

/// Detail column order follows the command line: the first flag of each
/// kind seen wins its position. clap records no relative flag order across
/// distinct arguments, so scan the raw argv. Value tokens are ignored (they
/// are not flag-shaped), and `--` ends the options.
fn detail_order_from_argv(argv: &[String]) -> Vec<DisplayMode> {
    let mut order: Vec<DisplayMode> = Vec::new();
    let mut push = |m: DisplayMode| {
        if !order.contains(&m) {
            order.push(m);
        }
    };

    for tok in argv {
        if tok == "--" {
            break; // positional target and beyond; no more options
        }
        // `-s<spec>` / `--sort=<spec>`: every character after -s is part of
        // the sort spec, never a flag ('u' in -su is the size field).
        if tok == "-s" || tok.starts_with("-s") && tok.len() > 2 || tok == "--sort" || tok.starts_with("--sort=") {
            continue;
        }
        if let Some(long) = tok.strip_prefix("--") {
            let name = long.split('=').next().unwrap_or(long);
            let mode = match name {
                "usage" | "size" => Some(DisplayMode::Usage),
                "permissions" | "perm" => Some(DisplayMode::Permissions),
                "owner" => Some(DisplayMode::Owner),
                "keyword" => Some(DisplayMode::KeywordHits),
                _ => None,
            };
            if let Some(m) = mode {
                push(m);
            }
        } else if tok.len() > 1 && tok.starts_with('-') {
            for c in tok[1..].chars() {
                match c {
                    'u' => push(DisplayMode::Usage),
                    'p' => push(DisplayMode::Permissions),
                    'o' => push(DisplayMode::Owner),
                    'k' => push(DisplayMode::KeywordHits),
                    _ => {}
                }
            }
        }
    }

    order
}

/// Build Args from clap matches.
fn build_args(matches: &ArgMatches, argv: &[String]) -> Args {
    let target = matches
        .get_one::<String>("target")
        .cloned()
        .unwrap_or_else(|| ".".to_string());

    // Detail columns are recorded in the order their flags appear on the
    // command line, so `-po` renders permissions, owner (see
    // detail_order_from_argv). Type is always first (added in main()).
    // Sorting by size (`-su`) deliberately does NOT add the size column;
    // sizes are still computed for the sort itself (see list.rs).
    let detail_order = detail_order_from_argv(argv);

    let mut sort_field = 'c';
    let mut sort_dir = 'a';
    if let Some(spec) = matches.get_one::<String>("sort") {
        match sort_spec(spec) {
            Some((f, d)) => {
                sort_field = f;
                sort_dir = d;
            }
            None => {
                eprintln!(
                    "error: invalid sort specifier '{}'. Must be a field (n, c, m, u) followed by an optional direction (a, d)",
                    spec
                );
                std::process::exit(1);
            }
        }
    }

    Args {
        target,
        show_hidden: matches.get_flag("all") || matches.get_flag("all_h"),
        show_details: matches.get_flag("details"),
        detail_order,
        sort_field,
        sort_dir,
        keyword: matches.get_one::<String>("keyword").map(|k| k.to_lowercase()),
        filename: matches.get_one::<String>("filename").map(|f| f.to_lowercase()),
        // With both caps given, the keyword cap wins (unchanged legacy
        // behavior; the two caps are for different filters).
        limit: matches
            .get_one::<usize>("klimit")
            .copied()
            .or_else(|| matches.get_one::<usize>("filimit").copied()),
        recursive: matches.get_flag("recursive"),
        max_depth: matches.get_one::<i32>("depth").copied(),
        color: ColorSettings {
            mode: match matches.get_one::<String>("color").map(|s| s.as_str()) {
                Some("always") => ColorMode::Always,
                Some("never") => ColorMode::Never,
                _ => ColorMode::Auto,
            },
            detail_color: matches.get_flag("dcolor"),
        },
    }
}

// ==================== TESTS ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(argv: &[&str]) -> Vec<String> {
        normalize(argv.iter().map(|s| s.to_string()).collect())
    }

    fn parse(argv: &[&str]) -> Args {
        args_from(normalize(argv.iter().map(|s| s.to_string()).collect()))
    }

    // ---- normalize: -r depth grammar ----

    #[test]
    fn normalize_r_depth_positive() {
        assert_eq!(
            norm(&["-r", "2"]),
            vec!["--recursive".to_string(), "--depth=2".to_string()]
        );
    }

    #[test]
    fn normalize_r_depth_negative() {
        assert_eq!(
            norm(&["-r", "-1"]),
            vec!["--recursive".to_string(), "--depth=-1".to_string()]
        );
    }

    #[test]
    fn normalize_r_bare_leaves_flag() {
        assert_eq!(norm(&["-r"]), vec!["--recursive".to_string()]);
    }

    // A non-integer token after -r is a directory, not a depth.
    #[test]
    fn normalize_r_then_target() {
        assert_eq!(
            norm(&["-r", "src"]),
            vec!["--recursive".to_string(), "src".to_string()]
        );
    }

    #[test]
    fn normalize_recursive_eq() {
        assert_eq!(
            norm(&["--recursive=3"]),
            vec!["--recursive".to_string(), "--depth=3".to_string()]
        );
        // Bare `--recursive=` sets the flag with no depth.
        assert_eq!(norm(&["--recursive="]), vec!["--recursive".to_string()]);
    }

    // Glued `-r2` intentionally stays untouched: clap rejects it, matching
    // the old parser's dedicated error for digits inside flag groups.
    #[test]
    fn normalize_group_glue_left_to_clap() {
        assert_eq!(norm(&["-opdr2"]), vec!["-opdr2".to_string()]);
        assert_eq!(norm(&["-opd"]), vec!["-opd".to_string()]);
    }

    // ---- normalize: legacy caps ----

    #[test]
    fn normalize_legacy_k_cap() {
        assert_eq!(
            norm(&["-k:2", "world"]),
            vec!["--klimit=2".to_string(), "--keyword=world".to_string()]
        );
    }

    #[test]
    fn normalize_legacy_f_cap() {
        assert_eq!(
            norm(&["-f:2", "ace"]),
            vec!["--filimit=2".to_string(), "--filename=ace".to_string()]
        );
    }

    // ---- sort_spec ----

    #[test]
    fn spec_field_only() {
        assert_eq!(sort_spec("n"), Some(('n', 'a')));
        assert_eq!(sort_spec("m"), Some(('m', 'a')));
    }

    #[test]
    fn spec_field_and_dir() {
        assert_eq!(sort_spec("cd"), Some(('c', 'd')));
        assert_eq!(sort_spec("ua"), Some(('u', 'a')));
    }

    #[test]
    fn spec_rejects_bad_field_and_dir() {
        assert_eq!(sort_spec("x"), None);
        assert_eq!(sort_spec("nx"), None);
        assert_eq!(sort_spec(""), None);
    }

    // A third character is rejected (the old parser did the same: only
    // field + optional direction).
    #[test]
    fn spec_rejects_extra_chars() {
        assert_eq!(sort_spec("ncd"), None);
    }

    // ---- args_from: defaults and groups ----

    #[test]
    fn defaults() {
        let args = parse(&[]);
        assert_eq!(args.target, ".");
        assert!(!args.show_hidden);
        assert!(!args.show_details);
        assert!(!args.recursive);
        assert_eq!(args.max_depth, None);
        assert_eq!(args.sort_field, 'c');
        assert_eq!(args.sort_dir, 'a');
        assert!(args.detail_order.is_empty());
        assert!(args.keyword.is_none());
        assert!(args.filename.is_none());
        assert!(args.limit.is_none());
        assert_eq!(args.color.mode, ColorMode::Auto);
        assert!(!args.color.detail_color);
    }

    #[test]
    fn target_is_positional() {
        assert_eq!(parse(&["src"]).target, "src");
    }

    #[test]
    fn short_group_expands() {
        let args = parse(&["-opd"]);
        // Order inside the group follows the characters: o then p.
        assert_eq!(
            args.detail_order,
            vec![DisplayMode::Owner, DisplayMode::Permissions]
        );
        assert!(args.show_details);
    }

    // Detail columns keep the order their flags were given, not any fixed
    // priority: `-o -p` must render owner before permissions.
    #[test]
    fn detail_columns_keep_flag_order() {
        let args = parse(&["-o", "-p", "-u"]);
        assert_eq!(
            args.detail_order,
            vec![DisplayMode::Owner, DisplayMode::Permissions, DisplayMode::Usage]
        );
    }

    #[test]
    fn keyword_column_activates_with_keyword() {
        let args = parse(&["-k", "hello"]);
        assert!(args.keyword.as_deref() == Some("hello"));
        assert!(args.detail_order.contains(&DisplayMode::KeywordHits));
    }

    // Sorting by a size field no longer forces the size column on (-su);
    // the field itself is still the sort key.
    #[test]
    fn sort_size_does_not_add_column() {
        let args = parse(&["-su"]);
        assert_eq!(args.sort_field, 'u');
        assert!(args.detail_order.is_empty());

        let args_m = parse(&["-sm"]);
        assert_eq!(args_m.sort_field, 'm');
        assert!(args_m.detail_order.is_empty());
    }

    #[test]
    fn depth_from_normalize_shape() {
        let args = parse(&["-r", "2"]);
        assert!(args.recursive);
        assert_eq!(args.max_depth, Some(2));

        let args_neg = parse(&["-r", "-1"]);
        assert_eq!(args_neg.max_depth, Some(-1));
    }

    #[test]
    fn legacy_caps_become_limits() {
        let args = parse(&["-k:2", "world"]);
        assert_eq!(args.keyword.as_deref(), Some("world"));
        assert_eq!(args.limit, Some(2));

        let args_f = parse(&["-f:2", "ace"]);
        assert_eq!(args_f.filename.as_deref(), Some("ace"));
        assert_eq!(args_f.limit, Some(2));
    }

    // Values are lowercased for case-insensitive matching.
    #[test]
    fn keyword_and_filename_lowercased() {
        let args = parse(&["-k", "HeLLo"]);
        assert_eq!(args.keyword.as_deref(), Some("hello"));
        let args_f = parse(&["-f", "NaMe"]);
        assert_eq!(args_f.filename.as_deref(), Some("name"));
    }

    #[test]
    fn color_always_never() {
        assert_eq!(parse(&["--color=always"]).color.mode, ColorMode::Always);
        assert_eq!(parse(&["--color=never"]).color.mode, ColorMode::Never);
        assert_eq!(parse(&["--color=auto"]).color.mode, ColorMode::Auto);
        // A valueless `--color` is rejected by clap (--color=<WHEN> needs the
        // "="); exit-on-error paths are covered by the CLI integration tests.
    }

    #[test]
    fn c_flag_is_detail_color() {
        let args = parse(&["-c"]);
        assert!(args.color.detail_color);
        assert_eq!(args.color.mode, ColorMode::Auto);
    }

    // ---- ColorSettings::enabled ----

    #[test]
    fn color_enabled_always_never() {
        let always = ColorSettings {
            mode: ColorMode::Always,
            detail_color: false,
        };
        let never = ColorSettings {
            mode: ColorMode::Never,
            detail_color: false,
        };
        assert!(always.enabled(false));
        assert!(always.enabled(true));
        assert!(!never.enabled(false));
        assert!(!never.enabled(true));
    }

    // Auto: color only on a foreground TTY; NO_COLOR (any non-empty value)
    // disables it even there. ALWAYS/Never are intentionally unaffected.
    #[test]
    fn color_auto_respects_tty_and_no_color() {
        let auto = ColorSettings {
            mode: ColorMode::Auto,
            detail_color: false,
        };
        assert!(!auto.enabled(false));

        env::remove_var("NO_COLOR");
        assert!(auto.enabled(true));
        env::set_var("NO_COLOR", "1");
        assert!(!auto.enabled(true));
        env::set_var("NO_COLOR", "");
        assert!(auto.enabled(true));
        env::remove_var("NO_COLOR");
    }
}
