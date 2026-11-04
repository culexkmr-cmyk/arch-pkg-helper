//! Thin, typed wrappers around pacman/paru and the few shell utilities we need.
//!
//! All functions return owned data; failures are represented as `Err(String)`
//! rather than panics so callers can show a proper message.

use std::collections::BTreeMap;
use std::io::BufRead;
use std::io::IsTerminal;
use std::process::{Command, Stdio};

// ─── process helpers ─────────────────────────────────────────────────────────

/// Run a command and capture stdout. Returns `Err` with stderr text on failure.
pub fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run {cmd}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Run a command and report only whether it succeeded.
pub fn ok(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Stream command output line by line, keeping only lines that contain any of
/// `keep` (empty `keep` means "print everything"). stderr is inherited so
/// sudo can still prompt for a password.
pub fn stream(cmd: &str, args: &[String], keep: &[&str]) -> i32 {
    let mut child = match Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cannot run {cmd}: {e}");
            return 1;
        }
    };

    if let Some(stdout) = child.stdout.take() {
        let reader = std::io::BufReader::new(stdout);
        for line in reader.split(b'\n').map_while(Result::ok) {
            let text = String::from_utf8_lossy(&line);
            // pacman draws progress with \r; treat each segment separately
            for segment in text.split('\r') {
                let s = segment.trim_end();
                if s.is_empty() {
                    continue;
                }
                let show = keep.is_empty() || keep.iter().any(|k| s.contains(k));
                if show {
                    println!("    {s}");
                }
            }
        }
    }

    child.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1)
}

/// True when the current process runs as root.
pub fn is_root() -> bool {
    run("id", &["-u"]).map(|s| s.trim() == "0").unwrap_or(false)
}

/// Prefix arguments with `sudo` unless we are already root.
pub fn sudo_args(args: &[String]) -> Vec<String> {
    let mut v = Vec::with_capacity(args.len() + 1);
    if !is_root() {
        v.push("sudo".to_string());
    }
    v.extend(args.iter().cloned());
    v
}

// ─── package information ─────────────────────────────────────────────────────

#[derive(Default, Clone)]
pub struct PkgInfo {
    pub name: String,
    pub version: String,
    pub repo: String,
    pub description: String,
    pub installed_size: String,
    pub download_size: String,
    pub url: String,
    pub depends: String,
    pub optional_deps: String,
    pub required_by: String,
    pub install_date: String,
    pub install_reason: String,
}

impl PkgInfo {
    /// Parse the `Key : value` layout used by `pacman -Si`, `-Qi` and `paru -Si`.
    fn parse(text: &str) -> Self {
        let mut map: BTreeMap<&str, String> = BTreeMap::new();
        for line in text.lines() {
            if let Some((k, v)) = line.split_once(" : ") {
                map.insert(k.trim(), v.trim().to_string());
            }
        }
        let g = |k: &str| map.get(k).cloned().unwrap_or_default();
        Self {
            name: g("Name"),
            version: g("Version"),
            repo: g("Repository"),
            description: g("Description"),
            installed_size: g("Installed Size"),
            download_size: g("Download Size"),
            url: g("URL"),
            depends: g("Depends On"),
            optional_deps: g("Optional Deps"),
            required_by: g("Required By"),
            install_date: g("Install Date"),
            install_reason: g("Install Reason"),
        }
    }
}

// ─── queries ─────────────────────────────────────────────────────────────────

pub fn installed(name: &str) -> bool {
    ok("pacman", &["-Q", name])
}

pub fn in_repo(name: &str) -> bool {
    ok("pacman", &["-Si", name])
}

pub fn has_paru() -> bool {
    ok("sh", &["-c", "command -v paru"]) || which("paru")
}

pub fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(bin);
                candidate.is_file()
            })
        })
        .unwrap_or(false)
}

pub fn is_aur(name: &str) -> bool {
    if !which("paru") {
        return false;
    }
    run("paru", &["-Si", name])
        .map(|t| t.contains("Repository     : aur") || t.contains("Repository : aur"))
        .unwrap_or(false)
}

pub fn repo_info(name: &str) -> Option<PkgInfo> {
    run("pacman", &["-Si", name]).ok().map(|t| PkgInfo::parse(&t))
}

pub fn aur_info(name: &str) -> Option<PkgInfo> {
    run("paru", &["-Si", name]).ok().map(|t| PkgInfo::parse(&t))
}

pub fn installed_info(name: &str) -> Option<PkgInfo> {
    run("pacman", &["-Qi", name]).ok().map(|t| PkgInfo::parse(&t))
}

/// `pacman -Q` style listing: (name, version) pairs.
fn pairs(args: &[&str]) -> Vec<(String, String)> {
    run("pacman", args)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let name = it.next()?.to_string();
            let ver = it.next().unwrap_or("").to_string();
            Some((name, ver))
        })
        .collect()
}

pub fn explicit_packages() -> Vec<(String, String)> { pairs(&["-Qe"]) }
pub fn aur_packages() -> Vec<(String, String)> { pairs(&["-Qm"]) }
pub fn all_packages() -> Vec<(String, String)> { pairs(&["-Q"]) }

pub fn count(args: &[&str]) -> usize {
    run("pacman", args).unwrap_or_default().lines().filter(|l| !l.trim().is_empty()).count()
}

pub fn orphans() -> Vec<String> {
    run("pacman", &["-Qtdq"])
        .unwrap_or_default()
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Repo updates: `pacman -Qu` lines.
pub fn updates_repo() -> Vec<String> {
    run("pacman", &["-Qu"])
        .unwrap_or_default()
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .collect()
}

/// AUR updates: `paru -Qua` lines, with ANSI escapes stripped.
pub fn updates_aur() -> Vec<String> {
    if !which("paru") {
        return Vec::new();
    }
    run("paru", &["-Qua"])
        .unwrap_or_default()
        .lines()
        .map(|l| strip_ansi(l).trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // skip ESC [ ... <alpha>
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&n) = chars.peek() {
                    chars.next();
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Size of the pacman package cache in bytes.
pub fn cache_size() -> u64 {
    let dir = std::path::Path::new("/var/cache/pacman/pkg");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

/// Number of `.pkg.tar.*` archives in the cache.
pub fn cache_archives() -> usize {
    let Ok(entries) = std::fs::read_dir("/var/cache/pacman/pkg") else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.contains(".pkg.tar."))
                .unwrap_or(false)
        })
        .count()
}

/// Free space on `/` as reported by df (human readable).
pub fn disk_free() -> String {
    run("df", &["-h", "/"])
        .ok()
        .and_then(|out| {
            out.lines()
                .nth(1)
                .and_then(|l| l.split_whitespace().nth(3).map(|s| s.to_string()))
        })
        .unwrap_or_else(|| "?".to_string())
}

/// Reverse dependency tree for a package.
pub fn reverse_deps(name: &str) -> Vec<String> {
    run("pactree", &["-r", name])
        .unwrap_or_default()
        .lines()
        .skip(1)
        .map(|l| l.trim_start_matches(['│', '├', '└', '─', ' ']).trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Files owned by a package.
pub fn package_files(name: &str) -> Vec<String> {
    run("pacman", &["-Ql", name])
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once(' ').map(|(_, p)| p.to_string()))
        .collect()
}

pub fn owner_of(path: &str) -> Option<String> {
    run("pacman", &["-Qo", path]).ok().map(|s| s.trim().to_string())
}

/// True when stdin is a terminal (used to decide on interactive flows).
pub fn stdin_is_tty() -> bool {
    std::io::stdin().is_terminal()
}

/// Stream command output while running inside `dir`.
pub fn stream_in(cmd: &str, args: &[String], dir: &std::path::Path) -> i32 {
    let mut child = match Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cannot run {cmd}: {e}");
            return 1;
        }
    };
    if let Some(stdout) = child.stdout.take() {
        let reader = std::io::BufReader::new(stdout);
        for line in reader.split(b'\n').map_while(Result::ok) {
            let text = String::from_utf8_lossy(&line);
            for segment in text.split('\r') {
                let s = segment.trim_end();
                if !s.is_empty() {
                    println!("    {s}");
                }
            }
        }
    }
    child.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1)
}
