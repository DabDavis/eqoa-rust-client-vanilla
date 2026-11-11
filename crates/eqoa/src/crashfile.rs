//! The client's own crash, kept and handed to the server it plays on at the
//! next login (the server owner, 2026-11-11, after two players' pages stopped
//! at once in a raid: "we may need to add crash reports back"). Vanilla sent
//! crash reports to a report server of its own until 2026-10-31; now a crash
//! goes, like a bug report, only to the server the player plays on, as a
//! petition of its character's (go-eqoa-server's Bug Report / Crash), and only
//! to one that asks for them (0x7E64, `net::session::OP_CRASH_OFFER`).
//!
//! A panic (one `logfile::caught` does not step over) is written as it
//! happens: natively a file in the config directory's `crashes/`
//! (`crash-<stamp>.txt`, at most [`KEPT`], the oldest let go), in a page the
//! origin's localStorage ([`PAGE_KEY`]). What it says: the build, the
//! platform, the graphics, when, the thread, the panic's place and message and
//! the backtrace (natively Rust's, in a page the JavaScript stack), file paths
//! cut to the source tree's (`crates/...`) and the player's own folders said
//! as `~`: no account name, no address, no path of the player's
//! ([`scrub`]). At the next login a server that offers to take them is sent
//! each kept ([`send_pending`], at most [`SENT_A_LOGIN`], newest first, each
//! cut to what the offer says it takes), and each is let go at the server's
//! answer (0x7E66), filed or not: a report is never sent twice. A server that
//! makes no offer (Sandstorm's, eqoa.live's, an older go-eqoa-server) is sent
//! nothing, and a crash stays kept for one that does. Nothing else leaves:
//! the log stays the player's (`logfile`).

use net::session::Session;

/// The most crashes kept natively.
pub const KEPT: usize = 5;
/// The most sent at a login.
pub const SENT_A_LOGIN: usize = 3;
/// In a page, where the last crash's report is kept (web/notices.js keeps its
/// one line for the bug note, `bugnote::last_crash`, apart).
#[cfg(target_arch = "wasm32")]
pub const PAGE_KEY: &str = "eqoa.crashReport";

/// A report's text: the facts, then the panic and its stack.
pub fn compose(facts: &[(&str, String)], panic: &str, stack: &str) -> String {
    let mut out = String::from("EQOA client crash report\n");
    for (k, v) in facts {
        out.push_str(&format!("{k}: {v}\n"));
    }
    out.push_str(&format!("panic: {}\n", scrub(panic).replace('\n', " ")));
    let stack = scrub(stack);
    if !stack.trim().is_empty() {
        out.push_str("stack backtrace:\n");
        out.push_str(stack.trim_end());
        out.push('\n');
    }
    out
}

/// `text` with no path of the player's: each path cut to the source tree's
/// part (`crates/`, `src/`, `library/`: the build's, never the player's) or
/// else to its file name, and the player's home said as `~`.
pub fn scrub(text: &str) -> String {
    let home = home();
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let line = match home.as_deref() {
            Some(h) if h.len() > 1 => line.replace(h, "~"),
            _ => line.to_string(),
        };
        let words: Vec<String> = line.split(' ').map(cut_path).collect();
        out.push_str(&words.join(" "));
    }
    out
}

/// One word, a path cut to its source tree's part, else its file name.
fn cut_path(w: &str) -> String {
    let path_like = (w.starts_with('/') || w.starts_with("~/") || w.get(1..3) == Some(":\\") || w.get(1..3) == Some(":/")) && w.len() > 2;
    if !path_like {
        return w.to_string();
    }
    let w2 = w.replace('\\', "/");
    for root in ["/crates/", "/library/", "/src/"] {
        if let Some(i) = w2.find(root) {
            return w2[i + 1..].to_string();
        }
    }
    w2.rsplit('/').next().unwrap_or("").to_string()
}

/// The player's home folder, natively; none in a page.
fn home() -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var("HOME").ok().or_else(|| std::env::var("USERPROFILE").ok()).filter(|h| !h.is_empty())
    }
    #[cfg(target_arch = "wasm32")]
    None
}

/// The facts a report starts with: the build, the platform, the graphics
/// (`bugnote`, read without waiting on a lock: a panic may hold one), when.
pub fn facts(when: String) -> Vec<(&'static str, String)> {
    let mut f = vec![("build", crate::bugnote::build()), ("platform", crate::bugnote::platform())];
    if let Some(g) = crate::bugnote::graphics_now() {
        f.push(("graphics", g));
    }
    f.push(("when", when));
    f
}

/// The message of a panic's payload.
pub fn payload_text(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "a panic".into())
}

/// A report kept: its number (the server's answer names it) and its text.
#[derive(Clone, Debug, PartialEq)]
pub struct Kept {
    pub number: u32,
    pub text: String,
}

/// The number a report is known by: a CRC of its name (natively its file's,
/// in a page its key's and text's).
fn number_of(name: &str) -> u32 {
    let mut c = flate2::Crc::new();
    c.update(name.as_bytes());
    c.sum().max(1)
}

// ---- natively: files in the config directory's crashes/ ------------------

#[cfg(not(target_arch = "wasm32"))]
fn dir() -> Option<std::path::PathBuf> {
    crate::settings::config_dir().map(|d| d.join("crashes"))
}

/// The kept reports' files in `d`, newest first.
#[cfg(not(target_arch = "wasm32"))]
fn files_in(d: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut v: Vec<std::path::PathBuf> = std::fs::read_dir(d)
        .map(|r| r.flatten().map(|e| e.path()).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("crash-") && n.ends_with(".txt"))).collect())
        .unwrap_or_default();
    v.sort();
    v.reverse();
    v
}

/// Writes a report as a panic happens (`logfile`'s hook), the oldest past
/// [`KEPT`] let go; its file, or None when it could not be written.
#[cfg(not(target_arch = "wasm32"))]
pub fn write(stamp: &str, text: &str) -> Option<std::path::PathBuf> {
    write_in(&dir()?, stamp, text)
}

#[cfg(not(target_arch = "wasm32"))]
fn write_in(d: &std::path::Path, stamp: &str, text: &str) -> Option<std::path::PathBuf> {
    std::fs::create_dir_all(d).ok()?;
    let path = d.join(format!("crash-{stamp}.txt"));
    std::fs::write(&path, text).ok()?;
    for old in files_in(d).into_iter().skip(KEPT) {
        let _ = std::fs::remove_file(old);
    }
    Some(path)
}

/// The reports kept, newest first, at most [`SENT_A_LOGIN`].
#[cfg(not(target_arch = "wasm32"))]
pub fn pending() -> Vec<Kept> {
    dir().map(|d| pending_in(&d)).unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
fn pending_in(d: &std::path::Path) -> Vec<Kept> {
    files_in(d)
        .into_iter()
        .take(SENT_A_LOGIN)
        .filter_map(|p| {
            let text = std::fs::read_to_string(&p).ok()?;
            let name = p.file_name()?.to_str()?.to_string();
            Some(Kept { number: number_of(&name), text })
        })
        .collect()
}

/// The report the server answered (`number`): let go.
#[cfg(not(target_arch = "wasm32"))]
pub fn answered(number: u32) -> bool {
    dir().is_some_and(|d| answered_in(&d, number))
}

#[cfg(not(target_arch = "wasm32"))]
fn answered_in(d: &std::path::Path, number: u32) -> bool {
    for p in files_in(d) {
        if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| number_of(n) == number) {
            return std::fs::remove_file(p).is_ok();
        }
    }
    false
}

// ---- in a page: localStorage ----------------------------------------------

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// Keeps the page's report as it stops (`web::start`'s hook): one, the last.
#[cfg(target_arch = "wasm32")]
pub fn write_page(text: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(PAGE_KEY, text);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn pending() -> Vec<Kept> {
    storage()
        .and_then(|s| s.get_item(PAGE_KEY).ok().flatten())
        .filter(|t| !t.trim().is_empty())
        .map(|text| vec![Kept { number: number_of(&text), text }])
        .unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
pub fn answered(number: u32) -> bool {
    let Some(s) = storage() else { return false };
    match s.get_item(PAGE_KEY).ok().flatten() {
        Some(t) if number_of(&t) == number => s.remove_item(PAGE_KEY).is_ok(),
        _ => false,
    }
}

// ---- both: the session --------------------------------------------------

/// The server offered to take crash reports (`most` bytes of each): each kept
/// is sent. How many went.
pub fn send_pending(session: &mut Session, most: usize) -> usize {
    let kept = pending();
    for k in &kept {
        session.send_crash_report(k.number, &k.text, most);
    }
    if !kept.is_empty() {
        eprintln!("crash: {} report(s) of this client's sent to the server", kept.len());
    }
    kept.len()
}

/// The server's answers (`Session::crash_filed`): each report let go.
pub fn take_answers(session: &mut Session) {
    for (number, ticket) in std::mem::take(&mut session.crash_filed) {
        let gone = answered(number);
        if ticket != 0 {
            eprintln!("crash: report {number:08x} filed as Ticket #{ticket} (let go: {gone})");
        } else {
            eprintln!("crash: report {number:08x} not kept by the server (let go: {gone})");
        }
    }
}

/// `EQOA_CRASH_TEST=<ms>`, a debug build's alone: the game panics that many
/// ms into play, for a test of the reports (never in a release build).
pub fn test_panic_due(play_ms: i64) -> bool {
    #[cfg(debug_assertions)]
    {
        static AT: std::sync::OnceLock<Option<i64>> = std::sync::OnceLock::new();
        AT.get_or_init(|| std::env::var("EQOA_CRASH_TEST").ok().and_then(|v| v.parse().ok())).is_some_and(|at| play_ms >= at)
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = play_ms;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path in a panic or a frame is cut to the source tree's part, or its
    /// file name, and the home folder said as `~`; other words stay. The
    /// control: a line with no path is as it was.
    #[test]
    fn a_report_carries_no_path_of_the_players() {
        let stack = "   2: eqoa::play::Play::frame\n             at /home/runner/work/eqoa/crates/eqoa/src/play.rs:1234:56\n             at /rustc/abc/library/core/src/panicking.rs:75:14";
        let s = scrub(stack);
        assert!(s.contains("at crates/eqoa/src/play.rs:1234:56"), "{s}");
        assert!(s.contains("at library/core/src/panicking.rs:75:14"), "{s}");
        assert!(!s.contains("/home/runner"), "{s}");
        assert_eq!(scrub("open C:\\Users\\ann\\eqoa\\disc.iso failed"), "open disc.iso failed");
        assert_eq!(scrub("index out of bounds: the len is 4 but the index is 7"), "index out of bounds: the len is 4 but the index is 7");
        let r = compose(&[("build", "local".into())], "panicked at crates/a.rs:1:2:\nboom", "  0: x\n      at /opt/x/crates/a.rs:1:2\n");
        assert_eq!(r, "EQOA client crash report\nbuild: local\npanic: panicked at crates/a.rs:1:2: boom\nstack backtrace:\n  0: x\n      at crates/a.rs:1:2\n");
    }

    /// Natively a report is a file in the config directory's crashes/, the
    /// newest first, at most KEPT; an answer lets its file go, another
    /// number none. The control: with none kept nothing is pending.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn reports_are_kept_and_let_go_at_the_answer() {
        let d = std::env::temp_dir().join(format!("eqoa-crashfile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        assert!(pending_in(&d).is_empty(), "none kept");
        for i in 0..KEPT + 2 {
            write_in(&d, &format!("20261111-0452{i:02}"), &format!("report {i}")).unwrap();
        }
        assert_eq!(files_in(&d).len(), KEPT, "the oldest let go");
        let p = pending_in(&d);
        assert_eq!(p.len(), SENT_A_LOGIN);
        assert_eq!(p[0].text, format!("report {}", KEPT + 1), "newest first");
        assert!(!answered_in(&d, p[0].number ^ 1), "another number lets none go");
        assert!(answered_in(&d, p[0].number));
        assert_eq!(files_in(&d).len(), KEPT - 1);
        assert_eq!(pending_in(&d)[0].text, format!("report {}", KEPT));
        let _ = std::fs::remove_dir_all(&d);
    }
}
