//! Terminal UI primitives: colors, prompts, menus, tables.
//!
//! Everything degrades gracefully: when stdout is not a terminal, colors are
//! disabled so output stays clean for pipes and logs.

use std::io::{self, IsTerminal, Write};
use std::sync::OnceLock;

const RESET: &str = "\x1b[0m";

// 256-color codes chosen to match the original shell palette.
const C_REPO: &str = "38;5;39"; // blue   — official repositories
const C_AUR: &str = "38;5;214"; // orange — AUR
const C_GIT: &str = "38;5;141"; // violet — git remotes
const C_SCRIPT: &str = "38;5;209"; // coral  — install scripts
const C_OK: &str = "38;5;114"; // green
const C_WARN: &str = "38;5;221"; // yellow
const C_ERR: &str = "38;5;203"; // red
const C_HEAD: &str = "38;5;111"; // steel blue — section headers

pub struct Palette {
    enabled: bool,
}

impl Palette {
    fn new() -> Self {
        Self { enabled: io::stdout().is_terminal() }
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}{RESET}")
        } else {
            text.to_string()
        }
    }

    pub fn bold(&self, s: &str) -> String { self.paint("1", s) }
    pub fn dim(&self, s: &str) -> String { self.paint("2", s) }
    pub fn repo(&self, s: &str) -> String { self.paint(C_REPO, s) }
    pub fn aur(&self, s: &str) -> String { self.paint(C_AUR, s) }
    pub fn git(&self, s: &str) -> String { self.paint(C_GIT, s) }
    pub fn script(&self, s: &str) -> String { self.paint(C_SCRIPT, s) }
    pub fn ok(&self, s: &str) -> String { self.paint(C_OK, s) }
    pub fn warn(&self, s: &str) -> String { self.paint(C_WARN, s) }
    pub fn err(&self, s: &str) -> String { self.paint(C_ERR, s) }
    pub fn head(&self, s: &str) -> String { self.paint(C_HEAD, s) }
}

static PALETTE: OnceLock<Palette> = OnceLock::new();

/// Global palette; initialized on first use.
pub fn pal() -> &'static Palette {
    PALETTE.get_or_init(Palette::new)
}

/// True when we can hold an interactive conversation.
pub fn interactive() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

// ─── messages ────────────────────────────────────────────────────────────────

pub fn info(msg: &str) {
    println!("{}  {msg}", pal().dim("::"));
}

pub fn success(msg: &str) {
    println!("{}  {msg}", pal().ok("✓"));
}

pub fn warning(msg: &str) {
    eprintln!("{}  {msg}", pal().warn("!"));
}

pub fn fail(msg: &str) {
    eprintln!("{}  {msg}", pal().err("✗"));
}

/// Section header: `▌ Title`
pub fn header(color: fn(&Palette, &str) -> String, title: &str) {
    println!("\n{} {}", color(pal(), "▌"), pal().bold(title));
}

pub fn header_repo(title: &str) { header(Palette::repo, title); }
pub fn header_aur(title: &str) { header(Palette::aur, title); }
pub fn header_head(title: &str) { header(Palette::head, title); }
pub fn header_err(title: &str) { header(Palette::err, title); }

pub fn blank() { println!(); }

pub fn rule() {
    println!(" {}", pal().dim(&"─".repeat(46)));
}

// ─── sizes ───────────────────────────────────────────────────────────────────

/// Human readable byte count using binary units.
pub fn human_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.1} GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.1} MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.0} KiB", b / KIB)
    } else {
        format!("{} B", bytes)
    }
}

// ─── tables ──────────────────────────────────────────────────────────────────

pub struct Row {
    pub name: String,
    pub version: String,
    pub note: String,
    pub kind: Kind,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Repo,
    Aur,
    Git,
    Script,
    Plain,
    Warn,
}

impl Row {
    pub fn repo(name: &str, version: &str, note: &str) -> Self {
        Self { name: name.into(), version: version.into(), note: note.into(), kind: Kind::Repo }
    }
    pub fn aur(name: &str, version: &str, note: &str) -> Self {
        Self { name: name.into(), version: version.into(), note: note.into(), kind: Kind::Aur }
    }
    pub fn plain(name: &str, version: &str, note: &str) -> Self {
        Self { name: name.into(), version: version.into(), note: note.into(), kind: Kind::Plain }
    }
    pub fn warn(name: &str, version: &str, note: &str) -> Self {
        Self { name: name.into(), version: version.into(), note: note.into(), kind: Kind::Warn }
    }
}

/// Print rows with aligned columns; column width adapts to content.
pub fn table(rows: &[Row]) {
    if rows.is_empty() {
        return;
    }
    let name_w = rows.iter().map(|r| r.name.chars().count()).max().unwrap_or(0).clamp(12, 30);
    let ver_w = rows.iter().map(|r| r.version.chars().count()).max().unwrap_or(0).clamp(6, 18);

    for r in rows {
        let note = match r.kind {
            Kind::Repo => pal().repo(&r.note),
            Kind::Aur => pal().aur(&r.note),
            Kind::Git => pal().git(&r.note),
            Kind::Script => pal().script(&r.note),
            Kind::Warn => pal().warn(&r.note),
            Kind::Plain => pal().dim(&r.note),
        };
        println!(
            "  {}{} {}{} {}",
            pal().bold(&format!("{:<w$}", r.name, w = name_w)),
            "",
            pal().dim(&format!("{:<w$}", r.version, w = ver_w)),
            "",
            note
        );
    }
}

/// Numbered list used by interactive pickers.
pub fn numbered(items: &[String]) {
    let w = items.len().to_string().len().max(2);
    for (i, item) in items.iter().enumerate() {
        println!("   {}) {item}", pal().dim(&format!("{:>w$}", i + 1, w = w)));
    }
}

// ─── prompts ─────────────────────────────────────────────────────────────────

fn flush() {
    let _ = io::stdout().flush();
}

fn read_line() -> Option<String> {
    let mut buf = String::new();
    match io::stdin().read_line(&mut buf) {
        Ok(0) => None,
        Ok(_) => Some(buf.trim_end_matches(['\n', '\r']).to_string()),
        Err(_) => None,
    }
}

/// Ask for a line of text. `default` is used when the answer is empty.
pub fn ask(prompt: &str, default: Option<&str>) -> Option<String> {
    match default {
        Some(d) => print!("  {} {} › ", pal().bold(prompt), pal().dim(&format!("[{d}]"))),
        None => print!("  {} › ", pal().bold(prompt)),
    }
    flush();
    let input = read_line()?;
    if input.trim().eq_ignore_ascii_case("exit") {
        std::process::exit(0);
    }
    if input.trim().eq_ignore_ascii_case("back") {
        return None;
    }
    if input.is_empty() {
        default.map(|d| d.to_string())
    } else {
        Some(input)
    }
}

/// Yes/no question; defaults to `default_yes` on an empty answer.
pub fn confirm(prompt: &str, default_yes: bool) -> bool {
    let hint = if default_yes { "[Y/n]" } else { "[y/N]" };
    print!("  {} {} › ", pal().bold(prompt), pal().dim(hint));
    flush();
    match read_line() {
        None => false,
        Some(line) => {
            if line.trim().eq_ignore_ascii_case("exit") {
                std::process::exit(0);
            }
            let a = line.trim().to_lowercase();
            if a.is_empty() {
                default_yes
            } else {
                a.starts_with('y')
            }
        }
    }
}

/// Numbered menu. Returns `Some(index)` (0-based), or `None` for "back"/EOF.
pub fn menu(title: &str, items: &[String]) -> Option<usize> {
    if items.is_empty() {
        return None;
    }
    println!("  {}", pal().bold(title));
    for (i, item) in items.iter().enumerate() {
        println!("   {}) {item}", pal().dim(&(i + 1).to_string()));
    }
    loop {
        print!(
            "  {} {}{} › ",
            pal().bold("choice"),
            pal().dim("(back — назад) "),
            ""
        );
        flush();
        let line = read_line()?;
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.eq_ignore_ascii_case("back") {
            return None;
        }
        if t.eq_ignore_ascii_case("exit") {
            std::process::exit(0);
        }
        if let Ok(n) = t.parse::<usize>() {
            if n <= items.len() {
                return Some(n - 1);
            }
        }
        println!("  {} enter a number 1..{}", pal().warn("!"), items.len());
    }
}

/// Multi-select by index: accepts `1,3-5`, `all`, or `0` for back.
pub fn pick_many(max: usize, prompt: &str) -> Option<Vec<usize>> {
    loop {
        print!("  {} › ", pal().bold(prompt));
        flush();
        let line = read_line()?;
        if line.trim().eq_ignore_ascii_case("exit") {
            std::process::exit(0);
        }
        let t: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        if t.is_empty() {
            println!("  {} nothing entered", pal().warn("!"));
            continue;
        }
        if t == "all" || t == "*" {
            return Some((1..=max).collect());
        }
        let mut picked = Vec::new();
        let mut ok = true;
        for part in t.split(',') {
            if let Some((a, b)) = part.split_once('-') {
                match (a.parse::<usize>(), b.parse::<usize>()) {
                    (Ok(a), Ok(b)) if a >= 1 && b <= max && a <= b => {
                        picked.extend(a..=b);
                    }
                    _ => {
                        ok = false;
                        break;
                    }
                }
            } else {
                match part.parse::<usize>() {
                    Ok(n) if n >= 1 && n <= max => picked.push(n),
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
        }
        if ok && !picked.is_empty() {
            return Some(picked);
        }
        println!("  {} cannot parse: {t}", pal().warn("!"));
    }
}

/// Wait for Enter so the user can read the output before the menu redraws.
pub fn pause() {
    if !interactive() {
        return;
    }
    print!("\n  {} ", pal().dim("press Enter to return to menu"));
    flush();
    let _ = read_line();
}
