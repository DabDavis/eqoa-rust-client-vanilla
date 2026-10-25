//! Display (the Extended page's Video, natively; F3 20261024-201507: "Cant
//! get the client out of windowed mode", where F11 alone did it and nothing
//! said so): the window in a window, or full screen, borderless on the
//! monitor it is on. The row, F11 and Alt+Enter are one state: the viewer
//! reads the window's own each frame (`seen`), a key toggles it there, and the
//! row asks (`ask`), shown at once and carried out on the next frame
//! (`take`). Kept as the window's place keeps it (`winplace`: full screen in
//! `window.opt`), not with the options: Triangle leaves it as it is. A page's
//! full screen is the browser's (F11), and the row is not on its page.

use std::cell::Cell;

thread_local! {
    /// The window is full screen, as last seen or asked; and the row's ask
    /// not carried out yet. The window's thread's: the options and the
    /// viewer are on it.
    static FULL: Cell<bool> = const { Cell::new(false) };
    static ASK: Cell<Option<bool>> = const { Cell::new(None) };
}

/// Whether the window is full screen (the row's value).
pub fn full() -> bool {
    FULL.get()
}

/// The window as the viewer sees it, each frame and after F11 or Alt+Enter.
/// An ask not carried out yet stands until it is.
pub fn seen(full: bool) {
    if ASK.get().is_none() {
        FULL.set(full);
    }
}

/// The row moved: full screen or a window, shown at once.
pub fn ask(full: bool) {
    FULL.set(full);
    ASK.set(Some(full));
}

/// The row's ask, once, for the viewer to carry out.
pub fn take() -> Option<bool> {
    ASK.take()
}

/// Whether a key toggles full screen: F11, or Enter with Alt held (Windows'
/// own key for it), pressed. Enter alone is the game's.
pub fn toggles(f11: bool, enter: bool, alt: bool) -> bool {
    f11 || (enter && alt)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The row shows what it asked at once, the viewer's next look at the
    /// window not taking it back before it is carried out, and then what the
    /// window is (a window manager that refused it shows as refused). F11
    /// and Alt+Enter toggle; Enter alone and Alt alone do not (the controls).
    #[test]
    fn the_row_the_keys_and_the_window_agree() {
        seen(false);
        assert!(!full() && take().is_none());
        ask(true);
        assert!(full(), "shown at once");
        seen(false);
        assert!(full(), "the frame before it is carried out does not take it back");
        assert_eq!(take(), Some(true));
        assert_eq!(take(), None, "once");
        seen(true);
        assert!(full());
        ask(false);
        assert_eq!(take(), Some(false));
        seen(true);
        assert!(full(), "refused by the window manager: what the window is");
        seen(false);
        assert!(!full());
        assert!(toggles(true, false, false) && toggles(false, true, true));
        assert!(!toggles(false, true, false), "Enter alone is the game's");
        assert!(!toggles(false, false, true), "Alt alone is nothing");
    }
}
