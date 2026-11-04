//! Conversational flows: numbered menus and prompts (no full-screen TUI).

use crate::install::{self, Opts};
use crate::sys;
use crate::ui;

#[derive(Clone)]
pub struct SearchResult {
    pub name: String,
    pub version: String,
    pub source: String,
    pub description: String,
}

impl SearchResult {
    fn is_aur(&self) -> bool {
        self.source == "aur"
    }
}

/// Search official repositories and (if paru is present) the AUR.
pub fn do_search(query: &str, limit: usize) -> Vec<SearchResult> {
    let mut out = Vec::new();

    if let Ok(text) = sys::run("pacman", &["-Ss", query]) {
        let mut current: Option<SearchResult> = None;
        for line in text.lines() {
            if line.starts_with(' ') {
                if let Some(mut r) = current.take() {
                    r.description = line.trim().to_string();
                    out.push(r);
                }
                continue;
            }
            if let Some((left, _right)) = line.split_once(' ') {
                if let Some((repo, name)) = left.split_once('/') {
                    let version = line
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("")
                        .to_string();
                    current = Some(SearchResult {
                        name: name.to_string(),
                        version,
                        source: repo.to_string(),
                        description: String::new(),
                    });
                }
            }
            if out.len() >= limit {
                break;
            }
        }
        if let Some(r) = current.take() {
            out.push(r);
        }
    }
    out.truncate(limit);

    if sys::which("paru") {
        if let Ok(text) = sys::run("paru", &["-Ss", query]) {
            let mut n = 0;
            for line in text.lines() {
                let clean = sys::strip_ansi(line);
                if let Some(rest) = clean.strip_prefix("aur/") {
                    let name = rest.split_whitespace().next().unwrap_or("").to_string();
                    if name.is_empty() {
                        continue;
                    }
                    out.push(SearchResult {
                        name,
                        version: String::new(),
                        source: "aur".into(),
                        description: String::new(),
                    });
                    n += 1;
                    if n >= 8 {
                        break;
                    }
                } else if clean.starts_with(' ') {
                    if let Some(last) = out.last_mut() {
                        if last.is_aur() && last.description.is_empty() {
                            last.description = clean.trim().to_string();
                        }
                    }
                }
            }
        }
    }

    out
}

/// Print search results as a numbered list with colour-coded sources.
pub fn print_results(results: &[SearchResult]) {
    let p = ui::pal();
    let width = results.len().to_string().len().max(2);
    for (i, r) in results.iter().enumerate() {
        let src = if r.is_aur() { p.aur(&r.source) } else { p.repo(&r.source) };
        println!(
            "   {}) {:<24} {:<12} {}",
            p.dim(&format!("{:>w$}", i + 1, w = width)),
            p.bold(&r.name),
            p.dim(if r.version.is_empty() { "—" } else { &r.version }),
            src
        );
        if !r.description.is_empty() {
            println!("       {}", p.dim(&r.description));
        }
    }
}

// ─── install ─────────────────────────────────────────────────────────────────

pub fn install() -> Result<(), String> {
    loop {
        ui::header_head("install");
        let Some(answer) = ui::ask("what to install? (name, several names, or search text)", None)
        else {
            return Ok(());
        };
        let q = answer.trim().to_string();
        if q.is_empty() {
            continue;
        }

        // URL or path: install directly
        if q.contains("://") || q.starts_with("./") || q.starts_with('/') || q.contains("github.com")
        {
            install::install_batch(&[q], Opts::default())?;
            ui::pause();
            return Ok(());
        }

        // several words: if all resolve, install as a batch
        let words: Vec<String> = q.split_whitespace().map(str::to_string).collect();
        let all_known = words
            .iter()
            .all(|w| sys::in_repo(w) || sys::is_aur(w) || sys::installed(w));

        if all_known {
            ui::blank();
            let rows: Vec<ui::Row> = words
                .iter()
                .map(|w| {
                    if sys::is_aur(w) {
                        ui::Row::aur(w, "", "AUR")
                    } else {
                        ui::Row::repo(w, "", "repository")
                    }
                })
                .collect();
            ui::table(&rows);
            if ui::confirm(&format!("Install {} package(s)?", words.len()), true) {
                install::install_batch(&words, Opts::default())?;
            } else {
                ui::info("cancelled");
            }
            ui::pause();
            return Ok(());
        }

        // otherwise: search
        let results = do_search(&q, 12);
        if results.is_empty() {
            ui::warning(&format!("nothing found for “{q}”"));
            if !ui::confirm("Try another query?", true) {
                return Ok(());
            }
            continue;
        }
        ui::blank();
        print_results(&results);
        ui::blank();

        let picked = match ui::pick_many(results.len(), "numbers to install (1,3-5 / all) · 0 — back")
        {
            Some(v) => v,
            None => return Ok(()),
        };

        let chosen: Vec<String> = picked
            .iter()
            .filter_map(|i| results.get(i - 1).map(|r| r.name.clone()))
            .collect();
        if chosen.is_empty() {
            continue;
        }

        ui::blank();
        ui::header_head("to install");
        let rows: Vec<ui::Row> = chosen
            .iter()
            .map(|c| {
                if sys::is_aur(c) {
                    ui::Row::aur(c, "", "AUR")
                } else {
                    ui::Row::repo(c, "", "repository")
                }
            })
            .collect();
        ui::table(&rows);

        if ui::confirm("Install these?", true) {
            install::install_batch(&chosen, Opts::default())?;
        } else {
            ui::info("cancelled");
        }
        ui::pause();
        return Ok(());
    }
}

// ─── remove ──────────────────────────────────────────────────────────────────

pub fn remove() -> Result<(), String> {
    ui::header_err("remove");
    let Some(filter) = ui::ask("name filter (empty — explicit packages)", None) else {
        return Ok(());
    };
    let filter = filter.trim().to_string();

    // With a filter we search everything (including dependencies and orphans);
    // without one we only offer explicit packages, which is safer.
    let source = if filter.is_empty() {
        sys::explicit_packages()
    } else {
        sys::all_packages()
    };

    let names: Vec<String> = source
        .into_iter()
        .map(|(n, _)| n)
        .filter(|n| filter.is_empty() || n.contains(&filter))
        .collect();

    if names.is_empty() {
        ui::warning("nothing found");
        ui::pause();
        return Ok(());
    }

    let names = &names[..names.len().min(40)];
    if names.len() == 40 {
        ui::info("showing first 40 — narrow the filter");
    }

    ui::blank();
    let rows: Vec<ui::Row> = names
        .iter()
        .map(|n| {
            let size = sys::installed_info(n).map(|i| i.installed_size).unwrap_or_default();
            let deps = sys::reverse_deps(n).len();
            if deps > 0 {
                ui::Row::warn(n, &size, &format!("needed by {deps}"))
            } else {
                ui::Row::plain(n, &size, "")
            }
        })
        .collect();
    ui::table(&rows);
    ui::blank();

    let Some(picked) = ui::pick_many(names.len(), "numbers to remove (1,3-5) · 0 — back") else {
        return Ok(());
    };
    let chosen: Vec<String> = picked
        .iter()
        .filter_map(|i| names.get(i - 1).cloned())
        .collect();
    if chosen.is_empty() {
        return Ok(());
    }

    ui::blank();
    ui::header_err("will be removed");
    let rows: Vec<ui::Row> = chosen
        .iter()
        .map(|n| {
            let deps = sys::reverse_deps(n);
            if deps.is_empty() {
                ui::Row::plain(n, "", "leaf")
            } else {
                ui::Row::warn(n, "", &format!("pulls in {} dependent(s)", deps.len()))
            }
        })
        .collect();
    ui::table(&rows);

    let mut dependents: Vec<String> = Vec::new();
    for n in &chosen {
        for d in sys::reverse_deps(n) {
            if !chosen.contains(&d) && !dependents.contains(&d) {
                dependents.push(d);
            }
        }
    }
    if !dependents.is_empty() {
        println!("\n  {}", ui::pal().warn("these will go away too:"));
        for d in dependents.iter().take(12) {
            println!("     {d}");
        }
    }

    ui::blank();
    if !ui::confirm("Remove (with dependencies and configs)?", false) {
        ui::info("cancelled");
        ui::pause();
        return Ok(());
    }
    if chosen.len() > 3 && !ui::confirm(&format!("Really remove {} packages?", chosen.len()), false) {
        ui::info("cancelled");
        ui::pause();
        return Ok(());
    }

    let mut args = vec!["pacman".to_string(), "-Rns".to_string(), "--noconfirm".to_string()];
    args.extend(chosen.iter().cloned());
    let full = sys::sudo_args(&args);
    let (cmd, rest) = full.split_first().unwrap();
    let code = sys::stream(cmd, rest, &["error", "removing", "failed"]);
    if code == 0 {
        ui::success(&chosen.join(" "));
    } else {
        ui::warning("not everything was removed");
    }
    ui::pause();
    Ok(())
}

// ─── update ──────────────────────────────────────────────────────────────────

pub fn update() -> Result<(), String> {
    ui::header_head("update");
    let repo = sys::updates_repo();
    let aur = sys::updates_aur();

    if repo.is_empty() && aur.is_empty() {
        ui::success("everything is up to date");
        ui::pause();
        return Ok(());
    }

    if !repo.is_empty() {
        println!("  {}", ui::pal().repo(&format!("repository ({})", repo.len())));
        for l in &repo {
            println!("     {l}");
        }
    }
    if !aur.is_empty() {
        println!("\n  {}", ui::pal().aur(&format!("AUR ({})", aur.len())));
        for l in &aur {
            println!("     {l}");
        }
    }

    ui::blank();
    let items = vec![
        "Everything (repository + AUR)".to_string(),
        "Repository only".to_string(),
        "AUR only".to_string(),
    ];
    let Some(choice) = ui::menu("what to update?", &items) else {
        return Ok(());
    };

    if !ui::confirm("Start the update?", true) {
        ui::info("cancelled");
        ui::pause();
        return Ok(());
    }

    let keep_all: &[&str] = &["error", "conflict", "failed", ":: ", "upgrading", "Compiling"];
    let code = match choice {
        0 => {
            if sys::which("paru") {
                sys::stream("paru", &["-Syu".into(), "--noconfirm".into()], keep_all)
            } else {
                run_sudo_stream(&["-Syu", "--noconfirm"], keep_all)
            }
        }
        1 => run_sudo_stream(&["-Syu", "--noconfirm"], keep_all),
        _ => {
            if !sys::which("paru") {
                ui::warning("paru is not installed");
                ui::pause();
                return Ok(());
            }
            sys::stream("paru", &["-Sua".into(), "--noconfirm".into()], keep_all)
        }
    };

    if code == 0 {
        ui::success("updated");
    } else {
        ui::warning(&format!("exit code {code}"));
    }
    ui::pause();
    Ok(())
}

fn run_sudo_stream(args: &[&str], keep: &[&str]) -> i32 {
    let mut owned: Vec<String> = vec!["pacman".to_string()];
    owned.extend(args.iter().map(|s| s.to_string()));
    let full = sys::sudo_args(&owned);
    let (cmd, rest) = full.split_first().unwrap();
    sys::stream(cmd, rest, keep)
}

// ─── search ──────────────────────────────────────────────────────────────────

pub fn search() -> Result<(), String> {
    loop {
        ui::header_head("search");
        let Some(q) = ui::ask("what to search for?", None) else {
            return Ok(());
        };
        let q = q.trim().to_string();
        if q.is_empty() {
            continue;
        }

        let results = do_search(&q, 12);
        if results.is_empty() {
            ui::warning("nothing found");
            if !ui::confirm("Try again?", true) {
                return Ok(());
            }
            continue;
        }
        ui::blank();
        print_results(&results);
        ui::blank();

        let Some(picked) = ui::pick_many(results.len(), "number for details · 0 — back") else {
            return Ok(());
        };
        let name = results[picked[0] - 1].name.clone();

        ui::blank();
        crate::commands::info(&name);
        ui::blank();

        if ui::confirm(&format!("Install {name}?"), false) {
            install::install_batch(&[name], Opts::default())?;
            ui::pause();
            return Ok(());
        }
        if !ui::confirm("Search again?", true) {
            return Ok(());
        }
    }
}

// ─── info ────────────────────────────────────────────────────────────────────

pub fn info() -> Result<(), String> {
    ui::header_head("package info");
    let Some(name) = ui::ask("package name", None) else {
        return Ok(());
    };
    let name = name.trim();
    if name.is_empty() {
        return Ok(());
    }
    if sys::in_repo(name) || sys::installed(name) || sys::is_aur(name) {
        ui::blank();
        crate::commands::info(name);
    } else {
        ui::warning(&format!("not found: {name}"));
    }
    ui::pause();
    Ok(())
}

// ─── list ────────────────────────────────────────────────────────────────────

pub fn list() -> Result<(), String> {
    ui::header_head("lists");
    let items = vec![
        "Explicitly installed".to_string(),
        "From AUR".to_string(),
        "Orphans".to_string(),
        "All packages".to_string(),
    ];
    let Some(choice) = ui::menu("what to show?", &items) else {
        return Ok(());
    };
    let kind = ["exp", "aur", "orph", "all"][choice];
    crate::commands::list(kind);
    ui::pause();
    Ok(())
}

// ─── clean ───────────────────────────────────────────────────────────────────

pub fn clean() -> Result<(), String> {
    ui::header_head("cache cleanup");
    let before = sys::cache_size();
    let archives = sys::cache_archives();
    println!(
        "  now: {} · archives: {archives}\n",
        ui::pal().bold(&ui::human_bytes(before))
    );

    let items = vec![
        "Safe — keep one version of each".to_string(),
        "Only uninstalled packages".to_string(),
        "Everything — free the most space".to_string(),
    ];
    let Some(choice) = ui::menu("mode", &items) else {
        return Ok(());
    };

    match choice {
        0 => println!("  {}", ui::pal().dim("old versions are dropped, one stays")),
        1 => println!("  {}", ui::pal().dim("packages not present in the system are dropped")),
        _ => println!("  {}", ui::pal().warn("everything is deleted")),
    }

    if !ui::confirm("Proceed?", false) {
        ui::info("cancelled");
        ui::pause();
        return Ok(());
    }

    let arg = match choice {
        0 => "-rk1",
        1 => "-ruk0",
        _ => "-rk0",
    };
    let _ = run_sudo_stream(&["paccache", arg], &[]);

    let after = sys::cache_size();
    let saved = before.saturating_sub(after);
    if saved > 0 {
        ui::success(&format!("freed {}", ui::human_bytes(saved)));
    } else {
        ui::info("nothing to clean");
    }
    ui::pause();
    Ok(())
}

// ─── orphans ─────────────────────────────────────────────────────────────────

pub fn orphans() -> Result<(), String> {
    ui::header_head("orphans");
    let list = sys::orphans();
    if list.is_empty() {
        ui::success("no orphans");
        ui::pause();
        return Ok(());
    }

    println!("  found: {}\n", ui::pal().bold(&list.len().to_string()));
    let rows: Vec<ui::Row> = list
        .iter()
        .map(|n| {
            let size = sys::installed_info(n).map(|i| i.installed_size).unwrap_or_default();
            ui::Row::warn(n, &size, "nothing depends on it")
        })
        .collect();
    ui::table(&rows);

    ui::blank();
    let items = vec!["Remove all".to_string(), "Choose".to_string()];
    let Some(choice) = ui::menu("what to do?", &items) else {
        return Ok(());
    };

    let chosen: Vec<String> = match choice {
        0 => list.clone(),
        _ => {
            let Some(picked) = ui::pick_many(list.len(), "numbers (1,3-5 / all)") else {
                return Ok(());
            };
            picked
                .iter()
                .filter_map(|i| list.get(i - 1).cloned())
                .collect()
        }
    };
    if chosen.is_empty() {
        return Ok(());
    }

    if !ui::confirm(&format!("Remove {} package(s)?", chosen.len()), false) {
        ui::info("cancelled");
        ui::pause();
        return Ok(());
    }

    let mut args = vec!["pacman".to_string(), "-Rns".to_string(), "--noconfirm".to_string()];
    args.extend(chosen.iter().cloned());
    let full = sys::sudo_args(&args);
    let (cmd, rest) = full.split_first().unwrap();
    let code = sys::stream(cmd, rest, &["error", "removing", "failed"]);
    if code == 0 {
        ui::success(&format!("removed {}", chosen.len()));
    } else {
        ui::warning("not everything was removed");
    }
    ui::pause();
    Ok(())
}

// ─── main menu ───────────────────────────────────────────────────────────────

pub fn main_menu() -> Result<(), String> {
    loop {
        // Counts are intentionally not computed here: running pacman/paru before
        // drawing the menu makes startup feel slow. The detailed numbers are
        // shown by the corresponding commands instead.
        let update_label = "Update".to_string();
        let orphan_label = "Orphans".to_string();

        let items = vec![
            "Install a package".to_string(),
            "Remove a package".to_string(),
            update_label,
            "Search packages".to_string(),
            "Package info".to_string(),
            "Lists".to_string(),
            orphan_label,
            "Clean cache".to_string(),
            "Status".to_string(),
        ];

        let Some(choice) = ui::menu("what to do?", &items) else {
            return Ok(());
        };

        let outcome = match choice {
            0 => install(),
            1 => remove(),
            2 => update(),
            3 => search(),
            4 => info(),
            5 => list(),
            6 => orphans(),
            7 => clean(),
            8 => {
                crate::commands::status();
                ui::pause();
                Ok(())
            }
            _ => return Ok(()),
        };
        if let Err(e) = outcome {
            ui::fail(&e);
            ui::pause();
        }
    }
}
