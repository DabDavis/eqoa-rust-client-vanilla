//! The client's note with a bug report (go-eqoa-server's 0x7E0F,
//! `net::session::OP_BUG_NOTE`), sent just before a petition goes to a server
//! that offered bug reports (`Session::bug_reports`): what the server cannot
//! know of this client. Its build, the platform, the graphics adapter, the
//! frame rate it draws at, and what it shows of the player and its target
//! (`Play::bug_note`). The GMs read it beside the petition; the player never
//! does, and nothing goes to a server that did not ask. No account name, no
//! address, no file path is in it.

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

/// The build this client is: CI's commit, else a build of one's own.
pub fn build() -> String {
    match crate::update::COMMIT {
        Some(c) => format!("dev-{}", &c[..c.len().min(7)]),
        None => "local".into(),
    }
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
    for (k, v) in play {
        lines.push(format!("{k}: {v}"));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

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
