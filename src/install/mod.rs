pub mod domain;
pub mod ports;
pub mod infra;
pub mod service;
// Source detection and installation for every supported kind of target:
// official repositories, the AUR, git remotes, install scripts and local
// PKGBUILDs.

use crate::sys;
pub use domain::{Source, Opts};
use crate::ui;
use std::path::Path;


/// Decide where a target comes from.
pub fn classify(target: &str) -> Source {
    let t = target.trim();

    // local paths first
    if t.contains("PKGBUILD") && Path::new(t).exists() {
        return Source::Pkgbuild;
    }
    if t.ends_with(".sh") && Path::new(t).exists() {
        return Source::Script;
    }

    // remote URLs
    if t.starts_with("http://") || t.starts_with("https://") || t.starts_with("git@") {
        let lower = t.to_lowercase();
        if lower.ends_with(".sh") || lower.contains("install.sh") || lower.contains("install.py") {
            return Source::Script;
        }
        return Source::Git;
    }

    // human-friendly git hosts: github.com/user/repo
    for host in ["github.com/", "gitlab.com/", "codeberg.org/", "bitbucket.org/"] {
        if t.starts_with(host) {
            return Source::Git;
        }
    }

    // explicit override
    if let Some(rest) = t.strip_prefix("aur:") {
        let _ = rest;
        return Source::Aur;
    }

    if sys::in_repo(t) {
        return Source::Repo;
    }
    if sys::is_aur(t) {
        return Source::Aur;
    }
    Source::Unknown
}

/// Normalize a git target into a clone URL.
pub fn git_url(target: &str) -> String {
    let t = target.trim();
    if t.starts_with("git@") {
        return t.to_string();
    }
    let with_scheme = if t.starts_with("http") {
        t.to_string()
    } else {
        format!("https://{t}")
    };
    with_scheme.trim_end_matches(".git").to_string()
}

/// Default clone destination: `$HOME/<repo-name>`.
pub fn git_dest(target: &str) -> String {
    let url = git_url(target);
    let name = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("repo")
        .to_string();
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    format!("{home}/{name}")
}

// ─── preview ─────────────────────────────────────────────────────────────────

/// Show what an install would do, without doing it.
pub fn preview(target: &str, source: Source) {
    match source {
        Source::Repo | Source::Aur => {
            let info = if source == Source::Repo {
                sys::repo_info(target)
            } else {
                sys::aur_info(target)
            };
            let kind = if source == Source::Repo { ui::Kind::Repo } else { ui::Kind::Aur };
            let row = match info {
                Some(i) => ui::Row {
                    name: target.to_string(),
                    version: i.version,
                    note: if i.description.is_empty() {
                        source.label().to_string()
                    } else {
                        format!("{} · {}", source.label(), i.description)
                    },
                    kind,
                },
                None => ui::Row {
                    name: target.to_string(),
                    version: String::new(),
                    note: source.label().to_string(),
                    kind,
                },
            };
            ui::table(&[row]);
        }
        Source::Git => {
            let dest = git_dest(target);
            if Path::new(&dest).join(".git").exists() {
                println!("  {} already cloned → {}", ui::pal().ok("✓"), dest);
            } else {
                println!("  {} → {}", git_url(target), ui::pal().bold(&dest));
            }
        }
        Source::Script => {
            println!("  {target}");
            println!("  {}", ui::pal().dim("(will be downloaded and reviewed before running)"));
        }
        Source::Pkgbuild => {
            println!("  {}", ui::pal().dim("build and install from local PKGBUILD"));
        }
        Source::Unknown => {
            ui::warning(&format!("not found in repositories or AUR: {target}"));
        }
    }
}

fn rows_for(names: &[String], source: Source) -> Vec<ui::Row> {
    names
        .iter()
        .map(|n| {
            let info = if source == Source::Repo {
                sys::repo_info(n)
            } else {
                sys::aur_info(n)
            };
            let (version, desc) = info
                .map(|i| (i.version, i.description))
                .unwrap_or_default();
            let kind = if source == Source::Repo { ui::Kind::Repo } else { ui::Kind::Aur };
            ui::Row {
                name: n.clone(),
                version,
                note: desc,
                kind,
            }
        })
        .collect()
}


pub use service::InstallService;

pub fn install(target: &str, opts: Opts) -> Result<(), String> {
    InstallService::default().install(target, opts)
}

pub fn install_batch(targets: &[String], opts: Opts) -> Result<(), String> {
    InstallService::default().install_batch(targets, opts)
}
