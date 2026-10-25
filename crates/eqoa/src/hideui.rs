//! Hide UI, the client's own (the user, 2026-10-25: "lets add a hide UI
//! button"; the PS2 has no such thing): everything drawn over the world
//! hidden, the HUD and every 2D window, the chat box, the names over heads,
//! the floating damage numbers and the target's selection rings, while the
//! world, the characters and the spell effects stay. Its key is the Controls
//! page's row (H, `binds::ROWS`), and a controller's button can be put on it
//! as on the map's. Again, it shows them.
//!
//! Only what is drawn goes: the windows go on stepping and the chat box takes
//! its lines as before (`viewer` drops the frame's quads, plates and labels
//! after `Play::hud` made them, `Play::draw_rings` the rings), and a pointer
//! finds none of the HUD's buttons. Nobody is stuck with it: any window
//! coming up shows them again for good (`frame`, `Play::wants_ui`: a mode of
//! the game's, the map, the player search, the target's details, a line
//! typed, a death, the update's question or the report's dialog), and the
//! loading screen is never hidden. Kept for the window's session only: a
//! new world entry (`Play::new`) and a fresh start show them.

use std::cell::Cell;

thread_local! {
    /// Whether the player hid them (one window's: a later window of a
    /// multibox is a program of its own).
    static HIDDEN: Cell<bool> = const { Cell::new(false) };
}

/// Whether what is over the world is hidden now.
pub fn hidden() -> bool {
    HIDDEN.with(|h| h.get())
}

/// The key: hidden, or shown again.
pub fn toggle() {
    HIDDEN.with(|h| h.set(!h.get()));
}

/// Shown again (a window came up, a new world entry).
pub fn show() {
    HIDDEN.with(|h| h.set(false));
}

/// A frame's: with a window up (`up`) shown again for good; whether what is
/// over the world is hidden this frame.
pub fn frame(up: bool) -> bool {
    if up {
        show();
    }
    hidden()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The key hides and shows; a window up shows them for good (they stay
    /// up once it is gone), as does a new entry. The control: a frame with
    /// no window leaves them hidden.
    #[test]
    fn a_window_brings_the_ui_back() {
        show();
        assert!(!frame(false), "shown at the start");
        toggle();
        assert!(frame(false), "the control: hidden, no window, hidden it stays");
        assert!(frame(false));
        assert!(!frame(true), "a window up: shown");
        assert!(!frame(false), "and still shown once it is gone");
        toggle();
        toggle();
        assert!(!hidden(), "again: shown");
        toggle();
        show();
        assert!(!hidden(), "a new world entry: shown");
    }
}
