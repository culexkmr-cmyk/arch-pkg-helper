pub mod domain;
pub mod infra;
pub mod ports;
pub mod service;

pub use domain::{Opts, Source, Target};
pub use service::InstallService;

use crate::sys;
use std::path::Path;

const GIT_HOSTS: [&str; 4] = ["github.com", "gitlab.com", "codeberg.org", "bitbucket.org"];
const SCRIPT_EXT: [&str; 4] = [".sh", ".bash", ".zsh", ".py"];
const RAW_HOSTS: [&str; 2] = ["raw.githubusercontent.com", "gist.githubusercontent.com"];

pub fn resolve(raw: &str) -> Target {
    let t = expand_home(raw.trim());
    let (source, name) = classify_static(&t).unwrap_or_else(|| {
        let source = if sys::in_repo(&t) || !sys::group_members(&t).is_empty() {
            Source::Repo
        } else if sys::is_aur(&t) {
            Source::Aur
        } else {
            Source::Unknown
        };
        (source, t.clone())
    });
    Target { raw: raw.trim().to_string(), name, source }
}

/// Everything that can be decided without asking pacman or the AUR.
fn classify_static(t: &str) -> Option<(Source, String)> {
    let path = Path::new(t);
    if path.is_dir() && path.join("PKGBUILD").is_file() {
        return Some((Source::Pkgbuild, t.to_string()));
    }
    if path.is_file() {
        let file = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
        if file == "PKGBUILD" {
            let dir = path.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
            return Some((Source::Pkgbuild, if dir.is_empty() { ".".into() } else { dir }));
        }
        if is_pkg_archive(file) {
            return Some((Source::PkgFile, t.to_string()));
        }
        if is_script_name(file) || has_shebang(path) {
            return Some((Source::Script, t.to_string()));
        }
    }

    let lower = t.to_lowercase();
    if let Some(rest) = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://")) {
        let rest = rest.split(['?', '#']).next().unwrap_or("");
        let host = rest.split('/').next().unwrap_or("");
        let last = rest.trim_end_matches('/').rsplit('/').next().unwrap_or("");
        if is_pkg_archive(last) {
            return Some((Source::PkgFile, t.to_string()));
        }
        if is_script_name(last) || RAW_HOSTS.contains(&host) {
            return Some((Source::Script, t.to_string()));
        }
        return Some((Source::Git, git_url(t)));
    }
    if ["git@", "ssh://", "git://"].iter().any(|p| lower.starts_with(p)) {
        return Some((Source::Git, t.to_string()));
    }
    if GIT_HOSTS.iter().any(|h| lower.starts_with(&format!("{h}/"))) {
        return Some((Source::Git, git_url(t)));
    }
    if let Some(name) = t.strip_prefix("aur:").or_else(|| t.strip_prefix("aur/")) {
        return Some((Source::Aur, name.to_string()));
    }
    // a path that does not exist must not be looked up as a package name
    if t.starts_with('/') || t.starts_with("./") || t.starts_with("../") {
        return Some((Source::Unknown, t.to_string()));
    }
    None
}

fn expand_home(t: &str) -> String {
    match (t.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => format!("{home}/{rest}"),
        _ if t == "~" => std::env::var("HOME").unwrap_or_else(|_| t.into()),
        _ => t.to_string(),
    }
}

fn is_pkg_archive(name: &str) -> bool {
    name.contains(".pkg.tar") && !name.ends_with(".sig")
}

fn is_script_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    SCRIPT_EXT.iter().any(|e| lower.ends_with(e))
}

fn has_shebang(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 2];
    std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok() && &head == b"#!"
}

/// Clone URL for a git target; `github.com/u/r/tree/main` becomes `https://github.com/u/r`.
pub fn git_url(target: &str) -> String {
    let t = target.trim().trim_end_matches('/');
    if ["git@", "ssh://", "git://"].iter().any(|p| t.starts_with(p)) {
        return t.to_string();
    }
    let (scheme, rest) = t.split_once("://").unwrap_or(("https", t));
    let mut parts: Vec<&str> = rest.split('/').collect();
    if GIT_HOSTS.contains(&parts[0].to_lowercase().as_str()) && parts[0] != "gitlab.com" {
        parts.truncate(3);
    }
    format!("{scheme}://{}", parts.join("/"))
}

/// Default clone destination: `$HOME/<repo-name>`.
pub fn git_dest(url: &str) -> String {
    let name = url.trim_end_matches('/').rsplit(['/', ':']).next().unwrap_or("").trim_end_matches(".git");
    let name = if name.is_empty() { "repo" } else { name };
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    format!("{home}/{name}")
}

pub fn install_batch(targets: &[String], opts: Opts) -> Result<(), String> {
    InstallService::default().install_batch(targets, opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(t: &str) -> Option<Source> {
        classify_static(t).map(|(s, _)| s)
    }

    #[test]
    fn classifies_remote_targets() {
        assert_eq!(src("https://example.com/setup.sh"), Some(Source::Script));
        assert_eq!(src("https://example.com/install.py?x=1"), Some(Source::Script));
        assert_eq!(src("https://raw.githubusercontent.com/u/r/main/install"), Some(Source::Script));
        assert_eq!(src("https://example.com/foo-1-1-x86_64.pkg.tar.zst"), Some(Source::PkgFile));
        assert_eq!(src("https://github.com/sharkdp/fd"), Some(Source::Git));
        assert_eq!(src("github.com/sharkdp/fd"), Some(Source::Git));
        assert_eq!(src("git@github.com:sharkdp/fd.git"), Some(Source::Git));
        assert_eq!(src("./no-such-script.sh"), Some(Source::Unknown));
        assert_eq!(src("firefox"), None);
    }

    #[test]
    fn strips_aur_prefix() {
        assert_eq!(classify_static("aur:yay-bin"), Some((Source::Aur, "yay-bin".into())));
        assert_eq!(classify_static("aur/yay-bin"), Some((Source::Aur, "yay-bin".into())));
    }

    #[test]
    fn git_urls_and_destinations() {
        assert_eq!(git_url("github.com/sharkdp/fd"), "https://github.com/sharkdp/fd");
        assert_eq!(git_url("https://github.com/sharkdp/fd/tree/master/src"), "https://github.com/sharkdp/fd");
        assert_eq!(git_url("https://gitlab.com/group/sub/proj"), "https://gitlab.com/group/sub/proj");
        assert_eq!(git_url("git@github.com:sharkdp/fd.git"), "git@github.com:sharkdp/fd.git");
        assert!(git_dest("git@github.com:sharkdp/fd.git").ends_with("/fd"));
        assert!(git_dest("https://github.com/sharkdp/fd.git/").ends_with("/fd"));
    }

    #[test]
    fn local_pkgbuild_dir() {
        let dir = std::env::temp_dir().join(format!("pkg-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("PKGBUILD"), "pkgname=x\n").unwrap();
        let d = dir.to_string_lossy().to_string();
        assert_eq!(src(&d), Some(Source::Pkgbuild));
        assert_eq!(classify_static(&format!("{d}/PKGBUILD")), Some((Source::Pkgbuild, d.clone())));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
