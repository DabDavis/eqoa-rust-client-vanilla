//! The title's Network Setup: the client's own page (the user's choice,
//! 2026-10-22). On a PS2 the entry fades the title out and starts Sony's
//! network utility (NTGUI2.ELF) to make a "Your Network Configuration"; a
//! computer has its network already, and what it needs is the server: the
//! address Station's login (TCP 9735), the news (HTTP 7000) and the game's
//! session (UDP 10070) are at.
//!
//! Made of the game's pieces, as the Options window is: a `VIUIWindow`
//! titled with the menu's own word (1635), the server's row with its
//! address in a frame, the legend, the game's sounds; X on the row opens
//! the edit box (`editbox`, `VIWndEdit`: the soft keyboard with a pad, the
//! compact face with a keyboard) on the address. Done (Enter, R2, the soft
//! keyboard's Done, a click or a tap on the legend's) keeps what was typed
//! (`settings::set_server`, server.opt), a blank clearing it, and closes the
//! page back to the title, as a submit; what is neither a host name nor an
//! IPv4 address is refused in the game's message box, the page staying.
//! Triangle closes the page, keeping what it had. Its words are the client's
//! own, English: WORDS.TXT has none for it.
//!
//! Under the address, a second row is always there, whatever the address is
//! (the owner, 2026-11-15: "add a eqoaclassic.com preset always available"):
//! X on it (Enter, a click, a tap) keeps [`DEFAULT_SERVER`] and closes the
//! page as Done does. Up and down move between the two rows, the game's tick
//! beside the one chosen.

use crate::editbox::{EditBox, EditUp};
use crate::settings::DEFAULT_SERVER;
use crate::pointer::{Hits, key, rows_rect};
use crate::textlist::{Message, PROMPT_OK};
use crate::ui::Layout;
use vi::ui::{UI_COLORS, Vui, Window, wide};

const SERVER: &str = "Server";
const HINT: &str = "A host name or an IPv4 address";
const BAD: &str = "That is not a server address. Enter a host name (eqoa.example.com) or an IPv4 address (192.0.2.10), or leave it blank for none.";
const NONE: &str = "(none)";
/// The preset row's words (eqoaclassic.com, [`DEFAULT_SERVER`]).
const PRESET: &str = "Use eqoaclassic.com";

/// The longest address the edit box takes (the terminator counts).
const MAX_LEN: usize = 64;

/// Whether `s` can be kept: blank (none), a dotted IPv4 address (four parts
/// 0..255), or a host name (labels of letters, digits and hyphens, none
/// starting or ending with a hyphen, 63 at most; 253 in all). An address of
/// digits and dots that is not an IPv4 address is not a host name either.
pub fn valid_address(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    if s.len() > 253 {
        return false;
    }
    let parts: Vec<&str> = s.split('.').collect();
    if parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())) {
        return parts.len() == 4 && parts.iter().all(|p| p.len() <= 3 && p.parse::<u16>().is_ok_and(|v| v <= 255));
    }
    parts.iter().all(|l| !l.is_empty() && l.len() <= 63 && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') && !l.starts_with('-') && !l.ends_with('-'))
}

/// What the page tells the title.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetUp {
    None,
    /// closed (Triangle)
    Close,
    /// Done: a new address kept, or none (blank), and the page closed
    Kept(Option<String>),
}

/// Pointer targets.
const ROW: i32 = 0;
const SELECT: i32 = 1;
const BACK: i32 = 2;
const PRESET_ROW: i32 = 3;

/// The window rows of the page's two choices: the address, the preset.
const ROWS: [i32; 2] = [1, 3];
/// The legend's row, a row clear of the preset.
const LEGEND: i32 = 5;

#[derive(Default)]
pub struct NetSetup {
    window: Window,
    edit: EditBox,
    pub editing: bool,
    message: Message,
    hits: Hits,
    /// the address shown
    pub address: String,
    /// the row chosen: 0 the address, 1 the preset
    pub sel: i32,
}

impl NetSetup {
    /// The page opening (the focus arriving): 440 wide at the options'
    /// place, six rows.
    pub fn open(&mut self, vui: &Vui, r: &dyn Layout, current: Option<&str>) {
        self.address = current.unwrap_or("").to_string();
        self.window.close(true);
        self.window.init(vui, r, [320, 246], 440, 6, 0);
        self.window.set_title(Some(&wide(vui.words.get(1635, vui.language))));
        self.window.open(false);
        self.editing = false;
        self.message.visible = false;
        self.sel = 0;
    }

    /// The edit box on the address (`VIWndEdit`'s `SetRows(1)`,
    /// `SetMaxLength`, `SetText`, `SetTitle`, the focus).
    fn begin_edit(&mut self, now: i64) {
        let e = &mut self.edit;
        e.set_rows(1);
        e.set_max_length(MAX_LEN);
        e.set_center([320, 300]);
        e.set_text(&wide(&self.address));
        e.title = wide(SERVER);
        e.purpose = 0;
        e.focus(now);
        self.editing = true;
    }

    /// A key: the box's, the edit box's while it is open, else the page's.
    pub fn key(&mut self, vui: &Vui, r: &mut dyn Layout, k: i32, now: i64, cues: &mut Vec<i32>) -> NetUp {
        if self.message.visible {
            self.message.key(k, now, cues);
            return NetUp::None;
        }
        if self.editing {
            match self.edit.key(vui, r, k, now, cues) {
                EditUp::Done(_) => {
                    self.editing = false;
                    let typed = String::from_utf16_lossy(&self.edit.text).trim().to_string();
                    if !valid_address(&typed) {
                        self.message.setup(vui, r, &wide(vui.words.get(767, vui.language)), &wide(BAD), PROMPT_OK, [320, 246], 400);
                        return NetUp::None;
                    }
                    self.address = typed.clone();
                    return NetUp::Kept((!typed.is_empty()).then_some(typed));
                }
                EditUp::Cancel => self.editing = false,
                EditUp::None => {}
            }
            return NetUp::None;
        }
        match k {
            19 | key::UP => {
                if self.sel > 0 {
                    self.sel -= 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                } else {
                    cues.push(9);
                }
            }
            20 | key::DOWN => {
                if self.sel < 1 {
                    self.sel += 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                } else {
                    cues.push(9);
                }
            }
            13 | key::CROSS if self.sel == 1 => {
                cues.push(15);
                self.address = DEFAULT_SERVER.to_string();
                return NetUp::Kept(Some(DEFAULT_SERVER.to_string()));
            }
            13 | key::CROSS => {
                cues.push(15);
                self.begin_edit(now);
            }
            27 | key::TRIANGLE => {
                cues.push(8);
                return NetUp::Close;
            }
            _ => {}
        }
        NetUp::None
    }

    /// A key let go (the edit box's held keys).
    pub fn key_up(&mut self, k: i32) {
        if self.editing {
            self.edit.key_up(k);
        }
    }

    /// One frame: the window, the row, the hint, the legend; the edit box
    /// over them while it is open, the message box over everything.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Layout, now: i64, cues: &mut Vec<i32>) {
        if self.editing || self.message.visible {
            self.window.reset_tick_pulse(now);
        }
        self.window.draw(vui, r);
        let width = self.window.size[0];
        let c = UI_COLORS[1];
        self.window.text_box(vui, r, &wide(SERVER), 40, 1, 90, 1, c, 0);
        self.window.tick_r(vui, r, 36, ROWS[self.sel.clamp(0, 1) as usize], true);
        let shown = if self.address.is_empty() { NONE } else { &self.address };
        // the value in its frame, cut with an ellipsis when it is long
        let (vx, vw) = (140, width - 140 - 40);
        self.window.draw_frame(vui, r, vx, 1, vw, 1, 64);
        self.window.text_box(vui, r, &wide(shown), vx, 1, vw, 1, UI_COLORS[0], 2);
        self.window.text_box(vui, r, &wide(HINT), 40, 2, width - 80, 1, UI_COLORS[2], 0);
        self.window.text_box(vui, r, &wide(PRESET), 40, 3, width - 80, 1, c, 0);
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        let legend = wide(&format!("|x {}   |t {}", w(212), w(750)));
        self.window.text(vui, r, &legend, 40, LEGEND, UI_COLORS[0], 0);
        self.hits.clear();
        self.hits.push(rows_rect(&self.window, vui, r, 40, 1, width - 80, 1), ROW);
        self.hits.push(rows_rect(&self.window, vui, r, 40, 3, width - 80, 1), PRESET_ROW);
        let half = (width - 80) / 2;
        self.hits.push(rows_rect(&self.window, vui, r, 40, LEGEND, half, 1), SELECT);
        let mut back = rows_rect(&self.window, vui, r, 40 + half, LEGEND, half, 1);
        back[0] = self.window.top_left()[0] + 40 + half;
        self.hits.push(back, BACK);
        if self.editing {
            self.edit.draw(vui, r, now, cues);
        }
        if self.message.visible {
            self.message.draw(vui, r);
        }
    }

    /// A pointer at frame point `p`: the box's choice, the edit box's keys,
    /// else a row (chosen, then X) and the legend's halves (X, Triangle).
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.message.visible {
            return self.message.point(p).into_iter().collect();
        }
        if self.editing {
            return self.edit.point(p).flatten().into_iter().collect();
        }
        match self.hits.at(p) {
            Some(ROW) => {
                self.sel = 0;
                vec![key::CROSS]
            }
            Some(PRESET_ROW) => {
                self.sel = 1;
                vec![key::CROSS]
            }
            Some(SELECT) => vec![key::CROSS],
            Some(BACK) => vec![key::TRIANGLE],
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, mid, vui};

    #[test]
    fn addresses_are_host_names_or_ipv4() {
        for ok in ["", "localhost", "eqoa.example.com", "a-b.c", "192.0.2.10", "255.255.255.255", "0.0.0.0", "eqoa2.example"] {
            assert!(valid_address(ok), "{ok:?}");
        }
        for bad in ["256.1.1.1", "1.2.3", "1.2.3.4.5", "-a.com", "a-.com", "a..b", "a.com.", "a b", "a_b.com", "::1", "host:9735", "1.2.3.04444"] {
            assert!(!valid_address(bad), "{bad:?}");
        }
    }

    fn page() -> (NetSetup, Vui, NullRaster) {
        let (v, r) = (vui(), NullRaster);
        let mut n = NetSetup::default();
        n.open(&v, &r, Some("192.0.2.1"));
        (n, v, r)
    }

    fn typed(n: &mut NetSetup, v: &Vui, r: &mut NullRaster, s: &str) -> NetUp {
        typed_then(n, v, r, s, 13)
    }

    /// X on the row, the address typed over the old, then `done`.
    fn typed_then(n: &mut NetSetup, v: &Vui, r: &mut NullRaster, s: &str, done: i32) -> NetUp {
        let mut cues = Vec::new();
        assert_eq!(n.key(v, r, key::CROSS, 0, &mut cues), NetUp::None);
        assert!(n.editing, "X opens the edit box");
        for _ in 0..20 {
            n.key(v, r, 8, 0, &mut cues);
        }
        for ch in s.chars() {
            n.key(v, r, ch as i32, 0, &mut cues);
        }
        n.key(v, r, done, 0, &mut cues)
    }

    /// Typing an address and Done keeps it, the page's submit (`Kept`: the
    /// title closes the page): Done by Enter, or by R2, which a click or a
    /// tap on the legend's Done presses (F3 20261026-202044); a blank
    /// clears; a bad one is refused in the box and the old one stays, no
    /// submit; Triangle closes.
    #[test]
    fn an_address_is_kept_a_blank_clears_and_a_bad_one_is_refused() {
        let (mut n, v, mut r) = page();
        assert_eq!(typed(&mut n, &v, &mut r, "eqoa.example.com"), NetUp::Kept(Some("eqoa.example.com".into())));
        assert_eq!(n.address, "eqoa.example.com");
        assert!(!n.editing, "the edit box closed with it");
        assert_eq!(typed_then(&mut n, &v, &mut r, "192.0.2.7", key::R2), NetUp::Kept(Some("192.0.2.7".into())), "R2: the legend's Done");
        assert_eq!(typed(&mut n, &v, &mut r, ""), NetUp::Kept(None), "blank: none");
        assert_eq!(typed(&mut n, &v, &mut r, "300.1.1.1"), NetUp::None, "refused");
        assert!(n.message.visible, "said in the box");
        assert_eq!(n.address, "", "the last kept stays");
        let mut cues = Vec::new();
        n.key(&v, &mut r, 13, 0, &mut cues);
        assert!(!n.message.visible, "Enter closes the box");
        assert_eq!(n.key(&v, &mut r, key::TRIANGLE, 0, &mut cues), NetUp::Close);
        // the control: Escape in the edit box, or triangle, cancels it,
        // keeping nothing and closing nothing
        for cancel in [27, key::TRIANGLE] {
            let (mut m, v, mut r) = page();
            m.key(&v, &mut r, key::CROSS, 0, &mut cues);
            m.key(&v, &mut r, b'x' as i32, 0, &mut cues);
            assert_eq!(m.key(&v, &mut r, cancel, 0, &mut cues), NetUp::None);
            assert!(!m.editing);
            assert_eq!(m.address, "192.0.2.1");
        }
    }

    /// A pointer: the row opens the edit box (X), Back closes (Triangle).
    #[test]
    fn a_pointer_opens_the_row_and_goes_back() {
        let (mut n, v, mut r) = page();
        n.draw(&v, &mut r, 0, &mut Vec::new());
        let row = rows_rect(&n.window, &v, &r, 140, 1, 100, 1);
        assert_eq!(n.point(mid(row)), vec![key::CROSS]);
        let w = n.window.size[0];
        let back = rows_rect(&n.window, &v, &r, 40 + (w - 80) / 2 + 30, LEGEND, 40, 1);
        assert_eq!(n.point(mid(back)), vec![key::TRIANGLE]);
        // the control: without the hit notes, nothing
        n.hits.clear();
        assert!(n.point(mid(row)).is_empty());
    }

    /// The preset row keeps eqoaclassic.com and closes the page (`Kept`),
    /// whatever the address was: another, eqoaclassic.com itself, or none;
    /// by X, Enter, or a pointer on it. The control: X on the address row
    /// still opens the edit box, keeping nothing.
    #[test]
    fn the_preset_keeps_eqoaclassic_from_any_server() {
        let (v, mut r) = (vui(), NullRaster);
        let mut cues = Vec::new();
        for current in [Some("192.0.2.1"), Some(DEFAULT_SERVER), Some(""), None, Some("eqoa.example.com")] {
            for pick in [key::CROSS, 13] {
                let mut n = NetSetup::default();
                n.open(&v, &r, current);
                n.draw(&v, &mut r, 0, &mut Vec::new());
                assert_eq!(n.key(&v, &mut r, key::DOWN, 0, &mut cues), NetUp::None);
                assert_eq!(n.sel, 1, "down: the preset");
                assert_eq!(n.key(&v, &mut r, pick, 0, &mut cues), NetUp::Kept(Some("eqoaclassic.com".into())), "{current:?}");
                assert!(!n.editing, "no edit box");
                assert_eq!(n.address, "eqoaclassic.com");
            }
            // a pointer on the preset row chooses it and presses X
            let mut n = NetSetup::default();
            n.open(&v, &r, current);
            n.draw(&v, &mut r, 0, &mut Vec::new());
            let preset = rows_rect(&n.window, &v, &r, 140, 3, 100, 1);
            assert_eq!(n.point(mid(preset)), vec![key::CROSS]);
            assert_eq!(n.sel, 1);
            assert_eq!(n.key(&v, &mut r, key::CROSS, 0, &mut cues), NetUp::Kept(Some(DEFAULT_SERVER.into())));
        }
        // the control: the address row as ever
        let (mut n, v, mut r) = page();
        assert_eq!(n.key(&v, &mut r, key::CROSS, 0, &mut cues), NetUp::None);
        assert!(n.editing, "X on the address: the edit box");
        assert_eq!(n.address, "192.0.2.1");
    }

    /// Up and down move between the address and the preset, the ends
    /// buzzing (9) and staying; a pointer on the address row chooses it
    /// again; the page opens on the address.
    #[test]
    fn up_and_down_move_between_the_rows() {
        let (mut n, v, mut r) = page();
        assert_eq!(n.sel, 0, "opens on the address");
        let mut cues = Vec::new();
        n.key(&v, &mut r, key::UP, 0, &mut cues);
        assert_eq!((n.sel, cues.pop()), (0, Some(9)), "up at the top stays");
        n.key(&v, &mut r, 20, 0, &mut cues);
        assert_eq!((n.sel, cues.pop()), (1, Some(11)), "down (the keyboard's)");
        n.key(&v, &mut r, key::DOWN, 0, &mut cues);
        assert_eq!((n.sel, cues.pop()), (1, Some(9)), "down at the bottom stays");
        n.key(&v, &mut r, 19, 0, &mut cues);
        assert_eq!((n.sel, cues.pop()), (0, Some(11)), "up (the keyboard's)");
        n.sel = 1;
        n.draw(&v, &mut r, 0, &mut Vec::new());
        let row = rows_rect(&n.window, &v, &r, 140, 1, 100, 1);
        assert_eq!(n.point(mid(row)), vec![key::CROSS]);
        assert_eq!(n.sel, 0, "a pointer on the address chooses it");
        assert_eq!(n.key(&v, &mut r, key::CROSS, 0, &mut cues), NetUp::None);
        assert!(n.editing);
        // reopened, it is on the address again
        n.sel = 1;
        n.open(&v, &r, None);
        assert_eq!(n.sel, 0);
    }
}
