use super::domain::{Opts, Source};
use super::infra::{AurInstaller, GitInstaller, PacmanInstaller, PkgbuildInstaller, Resolver, ScriptInstaller};
use super::ports::{Installer, SourceResolver};
use crate::{ui};
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
        Self {
            resolver: Box::new(Resolver),
            installers,
        }
    }
}

impl InstallService {
    pub fn install(&self, target: &str, opts: Opts) -> Result<(), String> {
        self.install_batch(&[target.to_string()], opts)
    }

    pub fn install_batch(&self, targets: &[String], opts: Opts) -> Result<(), String> {
        let mut repo = Vec::new();
        let mut aur = Vec::new();
        let mut others = Vec::new();

        for t in targets {
            match self.resolver.classify(t) {
                Source::Repo => repo.push(t.clone()),
                Source::Aur => aur.push(t.clone()),
                _ => others.push(t.clone()),
            }
        }

        if !repo.is_empty() {
            ui::header_repo("repository");
            ui::table(&super::rows_for(&repo, Source::Repo));
        }
        if !aur.is_empty() {
            ui::header_aur("AUR");
            ui::table(&super::rows_for(&aur, Source::Aur));
        }
        for t in &others {
            let src = self.resolver.classify(t);
            if src == Source::Unknown {
                ui::warning(&format!("not found in repositories or AUR: {t}"));
            } else {
                ui::header_head(src.label());
                if let Some(installer) = self.installers.get(&src) {
                    installer.preview(t);
                }
            }
        }

        if repo.is_empty() && aur.is_empty() && others.is_empty() {
            return Err("nothing to install".into());
        }
        if opts.dry_run {
            ui::info("dry run: nothing was changed");
            return Ok(());
        }
        if !opts.assume_yes && !ui::confirm("Proceed?", true) {
            ui::info("cancelled");
            return Ok(());
        }

        let mut failures = 0;
        if !repo.is_empty() {
            if let Some(installer) = self.installers.get(&Source::Repo) {
                if let Err(e) = installer.install_batch(&repo, opts) {
                    ui::warning(&e);
                    failures += 1;
                }
            }
        }
        if !aur.is_empty() {
            if let Some(installer) = self.installers.get(&Source::Aur) {
                if let Err(e) = installer.install_batch(&aur, opts) {
                    ui::warning(&e);
                    failures += 1;
                }
            }
        }
        for t in &others {
            let src = self.resolver.classify(t);
            if src == Source::Unknown {
                continue;
            }
            if let Some(installer) = self.installers.get(&src) {
                if let Err(e) = installer.install_batch(&[t.clone()], opts) {
                    ui::warning(&e);
                    failures += 1;
                }
            }
        }

        if failures == 0 {
            Ok(())
        } else {
            Err(format!("{failures} installer(s) failed"))
        }
    }
}
