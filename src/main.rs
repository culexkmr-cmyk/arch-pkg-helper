//! apkg — helper for Arch Linux packages.
//!
//! One front-end for pacman, the AUR, git remotes, install scripts and local
//! PKGBUILDs, with a conversational mode when run without arguments.

mod commands;
mod install;
mod selfupdate;
mod sys;
mod ui;
mod wizard;

use commands::{RemoveOpts, UpdateScope};
use std::process::ExitCode;

use selfupdate::VERSION;

fn usage() {
    let p = ui::pal();
    println!("{} {} — helper for Arch Linux packages\n", p.bold("apkg"), p.dim(&format!("v{VERSION}")));
    println!("{}", p.bold("Interactive") + " " + &p.dim("(no arguments — asks questions)"));
    println!("    apkg                      main menu");
    println!("    apkg install              ask what to install");
    println!("    apkg remove               offer a list to remove");
    println!("    apkg update               show updates and ask");
    println!("    apkg search               ask for a query");
    println!("    apkg clean                ask for a cleanup mode");
    println!("    apkg orphans              ask whether to remove");
    println!();
    println!("{}", p.bold("Scriptable") + " " + &p.dim("(with arguments)"));
    println!("    apkg install <target…>    repo / group / AUR / git / *.sh / PKGBUILD / *.pkg.tar.*");
    println!("    apkg remove <name…>       with unneeded dependencies and configs");
    println!("    apkg update [-n|-y] [repo|aur]");
    println!("    apkg search <text>        list results");
    println!("    apkg list [exp|aur|orph|all]");
    println!("    apkg info <name>          details");
    println!("    apkg why <name>           who requires it");
    println!("    apkg files <name>         files owned by the package");
    println!("    apkg owns <path>          which package owns a file");
    println!("    apkg status               overview");
    println!("    apkg theme [name]         choose or set a color theme");
    println!("    apkg self-update [-n]     update apkg itself from GitHub releases");
    println!();
    println!("{}", p.bold("Flags"));
    println!("    -n, --dry-run            show the plan, change nothing (install, remove, update)");
    println!("    -y, --yes                skip confirmations");
    println!("    -k, --update             install: git pull instead of skipping existing clones");
    println!("    -s, --sudo               install: run scripts as root");
    println!("    -c, --cascade            remove: also remove packages that depend on the targets");
}

#[derive(Default)]
struct Flags {
    dry_run: bool,
    yes: bool,
    update_git: bool,
    sudo: bool,
    cascade: bool,
}

fn parse_flags(args: &[String], allowed: &[&str]) -> Result<(Flags, Vec<String>), String> {
    let mut f = Flags::default();
    let mut rest = Vec::new();
    let mut only_args = false;
    for a in args {
        if only_args || !a.starts_with('-') || a == "-" {
            rest.push(a.clone());
            continue;
        }
        if a == "--" {
            only_args = true;
            continue;
        }
        // -ny is the same as -n -y
        let shorts: Vec<String> =
            if a.starts_with("--") { vec![a.clone()] } else { a.chars().skip(1).map(|c| format!("-{c}")).collect() };
        for s in shorts {
            let name = match s.as_str() {
                "-n" | "--dry-run" => "dry-run",
                "-y" | "--yes" => "yes",
                "-k" | "--update" => "update",
                "-s" | "--sudo" => "sudo",
                "-c" | "--cascade" => "cascade",
                _ => return Err(format!("unknown flag: {s}")),
            };
            if !allowed.contains(&name) {
                return Err(format!("flag not supported here: {s}"));
            }
            match name {
                "dry-run" => f.dry_run = true,
                "yes" => f.yes = true,
                "update" => f.update_git = true,
                "sudo" => f.sudo = true,
                _ => f.cascade = true,
            }
        }
    }
    Ok((f, rest))
}

fn main() -> ExitCode {
    reset_sigpipe();
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        if ui::interactive() {
            return map(wizard::main_menu());
        }
        commands::status();
        println!();
        usage();
        return ExitCode::SUCCESS;
    }

    map(dispatch(&args[0], &args[1..]))
}

// Rust ignores SIGPIPE, so `apkg search x | head` would panic on a closed pipe.
fn reset_sigpipe() {
    extern "C" {
        fn signal(sig: i32, handler: usize) -> usize;
    }
    const SIGPIPE: i32 = 13;
    const SIG_DFL: usize = 0;
    unsafe {
        signal(SIGPIPE, SIG_DFL);
    }
}

fn map(r: Result<(), String>) -> ExitCode {
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            ui::fail(&e);
            ExitCode::FAILURE
        }
    }
}

fn need_terminal(what: &str) -> Result<(), String> {
    Err(format!("need {what} or a terminal"))
}

fn dispatch(cmd: &str, rest: &[String]) -> Result<(), String> {
    match cmd {
        "install" | "add" | "i" => {
            let (f, targets) = parse_flags(rest, &["dry-run", "yes", "update", "sudo"])?;
            if targets.is_empty() {
                return if ui::interactive() { wizard::install() } else { need_terminal("a target") };
            }
            let opts =
                install::Opts { dry_run: f.dry_run, assume_yes: f.yes, update_git: f.update_git, as_root: f.sudo };
            install::install_batch(&targets, opts)
        }
        "remove" | "rm" | "del" => {
            let (f, names) = parse_flags(rest, &["dry-run", "yes", "cascade"])?;
            if names.is_empty() {
                return if ui::interactive() { wizard::remove() } else { need_terminal("a package name") };
            }
            commands::remove_packages(&names, RemoveOpts { dry_run: f.dry_run, assume_yes: f.yes, cascade: f.cascade })
        }
        "search" | "find" | "s" => {
            if rest.is_empty() {
                return if ui::interactive() { wizard::search() } else { need_terminal("a query") };
            }
            commands::search(&rest.join(" "))
        }
        "info" | "show" => match rest.first() {
            Some(name) => commands::info(name),
            None if ui::interactive() => wizard::info(),
            None => need_terminal("a package name"),
        },
        "list" | "ls" => commands::list(rest.first().map(String::as_str).unwrap_or("exp")),
        "update" | "upgrade" | "up" => {
            let (f, what) = parse_flags(rest, &["dry-run", "yes"])?;
            let scope = match what.first().map(String::as_str) {
                None | Some("all") => UpdateScope::All,
                Some("repo") => UpdateScope::Repo,
                Some("aur") => UpdateScope::Aur,
                Some(other) => return Err(format!("update: unknown scope {other} (all|repo|aur)")),
            };
            if f.dry_run {
                commands::update_preview(scope);
                Ok(())
            } else if f.yes {
                commands::run_update(scope, true)
            } else if ui::interactive() && what.is_empty() {
                wizard::update()
            } else if ui::interactive() {
                commands::run_update(scope, false)
            } else {
                commands::update_preview(scope);
                Ok(())
            }
        }
        "clean" | "cache" => {
            if ui::interactive() {
                wizard::clean()
            } else {
                println!("cache: {} in {} archives", ui::human_bytes(sys::cache_size()), sys::cache_archives());
                Ok(())
            }
        }
        "orphans" => {
            if ui::interactive() {
                wizard::orphans()
            } else {
                sys::orphans().iter().for_each(|o| println!("{o}"));
                Ok(())
            }
        }
        "why" | "rdeps" => commands::why(rest.first().ok_or("need a package name")?),
        "files" => commands::files(rest.first().ok_or("need a package name")?),
        "owns" | "who" => commands::owns(rest.first().ok_or("need a path")?),
        "status" => {
            commands::status();
            Ok(())
        }
        "self-update" | "selfupdate" => {
            let (f, extra) = parse_flags(rest, &["dry-run"])?;
            if let Some(x) = extra.first() {
                return Err(format!("self-update takes no arguments: {x}"));
            }
            selfupdate::run(f.dry_run)
        }
        "theme" | "colors" => {
            if let Some(name) = rest.first() {
                ui::set_theme(name)?;
                ui::success(&format!("theme set to {}", name.trim()));
                Ok(())
            } else if ui::interactive() {
                wizard::theme()
            } else {
                need_terminal("a theme selection")
            }
        }
        "menu" | "m" => wizard::main_menu(),
        "-h" | "--help" | "help" => {
            usage();
            Ok(())
        }
        "-v" | "--version" => {
            println!("apkg {VERSION}");
            Ok(())
        }
        other => Err(format!("unknown command: {other} (try apkg --help)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn flags() {
        let (f, rest) = parse_flags(&v(&["-ny", "firefox", "--", "-weird"]), &["dry-run", "yes"]).unwrap();
        assert!(f.dry_run && f.yes);
        assert_eq!(rest, v(&["firefox", "-weird"]));
        assert!(parse_flags(&v(&["-c"]), &["yes"]).is_err());
        assert!(parse_flags(&v(&["--bogus"]), &["yes"]).is_err());
    }
}
