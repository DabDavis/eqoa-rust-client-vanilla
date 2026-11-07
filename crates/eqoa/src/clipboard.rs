//! The system's clipboard for the game's edit boxes (the user, 2026-11-07: "we
//! can add the copy/paste into vanilla client", `editbox`'s `paste_chars` holding
//! it to what a console's keyboard can type). Natively: `arboard`, on a thread of
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

/// Each frame: a read that has come back goes to the edit box on the screen, if
/// one still is.
pub fn poll() {
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    for text in std::mem::take(&mut *native::GOT.lock().unwrap()) {
        if crate::editbox::field_up() {
            crate::editbox::ask_paste(text);
        }
    }
}
