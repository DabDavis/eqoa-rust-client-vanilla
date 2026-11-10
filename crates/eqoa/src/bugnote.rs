//! The client's note with a bug report (go-eqoa-server's 0x7E0F,
//! `net::session::OP_BUG_NOTE`), sent just before a petition goes to a server
//! that offered bug reports (`Session::bug_reports`): what the server cannot
//! know of this client. Its build, the platform, the graphics adapter, the
//! frame rate it draws at, the controllers connected and how each is read
//! (`input::note`: its layout and where its d-pad comes from), and what it
//! shows of the player and its target (`Play::bug_note`). The GMs read it
//! beside the petition; the player never does, and nothing goes to a server
//! that did not ask. No account name, no address, no file path is in it.

use std::sync::Mutex;

/// The graphics adapter and its backend, as the window's device was made
/// (`set_graphics`).
static GRAPHICS: Mutex<String> = Mutex::new(String::new());
/// A page's browser (`navigator.userAgent`), `set_browser`.
static BROWSER: Mutex<String> = Mutex::new(String::new());
/// The frames drawn: the last one's time (ms) and the interval's running
/// average (ms), `frame_drawn`.
static FRAMES: Mutex<(f64, f64)> = Mutex::new((-1.0, 0.0));

/// The adapter the game draws with.
pub fn set_graphics(name: &str, backend: &str) {
    *GRAPHICS.lock().unwrap() = format!("{} ({backend})", name.trim());
}

/// The page's browser.
#[allow(dead_code)] // natively there is none
pub fn set_browser(user_agent: &str) {
    *BROWSER.lock().unwrap() = user_agent.trim().to_string();
}

/// A frame drawn at `now` (ms): the frame rate's running average moves a
/// twentieth of the way to it, a pause past a second not counted.
pub fn frame_drawn(now: f64) {
    let mut f = FRAMES.lock().unwrap();
    let (last, avg) = *f;
    let dt = now - last;
    if last >= 0.0 && dt > 0.0 && dt < 1000.0 {
        f.1 = if avg == 0.0 { dt } else { avg + (dt - avg) / 20.0 };
    }
    f.0 = now;
}

/// The frames drawn a second, lately; None before any were.
pub fn frame_rate() -> Option<f64> {
    let avg = FRAMES.lock().unwrap().1;
    (avg > 0.0).then(|| 1000.0 / avg)
}

/// The build this client is: CI's commit, a page's build (main.js's
/// `window.eqoaBuild`, the `v/<build>/` it was loaded from: a page is built by
/// `web/build.sh`, not CI, so it carries no commit), else a build of one's own.
pub fn build() -> String {
    match crate::update::COMMIT {
        Some(c) => format!("dev-{}", &c[..c.len().min(7)]),
        None => page_build().map_or_else(|| "local".into(), |b| format!("page {b}")),
    }
}

/// A page's build (`window.eqoaBuild`), None natively or in a page that is
/// not a built one.
fn page_build() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsValue;
        let w = web_sys::window()?;
        js_sys::Reflect::get(&w, &JsValue::from_str("eqoaBuild")).ok()?.as_string().filter(|b| !b.is_empty())
    }
    #[cfg(not(target_arch = "wasm32"))]
    None
}

/// Where a page keeps its last crash (web/notices.js writes it as the client
/// stops: when, the page's build, the title and the panic's place and words),
/// for the next session's note: the page sends no crash reports, so a crash a
/// player can only describe comes with its words the next time they report.
#[cfg(target_arch = "wasm32")]
const LAST_CRASH: &str = "eqoa.lastCrash";

/// The page's last crash, not yet in a note; None natively.
pub fn last_crash() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let s = web_sys::window()?.local_storage().ok().flatten()?;
        s.get_item(LAST_CRASH).ok().flatten().map(|t| crash_line(&t)).filter(|t| !t.is_empty())
    }
    #[cfg(not(target_arch = "wasm32"))]
    None
}

/// The last crash has gone with a note: it is not said again.
pub fn crash_sent() {
    #[cfg(target_arch = "wasm32")]
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.remove_item(LAST_CRASH);
    }
}

/// A kept crash as the note says it: one line, no control characters, at
/// most 600 characters (what the page wrote, should it have written more).
pub fn crash_line(kept: &str) -> String {
    kept.chars().map(|c| if c.is_control() { ' ' } else { c }).take(600).collect::<String>().trim().to_string()
}

/// The platform: the operating system and machine natively, the browser in
/// a page.
pub fn platform() -> String {
    let browser = BROWSER.lock().unwrap().clone();
    if !browser.is_empty() {
        return format!("web page, {browser}");
    }
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

/// The graphics line, None before a device was made.
pub fn graphics() -> Option<String> {
    let g = GRAPHICS.lock().unwrap().clone();
    (!g.is_empty()).then_some(g)
}

/// The note's lines, the client's facts first, then what play adds.
pub fn compose(play: &[(&str, String)]) -> String {
    let mut lines = vec![format!("build: {}", build()), format!("platform: {}", platform())];
    if let Some(g) = graphics() {
        lines.push(format!("graphics: {g}"));
    }
    if let Some(r) = frame_rate() {
        lines.push(format!("frame rate: {r:.0} a second"));
    }
    // the controllers and how each is read (its layout, where its d-pad
    // comes from): what a player's controller sends
    let pads = crate::input::note();
    if !pads.is_empty() {
        lines.push(format!("controllers: {pads}"));
    }
    for (k, v) in play {
        lines.push(format!("{k}: {v}"));
    }
    if let Some(c) = last_crash() {
        lines.push(format!("last crash: {c}"));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page's kept crash (web/notices.js) as the note says it: on one line,
    /// trimmed, at most 600 characters. The control: a plain line is taken as
    /// it is.
    #[test]
    fn a_kept_crash_is_one_line_in_the_note() {
        let kept = "2026-11-09T05:00:00Z, page 54bb79225fde: The client stopped: panicked at crates/drdp/src/list.rs:110:23:\ndangling handle\t";
        assert_eq!(crash_line(kept), "2026-11-09T05:00:00Z, page 54bb79225fde: The client stopped: panicked at crates/drdp/src/list.rs:110:23: dangling handle");
        assert_eq!(crash_line(&"x".repeat(900)).len(), 600);
        let plain = "2026-11-09T05:00:00Z, page abc: The graphics stopped: the graphics device was lost";
        assert_eq!(crash_line(plain), plain, "the control: a plain line as it is");
        // natively there is no page: no crash to say, and the build is local
        assert_eq!(last_crash(), None);
        assert!(!compose(&[]).contains("last crash"));
    }

    // The running average takes the frames' interval and leaves out a pause;
    // the note starts with the client's own facts, then play's lines.
    #[test]
    fn a_note_says_the_client_and_what_play_adds() {
        let mut t = 0.0;
        for _ in 0..200 {
            frame_drawn(t);
            t += 1000.0 / 60.0;
        }
        frame_drawn(t + 5000.0); // a pause: not a frame's interval
        let r = frame_rate().unwrap();
        assert!((r - 60.0).abs() < 1.0, "{r}");
        set_graphics("Test GPU ", "Vulkan");
        let note = compose(&[("place", "world 0 (1.0, 2.0, 3.0)".into())]);
        let lines: Vec<&str> = note.lines().collect();
        assert!(lines[0].starts_with("build: "), "{note}");
        assert!(lines[1].starts_with("platform: "), "{note}");
        assert!(lines.contains(&"graphics: Test GPU (Vulkan)"), "{note}");
        assert!(lines.contains(&"frame rate: 60 a second"), "{note}");
        assert_eq!(lines.last(), Some(&"place: world 0 (1.0, 2.0, 3.0)"));
    }
}
