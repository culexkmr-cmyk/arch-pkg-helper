# pkg — arch-pkg-helper

`pkg` is a small Arch Linux package manager front-end written in Rust.
It unifies **pacman**, the **AUR**, **git** remotes and **install scripts**
behind one CLI, and drops into a conversational wizard when run without
arguments in a terminal.

## Features

- **Repository packages** — install, remove, query, search via `pacman`.
- **AUR** — AUR helpers shortcuts and foreign-package tracking.
- **Git sources** — clone or update repositories such as `github.com/user/repo`.
- **Install scripts** — download and review `.sh` scripts before running.
- **Local PKGBUILDs** — build and install from a local `PKGBUILD`.
- **Queries** — `info`, `search`, `why` (reverse deps), `files`, `owns`.
- **Maintenance** — `update`, `clean` (cache), `orphans`, `list`.
- **Safety** — `--dry-run` shows the plan without changing the system;
  `--yes` skips confirmations.
- **Interactive mode** — a menu-driven wizard for every operation.

## Requirements

- Arch Linux (or an Arch-based distribution).
- `pacman`, `sudo`, `git` (and an AUR helper for AUR targets).
- A terminal with interactive support for the wizard mode.

## Installation

### Prebuilt binary

A prebuilt, stripped binary is shipped in [`bin/pkg`](bin/pkg):

```sh
sudo install -m 0755 bin/pkg /usr/local/bin/pkg
pkg --version
```

### Build from source

```sh
cargo build --release
sudo install -m 0755 target/release/pkg /usr/local/bin/pkg
```

The release profile optimizes for size (`opt-level = "z"`, LTO, stripping).

## Usage

### Interactive

Running `pkg` with no arguments opens the main menu:

```text
1) Install a package
2) Remove a package
3) Update
4) Search packages
5) Package info
6) Lists
7) Orphans
8) Clean cache
9) Status
```

Each option launches the corresponding conversational flow. Use `0`/cancel or EOF to exit the menu.


```sh
pkg                 # main menu
pkg install         # ask what to install
pkg remove          # choose packages to remove
pkg update          # list available updates
pkg search          # ask for a query
pkg clean           # choose a cache-cleaning mode
pkg orphans         # offer to remove orphans
```

### Scriptable

```sh
pkg install <target…>
pkg remove <name…>
pkg search <text>
pkg list [exp|aur|orph|all]
pkg info <name>
pkg why <name>
pkg files <name>
pkg owns <path>
pkg status
```

### Flags for `install`

```text
-n, --dry-run   show the plan, change nothing
-y, --yes       skip confirmations
-k, --update    git: pull instead of skipping already-cloned repositories
```

### Examples

```sh
pkg install firefox                # repository package
pkg install yay                   # AUR package
pkg install github.com/sharkdp/fd # git source
pkg install aur:helix-bin        # explicit AUR source
pkg install ./install.sh         # local install script
pkg install https://example.com/setup.sh
pkg search ripgrep
pkg info firefox
pkg list aur
pkg orphans
pkg clean
```

## Development

```sh
cargo build
cargo test
cargo build --release
```

Install components are split under `src/install/` (`domain`, `ports`,
`infra`, `service`) to keep sources and installation strategies separate.

## License

MIT. See the package metadata in [`Cargo.toml`](Cargo.toml).
