//! Thin, typed wrappers around pacman, the AUR helper and the few shell
//! utilities we need.
//!
//! Every child process runs with `LC_ALL=C`: pacman and the AUR helpers
//! translate their field names (`Repository`, `Version`, …), and the parsers
//! below rely on the English layout.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

// ─── process helpers ─────────────────────────────────────────────────────────

/// A command with a predictable, untranslated output.
pub fn command(cmd: &str) -> Command {
    let mut c = Command::new(cmd);
    c.env("LC_ALL", "C").env_remove("LANGUAGE");
    c
}

/// Run a command and capture stdout. Returns `Err` with stderr text on failure.
pub fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = command(cmd).args(args).stdin(Stdio::null()).output().map_err(|e| format!("cannot run {cmd}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { format!("{cmd} exited with {}", out.status) } else { err })
    }
}

/// Run a command and report only whether it succeeded.
pub fn ok(cmd: &str, args: &[&str]) -> bool {
    command(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Stream command output line by line, keeping only lines that contain any of
/// `keep` (empty `keep` means "print everything"). stdin and stderr are
/// inherited so sudo, git and build tools can still prompt.
pub fn stream(cmd: &str, args: &[String], keep: &[&str]) -> i32 {
    stream_in(cmd, args, None, keep)
}

pub fn stream_in(cmd: &str, args: &[String], dir: Option<&Path>, keep: &[&str]) -> i32 {
    let mut c = command(cmd);
    c.args(args).stdout(Stdio::piped()).stderr(Stdio::inherit());
    if let Some(d) = dir {
        c.current_dir(d);
    }
    let mut child = match c.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cannot run {cmd}: {e}");
            return 127;
        }
    };

    if let Some(stdout) = child.stdout.take() {
        let reader = std::io::BufReader::new(stdout);
        for line in reader.split(b'\n').map_while(Result::ok) {
            let text = String::from_utf8_lossy(&line);
            // pacman draws progress with \r; only the last segment is final
            let s = text.rsplit('\r').next().unwrap_or("").trim_end();
            if s.is_empty() {
                continue;
            }
            if keep.is_empty() || keep.iter().any(|k| s.contains(k)) {
                println!("    {s}");
            }
        }
    }

    child.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1)
}

/// Run `args` as root: through sudo unless we already are root.
pub fn stream_root(args: &[&str], keep: &[&str]) -> i32 {
    let full = sudo_args(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    let (cmd, rest) = full.split_first().expect("non-empty command");
    stream(cmd, rest, keep)
}

/// True when the current process runs as root.
pub fn is_root() -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self").map(|m| m.uid() == 0).unwrap_or(false)
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

pub fn which(bin: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                std::fs::metadata(dir.join(bin))
                    .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// The AUR helper to use: `$PKG_AUR_HELPER`, else paru, else yay.
pub fn aur_helper() -> Option<&'static str> {
    static HELPER: OnceLock<Option<&'static str>> = OnceLock::new();
    *HELPER.get_or_init(|| {
        if let Ok(h) = std::env::var("PKG_AUR_HELPER") {
            let h = h.trim();
            if !h.is_empty() && which(h) {
                return Some(Box::leak(h.to_string().into_boxed_str()));
            }
        }
        ["paru", "yay"].into_iter().find(|h| which(h))
    })
}

// ─── package information ─────────────────────────────────────────────────────

#[derive(Default, Clone, Debug)]
pub struct PkgInfo {
    pub name: String,
    pub version: String,
    pub repo: String,
    pub description: String,
    pub installed_size: String,
    pub download_size: String,
    pub url: String,
    pub depends: String,
    pub optional_deps: Vec<String>,
    pub required_by: Vec<String>,
    pub install_date: String,
    pub install_reason: String,
}

impl PkgInfo {
    /// Parse one `Key : value` block as printed by `pacman -Si/-Qi` and the
    /// AUR helpers. Continuation lines (e.g. optional deps) are indented.
    pub fn parse(text: &str) -> Self {
        let mut map: Vec<(String, Vec<String>)> = Vec::new();
        for line in text.lines() {
            if let Some((k, v)) = line.split_once(" : ").filter(|(k, _)| !k.starts_with(' ')) {
                map.push((k.trim().to_string(), vec![v.trim().to_string()]));
            } else if line.starts_with(' ') && !line.trim().is_empty() {
                if let Some((_, values)) = map.last_mut() {
                    values.push(line.trim().to_string());
                }
            }
        }
        let get = |k: &str| -> Vec<String> {
            map.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.iter().filter(|s| s.as_str() != "None").cloned().collect())
                .unwrap_or_default()
        };
        let one = |k: &str| get(k).join(" ");
        Self {
            name: one("Name"),
            version: one("Version"),
            repo: one("Repository"),
            description: one("Description"),
            installed_size: tidy_size(&one("Installed Size")),
            download_size: tidy_size(&one("Download Size")),
            url: one("URL"),
            depends: one("Depends On"),
            optional_deps: get("Optional Deps"),
            required_by: one("Required By").split_whitespace().map(str::to_string).collect(),
            install_date: one("Install Date"),
            install_reason: one("Install Reason"),
        }
    }

    /// Parse several blocks separated by blank lines.
    pub fn parse_many(text: &str) -> Vec<Self> {
        text.split("\n\n").filter(|b| !b.trim().is_empty()).map(Self::parse).filter(|i| !i.name.is_empty()).collect()
    }
}

/// `pacman -Si` / helper `-Si` results are needed several times per run
/// (classification, preview tables), so they are cached per process.
fn cached(kind: &'static str, name: &str, fetch: impl FnOnce() -> Option<PkgInfo>) -> Option<PkgInfo> {
    type Cache = Mutex<HashMap<(&'static str, String), Option<PkgInfo>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap().get(&(kind, name.to_string())) {
        return hit.clone();
    }
    let value = fetch();
    cache.lock().unwrap().insert((kind, name.to_string()), value.clone());
    value
}

// ─── queries ─────────────────────────────────────────────────────────────────

pub fn installed(name: &str) -> bool {
    ok("pacman", &["-Q", name])
}

pub fn repo_info(name: &str) -> Option<PkgInfo> {
    cached("repo", name, || run("pacman", &["-Si", name]).ok().map(|t| PkgInfo::parse(&t)))
}

pub fn in_repo(name: &str) -> bool {
    repo_info(name).is_some()
}

/// Members of a repository package group (`gnome`, `xorg`, …).
pub fn group_members(name: &str) -> Vec<String> {
    run("pacman", &["-Sgq", name])
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

pub fn aur_info(name: &str) -> Option<PkgInfo> {
    let helper = aur_helper()?;
    cached("aur", name, || {
        run(helper, &["-Si", "--aur", name])
            .ok()
            .map(|t| PkgInfo::parse(&t))
            .filter(|i| i.repo == "aur" && !i.name.is_empty())
    })
}

pub fn is_aur(name: &str) -> bool {
    aur_info(name).is_some()
}

pub fn installed_info(name: &str) -> Option<PkgInfo> {
    run("pacman", &["-Qi", name]).ok().map(|t| PkgInfo::parse(&t))
}

/// `pacman -Qi` for many packages in one call.
pub fn installed_infos(names: &[String]) -> HashMap<String, PkgInfo> {
    if names.is_empty() {
        return HashMap::new();
    }
    let mut args = vec!["-Qi"];
    args.extend(names.iter().map(String::as_str));
    // pacman fails if any name is missing but still prints the others
    let out = command("pacman").args(&args).stdin(Stdio::null()).stderr(Stdio::null()).output();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    PkgInfo::parse_many(&text).into_iter().map(|i| (i.name.clone(), i)).collect()
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

pub fn explicit_packages() -> Vec<(String, String)> {
    pairs(&["-Qe"])
}
pub fn aur_packages() -> Vec<(String, String)> {
    pairs(&["-Qm"])
}
pub fn all_packages() -> Vec<(String, String)> {
    pairs(&["-Q"])
}

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

fn non_empty_lines(text: &str) -> Vec<String> {
    text.lines().map(|l| strip_ansi(l).trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// Repo updates. `checkupdates` syncs a private copy of the databases, so the
/// answer is fresh; `pacman -Qu` only knows about the last `-Sy`.
pub fn updates_repo() -> Vec<String> {
    if which("checkupdates") {
        // exit code 2 means "no updates"
        return run("checkupdates", &["--nocolor"]).map(|t| non_empty_lines(&t)).unwrap_or_default();
    }
    run("pacman", &["-Qu"]).map(|t| non_empty_lines(&t)).unwrap_or_default()
}

/// AUR updates as reported by the helper.
pub fn updates_aur() -> Vec<String> {
    let Some(helper) = aur_helper() else {
        return Vec::new();
    };
    run(helper, &["-Qua"]).map(|t| non_empty_lines(&t)).unwrap_or_default()
}

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // skip ESC [ ... <final byte>
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if ('@'..='~').contains(&n) {
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

const CACHE_DIR: &str = "/var/cache/pacman/pkg";

/// Size of the pacman package cache in bytes.
/// pacman prints sizes like "1863.64 KiB"; re-scale them, keep anything unparsable as is.
fn tidy_size(s: &str) -> String {
    let mut it = s.split_whitespace();
    let (Some(num), Some(unit), None) = (it.next(), it.next(), it.next()) else {
        return s.to_string();
    };
    let mul: f64 = match unit {
        "B" => 1.0,
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        _ => return s.to_string(),
    };
    match num.replace(',', ".").parse::<f64>() {
        Ok(n) if n >= 0.0 => crate::ui::human_bytes((n * mul) as u64),
        _ => s.to_string(),
    }
}

pub fn cache_size() -> u64 {
    let Ok(entries) = std::fs::read_dir(CACHE_DIR) else {
        return 0;
    };
    entries.filter_map(|e| e.ok()).filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum()
}

/// Number of package archives (signatures excluded) in the cache.
pub fn cache_archives() -> usize {
    let Ok(entries) = std::fs::read_dir(CACHE_DIR) else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_str().map(|n| n.contains(".pkg.tar.") && !n.ends_with(".sig")).unwrap_or(false))
        .count()
}

/// Free space on `/` as reported by df (human readable).
/// Root filesystem: (used percent, free space like "120G").
pub fn disk_usage() -> Option<(u32, String)> {
    let out = run("df", &["-h", "--output=pcent,avail", "/"]).ok()?;
    let line = out.lines().nth(1)?;
    let mut it = line.split_whitespace();
    let pct = it.next()?.trim_end_matches('%').parse().ok()?;
    Some((pct, it.next()?.to_string()))
}

/// Reverse dependency tree for a package, as drawn by `pactree -r`.
pub fn reverse_tree(name: &str) -> Vec<String> {
    run("pactree", &["-r", name])
        .unwrap_or_default()
        .lines()
        .skip(1)
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
        .collect()
}

/// Every package that directly or indirectly depends on `name`.
pub fn dependents(name: &str) -> Vec<String> {
    run("pactree", &["-ru", name])
        .unwrap_or_default()
        .lines()
        .skip(1)
        .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Files owned by a package.
pub fn package_files(name: &str) -> Vec<String> {
    run("pacman", &["-Qlq", name]).unwrap_or_default().lines().map(str::to_string).collect()
}

pub fn owner_of(path: &str) -> Option<String> {
    run("pacman", &["-Qo", path]).ok().map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SI: &str = "Repository      : aur\n\
Name            : visual-studio-code-bin\n\
Version         : 1.144.1-1\n\
Description     : Visual Studio Code : editor\n\
Optional Deps   : glib2: Needed for move to trash functionality\n                  \
libdbusmenu-glib: Needed for KDE global menu\n\
Required By     : None\n";

    #[test]
    fn parses_info_block() {
        let i = PkgInfo::parse(SI);
        assert_eq!(i.repo, "aur");
        assert_eq!(i.name, "visual-studio-code-bin");
        assert_eq!(i.version, "1.144.1-1");
        assert_eq!(i.description, "Visual Studio Code : editor");
        assert_eq!(i.optional_deps.len(), 2);
        assert!(i.required_by.is_empty());
    }

    #[test]
    fn parses_many_blocks() {
        let text = "Name            : zlib\nRequired By     : bash  curl\nInstalled Size  : 1 MiB\n\n\
Name            : bash\nRequired By     : None\n\n";
        let all = PkgInfo::parse_many(text);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].required_by, ["bash", "curl"]);
        assert_eq!(all[0].installed_size, "1.0 MiB");
        assert!(all[1].required_by.is_empty());
    }

    #[test]
    fn tidies_sizes() {
        assert_eq!(tidy_size("1863.64 KiB"), "1.8 MiB");
        assert_eq!(tidy_size("301,69 MiB"), "301.7 MiB");
        assert_eq!(tidy_size("0.00 B"), "0 B");
        assert_eq!(tidy_size("weird"), "weird");
    }

    #[test]
    fn strips_ansi() {
        assert_eq!(strip_ansi("\x1b[1;35maur/\x1b[0mfoo \x1b[32m1.0\x1b[0m"), "aur/foo 1.0");
    }
}
