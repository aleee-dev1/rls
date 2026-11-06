# rls

File listing tool for terminal

## Download

### One-command install

```bash
curl -fsSL https://raw.githubusercontent.com/aleee-dev1/rls/main/install.sh | bash
```

The installer automatically detects your OS and architecture, downloads the
appropriate binary, and verifies it against the checksums published with the
release.

### Linux

**x86_64**

```bash
curl -fsSL https://github.com/aleee-dev1/rls/releases/latest/download/rls-linux-x86_64 -o ~/.local/bin/rls
chmod +x ~/.local/bin/rls
```

**ARM64**

```bash
curl -fsSL https://github.com/aleee-dev1/rls/releases/latest/download/rls-linux-aarch64 -o ~/.local/bin/rls
chmod +x ~/.local/bin/rls
```

### macOS

**Apple Silicon**

```bash
curl -fsSL https://github.com/aleee-dev1/rls/releases/latest/download/rls-macos-aarch64 -o ~/.local/bin/rls
chmod +x ~/.local/bin/rls
```

**Intel**

```bash
curl -fsSL https://github.com/aleee-dev1/rls/releases/latest/download/rls-macos-x86_64 -o ~/.local/bin/rls
chmod +x ~/.local/bin/rls
```

### Build from source

```bash
cargo install --git https://github.com/aleee-dev1/rls
```

## Usage

| Command / Args | Description | Notes |
| --- | --- | --- |
| `rls [DIRECTORY]` | List files in the target directory | Defaults to current directory (`.`) if omitted; integer-named targets need `--` (`rls -- 2024`) |
| `-a`, `-H`, `--all`, `--hidden` | Show hidden files and directories | Displays entries starting with `.` |
| `-u`, `--usage`, `--size` | Show size for files and directories | Directory sizes: disk usage of the whole subtree — allocated blocks (not logical size), directory inodes included, and each hard-linked inode counted once per subtree (like `du`) | 
| `-p`, `--permissions`, `--perm` | Display standard file permissions | Formatted as standard 10-character string (e.g., `-rw-r--r--`) |
| `-o`, `--owner` | Show file owner and group | Formatted as `user:group` |
| `-c`, `--dcolor`, `--colour` | Colorize detail columns | `-r` blue, `-w` red, `-x` green, dir/link marker blue, dashes grey, usage yellow, owner purple. Detail columns appear before the name, aligned across the whole listing, in the order their flags were given |
| `--color=<WHEN>` | Colorize the whole listing (names included) | `always`, `never`, or `auto` (default: color only when stdout is a terminal; `NO_COLOR` disables it). Default `rls` piped into another command is always plain text |
| `-d`, `--details` | Show directory summary at the end | Displays total directory count, file count, and aggregated disk usage |
| `-r [DEPTH]`, `--recursive[=DEPTH]` | Recursively list subdirectories | Accepts optional depth integer limit (e.g., `-r 2` or `--recursive=2`): the top level plus that many nested levels are listed, each directory's entries keep their own sort order. Nested entries print as paths relative to the target directory (`folder/child/file.ext`), ready to paste into other commands |
| `-r -N` | Cut from the deepest level | A **negative** depth stops `N` levels above the tree's deepest directory: with a max depth of 4, `-r -1` lists everything inside the level-3-and-up directories |
| `-f <KEYWORD>`, `--filename=<KEYWORD>` | Filter files by filename | Case-insensitive substring match on file and directory names. With `-r`, matches print as relative paths (`folder/file.ext`), so matching files carry their parent folders; branches without matches are pruned. Optional cap: `-f:<N> <KEYWORD>` stops after N matches |
| `-k <KEYWORD>`, `-k:<N> <KEYWORD>` | Search file contents | Looks for the keyword in files case-insensitively; the file is read line by line and skipped when not UTF-8, so binaries disappear quietly. Adds a `k:<count>` detail column with per-file hit counts; matches print as relative paths with `-r`, so no separate parent-folder rows. Optional cap: `-k:<N> <KEYWORD>` stops after N matching files (in output/sort order) |
| `-s<field>[dir]`, `--sort=<field>[dir]` | Sort listings by specified field and order | Fields: `n` (name), `c` (created — falls back to modified when the filesystem has no birth time), `m` (modified), `u` (size). Directions: `a` (ascending, default), `d` (descending). E.g., `-sn`, `-scd`, `-su`. The default order is created-time ascending. Sorting by size (`-su`) computes sizes for the sort only — it does not force the `-u` column on |
| Flag groups | Combine switches freely | Boolean flags group: `-opd` = `-o -p -d`. `-r` may join a group but its depth value must stay separate: `-opd -r 2` (not `-opdr 2`). Value options (`-k`, `-f`, `-s`) always stand alone |
| `-h`, `--help` | Display help menu | Outputs usage guidance and exits immediately |

## Details worth knowing

- **Sizes are disk usage, not apparent size.** `-u` reports allocated blocks
  (512-byte `st_blocks`), so sparse files show what they actually occupy and
  directory totals never double-count a hard-linked inode twice (each
  directory reports the true size of its own subtree). This is what makes
  `rls -u` differ from a plain `ls`.
- **Color is safe in pipes.** `rls` never writes escape codes unless it
  decides to — `--color=always` forces them, `--color=never` strips them, and
  plain `rls` only colors a real terminal (`NO_COLOR` honored). The `-c`
  flag only picks the detail-column palette inside an otherwise colored run.
- **Detail columns follow flag order.** `rls -opu` renders permissions,
  owner, size left-to-right; the type column always leads.
