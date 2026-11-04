#!/usr/bin/env bash
# pkg installer.
#   curl -fsSL https://raw.githubusercontent.com/culexkmr-cmyk/arch-pkg-helper/main/install.sh | bash
#   ... | bash -s -- --system        install to /usr/local/bin for all users
#   ... | bash -s -- --uninstall
set -euo pipefail

REPO="culexkmr-cmyk/arch-pkg-helper"
ASSET="pkg-x86_64-linux"
MARK="# added by pkg installer"

if [ -t 2 ] && [ -z "${NO_COLOR:-}" ]; then
    c() { printf '\033[1;%sm%s\033[0m' "$1" "$2"; }
else
    c() { printf '%s' "$2"; }
fi
say()  { printf '%s %s\n' "$(c 34 ::)" "$*" >&2; }
ok()   { printf '%s  %s\n' "$(c 32 ✓)" "$*" >&2; }
warn() { printf '%s  %s\n' "$(c 33 !)" "$*" >&2; }
die()  { printf '%s  %s\n' "$(c 31 ✗)" "$*" >&2; exit 1; }

usage() {
    cat <<EOF
usage: install.sh [--system] [--version X.Y.Z] [--from FILE] [--uninstall]
  --system      install to /usr/local/bin (needs root) instead of ~/.local/bin
  --version     install this release instead of the latest
  --from FILE   install a local binary instead of downloading
  --uninstall   remove pkg and the PATH lines added by this script
EOF
}

system=0; version=""; from=""; uninstall=0
while [ $# -gt 0 ]; do
    case "$1" in
        --system) system=1 ;;
        --version) version="${2:?--version needs a value}"; version="${version#v}"; shift ;;
        --from) from="${2:?--from needs a file}"; shift ;;
        --uninstall) uninstall=1 ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; die "unknown option: $1" ;;
    esac
    shift
done
[ "$(id -u)" -eq 0 ] && system=1

as_root() { if [ "$(id -u)" -eq 0 ]; then "$@"; else sudo "$@"; fi; }

user_dir="${HOME}/.local/bin"
if [ "$system" -eq 1 ]; then dest_dir="/usr/local/bin"; else dest_dir="$user_dir"; fi
dest="$dest_dir/pkg"

rc_files() {
    local f
    for f in "$HOME/.bashrc" "${ZDOTDIR:-$HOME}/.zshrc"; do [ -f "$f" ] && echo "$f"; done
    case "${SHELL##*/}" in
        bash) [ -f "$HOME/.bashrc" ] || echo "$HOME/.bashrc" ;;
        zsh) [ -f "${ZDOTDIR:-$HOME}/.zshrc" ] || echo "${ZDOTDIR:-$HOME}/.zshrc" ;;
    esac
}

if [ "$uninstall" -eq 1 ]; then
    for f in "$user_dir/pkg" "/usr/local/bin/pkg"; do
        [ -e "$f" ] || continue
        "$f" --version 2>/dev/null | grep -q '^pkg ' || { warn "$f is not this pkg, left alone"; continue; }
        if [ -w "$(dirname "$f")" ]; then rm -f "$f"; else as_root rm -f "$f"; fi
        ok "removed $f"
    done
    for f in $(rc_files); do
        [ -f "$f" ] && grep -q "$MARK" "$f" && sed -i "/$MARK/d" "$f" && ok "cleaned $f"
    done
    rm -f "$HOME/.config/fish/conf.d/pkg.fish"
    rm -rf "${XDG_CACHE_HOME:-$HOME/.cache}/pkg"
    exit 0
fi

[ "$(uname -m)" = "x86_64" ] || die "only x86_64 builds are published; build from source: cargo build --release"
command -v pacman >/dev/null || warn "pacman not found: pkg is made for Arch Linux"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

if [ -n "$from" ]; then
    [ -f "$from" ] || die "no such file: $from"
    cp "$from" "$tmp/$ASSET"
else
    command -v curl >/dev/null || die "curl is required"
    if [ -z "$version" ]; then
        url="$(curl -sSLI -o /dev/null -w '%{url_effective}' --max-time 20 "https://github.com/$REPO/releases/latest")" \
            || die "cannot reach GitHub"
        case "$url" in
            */releases/tag/*) version="${url##*/releases/tag/}"; version="${version#v}" ;;
            *) die "no releases published yet" ;;
        esac
    fi
    base="https://github.com/$REPO/releases/download/v$version/$ASSET"
    say "downloading pkg $version"
    curl -fSL --progress-bar --max-time 300 -o "$tmp/$ASSET" "$base" || die "download failed: $base"
    curl -fsSL --max-time 30 -o "$tmp/$ASSET.sha256" "$base.sha256" || die "checksum download failed"
    (cd "$tmp" && sha256sum -c --status "$ASSET.sha256") || die "checksum mismatch"
    ok "sha256 verified"
fi

chmod 0755 "$tmp/$ASSET"
got="$("$tmp/$ASSET" --version 2>/dev/null)" || die "the binary does not run on this system"
case "$got" in pkg\ *) ;; *) die "unexpected binary: $got" ;; esac

if [ "$system" -eq 1 ]; then
    as_root install -Dm0755 "$tmp/$ASSET" "$dest"
else
    install -Dm0755 "$tmp/$ASSET" "$dest"
fi
ok "installed $got → $dest"

case ":$PATH:" in
    *":$dest_dir:"*) ;;
    *)
        shown="$dest_dir"
        case "$dest_dir" in "$HOME"/*) shown="\$HOME${dest_dir#"$HOME"}" ;; esac
        line="export PATH=\"$shown:\$PATH\" $MARK"
        for f in $(rc_files); do
            grep -qF "$MARK" "$f" 2>/dev/null || { echo "$line" >> "$f"; ok "added $dest_dir to PATH in $f"; }
        done
        if command -v fish >/dev/null; then
            mkdir -p "$HOME/.config/fish/conf.d"
            echo "fish_add_path -g $dest_dir $MARK" > "$HOME/.config/fish/conf.d/pkg.fish"
        fi
        warn "open a new terminal (or run: exec \$SHELL) to use pkg"
        ;;
esac

found="$(command -v pkg || true)"
if [ -n "$found" ] && [ "$found" != "$dest" ] && case ":$PATH:" in *":$dest_dir:"*) true ;; *) false ;; esac; then
    warn "another pkg comes first in PATH: $found"
fi

echo
say "run: pkg  (updates: pkg self-update, or pkg update with the system)"
