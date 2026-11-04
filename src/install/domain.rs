//! Domain types for install flows.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    Repo,
    Aur,
    Git,
    Script,
    Pkgbuild,
    Unknown,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Repo => "repo",
            Source::Aur => "AUR",
            Source::Git => "git",
            Source::Script => "script",
            Source::Pkgbuild => "PKGBUILD",
            Source::Unknown => "unknown",
        }
    }
}

#[derive(Default, Clone, Copy)]
pub struct Opts {
    pub dry_run: bool,
    pub assume_yes: bool,
    pub update_git: bool,
}
