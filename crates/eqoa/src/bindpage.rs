//! The Options window's Controls page: the client's own (F3 20261021: jib's
//! rebinding, dab's keyboard legend), laid out and driven as the window's
//! other pages are. Each row of the default layout (`binds::ROWS`) with what
//! it does in play, its key and its button; the selected row's help below
//! (what the key and the button are in a window), and a line that answers
//! the windows' legends for the keyboard: which key is ✕, △, □, ○, R1 and
//! R3 there. ✕ on a row's key or button waits for the key or the button to
//! put there (Escape, or the button on △, gives up); a row of the client's
//! keys (the targets, F9's art, Hide UI...) takes a button too, which then
//! does what its key does and leaves the game's use it had (F3
//! 20261024-145732). Defaults puts every one back. The changes apply at once;
//! Save Changes keeps them (`keys.opt`) and Triangle on the menu puts them
//! back, as the window does with the rest.
//!
//! A pointer: a row's words select it, a cell its column too, and the
//! selection already there is ✕; while a key is awaited any click gives up.
//!
//! The macros (`binds::Macro`, `macros`; the user's PCSX2-like macros) follow
//! the rows, two each: its trigger (a key, a controller's button, as a row's
//! are taken) and what it presses (✕ on the left cell takes the buttons
//! pressed, kept a second after the last, Backspace none; ✕ on the right one
//! steps the mode: Hold, Toggle, each turbo). Defaults clears them too.

use crate::binds::{self, Dir, MACROS, Moved, Pad, ROWS};
use crate::pointer::{Hits, key, rows_rect};
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// What a capture waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    Keyboard,
    Pad,
    /// a macro's buttons, as many as are pressed
    Chord,
}

/// What an entry of the page is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    /// a row of `ROWS`
    Row(usize),
    /// a macro's trigger: its key, its button
    Trigger(usize),
    /// a macro's buttons and its mode
    Chord(usize),
    Defaults,
}

fn entry(e: i32) -> Entry {
    let n = ROWS.len() as i32;
    match e {
        e if e < n => Entry::Row(e.max(0) as usize),
        e if e < defaults() => {
            let m = ((e - n) / 2) as usize;
            if (e - n) % 2 == 0 { Entry::Trigger(m) } else { Entry::Chord(m) }
        }
        _ => Entry::Defaults,
    }
}

/// Defaults' entry: after the rows and the macros' two each.
pub fn defaults() -> i32 {
    ROWS.len() as i32 + 2 * MACROS as i32
}

/// A macro's buttons are kept once nothing new was pressed for this long
/// (ms); with none pressed for this long the wait gives up.
const CHORD_SETTLE: i64 = 1000;
const CHORD_WAIT: i64 = 8000;

/// The page's own rows besides the list: the header, a gap, the help (two),
/// the windows' keys (two) and the legend.
const FIXED_ROWS: i32 = 8;
/// Where the cells are, from the window's right edge: room between them for
/// the selected one's side arrows, and right of them for the scroll bar.
const KEY_X: i32 = 280;
const KEY_W: i32 = 110;
const PAD_X: i32 = 138;
const PAD_W: i32 = 90;
/// A held direction repeats after this long, then this often (ms).
const REPEAT_AFTER: i64 = 400;
const REPEAT_EVERY: i64 = 90;

/// The Controls page.
#[derive(Clone, Debug, Default)]
pub struct ControlsPage {
    /// the selected entry: a row of `ROWS`, or `ROWS.len()` for Defaults
    pub row: i32,
    /// the column: 0 the keyboard's, 1 the controller's
    pub col: i32,
    /// the first entry shown
    top: i32,
    /// the entries shown at once, as last drawn
    shown: i32,
    /// waiting for a key or a button
    pub capture: Option<Column>,
    /// what the last change did, said until the selection moves
    note: Option<String>,
    /// up or down held (-1, 1), since when, and when it last stepped
    held: Option<(i32, i64, i64)>,
    hits: Hits,
    /// a macro's buttons being taken: those pressed so far (the game's
    /// fields), when the last came and when the wait began
    chord: u16,
    chord_at: i64,
    chord_from: i64,
}

impl ControlsPage {
    /// As the page opens: the first row, the keyboard's column, nothing
    /// awaited.
    pub fn reset(&mut self) {
        *self = ControlsPage::default();
    }

    /// The entries: the rows, the macros' two each, then Defaults.
    fn entries() -> i32 {
        defaults() + 1
    }

    /// Whether entry `row`'s column `col` can be moved: a key (△'s row,
    /// which the game gives none, can be given one), or a button (a row of
    /// the client's keys can be given one; the sticks stay sticks); every
    /// macro cell.
    fn bindable(row: i32, col: i32) -> bool {
        match entry(row) {
            Entry::Row(r) => {
                let r = &ROWS[r];
                let button = matches!(r.pad, Pad::Button(_));
                if col == 0 { !r.keys.is_empty() || button } else { !matches!(r.pad, Pad::Stick(..)) }
            }
            Entry::Trigger(_) | Entry::Chord(_) => true,
            Entry::Defaults => false,
        }
    }

    /// The window's rows this page wants for a font `h` high: what fits the
    /// frame round its middle with the title's bar above it (11 over the
    /// window's top) and a few pixels spare, at most 24.
    pub fn window_rows(h: i32) -> i32 {
        ((448 - 2 * 15 - 48) / (h - 4).max(1)).clamp(FIXED_ROWS + 4, 24)
    }

    /// The keys, as the window hands them on (pad keys and the keyboard's
    /// as the windows take them), down and up. True: back to the menu.
    pub fn key(&mut self, k: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> bool {
        if !down {
            if matches!(k, 19 | 138 | 20 | 139) {
                self.held = None;
            }
            return false;
        }
        if self.capture.is_some() {
            // △ (or Escape, which the window hands on as 27) gives up
            if matches!(k, 27 | 132) {
                self.capture = None;
                cues.push(8);
            }
            return false;
        }
        match k {
            19 | 138 => {
                self.step(-1, cues);
                self.held = Some((-1, now, now));
            }
            20 | 139 => {
                self.step(1, cues);
                self.held = Some((1, now, now));
            }
            17 | 136 | 18 | 137 => {
                let col = matches!(k, 18 | 137) as i32;
                cues.push(if col != self.col { 11 } else { 9 });
                self.col = col;
                self.note = None;
            }
            13 | 134 => self.select(now, cues),
            27 | 132 => {
                self.held = None;
                return true;
            }
            _ => {}
        }
        false
    }

    /// Up or down a row, sound 11, or 9 at an end.
    fn step(&mut self, d: i32, cues: &mut Vec<i32>) {
        let to = self.row + d;
        if (0..Self::entries()).contains(&to) {
            self.row = to;
            self.note = None;
            cues.push(11);
        } else {
            cues.push(9);
        }
    }

    /// ✕: Defaults puts every key and button back and clears the macros; a
    /// row's key or button is awaited, or a macro's, or its buttons; a
    /// macro's mode steps on; what cannot be moved says why.
    fn select(&mut self, now: i64, cues: &mut Vec<i32>) {
        match entry(self.row) {
            Entry::Defaults => {
                crate::settings::set_binds(binds::Binds::default());
                self.note = Some("Every key, button and macro is back where it began.".into());
                cues.push(13);
                return;
            }
            Entry::Chord(m) if self.col == 1 => {
                let mut b = crate::settings::binds();
                b.macros[m].next_mode();
                let said = format!("Macro {}: {}.", m + 1, b.macros[m].mode_text());
                crate::settings::set_binds(b);
                self.note = Some(said);
                cues.push(13);
                return;
            }
            Entry::Chord(_) => {
                self.capture = Some(Column::Chord);
                (self.chord, self.chord_at, self.chord_from) = (0, now, now);
                self.note = None;
                cues.push(8);
                return;
            }
            _ => {}
        }
        if Self::bindable(self.row, self.col) {
            self.capture = Some(if self.col == 0 { Column::Keyboard } else { Column::Pad });
            self.note = None;
            cues.push(8);
            return;
        }
        self.note = Some(if self.col == 1 { "The sticks stay sticks.".into() } else { "The game has no key for this: its button only.".into() });
        cues.push(9);
    }

    /// A key pressed while one is awaited (its name, `binds::name_of`):
    /// Escape gives up, a key of the page's own is refused, any other goes
    /// on the row (swapping with the row that had it). A key while a button
    /// is awaited is nothing.
    pub fn capture_key(&mut self, vui: &Vui, name: &str, cues: &mut Vec<i32>) {
        let Some(c) = self.capture else { return };
        if name == "escape" {
            self.capture = None;
            cues.push(8);
            return;
        }
        if c == Column::Chord {
            self.chord_key(vui, name, cues);
            return;
        }
        if c != Column::Keyboard {
            return;
        }
        if let Entry::Trigger(m) = entry(self.row) {
            self.macro_key(m, name, vui, cues);
            return;
        }
        if binds::reserved(name) {
            self.note = Some(format!("{} is not for binding: choose another key.", binds::display(name)));
            cues.push(9);
            return;
        }
        let row = self.row as usize;
        let mut b = crate::settings::binds();
        let had = b.macro_on_key(name);
        let moved = b.bind_key(row, name);
        crate::settings::set_binds(b.clone());
        self.capture = None;
        let me = format!("{}: {}.", label(vui, row), binds::display(name));
        let mut note = match moved {
            Moved::Swapped(o) if b.key(o).is_none_or(str::is_empty) => format!("{me} {} has no key now.", label(vui, o)),
            Moved::Swapped(o) => format!("{me} {}: {}.", label(vui, o), binds::display(b.key(o).unwrap_or(""))),
            Moved::Freed(Some(old)) if !old.is_empty() => format!("{me} {} does nothing now.", binds::display(&old)),
            _ => me,
        };
        // a macro the key was on gave it up (`bind_key`): said, as a row is
        if let Some(m) = had.filter(|&m| b.macros[m].key.is_none()) {
            note.push_str(&format!(" Macro {} has no key now.", m + 1));
        }
        self.note = Some(note);
        cues.push(13);
    }

    /// A controller's button pressed while a capture waits (its bit index,
    /// as the controller has it, before any remap): the button on △ gives
    /// up; while a button is awaited any other goes on the row: on its
    /// field, swapping with the field it drove (or the row of the client's
    /// keys it was on); on a row of the client's keys, the field it drove
    /// taking the button the row had, or none.
    pub fn capture_button(&mut self, vui: &Vui, phys: u8, cues: &mut Vec<i32>) {
        let Some(c) = self.capture else { return };
        let mut b = crate::settings::binds();
        // a macro's buttons take △ too (Escape or a click gives up)
        if c == Column::Chord {
            match b.field_of(phys) {
                Some(f) => {
                    self.chord |= 1 << f;
                    self.chord_at = vui.time;
                    cues.push(11);
                }
                None => self.note = Some("That button is a macro's or a key's now: it presses nothing of the game's.".into()),
            }
            return;
        }
        if b.field_of(phys) == binds::bit_index(crate::input::bit::TRIANGLE) {
            self.capture = None;
            cues.push(8);
            return;
        }
        if c != Column::Pad {
            return;
        }
        if let Entry::Trigger(m) = entry(self.row) {
            let moved = b.bind_macro_button(m, phys);
            crate::settings::set_binds(b.clone());
            self.capture = None;
            let me = format!("Macro {}: {}.", m + 1, binds::glyph(phys));
            self.note = Some(match moved {
                Moved::Swapped(f) => match ROWS.iter().position(|r| r.pad == Pad::Button(1 << f)) {
                    Some(or) => match Some(b.button(f as u8)).filter(|&g| g != binds::NO_BUTTON) {
                        Some(g) => format!("{me} {}: {}.", label(vui, or), binds::glyph(g)),
                        None => format!("{me} {} has no button now.", label(vui, or)),
                    },
                    None => me,
                },
                Moved::SwappedRow(r) => match b.row_button(r) {
                    Some(g) => format!("{me} {}: {}.", label(vui, r), binds::glyph(g)),
                    None => format!("{me} {} has no button now.", label(vui, r)),
                },
                Moved::SwappedMacro(o) => match b.macros[o].button {
                    Some(g) => format!("{me} Macro {}: {}.", o + 1, binds::glyph(g)),
                    None => format!("{me} Macro {} has no button now.", o + 1),
                },
                _ => me,
            });
            cues.push(13);
            return;
        }
        let row = self.row as usize;
        let moved = match ROWS[row].pad {
            Pad::Button(f) => match binds::bit_index(f) {
                Some(field) => b.bind_button(field, phys),
                None => return,
            },
            Pad::None => b.bind_row_button(row, phys),
            Pad::Stick(..) => return,
        };
        crate::settings::set_binds(b.clone());
        self.capture = None;
        let me = format!("{}: {}.", label(vui, row), binds::glyph(phys));
        // the other row, by what it has now: a button, or none
        let other = |or: usize, now: Option<u8>| match now {
            Some(g) => format!("{me} {}: {}.", label(vui, or), binds::glyph(g)),
            None => format!("{me} {} has no button now.", label(vui, or)),
        };
        self.note = Some(match moved {
            Moved::Swapped(o) => match ROWS.iter().position(|r| r.pad == Pad::Button(1 << o)) {
                Some(or) => other(or, Some(b.button(o as u8)).filter(|&g| g != binds::NO_BUTTON)),
                None => me,
            },
            Moved::SwappedRow(r) => other(r, b.row_button(r)),
            Moved::SwappedMacro(o) => match b.macros[o].button {
                Some(g) => format!("{me} Macro {}: {}.", o + 1, binds::glyph(g)),
                None => format!("{me} Macro {} has no button now.", o + 1),
            },
            _ => me,
        });
        cues.push(13);
    }

    /// A key while a macro's key is awaited: Backspace takes the macro's
    /// away; a key of the page's own, or one a row is on, is refused (the
    /// wait goes on); any other goes on the macro (another macro on it
    /// giving it up).
    fn macro_key(&mut self, m: usize, name: &str, vui: &Vui, cues: &mut Vec<i32>) {
        let mut b = crate::settings::binds();
        if name == "backspace" {
            b.bind_macro_key(m, None).ok();
            crate::settings::set_binds(b);
            self.capture = None;
            self.note = Some(format!("Macro {} has no key now.", m + 1));
            cues.push(13);
            return;
        }
        match b.bind_macro_key(m, Some(name)) {
            Ok(()) => {
                crate::settings::set_binds(b);
                self.capture = None;
                self.note = Some(format!("Macro {}: {}.", m + 1, binds::display(name)));
                cues.push(13);
            }
            Err(binds::MacroKey::Reserved) => {
                self.note = Some(format!("{} is not for binding: choose another key.", binds::display(name)));
                cues.push(9);
            }
            Err(binds::MacroKey::Row(r)) => {
                self.note = Some(format!("{} is {}'s: choose a free key, or move that row first.", binds::display(name), label(vui, r)));
                cues.push(9);
            }
        }
    }

    /// A key while a macro's buttons are awaited: Backspace leaves it none;
    /// a key a row with a button is on adds that button (✕ for Space, □ for
    /// Q...); any other presses nothing.
    fn chord_key(&mut self, vui: &Vui, name: &str, cues: &mut Vec<i32>) {
        let Entry::Chord(m) = entry(self.row) else { return };
        let mut b = crate::settings::binds();
        if name == "backspace" {
            b.macros[m].chord = 0;
            crate::settings::set_binds(b);
            self.capture = None;
            self.note = Some(format!("Macro {} presses nothing now.", m + 1));
            cues.push(13);
            return;
        }
        match b.row_on(name).map(|r| ROWS[r].pad) {
            Some(Pad::Button(bit)) => {
                self.chord |= bit;
                self.chord_at = vui.time;
                cues.push(11);
            }
            _ => {
                self.note = Some(format!("{} presses no button of the game's.", binds::display(name)));
                cues.push(9);
            }
        }
    }

    /// The macro's buttons kept once none came for a second (or the wait
    /// given up after eight with none).
    fn settle_chord(&mut self, now: i64, cues: &mut Vec<i32>) {
        if self.capture != Some(Column::Chord) {
            return;
        }
        let Entry::Chord(m) = entry(self.row) else {
            self.capture = None;
            return;
        };
        if self.chord != 0 && now - self.chord_at >= CHORD_SETTLE {
            let mut b = crate::settings::binds();
            b.macros[m].chord = self.chord;
            let said = format!("Macro {} presses {}.", m + 1, b.macros[m].chord_glyphs());
            crate::settings::set_binds(b);
            self.capture = None;
            self.note = Some(said);
            cues.push(13);
        } else if self.chord == 0 && now - self.chord_from >= CHORD_WAIT {
            self.capture = None;
            cues.push(8);
        }
    }

    /// A pointer at frame point `p`: the keys to press. While a key is
    /// awaited a click gives up (△).
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.capture.is_some() {
            return vec![key::TRIANGLE];
        }
        let Some(h) = self.hits.at(p) else { return Vec::new() };
        let (entry, part) = (h / 4, h % 4);
        let mut keys = crate::pointer::steps(self.row, entry, key::UP, key::DOWN);
        if let Some(c) = match part {
            1 => Some(0),
            2 => Some(1),
            _ => None,
        } && c != self.col
        {
            keys.push(if c == 0 { key::LEFT } else { key::RIGHT });
        }
        if keys.is_empty() {
            keys.push(key::CROSS);
        }
        keys
    }

    /// The page in window `w`: the header, the rows in view, the help, the
    /// windows' keys, the legend. A direction held steps on.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, w: &mut Window, cues: &mut Vec<i32>) {
        let now = vui.time;
        self.settle_chord(now, cues);
        if let Some((d, since, last)) = self.held
            && now - since >= REPEAT_AFTER
            && now - last >= REPEAT_EVERY
            && self.capture.is_none()
        {
            self.step(d, cues);
            self.held = Some((d, since, now));
        }
        self.hits.clear();
        w.draw(vui, r);
        let width = w.size[0];
        let rows = w.rows;
        self.shown = (rows - FIXED_ROWS).max(1);
        let n = Self::entries();
        if self.row < self.top {
            self.top = self.row;
        } else if self.row >= self.top + self.shown {
            self.top = self.row - self.shown + 1;
        }
        self.top = self.top.clamp(0, (n - self.shown).max(0));
        let dim = UI_COLORS[2];
        let (kx, px) = (width - KEY_X, width - PAD_X);
        w.text_box(vui, r, &wide("Keyboard"), kx, 1, KEY_W, 1, dim, 8);
        w.text_box(vui, r, &wide("Controller"), px, 1, PAD_W, 1, dim, 8);
        let b = crate::settings::binds();
        for (k, entry) in (self.top..(self.top + self.shown).min(n)).enumerate() {
            let at = k as i32 + 2;
            let on = entry == self.row;
            let c = UI_COLORS[on as usize];
            if entry == defaults() {
                w.text_box(vui, r, &wide("Defaults"), 40, at, width - KEY_X - 48, 1, c, 0);
                self.hits.push(rows_rect(w, vui, r, 40, at, width - KEY_X - 48, 1), entry * 4);
                continue;
            }
            if let kind @ (Entry::Trigger(m) | Entry::Chord(m)) = self::entry(entry) {
                let waiting = |col: Column| on && self.capture == Some(col);
                let mac = &b.macros[m];
                let (name, kt, pt) = match kind {
                    Entry::Trigger(_) => (
                        format!("Macro {}", m + 1),
                        if waiting(Column::Keyboard) { "Press a key".into() } else { mac.key.as_deref().map_or_else(|| "-".into(), binds::display) },
                        if waiting(Column::Pad) { "Press".into() } else { mac.button.map_or_else(|| "-".into(), |g| binds::glyph(g).to_string()) },
                    ),
                    _ => (
                        format!("Macro {}: presses", m + 1),
                        if waiting(Column::Chord) {
                            binds::Macro { chord: self.chord, ..binds::Macro::default() }.chord_glyphs().replace('-', "Press")
                        } else {
                            mac.chord_glyphs()
                        },
                        mac.mode_text(),
                    ),
                };
                w.text_box(vui, r, &wide(&name), 40, at, width - KEY_X - 48, 1, c, 2);
                self.hits.push(rows_rect(w, vui, r, 40, at, width - KEY_X - 48, 1), entry * 4);
                for (col, x, cw, t) in [(0, kx, KEY_W, kt), (1, px, PAD_W, pt)] {
                    let sel = on && self.col == col;
                    w.draw_frame(vui, r, x, at, cw, 1, if sel { 0x60 } else { 0x40 });
                    w.text_box(vui, r, &wide(&t), x, at, cw, 1, UI_COLORS[0], 8);
                    self.hits.push(rows_rect(w, vui, r, x + 20, at, cw - 20, 1), entry * 4 + 1 + col);
                }
                continue;
            }
            let row = &ROWS[entry as usize];
            w.text_box(vui, r, &wide(&label(vui, entry as usize)), 40, at, width - KEY_X - 48, 1, c, 2);
            self.hits.push(rows_rect(w, vui, r, 40, at, width - KEY_X - 48, 1), entry * 4);
            // the cells: the selected one framed with its arrows
            let waiting = |col: Column| on && self.capture == Some(col);
            let kt = if waiting(Column::Keyboard) {
                "Press a key".to_string()
            } else {
                b.key(entry as usize).map_or_else(|| "-".to_string(), binds::display)
            };
            let pt = if waiting(Column::Pad) {
                "Press".to_string()
            } else {
                let glyph = |g: Option<u8>| g.map_or_else(|| "-".to_string(), |g| binds::glyph(g).to_string());
                match row.pad {
                    Pad::None => glyph(b.row_button(entry as usize)),
                    Pad::Button(f) => glyph(Some(b.button(binds::bit_index(f).unwrap_or(0))).filter(|&g| g != binds::NO_BUTTON)),
                    Pad::Stick(right, d) => format!("{} stick {}", if right { "R" } else { "L" }, arrow(d)),
                }
            };
            for (col, x, cw, t) in [(0, kx, KEY_W, kt), (1, px, PAD_W, pt)] {
                let sel = on && self.col == col;
                w.draw_frame(vui, r, x, at, cw, 1, if sel { 0x60 } else { 0x40 });
                let tc = if Self::bindable(entry, col) { UI_COLORS[0] } else { dim };
                w.text_box(vui, r, &wide(&t), x, at, cw, 1, tc, 8);
                self.hits.push(rows_rect(w, vui, r, x + 20, at, cw - 20, 1), entry * 4 + 1 + col);
            }
        }
        if on_screen(self.row, self.top, self.shown) {
            w.tick_r(vui, r, 36, self.row - self.top + 2, true);
        }
        // a scroll bar down the list's right when it is longer than the view
        if n > self.shown {
            let h = vui.font_height(r) - 4;
            let top = w.row_center(vui, r, 2)[1] - 4;
            let t = self.top as f32 / (n - self.shown) as f32;
            w.draw_vscroll(vui, r, [w.top_left()[0] + width - 24, top - 3], self.shown * h + 4, t);
        }
        // the help: what is awaited, the last change, or the row's help
        let list_end = self.shown + 3;
        let help = if let Some(c) = self.capture {
            let what = match entry(self.row) {
                Entry::Trigger(m) | Entry::Chord(m) => format!("Macro {}", m + 1),
                _ => label(vui, self.row as usize),
            };
            match c {
                Column::Keyboard if matches!(entry(self.row), Entry::Trigger(_)) => {
                    format!("Press the key for {what}. Backspace takes its key away; Esc gives up.")
                }
                Column::Keyboard => format!("Press the key for {what}. Esc gives up."),
                Column::Pad => format!("Press the controller's button for {what}. |t gives up."),
                Column::Chord => format!(
                    "Press the buttons {what} presses (the controller's, or the keys for them), together or one by one: kept a second after the last. Backspace: none. Esc gives up."
                ),
            }
        } else if let Some(n) = &self.note {
            n.clone()
        } else if let Entry::Trigger(m) = entry(self.row) {
            if self.col == 0 {
                format!("A key for Macro {}: held, the macro presses its buttons below (Toggle: from one press to the next). A key a row is on is refused.", m + 1)
            } else {
                format!("A controller's button for Macro {}: it does the macro, and no longer what it did in the game.", m + 1)
            }
        } else if let Entry::Chord(m) = entry(self.row) {
            if self.col == 0 {
                format!("The buttons Macro {} presses together, as the controller would: |x, then press them.", m + 1)
            } else {
                "How it presses: Hold (while held), Toggle (from one press to the next), Turbo N (pressed and let go every N of the game's frames, 30 a second).".to_string()
            }
        } else if let Some(row) = ROWS.get(self.row as usize) {
            // a row of the client's keys, its button's cell: what a button
            // there does
            if self.col == 1 && row.pad == Pad::None {
                "A controller's button put here does what this key does, and no longer what it did in the game.".to_string()
            } else {
                expand(vui, row.help)
            }
        } else {
            "Puts every key and button back as the game began, and clears the macros.".into()
        };
        w.text_box(vui, r, &wide(&help), 40, list_end, width - 80, 2, UI_COLORS[0], 2);
        w.text_box(vui, r, &wide(&window_keys(&b)), 40, list_end + 2, width - 80, 2, UI_COLORS[1], 0);
        let legend = format!("|x Change   |V|v Keyboard, Controller   |t {}", vui.words.get(217, vui.language));
        w.text(vui, r, &wide(&legend), 40, rows - 1, UI_COLORS[0], 0);
    }
}

fn on_screen(row: i32, top: i32, shown: i32) -> bool {
    (top..top + shown).contains(&row)
}

fn arrow(d: Dir) -> char {
    match d {
        Dir::Up => '\u{2191}',
        Dir::Down => '\u{2193}',
        Dir::Left => '\u{2190}',
        Dir::Right => '\u{2192}',
    }
}

/// A row's label, its WORDS.TXT words in.
pub fn label(vui: &Vui, row: usize) -> String {
    ROWS.get(row).map_or_else(String::new, |r| expand(vui, r.label))
}

/// `s` with each `{N}` the word N of WORDS.TXT.
pub fn expand(vui: &Vui, s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('}').and_then(|j| after[..j].parse::<i32>().ok().map(|n| (j, n))) {
            Some((j, n)) => {
                out.push_str(vui.words.get(n, vui.language));
                rest = &after[j + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The keys that are the windows' buttons now, by the glyph their legends
/// draw: ✕ Enter and E, △ Escape and Backspace, □ Q, ○ C, R1 Tab, R3 Space
/// (wherever the player moved those rows), on two lines.
pub fn window_keys(b: &binds::Binds) -> String {
    let keys = |ids: &[&str]| -> String {
        let v: Vec<String> = ids.iter().filter_map(|id| binds::row_named(id)).filter_map(|r| b.key(r)).filter(|k| !k.is_empty()).map(binds::display).collect();
        if v.is_empty() { "-".into() } else { v.join(", ") }
    };
    format!(
        "In a window:  |x {}   |t {}   |s {}\n|c {}   |8 {}   |( {}",
        keys(&["chat", "use"]),
        keys(&["start_menu", "back"]),
        keys(&["talk"]),
        keys(&["history"]),
        keys(&["next_target"]),
        keys(&["attack"]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, mid, vui};
    use crate::settings::{self, testing};

    fn drawn(page: &mut ControlsPage, w: &mut Window) {
        let vui = vui();
        page.draw(&vui, &mut NullRaster, w, &mut Vec::new());
    }

    fn window() -> Window {
        let vui = vui();
        let mut w = Window::default();
        w.init(&vui, &NullRaster, [320, 224], 600, ControlsPage::window_rows(vui.font_height(&NullRaster)), 0);
        w.open(true);
        w
    }

    /// The windows' keys line answers dab's player: □ is Q, so a trade is
    /// confirmed with Q; moving Talk's Q onto 7 says 7.
    #[test]
    fn the_windows_keys_say_which_key_is_which_glyph() {
        let mut b = binds::Binds::default();
        assert_eq!(window_keys(&b), "In a window:  |x Enter, E   |t Esc, Backspace   |s Q\n|c C   |8 Tab   |( Space");
        b.bind_key(binds::row_named("talk").unwrap(), "7");
        assert!(window_keys(&b).contains("|s 7\n"), "{}", window_keys(&b));
        b.bind_key(binds::row_named("use").unwrap(), "q");
        assert!(window_keys(&b).contains("|x Enter, Q "), "{}", window_keys(&b));
    }

    /// By the keys: down a row, ✕ waits for a key; the key goes on the row
    /// (a swap with the row that had it, said); Escape while waiting gives
    /// up; right to the controller's column, ✕ waits for a button, the
    /// button goes on the field (the swap's other field said); △ backs out.
    /// The control: the same steps without the capture's key leave the
    /// layout the default.
    #[test]
    fn rebinding_by_the_keys() {
        testing::reset(false);
        let vui = vui();
        let mut page = ControlsPage::default();
        let mut w = window();
        let mut cues = Vec::new();
        drawn(&mut page, &mut w);
        let attack = binds::row_named("attack").unwrap() as i32;
        for _ in 0..attack {
            page.key(key::DOWN, true, 0, &mut cues);
            page.key(key::DOWN, false, 0, &mut cues);
        }
        assert_eq!(page.row, attack);
        page.key(key::CROSS, true, 0, &mut cues);
        assert_eq!(page.capture, Some(Column::Keyboard));
        // a button while a key is awaited: nothing, unless it is △
        page.capture_button(&vui, binds::bit_index(crate::input::bit::CROSS).unwrap(), &mut cues);
        assert_eq!(page.capture, Some(Column::Keyboard));
        assert!(settings::binds().is_default(), "the control: nothing yet");
        page.capture_key(&vui, "q", &mut cues);
        assert_eq!(page.capture, None);
        let b = settings::binds();
        assert_eq!((b.key(attack as usize), b.key(binds::row_named("talk").unwrap())), (Some("q"), Some("space")));
        assert_eq!(page.note.as_deref(), Some("(null) (hold): Q. Talk (hold: (null)): Space."));
        // F11 is refused and the capture waits on; Escape gives up
        page.key(key::CROSS, true, 0, &mut cues);
        page.capture_key(&vui, "f11", &mut cues);
        assert_eq!(page.capture, Some(Column::Keyboard));
        page.capture_key(&vui, "escape", &mut cues);
        assert_eq!(page.capture, None);
        assert_eq!(settings::binds(), b, "unchanged");
        // the controller's column: ✕ on ○
        page.key(key::RIGHT, true, 0, &mut cues);
        page.key(key::CROSS, true, 0, &mut cues);
        assert_eq!(page.capture, Some(Column::Pad));
        page.capture_key(&vui, "x", &mut cues);
        assert_eq!(page.capture, Some(Column::Pad), "a key while a button is awaited");
        let (cross, circle) = (binds::bit_index(crate::input::bit::CROSS).unwrap(), binds::bit_index(crate::input::bit::CIRCLE).unwrap());
        page.capture_button(&vui, circle, &mut cues);
        let b = settings::binds();
        assert_eq!((b.button(cross), b.button(circle)), (circle, cross));
        assert_eq!(page.note.as_deref(), Some("(null) (hold): |c. Quick menu: (null): |x."));
        // the button on △ gives up a capture
        page.key(key::CROSS, true, 0, &mut cues);
        page.capture_button(&vui, binds::bit_index(crate::input::bit::TRIANGLE).unwrap(), &mut cues);
        assert_eq!(page.capture, None);
        assert_eq!(settings::binds(), b);
        // △ backs out to the menu
        assert!(page.key(key::TRIANGLE, true, 0, &mut cues));
        testing::reset(false);
    }

    /// The macros' rows: Macro 1's key (a row's refused, a free one taken),
    /// its button (taken off its field, said), its buttons by the keyboard's
    /// key for ✕ and the controller's ○, kept a second after the last, and
    /// its mode stepped; Defaults clears it. The control: a wait for its
    /// buttons with nothing pressed gives up after eight seconds, the macro
    /// unchanged.
    #[test]
    fn a_macro_is_set_on_the_page() {
        testing::reset(false);
        let mut vui = vui();
        let mut page = ControlsPage::default();
        let mut w = window();
        let mut cues = Vec::new();
        let trigger = ROWS.len() as i32;
        page.row = trigger;
        page.key(key::CROSS, true, 0, &mut cues);
        assert_eq!(page.capture, Some(Column::Keyboard));
        page.capture_key(&vui, "q", &mut cues);
        assert_eq!(page.capture, Some(Column::Keyboard), "Q is Talk's");
        page.capture_key(&vui, "b", &mut cues);
        assert_eq!((page.capture, settings::binds().macros[0].key.as_deref()), (None, Some("b")));
        // its button: L3, off Autorun's field
        page.col = 1;
        page.key(key::CROSS, true, 0, &mut cues);
        let l3 = binds::bit_index(crate::input::bit::L3).unwrap();
        page.capture_button(&vui, l3, &mut cues);
        assert_eq!(settings::binds().macros[0].button, Some(l3));
        assert!(page.note.as_deref().is_some_and(|n| n.contains("has no button now")), "{:?}", page.note);
        // its buttons: Space (Attack's, ✕) and the controller's ○
        page.row = trigger + 1;
        page.col = 0;
        vui.time = 1000;
        page.key(key::CROSS, true, 1000, &mut cues);
        assert_eq!(page.capture, Some(Column::Chord));
        page.capture_key(&vui, "space", &mut cues);
        vui.time = 1400;
        page.capture_button(&vui, binds::bit_index(crate::input::bit::CIRCLE).unwrap(), &mut cues);
        vui.time = 2000;
        page.draw(&vui, &mut NullRaster, &mut w, &mut cues);
        assert_eq!(page.capture, Some(Column::Chord), "not yet a second after the last");
        vui.time = 2500;
        page.draw(&vui, &mut NullRaster, &mut w, &mut cues);
        assert_eq!(page.capture, None);
        assert_eq!(settings::binds().macros[0].chord, crate::input::bit::CROSS | crate::input::bit::CIRCLE);
        // the mode: Hold, Toggle, Turbo 1
        page.col = 1;
        page.key(key::CROSS, true, 2500, &mut cues);
        page.key(key::CROSS, true, 2500, &mut cues);
        assert_eq!(settings::binds().macros[0].mode_text(), "Turbo 1");
        // a row put on the macro's key takes it, and says the macro lost it;
        // the control: a row put on a free key says nothing of a macro
        page.row = 0;
        page.col = 0;
        page.key(key::CROSS, true, 2500, &mut cues);
        page.capture_key(&vui, "b", &mut cues);
        assert_eq!(settings::binds().macros[0].key, None);
        assert!(page.note.as_deref().is_some_and(|n| n.ends_with("Macro 1 has no key now.")), "{:?}", page.note);
        page.key(key::CROSS, true, 2500, &mut cues);
        page.capture_key(&vui, "u", &mut cues);
        assert_eq!(page.note.as_deref(), Some("Walk forward: U. B does nothing now."));
        page.row = trigger + 1;
        // the control: nothing pressed for eight seconds gives up, unchanged
        page.col = 0;
        page.key(key::CROSS, true, 3000, &mut cues);
        vui.time = 11_500;
        page.draw(&vui, &mut NullRaster, &mut w, &mut cues);
        assert_eq!(page.capture, None);
        assert_eq!(settings::binds().macros[0].chord, crate::input::bit::CROSS | crate::input::bit::CIRCLE);
        // Defaults clears it
        page.row = defaults();
        page.key(key::CROSS, true, 12_000, &mut cues);
        assert!(settings::binds().is_default());
        testing::reset(false);
    }

    /// A stick's button cannot be moved (sound 9, said); a row moved onto a
    /// free key says its old key does nothing; △'s row takes a key, and a row
    /// it takes one from is said to have none; Defaults puts everything back.
    #[test]
    fn what_cannot_move_and_defaults() {
        testing::reset(false);
        let vui = vui();
        let mut page = ControlsPage { col: 1, ..Default::default() };
        let mut cues = Vec::new();
        page.key(key::CROSS, true, 0, &mut cues);
        assert_eq!((page.capture, cues.last()), (None, Some(&9)), "walk forward's stick");
        assert_eq!(page.note.as_deref(), Some("The sticks stay sticks."));
        page.row = binds::row_named("hide_ui").unwrap() as i32;
        page.col = 0;
        page.key(key::CROSS, true, 0, &mut cues);
        page.capture_key(&vui, "f2", &mut cues);
        assert_eq!(page.note.as_deref(), Some("Hide UI: F2. H does nothing now."));
        // △'s row, which the game gives no key, takes one; Talk's, given it,
        // is left with none
        page.row = binds::row_named("clear_target").unwrap() as i32;
        page.key(key::CROSS, true, 0, &mut cues);
        assert_eq!(page.capture, Some(Column::Keyboard));
        page.capture_key(&vui, "q", &mut cues);
        assert_eq!(page.note.as_deref(), Some("Let the target go: Q. Talk (hold: (null)) has no key now."));
        assert_eq!(settings::binds().alias("q"), binds::Alias::Button(crate::input::bit::TRIANGLE));
        assert!(!settings::binds().is_default());
        page.row = defaults();
        page.key(key::CROSS, true, 0, &mut cues);
        assert!(settings::binds().is_default());
        testing::reset(false);
    }

    /// F3 20261024-145732 (a key of the client's on R3): a row of the client's
    /// keys takes a button in its controller's cell; the field it came from is said to
    /// have none; moved on to another button, the field that button drove
    /// takes R3 back; a field given the row's button gives the row its own,
    /// here none.
    /// The control: a stick's row still takes none.
    #[test]
    fn a_row_of_the_clients_keys_takes_a_button() {
        testing::reset(false);
        let vui = vui();
        let mut cues = Vec::new();
        let (hide, chat) = (binds::row_named("hide_ui").unwrap(), binds::row_named("chat").unwrap());
        let bit = |b: u16| binds::bit_index(b).unwrap();
        let mut page = ControlsPage { row: hide as i32, col: 1, ..Default::default() };
        page.key(key::CROSS, true, 0, &mut cues);
        assert_eq!(page.capture, Some(Column::Pad));
        page.capture_button(&vui, bit(crate::input::bit::R3), &mut cues);
        let b = settings::binds();
        assert_eq!((b.row_button(hide), b.button(bit(crate::input::bit::R3))), (Some(bit(crate::input::bit::R3)), binds::NO_BUTTON));
        assert_eq!(page.note.as_deref(), Some("Hide UI: |(. (null) has no button now."));
        page.key(key::CROSS, true, 0, &mut cues);
        page.capture_button(&vui, bit(crate::input::bit::L3), &mut cues);
        assert_eq!(page.note.as_deref(), Some("Hide UI: |9. Autorun: |(."));
        // the chat line's own cell takes L3 off Hide UI, and had no button to
        // give it: R3 is Autorun's now
        page.row = chat as i32;
        page.key(key::CROSS, true, 0, &mut cues);
        page.capture_button(&vui, bit(crate::input::bit::L3), &mut cues);
        assert_eq!(page.note.as_deref(), Some("(null): |9. Hide UI has no button now."));
        assert_eq!(settings::binds().row_button(hide), None);
        // the control
        page.row = binds::row_named("walk_forward").unwrap() as i32;
        page.key(key::CROSS, true, 0, &mut cues);
        assert_eq!((page.capture, page.note.as_deref()), (None, Some("The sticks stay sticks.")));
        testing::reset(false);
    }

    /// Held up or down steps on after 400 ms, every 90 ms, and stops when
    /// let go; the list scrolls to keep the selection in view.
    #[test]
    fn a_held_direction_repeats_and_the_list_scrolls() {
        testing::reset(false);
        let mut vui = vui();
        let mut page = ControlsPage::default();
        let mut w = window();
        let mut cues = Vec::new();
        page.draw(&vui, &mut NullRaster, &mut w, &mut cues);
        page.key(key::DOWN, true, 1000, &mut cues);
        for t in (1000..=2000).step_by(10) {
            vui.time = t;
            page.draw(&vui, &mut NullRaster, &mut w, &mut cues);
        }
        // 1 at the press, then at 1400, 1490, ... 1940: 7 more
        assert_eq!(page.row, 8);
        page.key(key::DOWN, false, 2000, &mut cues);
        vui.time = 3000;
        page.draw(&vui, &mut NullRaster, &mut w, &mut cues);
        assert_eq!(page.row, 8, "let go");
        // to the end: Defaults in view at the bottom
        for _ in 0..100 {
            page.key(key::DOWN, true, 0, &mut cues);
            page.key(key::DOWN, false, 0, &mut cues);
        }
        page.draw(&vui, &mut NullRaster, &mut w, &mut cues);
        assert_eq!(page.row, defaults());
        assert_eq!(page.top, ControlsPage::entries() - page.shown);
        assert_eq!(cues.last(), Some(&9), "the end");
        testing::reset(false);
    }

    /// A pointer: a row's words select it, a cell its column, the selection
    /// already there is ✕; while a key is awaited, any click gives up. The
    /// control: with no rows drawn nothing is hit.
    #[test]
    fn a_pointer_on_the_controls() {
        testing::reset(false);
        let vui = vui();
        let mut page = ControlsPage::default();
        let mut w = window();
        assert!(page.point([320.0, 200.0]).is_empty(), "nothing drawn yet");
        drawn(&mut page, &mut w);
        let width = w.size[0];
        let cell = |w: &Window, k: i32, x: i32, cw: i32| mid(rows_rect(w, &vui, &NullRaster, x + 20, k + 2, cw - 20, 1));
        let words = |w: &Window, k: i32| mid(rows_rect(w, &vui, &NullRaster, 40, k + 2, 100, 1));
        assert_eq!(page.point(words(&w, 3)), vec![key::DOWN, key::DOWN, key::DOWN]);
        assert_eq!(page.point(cell(&w, 0, width - PAD_X, PAD_W)), vec![key::RIGHT], "the controller's column");
        assert_eq!(page.point(cell(&w, 0, width - KEY_X, KEY_W)), vec![key::CROSS], "the selection: ✕");
        assert_eq!(page.point(words(&w, 0)), vec![key::CROSS]);
        page.capture = Some(Column::Keyboard);
        assert_eq!(page.point([1.0, 1.0]), vec![key::TRIANGLE], "a click gives up");
        testing::reset(false);
    }
}
