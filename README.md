# pkg — arch-pkg-helper

> **Vibecoded project** — built with the help of AI coding tools.

One CLI for Arch Linux packages: repositories, the AUR, git repos, install scripts
and local PKGBUILDs. It does not replace `pacman`; it wraps it (and `paru`/`yay`)
and opens a friendly menu when run without arguments.

```sh
curl -fsSL https://raw.githubusercontent.com/culexkmr-cmyk/arch-pkg-helper/main/install.sh | bash
```

## Features

- **Repositories and groups** — install, remove, search, query through `pacman`.
- **AUR** — through `paru` or `yay`; `aur:name` forces the AUR.
- **Git** — clone or update `github.com/user/repo` and other remotes.
- **Scripts** — `.sh` / `.py`, local or by URL, shown for review before running;
  as your user by default, root only with `-s`.
- **Local packages** — a directory with a `PKGBUILD` or a `*.pkg.tar.*` file.
- **Maintenance** — updates, cache cleanup, orphans, lists, `why` / `files` / `owns`.
- **Safe by default** — `-n` shows the plan without touching the system.
- **Self-updating** — `pkg update` keeps pkg itself current too.

## Install

The installer downloads the latest release, verifies its sha256, puts it in
`~/.local/bin` and adds that directory to `PATH` (bash, zsh, fish) if needed.
Options go after `bash -s --`:

```sh
... | bash -s -- --system       # /usr/local/bin for all users
... | bash -s -- --version 1.1.0
... | bash -s -- --uninstall
```

Requirements: Arch Linux or an Arch-based distro (x86_64), `pacman`, `sudo`, `git`;
`pacman-contrib` for updates and cache cleanup; `paru` or `yay` for the AUR.

From source:

```sh
cargo build --release
install -Dm0755 target/release/pkg ~/.local/bin/pkg
```

## Updates

- `pkg self-update` replaces the binary with the latest release (sha256-checked, atomic).
- `pkg update` updates the system and pkg itself.
- The main menu mentions a new version, checked at most once a day
  (`PKG_NO_UPDATE_CHECK=1` turns it off).

## Usage

`pkg` with no arguments opens the main menu:

```text
  ╭──────────────────────────────────────────────────────╮
  │ ▲ pkg  v1.1.0   Arch package helper                  │
  │ 1532 packages · 11 AUR · 2 orphans · 6.7 GiB cache   │
  ╰──────────────────────────────────────────────────────╯

      Packages                  System
   1  Install                6  Update
   2  Remove                 7  Orphans
   3  Search                 8  Clean cache
   4  Package info           9  Status
   5  Lists

   0  exit
```

`0`, `q` or Ctrl+D goes back; `exit` quits from any prompt. Commands without
arguments (`pkg install`, `pkg remove`, `pkg update`, …) open the same flows directly.

### Commands

```text
pkg install <target…>          repo / group / AUR / git / script / PKGBUILD / *.pkg.tar.*
pkg remove <name…>             with unneeded dependencies and configs
pkg update [-n|-y] [repo|aur]  system, AUR and pkg itself
pkg search <text>              repositories and AUR
pkg info <name>                details
pkg list [exp|aur|orph|all]    installed packages
pkg why <name>                 who requires it
pkg files <name>               files of a package
pkg owns <path>                which package owns a file
pkg status                     overview
pkg self-update [-n]           update pkg only
```

### Flags

```text
-n, --dry-run   show the plan, change nothing (install, remove, update)
-y, --yes       skip confirmations
-k, --update    install: git pull an already cloned repository
-s, --sudo      install: run scripts as root
-c, --cascade   remove: also remove packages that depend on the targets
```

Short flags combine (`-ny`); `--` ends flags. Without a terminal pkg never prompts,
prints plain text and returns a non-zero exit code on failure.

### Examples

```sh
pkg install firefox                 # repository
pkg install visual-studio-code-bin  # AUR, detected automatically
pkg install github.com/sharkdp/fd   # git
pkg install https://example.com/setup.sh
pkg install ./mypkg/                # PKGBUILD directory
pkg install -n gnome                # preview a group
pkg remove -c firefox               # with its dependents
pkg update -y repo                  # unattended
```

## Development

```sh
cargo test
cargo build --release
```

To release, bump `version` in `Cargo.toml`, commit and push a matching tag
(`git tag v1.1.0 && git push origin v1.1.0`). The workflow builds the binary on
Arch and publishes it with its checksum and `install.sh`.

## License

Copyright (C) 2026 culex and arch-pkg-helper contributors.

Licensed under the [GNU Affero General Public License v3.0 only](LICENSE)
(`AGPL-3.0-only`).
