    use crate::sys;
    use crate::ui;

    pub fn status() {
        let total = sys::count(&["-Q"]);
        let explicit = sys::count(&["-Qe"]);
        let deps = sys::count(&["-Qd"]);
        let aur = sys::count(&["-Qm"]);
        let orphans = sys::orphans().len();
        let repo_updates = sys::updates_repo().len();
        let aur_updates = sys::updates_aur().len();

        println!(
            "\n{} {} {}",
            ui::pal().head("▌"),
            ui::pal().bold("PACKAGE MANAGER"),
            ui::pal().dim(&format!("v{}", crate::VERSION))
        );
        println!();
        kv("total", &total.to_string(), None, ui::Palette::bold);
        kv("explicit", &explicit.to_string(), None, ui::Palette::bold);
        kv("dependencies", &deps.to_string(), None, ui::Palette::bold);
        kv("from AUR", &aur.to_string(), None, ui::Palette::aur);
        if orphans > 0 {
            kv("orphans", &orphans.to_string(), Some("pkg orphans"), ui::Palette::warn);
        } else {
            kv("orphans", "0", None, ui::Palette::ok);
        }

        println!();
        if repo_updates == 0 && aur_updates == 0 {
            kv("updates", "none, up to date", None, ui::Palette::ok);
        } else {
            if repo_updates > 0 {
                kv("repository", &repo_updates.to_string(), None, ui::Palette::repo);
            }
            if aur_updates > 0 {
                kv("AUR", &aur_updates.to_string(), None, ui::Palette::aur);
            }
        }

        println!();
        kv("cache", &ui::human_bytes(sys::cache_size()), Some("pkg clean"), ui::Palette::bold);
        kv("disk /", &format!("{} free", sys::disk_free()), None, ui::Palette::bold);
        println!();
    }

    fn kv(label: &str, value: &str, extra: Option<&str>, color: fn(&ui::Palette, &str) -> String) {
        let p = ui::pal();
        let pad = 14usize.saturating_sub(label.chars().count()).max(1);
        let spaces = " ".repeat(pad);
        match extra {
            Some(e) => println!(
                "  {}{}{} {}{}  {}",
                p.dim(label),
                spaces,
                color(p, value),
                "",
                p.dim(e),
                ""
            ),
            None => println!("  {}{}{}", p.dim(label), spaces, color(p, value)),
        }
    }

    pub fn list(what: &str) {
        match what {
            "exp" | "explicit" => {
                let rows: Vec<(String, String)> = sys::explicit_packages();
                ui::header_repo(&format!("explicitly installed ({})", rows.len()));
                let table: Vec<ui::Row> = rows
                    .iter()
                    .map(|(n, v)| ui::Row::plain(n, v, "explicit"))
                    .collect();
                ui::table(&table);
            }
            "aur" | "foreign" => {
                let rows = sys::aur_packages();
                ui::header_aur(&format!("from AUR ({})", rows.len()));
                let table: Vec<ui::Row> =
                    rows.iter().map(|(n, v)| ui::Row::aur(n, v, "aur")).collect();
                ui::table(&table);
            }
            "orph" | "orphans" => {
                let rows = sys::orphans();
                if rows.is_empty() {
                    ui::header_head("orphans");
                    ui::info("none");
                    return;
                }
                ui::header_head(&format!("orphans ({})", rows.len()));
                let table: Vec<ui::Row> = rows
                    .iter()
                    .map(|n| {
                        let size = sys::installed_info(n)
                            .map(|i| i.installed_size)
                            .unwrap_or_default();
                        ui::Row::warn(n, &size, "nothing depends on it")
                    })
                    .collect();
                ui::table(&table);
            }
            "all" => {
                let rows = sys::all_packages();
                ui::header_head(&format!("all packages ({})", rows.len()));
                let table: Vec<ui::Row> =
                    rows.iter().map(|(n, v)| ui::Row::plain(n, v, "")).collect();
                ui::table(&table);
            }
            other => ui::fail(&format!("list: unknown kind {other} (exp|aur|orph|all)")),
        }
    }

    pub fn search(query: &str) {
        let results = crate::wizard::do_search(query, 15);
        if results.is_empty() {
            ui::info("not found");
            return;
        }
        crate::wizard::print_results(&results);
    }

    pub fn info(name: &str) {
        if let Some(i) = sys::installed_info(name) {
            show(name, "installed", &i, ui::Palette::ok);
        } else if let Some(i) = sys::repo_info(name) {
            show(name, "repository", &i, ui::Palette::repo);
        } else if let Some(i) = sys::aur_info(name) {
            show(name, "AUR", &i, ui::Palette::aur);
        } else {
            ui::fail(&format!("package not found: {name}"));
        }
    }

    fn show(name: &str, source: &str, i: &sys::PkgInfo, color: fn(&ui::Palette, &str) -> String) {
        let p = ui::pal();
        println!(
            "\n{} {} {}",
            color(p, "▌"),
            p.bold(name),
            p.dim(&format!("({source})"))
        );
        ui::rule();
        let line = |k: &str, v: &str| {
            if !v.is_empty() {
                println!("   {} {:10} {}", p.dim(k), "", v);
            }
        };
        line("version", &i.version);
        line("repository", &i.repo);
        line("description", &i.description);
        line("size", &i.installed_size);
        line("download", &i.download_size);
        line("url", &i.url);
        line("depends", &none_if_empty(&i.depends));
        line("optional", &none_if_empty(&i.optional_deps));
        line("required by", &none_if_empty(&i.required_by));
        line("installed", &i.install_date);
        line("reason", &i.install_reason);
    }

    fn none_if_empty(s: &str) -> String {
        if s == "None" {
            String::new()
        } else {
            s.to_string()
        }
    }

    pub fn remove(names: &[String]) {
        let present: Vec<String> = names.iter().filter(|n| sys::installed(n)).cloned().collect();
        for n in names {
            if !sys::installed(n) {
                ui::warning(&format!("not installed: {n}"));
            }
        }
        if present.is_empty() {
            ui::fail("nothing to remove");
            return;
        }

        ui::header_err("to be removed");
        let rows: Vec<ui::Row> = present
            .iter()
            .map(|n| {
                let rd = sys::reverse_deps(n);
                if rd.is_empty() {
                    ui::Row::plain(n, "", "leaf")
                } else {
                    ui::Row::warn(n, "", &format!("required by {}", rd.len()))
                }
            })
            .collect();
        ui::table(&rows);

        if !ui::confirm("Remove (with dependencies and configs)?", false) {
            ui::info("cancelled");
            return;
        }

        let mut args = vec!["pacman".to_string(), "-Rns".to_string(), "--noconfirm".to_string()];
        args.extend(present.iter().cloned());
        let full = sys::sudo_args(&args);
        let (cmd, rest) = full.split_first().unwrap();
        let code = sys::stream(cmd, rest, &["error", "removing", "failed"]);
        if code == 0 {
            ui::success(&present.join(" "));
        } else {
            ui::fail(&format!("pacman exited with code {code}"));
        }
    }

    pub fn update_preview() {
        let repo = sys::updates_repo();
        let aur = sys::updates_aur();
        if repo.is_empty() && aur.is_empty() {
            ui::success("everything is up to date");
            return;
        }
        ui::header_repo("repository updates");
        if repo.is_empty() {
            ui::info("none");
        } else {
            for l in &repo {
                println!("  {l}");
            }
        }
        ui::header_aur("AUR updates");
        if aur.is_empty() {
            ui::info("none");
        } else {
            for l in &aur {
                println!("  {l}");
            }
        }
    }

    pub fn why(name: &str) {
        if !sys::installed(name) {
            ui::fail(&format!("not installed: {name}"));
            return;
        }
        ui::header_head(&format!("who requires {name}"));
        let tree = sys::reverse_deps(name);
        if tree.is_empty() {
            ui::info("nothing depends on it (leaf)");
        } else {
            for t in tree {
                println!("  {t}");
            }
        }
    }

    pub fn files(name: &str) {
        if !sys::installed(name) {
            ui::fail(&format!("not installed: {name}"));
            return;
        }
        ui::header_head(&format!("files of {name}"));
        for f in sys::package_files(name) {
            println!("  {f}");
        }
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
