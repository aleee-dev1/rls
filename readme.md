# rls

File listing tool for terminal

## Download

### One-command install

```bash
curl -fsSL https://raw.githubusercontent.com/aleee-dev1/rls/main/install.sh | bash
```

The installer automatically detects your OS and architecture and downloads the appropriate binary.

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
| `rls [DIRECTORY]` | List files in the target directory | Defaults to current directory (`.`) if omitted |
| `-a`, `-H`, `--all`, `--hidden` | Show hidden files and directories | Displays entries starting with `.` |
| `-u`, `--usage`, `--size` | Show size for files and directories | Directory sizes are calculated as apparent total size recursively |
| `-p`, `--permissions`, `--perm` | Display standard file permissions | Formatted as standard 10-character string (e.g., `-rw-r--r--`) |
| `-pd`, `--perm-detail` | Display detailed permissions breakdown | Displays individual permission components (`ow:rwx g:rx ot:rx`) |
| `-o`, `--owner` | Show file owner and group | Formatted as `user:group` |
| `-d`, `--summary` | Show directory summary at the end | Displays total directory count, file count, and aggregated size |
| `-r [DEPTH]`, `--recursive[=DEPTH]` | Recursively list subdirectories | Accepts optional depth integer limit (e.g., `-r 2` or `--recursive=2`) |
| `-k <KEYWORD>`, `--keyword=<KEYWORD>` | Filter files matching keyword | Case-insensitive substring match on file and directory names |
| `-s<field>[dir]`, `--sort=<field>[dir]` | Sort listings by specified field and order | Fields: `n` (name), `c` (created), `m` (modified), `u` (size). Directions: `a` (ascending, default), `d` (descending). E.g., `-sn`, `-scd`, `-su` |
| `-h`, `--help` | Display help menu | Outputs usage guidance and exits immediately |
