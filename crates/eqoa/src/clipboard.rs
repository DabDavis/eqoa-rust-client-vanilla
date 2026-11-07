//! The system's clipboard for the game's edit boxes (the user, 2026-11-07: "we
//! can add the copy/paste into vanilla client", `editbox`'s `paste_chars` holding
//! it to what a console's keyboard can type) and the chat box's lines
//! (`chat::copy_text`). Natively: `arboard`, on a thread of
//! its own (another program answers a read when it answers, and the game does not
//! wait for it; on X11 the text copied is served from the one `Clipboard` kept for
//! the whole run). A page has its own way in, the browser's `paste` event
//! (`web/main.js`), and Android has none: there the device keyboard's own paste
//! comes as typing (`android`).

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod native {
    use std::sync::mpsc::{self, Sender};
    use std::sync::{Mutex, OnceLock};

    enum Ask {
        Get,
        Set(String),
    }

    static ASKS: OnceLock<Sender<Ask>> = OnceLock::new();
    /// What reads have brought, for the next frame.
    pub static GOT: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn ask(a: Ask) {
        let tx = ASKS.get_or_init(|| {
            let (tx, rx) = mpsc::channel();
            let spawned = std::thread::Builder::new().name("clipboard".into()).spawn(move || {
                let mut board = match arboard::Clipboard::new() {
                    Ok(b) => Some(b),
                    Err(e) => {
                        eprintln!("clipboard: none here: {e}");
                        None
                    }
                };
                for a in rx {
                    let Some(b) = board.as_mut() else { continue };
                    match a {
                        Ask::Get => match b.get_text() {
                            Ok(t) => GOT.lock().unwrap().push(t),
                            Err(e) => eprintln!("clipboard: read: {e}"),
                        },
                        Ask::Set(t) => {
                            if let Err(e) = b.set_text(t) {
                                eprintln!("clipboard: write: {e}");
                            }
                        }
                    }
                }
            });
            if let Err(e) = spawned {
                eprintln!("clipboard: no thread: {e}");
            }
            tx
        });
        let _ = tx.send(a);
    }

    pub fn paste() {
        ask(Ask::Get);
    }

    pub fn copy(text: String) {
        // a line end as that system's own programs write it
        #[cfg(windows)]
        let text = text.replace('\n', "\r\n");
        ask(Ask::Set(text));
    }
}

/// Paste asked for: the clipboard is read, and `poll` hands its text to the edit
/// box on the screen.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub fn paste() {
    native::paste();
}

/// Copy: the text on the clipboard.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub fn copy(text: String) {
    native::copy(text);
}

thread_local! {
    /// `screen_time`'s clock, and the redraw that last moved it.
    static SCREEN: std::cell::Cell<(f64, Option<web_time::Instant>)> = const { std::cell::Cell::new((0.0, None)) };
}

/// The most one redraw moves `screen_time`, in seconds.
const SCREEN_STEP: f64 = 0.1;

/// The seconds the game has been drawing, by which a box tells how long it has
/// been off the screen: not the wall clock, or a key let out of a stall (a Mac's
/// first Cmd+key takes seconds on a slow machine) finds every box gone, as it has
/// not been drawn since the stall began, though nothing else has either.
pub fn screen_time() -> f64 {
    SCREEN.get().0
}

/// Each frame: `screen_time` goes on, and a read that has come back goes to the
/// edit box on the screen, if one still is.
pub fn poll() {
    let now = web_time::Instant::now();
    let (t, last) = SCREEN.get();
    let step = last.map_or(0.0, |l| now.duration_since(l).as_secs_f64()).min(SCREEN_STEP);
    SCREEN.set((t + step, Some(now)));
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    for text in std::mem::take(&mut *native::GOT.lock().unwrap()) {
        if crate::editbox::field_up() {
            crate::editbox::ask_paste(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A redraw moves the clock by what passed since the last, a stall of seconds
    /// by no more than a step; with no redraw it stands still.
    #[test]
    fn the_screens_time_stands_still_through_a_stall() {
        poll();
        let stall = || {
            let (t, _) = SCREEN.get();
            SCREEN.set((t, web_time::Instant::now().checked_sub(std::time::Duration::from_secs(3))));
        };
        stall();
        let before = screen_time();
        assert_eq!(screen_time(), before, "no redraw, no time");
        poll();
        let moved = screen_time() - before;
        assert!(moved > 0.0 && moved <= SCREEN_STEP, "moved {moved}");
    }
}
