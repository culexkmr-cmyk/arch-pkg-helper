use super::domain::{Opts, Source};
use super::ports::Installer;
use crate::{sys, ui};
use std::path::Path;

fn install_repo(pkgs: &[String], opts: Opts) -> Result<(), String> {
    let todo: Vec<String> = pkgs.iter().filter(|p| !sys::installed(p)).cloned().collect();
    if todo.is_empty() {
        ui::success(&format!("already installed: {}", pkgs.join(" ")));
        return Ok(());
    }
    if opts.dry_run {
        ui::info(&format!("would run: pacman -S --needed {}", todo.join(" ")));
        return Ok(());
    }
    let mut args = vec!["pacman".to_string(), "-S".to_string(), "--needed".to_string()];
    args.push("--noconfirm".to_string());
    args.extend(todo.iter().cloned());
    let full = sys::sudo_args(&args);
    let (cmd, rest) = full.split_first().unwrap();
    let code = sys::stream(cmd, rest, &["error", "conflict", "failed", "installing", "upgrading"]);
    if code == 0 { ui::success(&todo.join(" ")); Ok(()) }
    else { Err(format!("pacman exited with code {code}")) }
}

pub struct PacmanInstaller;
impl Installer for PacmanInstaller {
    fn install_batch(&self, targets: &[String], opts: Opts) -> Result<(), String> {
        install_repo(targets, opts)
    }
    fn preview(&self, target: &str) {
        super::preview(target, Source::Repo);
    }
}

fn install_aur(pkgs: &[String], opts: Opts) -> Result<(), String> {
    if !sys::which("paru") { return Err("paru is required for AUR packages".into()); }
    let todo: Vec<String> = pkgs.iter().filter(|p| !sys::installed(p)).cloned().collect();
    if todo.is_empty() {
        ui::success(&format!("already installed: {}", pkgs.join(" ")));
        return Ok(());
    }
    if opts.dry_run {
        ui::info(&format!("would run: paru -S --needed {}", todo.join(" ")));
        return Ok(());
    }
    let mut args = vec!["-S".to_string(), "--needed".to_string(), "--noconfirm".to_string()];
    args.extend(todo.iter().cloned());
    let code = sys::stream("paru", &args, &["Compiling","Building",":: ","error","conflict","installing"]);
    if code == 0 { ui::success(&todo.join(" ")); Ok(()) }
    else { Err(format!("paru exited with code {code}")) }
}

pub struct AurInstaller;
impl Installer for AurInstaller {
    fn install_batch(&self, targets: &[String], opts: Opts) -> Result<(), String> {
        install_aur(targets, opts)
    }
    fn preview(&self, target: &str) {
        super::preview(target, Source::Aur);
    }
}

fn install_git(target: &str, opts: Opts) -> Result<(), String> {
    let url = super::git_url(target);
    let dest = super::git_dest(target);
    if Path::new(&dest).join(".git").exists() {
        if !opts.update_git {
            ui::warning(&format!("{dest} already exists (use --update to pull)"));
            return Ok(());
        }
        if opts.dry_run {
            ui::info(&format!("would run: git -C {dest} pull"));
            return Ok(());
        }
        return match sys::run("git", &["-C", &dest, "pull", "--ff-only"]) {
            Ok(_) => { ui::success(&format!("updated {dest}")); Ok(()) }
            Err(e) => Err(format!("pull failed: {e}")),
        };
    }
    if opts.dry_run {
        ui::info(&format!("would run: git clone {url} → {dest}"));
        return Ok(());
    }
    match sys::run("git", &["clone", "--depth=1", &url, &dest]) {
        Ok(_) => { ui::success(&format!("cloned {dest}")); Ok(()) }
        Err(e) => Err(format!("clone failed: {e}")),
    }
}

pub struct GitInstaller;
impl Installer for GitInstaller {
    fn install_batch(&self, targets: &[String], opts: Opts) -> Result<(), String> {
        for t in targets { install_git(t, opts)?; }
        Ok(())
    }
    fn preview(&self, target: &str) { super::preview(target, Source::Git); }
}

fn install_script(target: &str, opts: Opts) -> Result<(), String> {
    if opts.dry_run {
        ui::info(&format!("would download and review {target}"));
        return Ok(());
    }
    let body = if Path::new(target).exists() {
        std::fs::read_to_string(target).map_err(|e| format!("cannot read {target}: {e}"))?
    } else {
        sys::run("curl", &["-fsSL", "--max-time", "30", target]).map_err(|e| format!("download failed: {e}"))?
    };
    println!("\n  {}", ui::pal().dim("first lines of the script:"));
    for line in body.lines().take(12) { println!("    {} {}", ui::pal().dim("│"), line); }
    let risky: Vec<&str> = body.lines().filter(|l| l.contains("rm -rf /") || l.contains("| sh") || l.contains("| bash") || l.contains(":(){")).take(3).collect();
    if !risky.is_empty() {
        ui::warning("suspicious lines:");
        for r in risky { println!("    {}", ui::pal().warn(r)); }
    }
    if !opts.assume_yes && !ui::confirm("Run this script?", false) { ui::info("skipped"); return Ok(()); }
    let tmp = std::env::temp_dir().join(format!("pkg-install-{}.sh", std::process::id()));
    std::fs::write(&tmp, &body).map_err(|e| format!("cannot write temp file: {e}"))?;
    let path = tmp.to_string_lossy().to_string();
    let mut args = vec!["bash".to_string(), path.clone()];
    if !sys::is_root() { args.insert(0, "sudo".to_string()); }
    let (cmd, rest) = args.split_first().unwrap();
    let code = sys::stream(cmd, rest, &[]);
    let _ = std::fs::remove_file(&tmp);
    if code == 0 { ui::success(target); Ok(()) } else { Err(format!("script exited with code {code}")) }
}

pub struct ScriptInstaller;
impl Installer for ScriptInstaller {
    fn install_batch(&self, targets: &[String], opts: Opts) -> Result<(), String> {
        for t in targets { install_script(t, opts)?; }
        Ok(())
    }
    fn preview(&self, target: &str) { super::preview(target, Source::Script); }
}

fn install_pkgbuild(target: &str, opts: Opts) -> Result<(), String> {
    let dir = Path::new(target).parent().map(|p| p.to_path_buf()).unwrap_or_else(|| Path::new(".").to_path_buf());
    if opts.dry_run {
        ui::info(&format!("would run makepkg -si in {}", dir.display()));
        return Ok(());
    }
    let args = vec!["-si".to_string(), "--needed".to_string(), "--noconfirm".to_string()];
    let code = sys::stream_in("makepkg", &args, &dir);
    if code == 0 { ui::success(target); Ok(()) }
    else { Err(format!("makepkg exited with code {code}")) }
}

pub struct PkgbuildInstaller;
impl Installer for PkgbuildInstaller {
    fn install_batch(&self, targets: &[String], opts: Opts) -> Result<(), String> {
        for t in targets { install_pkgbuild(t, opts)?; }
        Ok(())
    }
    fn preview(&self, target: &str) { super::preview(target, Source::Pkgbuild); }
}

pub struct Resolver;
impl super::ports::SourceResolver for Resolver {
    fn classify(&self, target: &str) -> Source { super::classify(target) }
}

