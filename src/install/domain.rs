//! Domain types for install flows.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    Repo,
    Aur,
    Git,
    Script,
    Pkgbuild,
    PkgFile,
    Unknown,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Repo => "repository",
            Source::Aur => "AUR",
            Source::Git => "git",
            Source::Script => "script",
            Source::Pkgbuild => "PKGBUILD",
            Source::PkgFile => "package file",
            Source::Unknown => "unknown",
        }
    }
}

/// A resolved install target. `name` is what the installer works with: a
/// package name without the `aur:` prefix, a clone URL, a script location or
/// a PKGBUILD directory.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub raw: String,
    pub name: String,
    pub source: Source,
}

#[derive(Default, Clone, Copy)]
pub struct Opts {
    pub dry_run: bool,
    pub assume_yes: bool,
    pub update_git: bool,
    /// Run install scripts through sudo.
    pub as_root: bool,
}
