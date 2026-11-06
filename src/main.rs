mod args;
mod colors;
mod format;
mod item;
mod list;
mod usage;

use std::io::IsTerminal;
use std::path::Path;

use args::{parse_args, DisplayMode};
use colors::{get_color, COL_DIR, COL_RESET};
use format::format_size;
use list::{detail_display, process_directory, DetailsStats};
use usage::compute_max_depth;

// ==================== MAIN ====================

fn main() {
    // Pipe consumers (e.g. `rls -r huge | head`) close stdin early; the
    // default SIGPIPE disposition must be restored so the write to stdout
    // kills the process instead of panicking with "Broken pipe".
    extern "C" {
        fn signal(signum: i32, handler: usize) -> usize;
    }
    const SIGPIPE: i32 = 13;
    const SIG_DFL: usize = 0;
    unsafe { signal(SIGPIPE, SIG_DFL) };

    let args = parse_args();

    // A single gate decides whether ANY escape code reaches stdout: plain
    // `rls` is clean even mid-pipe unless the user opts in with --color=always.
    let color_enabled = args.color.enabled(std::io::stdout().is_terminal());

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

    // A file target previously produced an empty listing (read_dir fails and
    // the error is swallowed); say so explicitly instead.
    if !target_path.is_dir() {
        eprintln!("error: '{}' is not a directory", args.target);
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
    // across the whole tree, then render. The walk cache makes repeated
    // subtree traversals for sizes and keyword lookahead read each directory
    // only once; `remaining` carries the -k:<n>/-f:<n> result budget.
    let mut rows: Vec<list::Row> = Vec::new();
    let mut cache = usage::WalkCache::new();
    let mut remaining = args.limit.unwrap_or(usize::MAX);
    process_directory(target_path, 0, allowed_depth, &args, &detail_modes, &mut stats, "", &mut rows, &mut cache, &mut remaining);

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
            let disp = detail_display(
                mode,
                plain,
                &row.item,
                color_enabled,
                args.color.detail_color,
            );
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
        // Nested entries print their path relative to the target directory
        // (`src/list/mod.rs`) so it can be copied into later commands. The
        // parent segments of the path are always directories, so they are
        // colored with the directory color, like the name they belong to.
        if !row.path_prefix.is_empty() {
            if color_enabled {
                line.push_str(COL_DIR);
            }
            line.push_str(&row.path_prefix);
            if color_enabled {
                line.push_str(COL_RESET);
            }
        }
        if color_enabled {
            line.push_str(get_color(row.item.is_dir, row.item.is_symlink, row.item.mode));
        }
        line.push_str(&row.item.name);
        if color_enabled {
            line.push_str(COL_RESET);
        }

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
