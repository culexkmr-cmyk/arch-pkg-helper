//! Self-update from GitHub releases.

use crate::{sys, ui};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const REPO: &str = "culexkmr-cmyk/arch-apkg-helper";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const ASSETS: [&str; 2] = ["apkg-x86_64-linux", "pkg-x86_64-linux"];
const CHECK_EVERY: u64 = 24 * 3600;

fn parse(v: &str) -> Vec<u64> {
    v.trim_start_matches('v')
        .split(['-', '+'])
        .next()
        .unwrap_or("")
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

pub fn newer(latest: &str, current: &str) -> bool {
    let (mut a, mut b) = (parse(latest), parse(current));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a > b
}

/// Latest release version, read from the /releases/latest redirect (no API rate limit, no JSON).
pub fn latest(timeout_secs: u32) -> Result<String, String> {
    let url = format!("https://github.com/{REPO}/releases/latest");
    let t = timeout_secs.to_string();
    let effective = sys::run("curl", &["-sSLI", "-o", "/dev/null", "-w", "%{url_effective}", "--max-time", &t, &url])
        .map_err(|e| format!("cannot reach GitHub: {e}"))?;
    match effective.trim().rsplit_once("/releases/tag/") {
        Some((_, tag)) if !tag.is_empty() => Ok(tag.trim_start_matches('v').to_string()),
        _ => Err("no releases published yet".into()),
    }
}

/// Newer version, if any. Network errors count as "no update".
pub fn available(timeout_secs: u32) -> Option<String> {
    let v = latest(timeout_secs).ok()?;
    remember(&v);
    newer(&v, VERSION).then_some(v)
}

fn cache_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("apkg").join("update-check"))
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn remember(version: &str) {
    if let Some(f) = cache_file() {
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(f, format!("{} {version}\n", now()));
    }
}

/// Newer version from a check made at most once a day. Off with APKG_NO_UPDATE_CHECK (or PKG_NO_UPDATE_CHECK).
pub fn notice() -> Option<String> {
    if std::env::var_os("APKG_NO_UPDATE_CHECK").is_some() || std::env::var_os("PKG_NO_UPDATE_CHECK").is_some() {
        return None;
    }
    let cached = cache_file().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|s| {
        let (t, v) = s.trim().split_once(' ')?;
        Some((t.parse::<u64>().ok()?, v.to_string()))
    });
    let version = match cached {
        Some((t, v)) if now().saturating_sub(t) < CHECK_EVERY => v,
        _ => {
            let v = latest(3).unwrap_or_else(|_| VERSION.to_string());
            remember(&v);
            v
        }
    };
    newer(&version, VERSION).then_some(version)
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Result<Self, String> {
        use std::os::unix::fs::DirBuilderExt;
        let dir = std::env::temp_dir().join(format!("apkg-update-{}-{}", std::process::id(), now()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        Ok(Self(dir))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sha256(path: &Path) -> Result<String, String> {
    let out = sys::run("sha256sum", &[&path.to_string_lossy()])?;
    Ok(out.split_whitespace().next().unwrap_or("").to_string())
}

pub fn run(dry_run: bool) -> Result<(), String> {
    if std::env::consts::ARCH != "x86_64" {
        return Err(format!("no prebuilt binary for {}", std::env::consts::ARCH));
    }
    ui::info("checking GitHub for a new apkg…");
    let latest = latest(15)?;
    remember(&latest);
    if !newer(&latest, VERSION) {
        ui::success(&format!("apkg {VERSION} is the latest"));
        return Ok(());
    }
    let exe =
        std::env::current_exe().and_then(|p| p.canonicalize()).map_err(|e| format!("cannot locate own binary: {e}"))?;
    if let Some(owner) = sys::owner_of(&exe.to_string_lossy()) {
        return Err(format!("{owner}: update it through pacman (apkg update)"));
    }
    ui::info(&format!("apkg {VERSION} → {latest}"));
    if dry_run {
        ui::info(&format!("would download v{latest}, verify sha256 and replace {}", exe.display()));
        return Ok(());
    }

    let tmp = TempDir::new()?;
    // Transition fallback: preferred new asset, then the legacy one.
    let mut bin: Option<PathBuf> = None;
    let mut last_error = None;
    for asset in ASSETS {
        let candidate = tmp.0.join(asset);
        let url = format!("https://github.com/{REPO}/releases/download/v{latest}/{asset}");
        if sys::run("curl", &["-fsSL", "--max-time", "300", "-o", &candidate.to_string_lossy(), &url]).is_err() {
            last_error = Some(format!("download failed: {url}"));
            continue;
        }
        let Ok(sums) = sys::run("curl", &["-fsSL", "--max-time", "30", &format!("{url}.sha256")]) else {
            last_error = Some(format!("checksum download failed: {url}.sha256"));
            continue;
        };
        let want = sums.split_whitespace().next().unwrap_or("");
        if want.len() != 64 || sha256(&candidate).unwrap_or_default() != want {
            last_error = Some(format!("checksum mismatch for {asset}"));
            continue;
        }
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o755)) {
            last_error = Some(format!("cannot mark {asset} executable: {e}"));
            continue;
        }
        let reported = sys::run(&candidate.to_string_lossy(), &["--version"]).unwrap_or_default();
        if !reported.contains(&latest) {
            last_error = Some(format!("downloaded {asset} reports {:?}, nothing changed", reported.trim()));
            continue;
        }
        bin = Some(candidate);
        break;
    }

    let bin = bin.ok_or_else(|| last_error.unwrap_or_else(|| format!("no usable release asset found for {latest}")))?;
    replace(&bin, &exe)?;
    ui::success(&format!("apkg updated to {latest}"));
    Ok(())
}

fn replace(new: &Path, exe: &Path) -> Result<(), String> {
    let dir = exe.parent().ok_or("own binary has no parent directory")?;
    let staged = dir.join(".apkg.new");
    match std::fs::copy(new, &staged) {
        // rename is atomic and safe while the old binary is running
        Ok(_) => std::fs::rename(&staged, exe).map_err(|e| {
            let _ = std::fs::remove_file(&staged);
            format!("cannot replace {}: {e}", exe.display())
        }),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            ui::info(&format!("{} is not writable, asking for root", dir.display()));
            let code =
                sys::stream_root(&["install", "-m", "0755", &new.to_string_lossy(), &exe.to_string_lossy()], &[]);
            if code == 0 {
                Ok(())
            } else {
                Err(format!("install exited with code {code}"))
            }
        }
        Err(e) => Err(format!("cannot write to {}: {e}", dir.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(newer("5.2.0", "5.1.0"));
        assert!(newer("5.10.0", "5.9.9"));
        assert!(newer("6", "5.9"));
        assert!(!newer("5.1.0", "5.1.0"));
        assert!(!newer("5.1", "5.1.0"));
        assert!(!newer("5.0.9", "5.1.0"));
        assert!(newer("v5.1.1-rc1", "5.1.0"));
    }
}
