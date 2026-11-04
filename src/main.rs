//! pkg — package manager for Arch Linux.
//!
//! One front-end for pacman, the AUR, git remotes and install scripts, with an
//! optional conversational mode when run without arguments on a terminal.

mod install;
mod sys;
mod ui;
mod wizard;
mod commands;

use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn usage() {
    let p = ui::pal();
    println!(
        "{} {} — package manager\n",
        p.bold("pkg"),
        p.dim(&format!("v{VERSION}"))
    );
    println!("{}", p.bold("Interactive") + " " + &p.dim("(no arguments — asks questions)"));
    println!("    pkg                      main menu");
    println!("    pkg install              ask what to install");
    println!("    pkg remove               offer a list to remove");
    println!("    pkg update               show updates and ask");
    println!("    pkg search               ask for a query");
    println!("    pkg clean                ask for a cleanup mode");
    println!("    pkg orphans              ask whether to remove");
    println!();
    println!("{}", p.bold("Scriptable") + " " + &p.dim("(with arguments — no questions)"));
    println!("    pkg install <target…>    repo / AUR / github.com/u/r / *.sh");
    println!("    pkg remove <name…>       with dependencies and configs");
    println!("    pkg search <text>        list results");
    println!("    pkg list [exp|aur|orph|all]");
    println!("    pkg info <name>          details");
    println!("    pkg why <name>           who requires it");
    println!("    pkg files <name>         files owned by the package");
    println!("    pkg owns <path>          which package owns a file");
    println!();
    println!("{}", p.bold("Flags for install"));
    println!("    -n, --dry-run            show the plan, change nothing");
    println!("    -y, --yes                skip confirmations");
    println!("    -k, --update             git: pull instead of skipping");
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // no arguments
    if args.is_empty() {
        if ui::interactive() {
            return map(wizard::main_menu());
        }
        commands::status();
        println!();
        usage();
        return ExitCode::SUCCESS;
    }

    let cmd = args[0].as_str();
    let rest = &args[1..];

    let result = match cmd {
        "install" | "add" | "i" => {
            let (opts, targets) = parse_install_flags(rest);
            if targets.is_empty() {
                if ui::interactive() {
                    wizard::install()
                } else {
                    ui::fail("need a target or a terminal");
                    return ExitCode::FAILURE;
                }
            } else {
                install::install_batch(&targets, opts)
            }
        }
        "remove" | "rm" | "del" => {
            if rest.is_empty() {
                if ui::interactive() {
                    wizard::remove()
                } else {
                    ui::fail("need a package name or a terminal");
                    return ExitCode::FAILURE;
                }
            } else {
                commands::remove(rest); Ok(())
            }
        }
        "search" | "find" | "s" => {
            if rest.is_empty() {
                if ui::interactive() {
                    wizard::search()
                } else {
                    ui::fail("need a query");
                    return ExitCode::FAILURE;
                }
            } else {
                commands::search(&rest.join(" "));
                Ok(())
            }
        }
        "info" | "show" => {
            if rest.is_empty() {
                if ui::interactive() {
                    wizard::info()
                } else {
                    ui::fail("need a package name");
                    return ExitCode::FAILURE;
                }
            } else {
                commands::info(&rest[0]);
                Ok(())
            }
        }
        "list" | "ls" => {
            commands::list(rest.first().map(String::as_str).unwrap_or("exp"));
            Ok(())
        }
        "update" | "upgrade" | "up" => {
            if ui::interactive() {
                wizard::update()
            } else {
                commands::update_preview();
                Ok(())
            }
        }
        "clean" | "cache" => {
            if ui::interactive() {
                wizard::clean()
            } else {
                println!("  cache: {}", ui::human_bytes(sys::cache_size()));
                Ok(())
            }
        }
        "orphans" => {
            if ui::interactive() {
                wizard::orphans()
            } else {
                for o in sys::orphans() {
                    println!("{o}");
                }
                Ok(())
            }
        }
        "why" | "rdeps" => {
            if rest.is_empty() {
                ui::fail("need a package name");
                return ExitCode::FAILURE;
            }
            commands::why(&rest[0]);
            Ok(())
        }
        "files" => {
            if rest.is_empty() {
                ui::fail("need a package name");
                return ExitCode::FAILURE;
            }
            commands::files(&rest[0]);
            Ok(())
        }
        "owns" | "who" => {
            if rest.is_empty() {
                ui::fail("need a path");
                return ExitCode::FAILURE;
            }
            commands::owns(&rest[0])
        }
        "status" => {
            commands::status();
            Ok(())
        }
        "menu" | "m" => wizard::main_menu(),
        "-h" | "--help" | "help" => {
            usage();
            Ok(())
        }
        "-v" | "--version" => {
            println!("pkg {VERSION}");
            Ok(())
        }
        other => {
            ui::fail(&format!("unknown command: {other} (try pkg --help)"));
            return ExitCode::FAILURE;
        }
    };

    map(result)
}

/// Convert a command result into a process exit code.
fn map(r: Result<(), String>) -> ExitCode {
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            ui::fail(&e);
            ExitCode::FAILURE
        }
    }
}

fn parse_install_flags(args: &[String]) -> (install::Opts, Vec<String>) {
    let mut opts = install::Opts::default();
    let mut targets = Vec::new();
    for a in args {
        match a.as_str() {
            "-n" | "--dry-run" => opts.dry_run = true,
            "-y" | "--yes" => opts.assume_yes = true,
            "-k" | "--update" => opts.update_git = true,
            "--" => {}
            other if other.starts_with('-') => {
                ui::warning(&format!("unknown flag: {other}"));
            }
            other => targets.push(other.to_string()),
        }
    }
    (opts, targets)
}

// ─── non-interactive commands ────────────────────────────────────────────────

