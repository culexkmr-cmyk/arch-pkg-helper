//! Terminal UI primitives: colors, prompts, menus, tables.
//!
//! Everything degrades gracefully: when stdout is not a terminal or `NO_COLOR`
//! is set, colors, spinners and screen clears are disabled.

use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

const RESET: &str = "\x1b[0m";

const C_REPO: &str = "38;5;39"; // blue   — official repositories
const C_AUR: &str = "38;5;214"; // orange — AUR
const C_GIT: &str = "38;5;141"; // violet — git remotes
const C_SCRIPT: &str = "38;5;209"; // coral  — install scripts
const C_OK: &str = "38;5;114"; // green
const C_WARN: &str = "38;5;221"; // yellow
const C_ERR: &str = "38;5;203"; // red
const C_HEAD: &str = "38;5;111"; // steel blue — section headers
const C_ACCENT: &str = "1;38;5;75"; // bright blue — numbers, prompts
const C_LOGO: &str = "1;38;5;32"; // Arch blue

pub struct Palette {
    enabled: bool,
}

impl Palette {
    fn new() -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        Self { enabled: io::stdout().is_terminal() && !no_color }
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}{RESET}")
        } else {
            text.to_string()
        }
    }

    pub fn bold(&self, s: &str) -> String {
        self.paint("1", s)
    }
    pub fn dim(&self, s: &str) -> String {
        self.paint("2", s)
    }
    pub fn repo(&self, s: &str) -> String {
        self.paint(C_REPO, s)
    }
    pub fn aur(&self, s: &str) -> String {
        self.paint(C_AUR, s)
    }
    pub fn git(&self, s: &str) -> String {
        self.paint(C_GIT, s)
    }
    pub fn script(&self, s: &str) -> String {
        self.paint(C_SCRIPT, s)
    }
    pub fn ok(&self, s: &str) -> String {
        self.paint(C_OK, s)
    }
    pub fn warn(&self, s: &str) -> String {
        self.paint(C_WARN, s)
    }
    pub fn err(&self, s: &str) -> String {
        self.paint(C_ERR, s)
    }
    pub fn head(&self, s: &str) -> String {
        self.paint(C_HEAD, s)
    }
    pub fn accent(&self, s: &str) -> String {
        self.paint(C_ACCENT, s)
    }
    pub fn logo(&self, s: &str) -> String {
        self.paint(C_LOGO, s)
    }
}

static PALETTE: OnceLock<Palette> = OnceLock::new();

pub fn pal() -> &'static Palette {
    PALETTE.get_or_init(Palette::new)
}

/// True when we can hold an interactive conversation.
pub fn interactive() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

/// Terminal width in columns (80 when unknown).
pub fn width() -> usize {
    #[repr(C)]
    #[derive(Default)]
    struct WinSize {
        rows: u16,
        cols: u16,
        xpixel: u16,
        ypixel: u16,
    }
    extern "C" {
        fn ioctl(fd: i32, request: u64, ...) -> i32;
    }
    const TIOCGWINSZ: u64 = 0x5413;
    let mut ws = WinSize::default();
    if unsafe { ioctl(1, TIOCGWINSZ, &mut ws as *mut WinSize) } == 0 && ws.cols > 0 {
        return ws.cols as usize;
    }
    std::env::var("COLUMNS").ok().and_then(|c| c.parse().ok()).unwrap_or(80)
}

/// Width used for boxes and rules: the terminal, capped so lines stay readable.
fn layout_width() -> usize {
    width().clamp(40, 76)
}

/// Visible length, ignoring ANSI escapes.
pub fn vis_len(s: &str) -> usize {
    crate::sys::strip_ansi(s).chars().count()
}

/// Cut plain text to `max` columns with an ellipsis.
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Word-wrap plain text into lines of at most `max` columns.
pub fn wrap(s: &str, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > max {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Clear the screen for a fresh view (scrollback is kept).
pub fn clear() {
    if interactive() && pal().enabled {
        print!("\x1b[H\x1b[2J");
        flush();
    }
}

// ─── messages ────────────────────────────────────────────────────────────────

pub fn info(msg: &str) {
    println!("  {} {msg}", pal().accent("›"));
}

pub fn success(msg: &str) {
    println!("  {} {msg}", pal().ok("✓"));
}

pub fn warning(msg: &str) {
    eprintln!("  {} {msg}", pal().warn("!"));
}

pub fn fail(msg: &str) {
    eprintln!("  {} {}", pal().err("✗"), pal().err(msg));
}

/// Section header: `── title ──────────`
pub fn header(color: fn(&Palette, &str) -> String, title: &str) {
    let p = pal();
    let rest = layout_width().saturating_sub(title.chars().count() + 6);
    println!("\n  {} {} {}", color(p, "──"), p.bold(title), p.dim(&"─".repeat(rest)));
}

pub fn header_repo(title: &str) {
    header(Palette::repo, title);
}
pub fn header_aur(title: &str) {
    header(Palette::aur, title);
}
pub fn header_head(title: &str) {
    header(Palette::head, title);
}
pub fn header_err(title: &str) {
    header(Palette::err, title);
}
pub fn header_git(title: &str) {
    header(Palette::git, title);
}
pub fn header_script(title: &str) {
    header(Palette::script, title);
}

pub fn blank() {
    println!();
}

pub fn rule() {
    println!("  {}", pal().dim(&"─".repeat(layout_width() - 2)));
}

/// Rounded box around already-colored lines.
pub fn boxed(lines: &[String]) {
    let p = pal();
    let inner = lines.iter().map(|l| vis_len(l)).max().unwrap_or(0).max(layout_width() - 6);
    println!("  {}", p.dim(&format!("╭{}╮", "─".repeat(inner + 2))));
    for l in lines {
        let pad = " ".repeat(inner - vis_len(l));
        println!("  {} {l}{pad} {}", p.dim("│"), p.dim("│"));
    }
    println!("  {}", p.dim(&format!("╰{}╯", "─".repeat(inner + 2))));
}

/// `██████░░░░` for a 0..=100 percentage.
pub fn bar(percent: u32, cells: usize) -> String {
    let p = pal();
    let filled = (percent.min(100) as usize * cells + 50) / 100;
    let color = match percent {
        0..=74 => Palette::ok,
        75..=89 => Palette::warn,
        _ => Palette::err,
    };
    format!("{}{}", color(p, &"█".repeat(filled)), p.dim(&"░".repeat(cells - filled)))
}

/// Run `work` while a spinner with `msg` turns on stderr.
pub fn spin<T>(msg: &str, work: impl FnOnce() -> T) -> T {
    if !interactive() || !pal().enabled {
        return work();
    }
    let done = AtomicBool::new(false);
    std::thread::scope(|s| {
        s.spawn(|| {
            const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let mut i = 0;
            // a short grace period keeps fast calls from flickering
            std::thread::sleep(Duration::from_millis(120));
            while !done.load(Ordering::Relaxed) {
                eprint!("\r  {} {}", pal().accent(FRAMES[i % FRAMES.len()]), pal().dim(msg));
                let _ = io::stderr().flush();
                i += 1;
                std::thread::sleep(Duration::from_millis(80));
            }
            if i > 0 {
                eprint!("\r\x1b[2K");
                let _ = io::stderr().flush();
            }
        });
        let out = work();
        done.store(true, Ordering::Relaxed);
        out
    })
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
    Plain,
    Warn,
}

impl Row {
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
    print_rows(rows, false);
}

/// Same as `table`, with 1-based indices for pickers.
pub fn numbered_table(rows: &[Row]) {
    print_rows(rows, true);
}

fn print_rows(rows: &[Row], numbered: bool) {
    if rows.is_empty() {
        return;
    }
    let p = pal();
    let name_w = rows.iter().map(|r| r.name.chars().count()).max().unwrap_or(0).clamp(8, 32);
    let ver_w = rows.iter().map(|r| r.version.chars().count()).max().unwrap_or(0).clamp(4, 20);
    let idx_w = rows.len().to_string().len();

    for (i, r) in rows.iter().enumerate() {
        let (dot, note) = match r.kind {
            Kind::Repo => (p.repo("●"), p.repo(&r.note)),
            Kind::Aur => (p.aur("●"), p.aur(&r.note)),
            Kind::Warn => (p.warn("●"), p.warn(&r.note)),
            Kind::Plain => (p.dim("·"), p.dim(&r.note)),
        };
        let idx = if numbered { format!("{} ", p.accent(&format!("{:>idx_w$}", i + 1))) } else { String::new() };
        let name = truncate(&r.name, name_w);
        println!(
            "  {idx}{dot} {} {} {note}",
            p.bold(&format!("{name:<name_w$}")),
            p.dim(&format!("{:<ver_w$}", truncate(&r.version, ver_w))),
        );
    }
}

/// `name old -> new` lines (checkupdates, paru -Qu) as an aligned table.
pub fn updates_table(lines: &[String], color: fn(&Palette, &str) -> String) {
    let p = pal();
    let parsed: Vec<Option<(&str, &str, &str)>> = lines
        .iter()
        .map(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            match w.as_slice() {
                [name, old, "->", new, ..] => Some((*name, *old, *new)),
                _ => None,
            }
        })
        .collect();
    let name_w = parsed.iter().flatten().map(|(n, _, _)| n.chars().count()).max().unwrap_or(0).min(32);
    let old_w = parsed.iter().flatten().map(|(_, o, _)| o.chars().count()).max().unwrap_or(0).min(24);
    for (line, item) in lines.iter().zip(&parsed) {
        match item {
            Some((name, old, new)) => println!(
                "  {} {} {} {} {}",
                color(p, "●"),
                p.bold(&format!("{:<name_w$}", truncate(name, name_w))),
                p.dim(&format!("{old:>old_w$}")),
                p.dim("→"),
                p.ok(new)
            ),
            None => println!("  {} {line}", color(p, "●")),
        }
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

fn question(prompt: &str, hint: &str) {
    let p = pal();
    let hint = if hint.is_empty() { String::new() } else { format!(" {}", p.dim(hint)) };
    print!("  {} {}{hint} {} ", p.accent("?"), p.bold(prompt), p.accent("›"));
    flush();
}

/// Ask for a line of text. `default` is used when the answer is empty.
pub fn ask(prompt: &str, default: Option<&str>) -> Option<String> {
    question(prompt, &default.map(|d| format!("[{d}]")).unwrap_or_default());
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
    question(prompt, if default_yes { "(Y/n)" } else { "(y/N)" });
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
                a.starts_with('y') || a.starts_with('д')
            }
        }
    }
}

/// One menu entry: `  3  Label`.
pub fn menu_item(n: usize, label: &str) -> String {
    format!("{}  {label}", pal().accent(&format!("{n:>2}")))
}

/// Read a menu number 1..=max. `None` for 0/back/EOF.
pub fn read_choice(max: usize) -> Option<usize> {
    loop {
        print!("\n  {} ", pal().accent("❯"));
        flush();
        let line = read_line()?;
        let t = line.trim();
        if t.is_empty() {
            if pal().enabled {
                print!("\x1b[2A");
            }
            continue;
        }
        if t.eq_ignore_ascii_case("back") || t.eq_ignore_ascii_case("q") {
            return None;
        }
        if t.eq_ignore_ascii_case("exit") {
            std::process::exit(0);
        }
        match t.parse::<usize>() {
            Ok(0) => return None,
            Ok(n) if n <= max => return Some(n - 1),
            _ => {}
        }
        warning(&format!("enter a number 1..{max} (0 — back)"));
    }
}

/// Numbered menu. Returns `Some(index)` (0-based), or `None` for "back"/EOF.
pub fn menu(title: &str, items: &[String]) -> Option<usize> {
    if items.is_empty() {
        return None;
    }
    let p = pal();
    println!("  {} {}", p.accent("?"), p.bold(title));
    for (i, item) in items.iter().enumerate() {
        println!("  {}", menu_item(i + 1, item));
    }
    println!("  {}", p.dim(&format!("{:>2}  back", 0)));
    read_choice(items.len())
}

/// Multi-select by index: accepts `1,3-5`, `1 3 5`, `all`, or `0` for back.
pub fn pick_many(max: usize, prompt: &str) -> Option<Vec<usize>> {
    loop {
        question(prompt, "");
        let line = read_line()?;
        let t = line.trim();
        if t.eq_ignore_ascii_case("exit") {
            std::process::exit(0);
        }
        if t.is_empty() {
            warning("nothing entered");
            continue;
        }
        match parse_selection(t, max) {
            Selection::Back => return None,
            Selection::Picked(v) => return Some(v),
            Selection::Invalid => warning(&format!("cannot parse: {t} (1..{max})")),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Selection {
    Back,
    Picked(Vec<usize>),
    Invalid,
}

/// Parse `1,3-5 7`, `all`/`*` or `0`/`back` into 1-based indices, in input
/// order and without duplicates.
pub fn parse_selection(input: &str, max: usize) -> Selection {
    let t = input.trim().to_lowercase();
    if t == "0" || t == "back" {
        return Selection::Back;
    }
    if t == "all" || t == "*" {
        return Selection::Picked((1..=max).collect());
    }
    let mut picked = Vec::new();
    // "3 - 5" is a range, not three separate tokens
    let normalized = t.replace(" -", "-").replace("- ", "-");
    for part in normalized.split(|c: char| c == ',' || c.is_whitespace()).filter(|p| !p.is_empty()) {
        let range = match part.split_once('-') {
            Some((a, b)) => match (a.parse::<usize>(), b.parse::<usize>()) {
                (Ok(a), Ok(b)) if a >= 1 && a <= b && b <= max => a..=b,
                _ => return Selection::Invalid,
            },
            None => match part.parse::<usize>() {
                Ok(n) if n >= 1 && n <= max => n..=n,
                _ => return Selection::Invalid,
            },
        };
        for n in range {
            if !picked.contains(&n) {
                picked.push(n);
            }
        }
    }
    if picked.is_empty() {
        Selection::Invalid
    } else {
        Selection::Picked(picked)
    }
}

/// Wait for Enter so the user can read the output before the menu redraws.
pub fn pause() {
    if !interactive() {
        return;
    }
    print!("\n  {} ", pal().dim("↵ Enter — back to menu"));
    flush();
    let _ = read_line();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection() {
        assert_eq!(parse_selection("1,3-5", 6), Selection::Picked(vec![1, 3, 4, 5]));
        assert_eq!(parse_selection("1 3 5", 6), Selection::Picked(vec![1, 3, 5]));
        assert_eq!(parse_selection("2 - 3, 2", 6), Selection::Picked(vec![2, 3]));
        assert_eq!(parse_selection("all", 3), Selection::Picked(vec![1, 2, 3]));
        assert_eq!(parse_selection("0", 3), Selection::Back);
        assert_eq!(parse_selection("4", 3), Selection::Invalid);
        assert_eq!(parse_selection("5-2", 6), Selection::Invalid);
        assert_eq!(parse_selection("x", 6), Selection::Invalid);
    }

    #[test]
    fn bytes() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2 KiB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024 / 2), "1.5 GiB");
    }

    #[test]
    fn text_helpers() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("abc", 4), "abc");
        assert_eq!(wrap("aa bb cc dd", 5), ["aa bb", "cc dd"]);
        assert_eq!(vis_len("\x1b[1mab\x1b[0m"), 2);
    }
}
