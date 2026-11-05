//! Conversational flows: numbered menus and prompts (no full-screen TUI).

use crate::commands::{self, RemoveOpts, UpdateScope};
use crate::install::{self, Opts, Source};
use crate::selfupdate;
use crate::sys;
use crate::ui;

#[derive(Clone, Debug, PartialEq)]
pub struct SearchResult {
    pub name: String,
    pub version: String,
    pub source: String,
    pub description: String,
    pub installed: bool,
}

impl SearchResult {
    fn is_aur(&self) -> bool {
        self.source == "aur"
    }
}

/// `repo/name version [...]` headers followed by indented descriptions; the
/// format shared by `pacman -Ss`, `paru -Ss` and `yay -Ss`.
pub fn parse_search(text: &str, only_aur: bool, limit: usize) -> Vec<SearchResult> {
    let mut out: Vec<SearchResult> = Vec::new();
    let mut last_kept = false;
    for raw in text.lines() {
        let line = sys::strip_ansi(raw);
        if line.starts_with(' ') {
            if last_kept {
                if let Some(r) = out.last_mut() {
                    if r.description.is_empty() {
                        r.description = line.trim().to_string();
                    }
                }
            }
            continue;
        }
        last_kept = false;
        if out.len() >= limit {
            continue;
        }
        let mut words = line.split_whitespace();
        let Some((repo, name)) = words.next().and_then(|w| w.split_once('/')) else {
            continue;
        };
        if only_aur != (repo == "aur") || out.iter().any(|r| r.name == name) {
            continue;
        }
        out.push(SearchResult {
            name: name.to_string(),
            version: words.next().unwrap_or("").to_string(),
            source: repo.to_string(),
            description: String::new(),
            installed: line.contains("[installed") || line.contains("(Installed"),
        });
        last_kept = true;
    }
    out
}

/// Search official repositories and, with an AUR helper, the AUR.
pub fn do_search(query: &str, limit: usize) -> Vec<SearchResult> {
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut args = vec!["-Ss"];
    args.extend(&words);
    let mut out = sys::run("pacman", &args).map(|t| parse_search(&t, false, limit)).unwrap_or_default();
    if let Some(helper) = sys::aur_helper() {
        let mut args = vec!["-Ss", "--aur"];
        args.extend(&words);
        if let Ok(text) = sys::run(helper, &args) {
            let aur: Vec<SearchResult> =
                parse_search(&text, true, 8).into_iter().filter(|a| !out.iter().any(|r| r.name == a.name)).collect();
            out.extend(aur);
        }
    }
    out
}

pub fn print_results(results: &[SearchResult]) {
    let p = ui::pal();
    let idx_w = results.len().to_string().len();
    let name_w = results.iter().map(|r| r.name.chars().count()).max().unwrap_or(0).clamp(8, 30);
    let ver_w = results.iter().map(|r| r.version.chars().count()).max().unwrap_or(0).clamp(4, 18);
    let indent = " ".repeat(idx_w + 3);
    let desc_w = ui::width().saturating_sub(idx_w + 6).clamp(20, 100);
    for (i, r) in results.iter().enumerate() {
        let (dot, src) = if r.is_aur() { (p.aur("●"), p.aur(&r.source)) } else { (p.repo("●"), p.repo(&r.source)) };
        let mark = if r.installed { format!("  {}", p.ok("✓ installed")) } else { String::new() };
        let ver = if r.version.is_empty() { "—" } else { &r.version };
        println!(
            "  {} {dot} {} {} {src}{mark}",
            p.accent(&format!("{:>idx_w$}", i + 1)),
            p.bold(&format!("{:<name_w$}", ui::truncate(&r.name, name_w))),
            p.dim(&format!("{:<ver_w$}", ui::truncate(ver, ver_w))),
        );
        if !r.description.is_empty() {
            println!("  {indent}{}", p.dim(&ui::truncate(&r.description, desc_w)));
        }
    }
}

fn report(result: Result<(), String>) {
    if let Err(e) = result {
        ui::fail(&e);
    }
}

// ─── install ─────────────────────────────────────────────────────────────────

pub fn install() -> Result<(), String> {
    loop {
        ui::header_head("install");
        let Some(answer) = ui::ask("what to install? (name, several names, URL, path or search text)", None) else {
            return Ok(());
        };
        let q = answer.trim().to_string();
        if q.is_empty() {
            continue;
        }

        let words: Vec<String> = q.split_whitespace().map(str::to_string).collect();
        let all_known = words.iter().all(|w| install::resolve(w).source != Source::Unknown);
        if all_known {
            report(install::install_batch(&words, Opts::default()));
            ui::pause();
            return Ok(());
        }

        let results = ui::spin("searching…", || do_search(&q, 12));
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

        let Some(picked) = ui::pick_many(results.len(), "numbers to install (1,3-5 / all) · 0 — back") else {
            continue;
        };
        let chosen: Vec<String> = picked
            .iter()
            .filter_map(|i| results.get(i - 1))
            .map(|r| if r.is_aur() { format!("aur:{}", r.name) } else { r.name.clone() })
            .collect();
        report(install::install_batch(&chosen, Opts::default()));
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
    let filter = filter.trim().to_lowercase();

    // without a filter only explicit packages are offered, which is safer
    let source = if filter.is_empty() { sys::explicit_packages() } else { sys::all_packages() };
    let mut names: Vec<String> =
        source.into_iter().map(|(n, _)| n).filter(|n| filter.is_empty() || n.contains(&filter)).collect();

    if names.is_empty() {
        ui::warning("nothing found");
        ui::pause();
        return Ok(());
    }
    const SHOWN: usize = 40;
    if names.len() > SHOWN {
        ui::info(&format!("showing {SHOWN} of {} — narrow the filter", names.len()));
        names.truncate(SHOWN);
    }

    ui::blank();
    let infos = sys::installed_infos(&names);
    let rows: Vec<ui::Row> = names
        .iter()
        .map(|n| {
            let info = infos.get(n);
            let size = info.map(|i| i.installed_size.clone()).unwrap_or_default();
            match info.map(|i| i.required_by.len()).unwrap_or(0) {
                0 => ui::Row::plain(n, &size, ""),
                k => ui::Row::warn(n, &size, &format!("needed by {k}")),
            }
        })
        .collect();
    ui::numbered_table(&rows);
    ui::blank();

    let Some(picked) = ui::pick_many(names.len(), "numbers to remove (1,3-5) · 0 — back") else {
        return Ok(());
    };
    let chosen: Vec<String> = picked.iter().filter_map(|i| names.get(i - 1).cloned()).collect();
    report(commands::remove_packages(&chosen, RemoveOpts::default()));
    ui::pause();
    Ok(())
}

// ─── update ──────────────────────────────────────────────────────────────────

pub fn update() -> Result<(), String> {
    ui::header_head("update");
    let (repo, aur, own) =
        ui::spin("checking for updates…", || (sys::updates_repo(), sys::updates_aur(), selfupdate::available(10)));

    if repo.is_empty() && aur.is_empty() && own.is_none() {
        ui::blank();
        ui::success("everything is up to date");
        ui::pause();
        return Ok(());
    }
    commands::print_updates(&repo, &aur, own.as_deref());
    ui::blank();

    if repo.is_empty() && aur.is_empty() {
        if ui::confirm("Update apkg?", true) {
            report(selfupdate::run(false));
            ui::pause();
        }
        return Ok(());
    }
    let mut items = vec!["Everything".to_string(), "Repositories only".to_string()];
    if sys::aur_helper().is_some() && !aur.is_empty() {
        items.push("AUR only".to_string());
    }
    let Some(choice) = ui::menu("what to update?", &items) else {
        return Ok(());
    };
    let scope = [UpdateScope::All, UpdateScope::Repo, UpdateScope::Aur][choice];
    report(commands::run_update(scope, false));
    ui::pause();
    Ok(())
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

        let results = ui::spin("searching…", || do_search(&q, 12));
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
            continue;
        };
        let r = &results[picked[0] - 1];

        ui::blank();
        report(commands::info(&r.name));
        ui::blank();

        if !r.installed && ui::confirm(&format!("Install {}?", r.name), false) {
            let target = if r.is_aur() { format!("aur:{}", r.name) } else { r.name.clone() };
            report(install::install_batch(&[target], Opts { assume_yes: true, ..Opts::default() }));
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
    if !name.is_empty() {
        ui::blank();
        report(commands::info(name));
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
    report(commands::list(["exp", "aur", "orph", "all"][choice]));
    ui::pause();
    Ok(())
}

// ─── clean ───────────────────────────────────────────────────────────────────

pub fn clean() -> Result<(), String> {
    ui::header_head("cache cleanup");
    ui::info(&format!(
        "now {} in {} archives",
        ui::pal().bold(&ui::human_bytes(sys::cache_size())),
        sys::cache_archives()
    ));
    ui::blank();

    let items = vec![
        "Safe — keep the latest version of each".to_string(),
        "Only uninstalled packages".to_string(),
        "Everything — free the most space".to_string(),
    ];
    let Some(choice) = ui::menu("mode", &items) else {
        return Ok(());
    };
    if choice == 2 {
        ui::warning("downgrading will need a download again");
    }
    if !ui::confirm("Proceed?", choice != 2) {
        ui::info("cancelled");
        ui::pause();
        return Ok(());
    }
    report(commands::clean(choice));
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

    ui::info(&format!("found {}", ui::pal().bold(&list.len().to_string())));
    ui::blank();
    ui::numbered_table(&commands::orphan_rows(&list));

    ui::blank();
    let items = vec!["Remove all".to_string(), "Choose".to_string()];
    let Some(choice) = ui::menu("what to do?", &items) else {
        return Ok(());
    };
    let chosen: Vec<String> = if choice == 0 {
        list.clone()
    } else {
        let Some(picked) = ui::pick_many(list.len(), "numbers (1,3-5 / all) · 0 — back") else {
            return Ok(());
        };
        picked.iter().filter_map(|i| list.get(i - 1).cloned()).collect()
    };
    report(commands::remove_packages(&chosen, RemoveOpts::default()));
    ui::pause();
    Ok(())
}

// ─── theme ───────────────────────────────────────────────────────────────────
pub fn theme() -> Result<(), String> {
    if !ui::interactive() {
        return Err("theme needs a terminal".into());
    }
    ui::header_head("theme");
    let themes = ui::available_themes();
    let current = ui::current_theme();
    let items: Vec<String> = themes
        .iter()
        .map(|t| {
            if *t == current {
                format!("{t}  (current)")
            } else {
                (*t).to_string()
            }
        })
        .collect();
    let Some(choice) = ui::menu("color theme", &items) else {
        ui::info("cancelled");
        return Ok(());
    };
    let selected = themes[choice].clone();
    ui::set_theme(&selected)?;
    ui::success(&format!("theme set to {selected}; reloading palette"));
    // Apply the new theme immediately when explicitly requested.
    ui::reload_palette();
    ui::pause();
    Ok(())
}

// ─── main menu ───────────────────────────────────────────────────────────────

pub fn main_menu() -> Result<(), String> {
    let notice = selfupdate::notice();
    loop {
        ui::clear();
        home(notice.as_deref());
        let Some(choice) = ui::read_choice(10) else {
            return Ok(());
        };
        let outcome = match choice {
            0 => install(),
            1 => remove(),
            2 => search(),
            3 => info(),
            4 => list(),
            5 => update(),
            6 => orphans(),
            7 => clean(),
            8 => {
                commands::status();
                ui::pause();
                Ok(())
            }
            9 => theme(),
            _ => {
                ui::info("cancelled");
                Ok(())
            }
        };
        if let Err(e) = outcome {
            ui::fail(&e);
            ui::pause();
        }
    }
}

fn home(notice: Option<&str>) {
    let p = ui::pal();
    let total = sys::count(&["-Q"]);
    let aur = sys::count(&["-Qm"]);
    let orphans = sys::orphans().len();
    let cache = sys::cache_size();

    let dot = p.dim(" · ");
    let orphans = if orphans > 0 { p.warn(&orphans.to_string()) } else { p.bold("0") };
    let mut lines = vec![
        format!(
            "{}  {}   {}",
            p.logo("▲ apkg"),
            p.dim(&format!("v{}", selfupdate::VERSION)),
            p.dim("Arch package helper")
        ),
        format!(
            "{} packages{dot}{} AUR{dot}{orphans} orphans{dot}{} cache",
            p.bold(&total.to_string()),
            p.aur(&aur.to_string()),
            p.bold(&ui::human_bytes(cache)),
        ),
    ];
    if let Some(v) = notice {
        lines.push(p.ok(&format!("↑ apkg {v} is out — press 6 to update")));
    }
    ui::boxed(&lines);

    const COL: usize = 26;
    let left = [(1, "Install"), (2, "Remove"), (3, "Search"), (4, "Package info"), (5, "Lists")];
    let right = [(6, "Update"), (7, "Orphans"), (8, "Clean cache"), (9, "Status"), (10, "Theme")];
    println!("\n  {}{}", p.head(&format!("{:<COL$}", "    Packages")), p.head("    System"));
    for row in 0..left.len() {
        let l = left.get(row).map(|(n, t)| ui::menu_item(*n, t)).unwrap_or_default();
        let r = right.get(row).map(|(n, t)| ui::menu_item(*n, t)).unwrap_or_default();
        let pad = " ".repeat(COL.saturating_sub(ui::vis_len(&l)));
        println!("  {l}{pad}{r}");
    }
    println!("\n  {}", p.dim(" 0  exit"));
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARU: &str = "cachyos-extra-v3/ripgrep-all 0.10.10-3.1 [0 B 12.53 MiB] [installed]
    rga: ripgrep, but also search in PDFs
extra/ripgrep-all 0.10.10-3 [2.21 MiB 13.53 MiB] [installed: 0.10.10-3.1]
    rga: duplicate
aur/ripgrep-all-git 0.10.10.462.g0f10fb9-1 [+2 ~0.00]
    rga: git version
aur/no-desc 1.0-1 [+0 ~0.00]
";

    #[test]
    fn parses_repo_results_without_duplicates() {
        let r = parse_search(PARU, false, 10);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].source, "cachyos-extra-v3");
        assert_eq!(r[0].version, "0.10.10-3.1");
        assert!(r[0].installed);
        assert_eq!(r[0].description, "rga: ripgrep, but also search in PDFs");
    }

    #[test]
    fn parses_aur_results() {
        let r = parse_search(PARU, true, 10);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].name, "ripgrep-all-git");
        assert_eq!(r[0].description, "rga: git version");
        assert_eq!(r[1].description, "");
        assert!(!r[0].installed);
    }

    #[test]
    fn respects_limit() {
        assert_eq!(parse_search(PARU, true, 1).len(), 1);
    }
}
