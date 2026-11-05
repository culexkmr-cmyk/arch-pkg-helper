# apkg — arch-pkg-helper

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
- **Self-updating** — `apkg update` keeps apkg itself current too.

## Install

The installer downloads the latest release, verifies its sha256, puts it in
`~/.local/bin` and adds that directory to `PATH` (bash, zsh, fish) if needed.
Options go after `bash -s --`:

```sh
... | bash -s -- --system       # /usr/local/bin for all users
... | bash -s -- --version 1.2.0
... | bash -s -- --uninstall
```

Requirements: Arch Linux or an Arch-based distro (x86_64), `pacman`, `sudo`, `git`;
`pacman-contrib` for updates and cache cleanup; `paru` or `yay` for the AUR.

From source:

```sh
cargo build --release
install -Dm0755 target/release/apkg ~/.local/bin/apkg
```

## Updates

- `apkg self-update` replaces the binary with the latest release (sha256-checked, atomic).
- `apkg update` updates the system and apkg itself.
- The main menu mentions a new version, checked at most once a day
  (`APKG_NO_UPDATE_CHECK=1` turns it off).

## Usage

`pkg` with no arguments opens the main menu:

```text
  ╭──────────────────────────────────────────────────────╮
  │ ▲ apkg  v1.2.0   Arch package helper                  │
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
arguments (`apkg install`, `apkg remove`, `apkg update`, …) open the same flows directly.

### Commands

```text
apkg install <target…>          repo / group / AUR / git / script / PKGBUILD / *.pkg.tar.*
apkg remove <name…>             with unneeded dependencies and configs
apkg update [-n|-y] [repo|aur]  system, AUR and apkg itself
apkg search <text>              repositories and AUR
apkg info <name>                details
apkg list [exp|aur|orph|all]    installed packages
apkg why <name>                 who requires it
apkg files <name>               files of a package
apkg owns <path>                which package owns a file
apkg status                     overview
aapkg self-update [-n]           update pkg only
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
apkg install firefox                 # repository
apkg install visual-studio-code-bin  # AUR, detected automatically
apkg install github.com/sharkdp/fd   # git
apkg install https://example.com/setup.sh
apkg install ./mypkg/                # PKGBUILD directory
apkg install -n gnome                # preview a group
apkg remove -c firefox               # with its dependents
apkg update -y repo                  # unattended
```

## Development

```sh
cargo test
cargo build --release
```

To release, bump `version` in `Cargo.toml`, commit and push a matching tag
(`git tag v1.2.0 && git push origin v1.2.0`). The workflow builds the binary on
Arch and publishes it with its checksum and `install.sh`.

## Color themes

`~/.config/apkg/config.conf` selects the active theme:

```conf
theme = dracula
```

Individual theme definitions live in:

```text
~/.config/apkg/themes/<name>.conf
```

For example:

```text
~/.config/apkg/themes/dracula.conf
~/.config/apkg/themes/matugen.conf
```

A theme file contains its colors:

```conf
theme = dracula
repo = 38;5;117
aur = 38;5;214
git = 38;5;141
script = 38;5;209
ok = 38;5;114
warn = 38;5;221
err = 38;5;203
head = 38;5;111
accent = 1;38;5;75
logo = 1;38;5;32
```

On first use, built-in themes are generated into that directory, so every theme
has an editable config. `apkg theme` lists and selects them. Colors in the selected
theme file are applied first, then optional overrides in `config.conf`.

Available themes include: `default`, `dracula`, `nord`, `catppuccin`, `gruvbox`,
`tokyo-night`, `solarized`, `monochrome`, `ansi`, `matugen`.

You can also override the theme with `APKG_THEME=...` or point to another main
file with `APKG_CONFIG=/path/to/config.conf`. `NO_COLOR=1` disables colors.

## License

Copyright (C) 2026 culex and arch-pkg-helper contributors.

Licensed under the [GNU Affero General Public License v3.0 only](LICENSE)
(`AGPL-3.0-only`).
