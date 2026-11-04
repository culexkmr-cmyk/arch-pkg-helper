use crate::selfupdate;
use crate::sys;
use crate::ui;

#[derive(Default, Clone, Copy)]
pub struct RemoveOpts {
    pub dry_run: bool,
    pub assume_yes: bool,
    pub cascade: bool,
}

struct Stats {
    total: usize,
    explicit: usize,
    deps: usize,
    aur: usize,
    orphans: usize,
    repo_updates: usize,
    aur_updates: usize,
    cache: u64,
    archives: usize,
    disk: Option<(u32, String)>,
}

pub fn status() {
    let s = ui::spin("gathering…", || Stats {
        total: sys::count(&["-Q"]),
        explicit: sys::count(&["-Qe"]),
        deps: sys::count(&["-Qd"]),
        aur: sys::count(&["-Qm"]),
        orphans: sys::orphans().len(),
        repo_updates: sys::updates_repo().len(),
        aur_updates: sys::updates_aur().len(),
        cache: sys::cache_size(),
        archives: sys::cache_archives(),
        disk: sys::disk_usage(),
    });

    let p = ui::pal();
    ui::header_head(&format!("apkg {}", crate::VERSION));
    ui::blank();
    kv("packages", &p.bold(&s.total.to_string()), &format!("{} explicit · {} deps", s.explicit, s.deps));
    kv("from AUR", &p.aur(&s.aur.to_string()), "");
    if s.orphans > 0 {
        kv("orphans", &p.warn(&s.orphans.to_string()), "apkg orphans");
    } else {
        kv("orphans", &p.ok("0"), "");
    }
    match sys::aur_helper() {
        Some(h) => kv("AUR helper", &p.bold(h), ""),
        None => kv("AUR helper", &p.warn("none"), "install paru or yay"),
    }

    ui::blank();
    if s.repo_updates == 0 && s.aur_updates == 0 {
        kv("updates", &p.ok("up to date"), "");
    } else {
        let mut parts = Vec::new();
        if s.repo_updates > 0 {
            parts.push(p.repo(&format!("{} repo", s.repo_updates)));
        }
        if s.aur_updates > 0 {
            parts.push(p.aur(&format!("{} AUR", s.aur_updates)));
        }
        kv("updates", &parts.join(&p.dim(" · ")), "apkg update");
    }
    kv("cache", &p.bold(&ui::human_bytes(s.cache)), &format!("{} archives · apkg clean", s.archives));
    match &s.disk {
        Some((pct, free)) => kv("disk /", &format!("{} {pct}%", ui::bar(*pct, 16)), &format!("{free} free")),
        None => kv("disk /", "?", ""),
    }
    ui::blank();
}

fn kv(label: &str, value: &str, extra: &str) {
    let p = ui::pal();
    let pad = " ".repeat(24usize.saturating_sub(ui::vis_len(value)).max(2));
    if extra.is_empty() {
        println!("  {} {value}", p.dim(&format!("{label:<12}")));
    } else {
        println!("  {} {value}{pad}{}", p.dim(&format!("{label:<12}")), p.dim(extra));
    }
}

pub fn list(what: &str) -> Result<(), String> {
    match what {
        "exp" | "explicit" => {
            let rows = sys::explicit_packages();
            ui::header_repo(&format!("explicitly installed ({})", rows.len()));
            ui::table(&rows.iter().map(|(n, v)| ui::Row::plain(n, v, "")).collect::<Vec<_>>());
        }
        "aur" | "foreign" => {
            let rows = sys::aur_packages();
            ui::header_aur(&format!("from AUR ({})", rows.len()));
            ui::table(&rows.iter().map(|(n, v)| ui::Row::aur(n, v, "")).collect::<Vec<_>>());
        }
        "orph" | "orphans" => {
            let rows = sys::orphans();
            ui::header_head(&format!("orphans ({})", rows.len()));
            if rows.is_empty() {
                ui::info("none");
            } else {
                ui::table(&orphan_rows(&rows));
            }
        }
        "all" => {
            let rows = sys::all_packages();
            ui::header_head(&format!("all packages ({})", rows.len()));
            ui::table(&rows.iter().map(|(n, v)| ui::Row::plain(n, v, "")).collect::<Vec<_>>());
        }
        other => return Err(format!("list: unknown kind {other} (exp|aur|orph|all)")),
    }
    Ok(())
}

pub fn orphan_rows(names: &[String]) -> Vec<ui::Row> {
    let infos = sys::installed_infos(names);
    names
        .iter()
        .map(|n| {
            let size = infos.get(n).map(|i| i.installed_size.clone()).unwrap_or_default();
            ui::Row::warn(n, &size, "nothing depends on it")
        })
        .collect()
}

pub fn search(query: &str) -> Result<(), String> {
    let results = ui::spin("searching…", || crate::wizard::do_search(query, 15));
    if results.is_empty() {
        return Err(format!("nothing found for {query}"));
    }
    crate::wizard::print_results(&results);
    Ok(())
}

pub fn info(name: &str) -> Result<(), String> {
    if let Some(i) = sys::installed_info(name) {
        show(name, "installed", &i, ui::Palette::ok);
    } else if let Some(i) = sys::repo_info(name) {
        show(name, &i.repo.clone(), &i, ui::Palette::repo);
    } else if let Some(i) = sys::aur_info(name) {
        show(name, "AUR", &i, ui::Palette::aur);
    } else {
        let members = sys::group_members(name);
        if members.is_empty() {
            return Err(format!("package not found: {name}"));
        }
        ui::header_repo(&format!("{name} · group of {}", members.len()));
        let rows: Vec<ui::Row> = members.iter().map(|m| ui::Row::plain(m, "", "")).collect();
        ui::table(&rows);
    }
    Ok(())
}

fn show(name: &str, source: &str, i: &sys::PkgInfo, color: fn(&ui::Palette, &str) -> String) {
    let p = ui::pal();
    let tag = format!("{} {}", color(p, "●"), color(p, source));
    let ver = p.dim(&i.version);
    println!("\n  {} {ver}  {tag}", p.bold(name));
    if !i.description.is_empty() {
        for l in ui::wrap(&i.description, ui::width().clamp(40, 76) - 4) {
            println!("  {l}");
        }
    }
    ui::rule();
    let value_w = ui::width().clamp(40, 100) - 17;
    let line = |k: &str, v: &str| {
        for (n, l) in ui::wrap(v, value_w).iter().enumerate() {
            let label = if n == 0 { k } else { "" };
            println!("  {} {l}", p.dim(&format!("{label:<14}")));
        }
    };
    if i.repo != source {
        line("repository", &i.repo);
    }
    line("size", &i.installed_size);
    line("download", &i.download_size);
    line("url", &i.url);
    line("depends", &i.depends);
    for (n, dep) in i.optional_deps.iter().enumerate() {
        line(if n == 0 { "optional" } else { "" }, dep);
    }
    line("required by", &i.required_by.join(" "));
    line("installed", &i.install_date);
    line("reason", &i.install_reason);
}

/// `pacman -Rns`, or `-Rcns` when dependents must go too.
pub fn remove_packages(names: &[String], opts: RemoveOpts) -> Result<(), String> {
    let mut present: Vec<String> = Vec::new();
    for n in names {
        if !sys::installed(n) {
            ui::warning(&format!("not installed: {n}"));
        } else if !present.contains(n) {
            present.push(n.clone());
        }
    }
    if present.is_empty() {
        return Err("nothing to remove".into());
    }

    let infos = sys::installed_infos(&present);
    ui::header_err("to be removed");
    let rows: Vec<ui::Row> = present
        .iter()
        .map(|n| {
            let info = infos.get(n);
            let size = info.map(|i| i.installed_size.clone()).unwrap_or_default();
            match info.map(|i| i.required_by.len()).unwrap_or(0) {
                0 => ui::Row::plain(n, &size, "leaf"),
                k => ui::Row::warn(n, &size, &format!("required by {k}")),
            }
        })
        .collect();
    ui::table(&rows);

    let mut dependents: Vec<String> = Vec::new();
    for n in &present {
        for d in sys::dependents(n) {
            if !present.contains(&d) && !dependents.contains(&d) {
                dependents.push(d);
            }
        }
    }

    let mut cascade = opts.cascade;
    if !dependents.is_empty() {
        let p = ui::pal();
        ui::blank();
        ui::warning(&format!("{} installed package(s) depend on this:", dependents.len()));
        for d in dependents.iter().take(12) {
            println!("    {} {d}", p.warn("·"));
        }
        if dependents.len() > 12 {
            println!("    {}", p.dim(&format!("… and {} more", dependents.len() - 12)));
        }
        if !cascade {
            if opts.assume_yes || !ui::interactive() {
                return Err("pacman would refuse: remove the dependents too (--cascade) or pick other packages".into());
            }
            ui::blank();
            let items = vec!["Remove them too (cascade)".to_string(), "Cancel".to_string()];
            if ui::menu("pacman cannot remove a package others depend on", &items) != Some(0) {
                ui::info("cancelled");
                return Ok(());
            }
            cascade = true;
        }
    }

    let flag = if cascade { "-Rcns" } else { "-Rns" };
    let mut args = vec!["pacman", flag, "--noconfirm"];
    args.extend(present.iter().map(String::as_str));
    if opts.dry_run {
        ui::info(&format!("would run: {}", args.join(" ")));
        return Ok(());
    }
    if !opts.assume_yes {
        ui::blank();
        let total = present.len() + if cascade { dependents.len() } else { 0 };
        if !ui::confirm(&format!("Remove {total} package(s) with unneeded dependencies and configs?"), false) {
            ui::info("cancelled");
            return Ok(());
        }
    }

    match sys::stream_root(&args, &["error", "removing", "failed"]) {
        0 => {
            ui::success(&format!("removed {}", present.join(" ")));
            Ok(())
        }
        code => Err(format!("pacman exited with code {code}")),
    }
}

pub fn update_preview(scope: UpdateScope) {
    let (repo, aur, own) = ui::spin("checking for updates…", || {
        (
            if scope == UpdateScope::Aur { Vec::new() } else { sys::updates_repo() },
            if scope == UpdateScope::Repo { Vec::new() } else { sys::updates_aur() },
            if scope == UpdateScope::All { selfupdate::available(10) } else { None },
        )
    });
    if repo.is_empty() && aur.is_empty() && own.is_none() {
        ui::success("everything is up to date");
        return;
    }
    print_updates(&repo, &aur, own.as_deref());
}

pub fn print_updates(repo: &[String], aur: &[String], own: Option<&str>) {
    if let Some(v) = own {
        ui::header_head("apkg itself");
        ui::updates_table(&[format!("apkg {} -> {v}", selfupdate::VERSION)], ui::Palette::head);
    }
    if !repo.is_empty() {
        ui::header_repo(&format!("repositories · {}", repo.len()));
        ui::updates_table(repo, ui::Palette::repo);
    }
    if !aur.is_empty() {
        ui::header_aur(&format!("AUR · {}", aur.len()));
        ui::updates_table(aur, ui::Palette::aur);
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum UpdateScope {
    All,
    Repo,
    Aur,
}

pub fn run_update(scope: UpdateScope, assume_yes: bool) -> Result<(), String> {
    if scope == UpdateScope::All && ui::spin("checking pkg itself…", || selfupdate::available(10)).is_some() {
        if let Err(e) = selfupdate::run(false) {
            ui::warning(&format!("apkg self-update: {e}"));
        }
    }
    let helper = sys::aur_helper();
    let keep: &[&str] = &["error", "conflict", "failed", ":: ", "upgrading", "installing"];
    let code = match (scope, helper) {
        (UpdateScope::Repo, _) | (UpdateScope::All, None) => {
            if scope == UpdateScope::All {
                ui::warning("no AUR helper: updating repository packages only");
            }
            let mut args = vec!["pacman", "-Syu"];
            if assume_yes {
                args.push("--noconfirm");
            }
            if assume_yes {
                sys::stream_root(&args, keep)
            } else {
                inherit_root(&args)
            }
        }
        (UpdateScope::Aur, None) => return Err("an AUR helper (paru or yay) is required".into()),
        (_, Some(h)) => {
            if sys::is_root() {
                return Err(format!("{h} must not run as root"));
            }
            let mut args = vec![if scope == UpdateScope::Aur { "-Sua" } else { "-Syu" }];
            if assume_yes {
                args.push("--noconfirm");
            }
            std::process::Command::new(h).args(&args).status().map(|s| s.code().unwrap_or(1)).unwrap_or(127)
        }
    };
    if code == 0 {
        ui::success("updated");
        Ok(())
    } else {
        Err(format!("update exited with code {code}"))
    }
}

/// Root command with the terminal attached, so pacman can ask its own questions.
fn inherit_root(args: &[&str]) -> i32 {
    let full = sys::sudo_args(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    std::process::Command::new(&full[0]).args(&full[1..]).status().map(|s| s.code().unwrap_or(1)).unwrap_or(127)
}

pub fn clean(mode: usize) -> Result<(), String> {
    if !sys::which("paccache") {
        return Err("paccache not found: install pacman-contrib".into());
    }
    let arg = match mode {
        0 => "-rk1",
        1 => "-ruk0",
        _ => "-rk0",
    };
    let before = sys::cache_size();
    let code = sys::stream_root(&["paccache", arg], &[]);
    if code != 0 {
        return Err(format!("paccache exited with code {code}"));
    }
    let saved = before.saturating_sub(sys::cache_size());
    if saved > 0 {
        ui::success(&format!("freed {}", ui::human_bytes(saved)));
    } else {
        ui::info("nothing to clean");
    }
    Ok(())
}

pub fn why(name: &str) -> Result<(), String> {
    if !sys::installed(name) {
        return Err(format!("not installed: {name}"));
    }
    ui::header_head(&format!("who requires {name}"));
    ui::blank();
    let tree = sys::reverse_tree(name);
    if tree.is_empty() {
        ui::info("nothing depends on it (leaf)");
    } else {
        tree.iter().for_each(|t| println!("  {t}"));
    }
    Ok(())
}

pub fn files(name: &str) -> Result<(), String> {
    if !sys::installed(name) {
        return Err(format!("not installed: {name}"));
    }
    sys::package_files(name).iter().for_each(|f| println!("{f}"));
    Ok(())
}

pub fn owns(path: &str) -> Result<(), String> {
    match sys::owner_of(path) {
        Some(owner) => {
            println!("{owner}");
            Ok(())
        }
        None => Err(format!("no package owns {path}")),
    }
}
