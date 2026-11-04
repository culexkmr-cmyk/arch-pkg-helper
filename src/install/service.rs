use super::domain::{Opts, Source, Target};
use super::infra::{
    AurInstaller, GitInstaller, PacmanInstaller, PkgFileInstaller, PkgbuildInstaller, Resolver, ScriptInstaller,
};
use super::ports::{Installer, SourceResolver};
use crate::ui;
use std::collections::BTreeMap;

pub struct InstallService {
    resolver: Box<dyn SourceResolver>,
    installers: BTreeMap<Source, Box<dyn Installer>>,
}

impl Default for InstallService {
    fn default() -> Self {
        let mut installers: BTreeMap<Source, Box<dyn Installer>> = BTreeMap::new();
        installers.insert(Source::Repo, Box::new(PacmanInstaller));
        installers.insert(Source::Aur, Box::new(AurInstaller));
        installers.insert(Source::Git, Box::new(GitInstaller));
        installers.insert(Source::Script, Box::new(ScriptInstaller));
        installers.insert(Source::Pkgbuild, Box::new(PkgbuildInstaller));
        installers.insert(Source::PkgFile, Box::new(PkgFileInstaller));
        Self { resolver: Box::new(Resolver), installers }
    }
}

fn header(source: Source) {
    match source {
        Source::Repo => ui::header_repo(source.label()),
        Source::Aur => ui::header_aur(source.label()),
        Source::Git => ui::header_git(source.label()),
        Source::Script => ui::header_script(source.label()),
        _ => ui::header_head(source.label()),
    }
}

impl InstallService {
    pub fn install_batch(&self, targets: &[String], opts: Opts) -> Result<(), String> {
        let mut groups: BTreeMap<Source, Vec<Target>> = BTreeMap::new();
        for t in targets {
            let target = self.resolver.resolve(t);
            let list = groups.entry(target.source).or_default();
            if !list.iter().any(|x| x.name == target.name) {
                list.push(target);
            }
        }

        let unknown = groups.remove(&Source::Unknown).unwrap_or_default();
        for t in &unknown {
            ui::warning(&format!("not found in repositories, AUR or on disk: {}", t.raw));
        }
        if groups.is_empty() {
            return Err("nothing to install".into());
        }

        for (source, list) in &groups {
            header(*source);
            if let Some(installer) = self.installers.get(source) {
                installer.preview(list);
            }
        }
        ui::blank();

        if !opts.dry_run && !opts.assume_yes && !ui::confirm("Proceed?", true) {
            ui::info("cancelled");
            return Ok(());
        }

        let mut failures = 0;
        for (source, list) in &groups {
            if let Some(installer) = self.installers.get(source) {
                if let Err(e) = installer.install(list, opts) {
                    ui::fail(&e);
                    failures += 1;
                }
            }
        }
        if opts.dry_run {
            ui::info("dry run: nothing was changed");
        }

        match (failures, unknown.len()) {
            (0, 0) => Ok(()),
            (0, n) => Err(format!("{n} target(s) not found")),
            (f, 0) => Err(format!("{f} installer(s) failed")),
            (f, n) => Err(format!("{f} installer(s) failed, {n} target(s) not found")),
        }
    }
}
