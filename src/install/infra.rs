use super::domain::{Opts, Source, Target};
use super::ports::{Installer, SourceResolver};
use crate::{sys, ui};
use std::path::{Path, PathBuf};
use std::process::Command;

fn names(targets: &[Target]) -> Vec<String> {
    targets.iter().map(|t| t.name.clone()).collect()
}

fn status(mut c: Command) -> i32 {
    c.status().map(|s| s.code().unwrap_or(1)).unwrap_or(127)
}

fn package_rows(targets: &[Target], source: Source) -> Vec<ui::Row> {
    targets
        .iter()
        .map(|t| {
            let info = if source == Source::Repo { sys::repo_info(&t.name) } else { sys::aur_info(&t.name) };
            let (version, mut note) = match info {
                Some(i) => (i.version, i.description),
                None if source == Source::Repo => {
                    let n = sys::group_members(&t.name).len();
                    (String::new(), format!("group · {n} packages"))
                }
                None => (String::new(), String::new()),
            };
            if sys::installed(&t.name) {
                note = format!("installed · {note}");
            }
            let kind = if source == Source::Repo { ui::Kind::Repo } else { ui::Kind::Aur };
            ui::Row { name: t.name.clone(), version, note, kind }
        })
        .collect()
}

// ─── repository ──────────────────────────────────────────────────────────────

pub struct PacmanInstaller;

impl Installer for PacmanInstaller {
    fn preview(&self, targets: &[Target]) {
        ui::table(&package_rows(targets, Source::Repo));
    }

    fn install(&self, targets: &[Target], opts: Opts) -> Result<(), String> {
        let all = names(targets);
        // groups are never "installed" as such; --needed skips their present members
        let todo: Vec<String> = all.iter().filter(|p| !sys::installed(p)).cloned().collect();
        if todo.is_empty() {
            ui::success(&format!("already installed: {}", all.join(" ")));
            return Ok(());
        }
        let mut args = vec!["pacman", "-S", "--needed", "--noconfirm"];
        args.extend(todo.iter().map(String::as_str));
        if opts.dry_run {
            ui::info(&format!("would run: {}", args.join(" ")));
            return Ok(());
        }
        let code = sys::stream_root(&args, &["error", "conflict", "failed", "installing", "upgrading"]);
        if code == 0 {
            ui::success(&todo.join(" "));
            Ok(())
        } else {
            Err(format!("pacman exited with code {code}"))
        }
    }
}

// ─── AUR ─────────────────────────────────────────────────────────────────────

pub struct AurInstaller;

impl Installer for AurInstaller {
    fn preview(&self, targets: &[Target]) {
        ui::table(&package_rows(targets, Source::Aur));
    }

    fn install(&self, targets: &[Target], opts: Opts) -> Result<(), String> {
        let helper = sys::aur_helper().ok_or("an AUR helper (paru or yay) is required for AUR packages")?;
        let all = names(targets);
        let todo: Vec<String> = all.iter().filter(|p| !sys::installed(p)).cloned().collect();
        if todo.is_empty() {
            ui::success(&format!("already installed: {}", all.join(" ")));
            return Ok(());
        }
        // without --yes the helper keeps its own PKGBUILD review and prompts
        let mut args = vec!["-S", "--needed", "--aur"];
        if opts.assume_yes {
            args.push("--noconfirm");
        }
        args.extend(todo.iter().map(String::as_str));
        if opts.dry_run {
            ui::info(&format!("would run: {helper} {}", args.join(" ")));
            return Ok(());
        }
        if sys::is_root() {
            return Err(format!("{helper} must not run as root"));
        }
        let mut c = Command::new(helper);
        c.args(&args);
        match status(c) {
            0 => {
                ui::success(&todo.join(" "));
                Ok(())
            }
            code => Err(format!("{helper} exited with code {code}")),
        }
    }
}

// ─── git ─────────────────────────────────────────────────────────────────────

pub struct GitInstaller;

impl GitInstaller {
    fn one(&self, url: &str, opts: Opts) -> Result<(), String> {
        let dest = super::git_dest(url);
        let mut c = Command::new("git");
        // fail fast on missing or private repositories instead of asking for a password
        c.env("GIT_TERMINAL_PROMPT", "0");
        if Path::new(&dest).join(".git").exists() {
            if !opts.update_git {
                ui::warning(&format!("{dest} already exists (use --update to pull)"));
                return Ok(());
            }
            if opts.dry_run {
                ui::info(&format!("would run: git -C {dest} pull --ff-only"));
                return Ok(());
            }
            c.args(["-C", &dest, "pull", "--ff-only"]);
            return match status(c) {
                0 => {
                    ui::success(&format!("updated {dest}"));
                    Ok(())
                }
                code => Err(format!("git pull exited with code {code}")),
            };
        }
        if Path::new(&dest).exists() {
            return Err(format!("{dest} exists and is not a git repository"));
        }
        if opts.dry_run {
            ui::info(&format!("would run: git clone --depth=1 {url} {dest}"));
            return Ok(());
        }
        c.args(["clone", "--depth=1", url, &dest]);
        match status(c) {
            0 => {
                ui::success(&format!("cloned {dest}"));
                if Path::new(&dest).join("PKGBUILD").is_file() {
                    ui::info(&format!("it has a PKGBUILD: pkg install {dest}"));
                }
                Ok(())
            }
            code => Err(format!("git clone exited with code {code}")),
        }
    }
}

impl Installer for GitInstaller {
    fn preview(&self, targets: &[Target]) {
        for t in targets {
            let dest = super::git_dest(&t.name);
            if Path::new(&dest).join(".git").exists() {
                println!("  {} already cloned → {dest}", ui::pal().ok("✓"));
            } else {
                println!("  {} → {}", t.name, ui::pal().bold(&dest));
            }
        }
    }

    fn install(&self, targets: &[Target], opts: Opts) -> Result<(), String> {
        targets.iter().try_for_each(|t| self.one(&t.name, opts))
    }
}

// ─── install scripts ─────────────────────────────────────────────────────────

pub struct ScriptInstaller;

/// Interpreter from the shebang, falling back to the file extension.
pub fn interpreter(body: &str, name: &str) -> String {
    if let Some(line) = body.lines().next().and_then(|l| l.strip_prefix("#!")) {
        let mut parts = line.split_whitespace();
        if let Some(prog) = parts.next() {
            let base = prog.rsplit('/').next().unwrap_or(prog);
            let base = if base == "env" { parts.find(|p| !p.starts_with('-')).unwrap_or("") } else { base };
            if !base.is_empty() {
                return base.to_string();
            }
        }
    }
    let lower = name.to_lowercase();
    let lower = lower.split(['?', '#']).next().unwrap_or("");
    if lower.ends_with(".py") { "python3" } else { "bash" }.to_string()
}

pub fn suspicious(line: &str) -> bool {
    let l = line.split_whitespace().collect::<Vec<_>>().join(" ");
    let l = l.trim_start();
    if l.starts_with('#') {
        return false;
    }
    let word = |p: &str| {
        l.match_indices(p)
            .any(|(i, _)| matches!(l[i + p.len()..].chars().next(), None | Some(' ' | '*' | ';' | '"' | ')')))
    };
    ["rm -rf /", "rm -fr /", "rm -rf ~", "rm -rf $HOME", "| sh", "| bash", "| sudo", "chmod -R 777 /"]
        .iter()
        .any(|p| word(p))
        || [":(){", "mkfs", "dd if=", "> /dev/sd", "> /dev/nvme"].iter().any(|p| l.contains(p))
}

fn private_copy(body: &str) -> Result<(PathBuf, PathBuf), String> {
    use std::os::unix::fs::DirBuilderExt;
    let nanos =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("pkg-{}-{nanos}", std::process::id()));
    std::fs::DirBuilder::new().mode(0o700).create(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let file = dir.join("script");
    std::fs::write(&file, body).map_err(|e| format!("cannot write temp file: {e}"))?;
    Ok((dir, file))
}

fn page(body: &str) {
    use std::io::Write;
    let pager = std::env::var("PAGER").unwrap_or_else(|_| "less".into());
    let child = Command::new(&pager).stdin(std::process::Stdio::piped()).spawn();
    match child {
        Ok(mut c) => {
            if let Some(mut stdin) = c.stdin.take() {
                let _ = stdin.write_all(body.as_bytes());
            }
            let _ = c.wait();
        }
        Err(_) => body.lines().enumerate().for_each(|(i, l)| println!("{:>5} │ {l}", i + 1)),
    }
}

impl ScriptInstaller {
    fn one(&self, target: &str, opts: Opts) -> Result<(), String> {
        let as_root_hint = if opts.as_root { " with sudo" } else { "" };
        if opts.dry_run {
            let verb = if Path::new(target).is_file() { "show and run" } else { "download, show and run" };
            ui::info(&format!("would {verb} {target}{as_root_hint}"));
            return Ok(());
        }
        let body = if Path::new(target).is_file() {
            std::fs::read_to_string(target).map_err(|e| format!("cannot read {target}: {e}"))?
        } else {
            sys::run("curl", &["-fsSL", "--max-time", "60", target]).map_err(|e| format!("download failed: {e}"))?
        };
        let interp = interpreter(&body, target);
        let lines: Vec<&str> = body.lines().collect();

        let p = ui::pal();
        println!("\n  {}", p.dim(&format!("{target} · {} lines · {interp}", lines.len())));
        for line in lines.iter().take(15) {
            println!("    {} {line}", p.dim("│"));
        }
        if lines.len() > 15 {
            println!("    {} {}", p.dim("│"), p.dim("…"));
        }
        let risky: Vec<(usize, &&str)> = lines.iter().enumerate().filter(|(_, l)| suspicious(l)).collect();
        if !risky.is_empty() {
            ui::warning("suspicious lines:");
            for (i, l) in risky.iter().take(8) {
                println!("    {} {}", p.dim(&format!("{:>4}", i + 1)), p.warn(l.trim()));
            }
        }

        let as_root = if opts.assume_yes {
            opts.as_root
        } else {
            let user = std::env::var("USER").unwrap_or_else(|_| "current user".into());
            let mut items = vec![format!("Run as {user}"), "Run as root (sudo)".to_string()];
            if opts.as_root {
                items.swap(0, 1);
            }
            items.push("Show the whole script".to_string());
            loop {
                match ui::menu("run this script?", &items) {
                    None => {
                        ui::info("skipped");
                        return Ok(());
                    }
                    Some(2) => page(&body),
                    Some(i) => break (i == 1) != opts.as_root,
                }
            }
        };

        if !sys::which(&interp) {
            return Err(format!("interpreter not found: {interp}"));
        }
        let (dir, file) = private_copy(&body)?;
        let path = file.to_string_lossy().to_string();
        let mut cmd = vec![interp, path];
        if as_root {
            cmd = sys::sudo_args(&cmd);
        }
        let mut c = Command::new(&cmd[0]);
        c.args(&cmd[1..]);
        let code = status(c);
        let _ = std::fs::remove_dir_all(dir);
        if code == 0 {
            ui::success(target);
            Ok(())
        } else {
            Err(format!("script exited with code {code}"))
        }
    }
}

impl Installer for ScriptInstaller {
    fn preview(&self, targets: &[Target]) {
        for t in targets {
            println!("  {}", t.name);
        }
        println!("  {}", ui::pal().dim("(shown before it runs)"));
    }

    fn install(&self, targets: &[Target], opts: Opts) -> Result<(), String> {
        targets.iter().try_for_each(|t| self.one(&t.name, opts))
    }
}

// ─── local PKGBUILD ──────────────────────────────────────────────────────────

pub struct PkgbuildInstaller;

impl Installer for PkgbuildInstaller {
    fn preview(&self, targets: &[Target]) {
        for t in targets {
            println!("  makepkg -si in {}", ui::pal().bold(&t.name));
        }
    }

    fn install(&self, targets: &[Target], opts: Opts) -> Result<(), String> {
        for t in targets {
            let mut args = vec!["-si", "--needed"];
            if opts.assume_yes {
                args.push("--noconfirm");
            }
            if opts.dry_run {
                ui::info(&format!("would run in {}: makepkg {}", t.name, args.join(" ")));
                continue;
            }
            if sys::is_root() {
                return Err("makepkg must not run as root".into());
            }
            let mut c = Command::new("makepkg");
            c.args(&args).current_dir(&t.name);
            match status(c) {
                0 => ui::success(&t.name),
                code => return Err(format!("makepkg exited with code {code}")),
            }
        }
        Ok(())
    }
}

// ─── package archives ────────────────────────────────────────────────────────

pub struct PkgFileInstaller;

impl Installer for PkgFileInstaller {
    fn preview(&self, targets: &[Target]) {
        for t in targets {
            println!("  {}", t.name);
        }
    }

    fn install(&self, targets: &[Target], opts: Opts) -> Result<(), String> {
        let mut args = vec!["pacman", "-U", "--needed", "--noconfirm"];
        args.extend(targets.iter().map(|t| t.name.as_str()));
        if opts.dry_run {
            ui::info(&format!("would run: {}", args.join(" ")));
            return Ok(());
        }
        match sys::stream_root(&args, &["error", "conflict", "failed", "installing", "upgrading", "downloading"]) {
            0 => {
                ui::success(&names(targets).join(" "));
                Ok(())
            }
            code => Err(format!("pacman exited with code {code}")),
        }
    }
}

pub struct Resolver;

impl SourceResolver for Resolver {
    fn resolve(&self, target: &str) -> Target {
        super::resolve(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_interpreter() {
        assert_eq!(interpreter("#!/usr/bin/env python3\nprint()", "x"), "python3");
        assert_eq!(interpreter("#!/usr/bin/env -S bash -e\n", "x"), "bash");
        assert_eq!(interpreter("#!/bin/sh\n", "x.py"), "sh");
        assert_eq!(interpreter("print()", "https://e.com/install.py?raw=1"), "python3");
        assert_eq!(interpreter("echo", "install"), "bash");
    }

    #[test]
    fn flags_dangerous_lines() {
        assert!(suspicious("sudo rm -rf / --no-preserve-root"));
        assert!(suspicious("rm  -rf  /*"));
        assert!(suspicious("curl -fsSL https://x | sudo bash"));
        assert!(!suspicious("rm -rf /tmp/build"));
        assert!(!suspicious("# curl x | sh"));
        assert!(!suspicious("echo done"));
    }
}
