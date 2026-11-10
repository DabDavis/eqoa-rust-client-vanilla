//! The Options window's Macros page: the client's own (the user, 2026-11-10:
//! "we are using pcsx2 macro system, but it is not the same yet. lets also
//! add it as its own menu entry"), laid out after PCSX2's macro editor
//! (`pcsx2-qt/Settings/ControllerMacroWidget.ui` and
//! `ControllerMacroEditWidget.ui`, and Big Picture's Macro Buttons): the
//! sixteen macros listed on the left, each with what it presses, and the one
//! selected on the right: its triggers (PCSX2's Trigger: bound by holding
//! them, another added as Shift+click adds one, all cleared as its right
//! click clears them), Press To Toggle, Frequency, Pressure, Deadzone, and
//! what it presses, ticked on a grid of the game's buttons and the sticks'
//! ways in PCSX2's order (its Binds list). Defaults, after Macro 16, clears
//! every macro. What each setting does is `binds::Macro`'s and `macros`'.
//!
//! The keys: up and down along the list; ✕ or right into the macro, △ back
//! out of it, and △ on the list back to the options menu. In the macro, up
//! and down go along its rows; left and right change a value (a slider by
//! one, L1 and R1 by ten; Press To Toggle over) or move along the grid; ✕
//! binds a trigger, turns Press To Toggle or ticks what the grid's cursor is
//! on; □ takes a trigger off. A direction held repeats.
//!
//! A trigger is bound as PCSX2 binds one (`InputBindingWidget`): ✕, then
//! keys, the controller's buttons and sticks pushed past half held together
//! (`binds::CHORD_MOST` at most, a chord), kept the moment one of them is let
//! go; five seconds with nothing kept give up, as do Escape and a click.
//! Backspace takes off the trigger being bound. A key a row of the Controls
//! page is on, or one of the window's own, is refused (the wait goes on); a
//! button taken leaves the game's field or the Controls page's row it was on,
//! and a button no trigger has any more goes back, each said.
//!
//! A pointer: a macro's row selects it (selected, ✕); a row of the macro
//! selects it, and the selected one is ✕ (a value's label steps it on); a
//! value's halves are left and right; a click on a slider puts its knob
//! there; a cell of the grid moves the cursor there (there, ✕); while a
//! trigger is awaited any click gives up.
//!
//! The changes apply at once; Save Changes keeps them (`keys.opt`) and
//! Triangle on the menu puts them back, as the window does with the rest.

use crate::bindpage::{self, Column};
use crate::binds::{self, CHORD_MOST, FREQUENCY_PAGE_MOST, Input, MACROS, Macro, MacroKey, PRESSURE_LEAST, TRIGGERS_MOST, WAYS};
use crate::input::bit;
use crate::pointer::{Hits, MOVED, key, rows_rect};
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// A row of the macro selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Line {
    /// a trigger, by its place (with none, place 0: the first to bind)
    Trigger(usize),
    /// another trigger (PCSX2's Shift+click)
    Add,
    /// every trigger off (PCSX2's right click)
    Clear,
    Toggle,
    Frequency,
    Pressure,
    Deadzone,
    /// a row of the grid of what it presses (0 to 2)
    Binds(i32),
}

/// The macro's rows: its triggers (one to bind when it has none), Add a
/// Trigger while it has room, Clear Triggers while it has one, the four
/// settings and the grid's three rows.
fn lines(m: &Macro) -> Vec<Line> {
    let n = m.triggers.len();
    let mut v: Vec<Line> = (0..n.max(1)).map(Line::Trigger).collect();
    if n > 0 && n < TRIGGERS_MOST {
        v.push(Line::Add);
    }
    if n > 0 {
        v.push(Line::Clear);
    }
    v.extend([Line::Toggle, Line::Frequency, Line::Pressure, Line::Deadzone, Line::Binds(0), Line::Binds(1), Line::Binds(2)]);
    v
}

/// What a cell of the grid ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bind {
    /// one of the game's buttons (`input::bit`)
    Button(u16),
    /// a stick's way (`WAYS`' index)
    Way(u8),
}

/// The grid, eight a row, in PCSX2's DualShock 2's order (`PadDualshock2`'s
/// bindings): the d-pad, the face buttons, Select, Start, the shoulders, L3
/// and R3, then the sticks' ways (its Analog and Pressure buttons are no
/// game's: the pad is always in analog mode here, and the game reads no
/// pressure).
const GRID: [Bind; 24] = {
    use Bind::{Button as B, Way as W};
    [
        B(bit::UP),
        B(bit::RIGHT),
        B(bit::DOWN),
        B(bit::LEFT),
        B(bit::TRIANGLE),
        B(bit::CIRCLE),
        B(bit::CROSS),
        B(bit::SQUARE),
        B(bit::SELECT),
        B(bit::START),
        B(bit::L1),
        B(bit::L2),
        B(bit::R1),
        B(bit::R2),
        B(bit::L3),
        B(bit::R3),
        W(0),
        W(1),
        W(2),
        W(3),
        W(4),
        W(5),
        W(6),
        W(7),
    ]
};

/// A trigger is given up if nothing was kept this long after it began (ms):
/// PCSX2's `TIMEOUT_FOR_SINGLE_BINDING`, five seconds.
pub const CAPTURE_MS: i64 = 5000;

/// A stick's way counts as pressed for a trigger being bound past half its
/// travel, and as let go back under it (PCSX2's `inputManagerHookCallback`).
const CAPTURE_PUSH: f32 = 0.5;

/// Where things are, from the window's left: the list (the macros' names and
/// what they press), the macro's labels, its values' frames, a slider and
/// the number before it, the grid's cells.
const LIST_X: i32 = 40;
const SUMMARY_X: i32 = 118;
const SUMMARY_W: i32 = 80;
const EDIT_X: i32 = 230;
const LABEL_W: i32 = 140;
const VALUE_X: i32 = 380;
const VALUE_W: i32 = 180;
const SLIDER_X: i32 = 432;
const SLIDER_W: i32 = 128;
const CELL_W: i32 = 41;

/// A held direction repeats after this long, then this often (ms); left and
/// right on a slider faster.
const REPEAT_AFTER: i64 = 400;
const REPEAT_EVERY: i64 = 90;
const REPEAT_SLIDE: i64 = 45;

/// What a pointer's hit is (`Hits`): a macro of the list, a row's label, a
/// value's left and right halves, a slider's bar, a cell of the grid.
const LIST: i32 = 0;
const ROW: i32 = 100;
const LEFT: i32 = 200;
const RIGHT: i32 = 300;
const BAR: i32 = 400;
const CELL: i32 = 500;

/// A trigger being bound.
#[derive(Clone, Debug, PartialEq)]
pub struct Capture {
    /// the trigger it replaces; none, another after the last
    slot: Option<usize>,
    /// the inputs held so far, in order
    parts: Vec<Input>,
    /// when it began (ms)
    since: i64,
    /// the sticks' ways past half as it began (`WAYS`' bits): none of them
    /// taken until let go (a stick resting pushed is no trigger), as the
    /// first axes read them
    stale: Option<u8>,
}

/// The Macros page.
#[derive(Clone, Debug, Default)]
pub struct MacrosPage {
    /// the macro selected (0 to `MACROS` - 1), `MACROS` for Defaults
    pub list: i32,
    /// the first of the list shown
    top: i32,
    /// the macro's rows have the keys (else the list)
    pub editing: bool,
    /// the macro's row selected (`lines`' index) and the grid's column
    pub row: i32,
    pub cell: i32,
    /// a trigger being bound
    pub capture: Option<Capture>,
    /// what the last change did, said until the selection moves
    note: Option<String>,
    /// a direction held: its key, since when, when it last stepped
    held: Option<(i32, i64, i64)>,
    hits: Hits,
    /// the sliders as drawn: their row and their bar's left and right
    /// (frame pixels), for a click's place
    bars: Vec<(i32, i32, i32)>,
}

impl MacrosPage {
    /// As the page opens: the first macro, on the list, nothing awaited.
    pub fn reset(&mut self) {
        *self = MacrosPage::default();
    }

    /// What the page waits for: a trigger.
    pub fn capturing(&self) -> Option<Column> {
        self.capture.as_ref().map(|_| Column::Trigger)
    }

    /// The macro selected, if the list is on one.
    fn macro_at(&self) -> Option<usize> {
        usize::try_from(self.list).ok().filter(|&m| m < MACROS)
    }

    /// The macro's row selected.
    fn line(&self, b: &binds::Binds) -> Option<Line> {
        let m = self.macro_at()?;
        lines(&b.macros[m]).get(self.row as usize).copied()
    }

    /// The keys, as the window hands them on (pad keys and the keyboard's
    /// as the windows take them; before the world the keyboard's codes: Q
    /// □, ` L1, Tab R1, Page Up and Down by ten), down and up. True: back to
    /// the menu.
    pub fn key(&mut self, k: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> bool {
        if !down {
            if self.held.is_some_and(|h| direction(h.0) == direction(k)) {
                self.held = None;
            }
            return false;
        }
        if self.capture.is_some() {
            // a pointer's click gives up (its △)
            if matches!(k, 27 | 132) {
                self.capture = None;
                cues.push(8);
            }
            return false;
        }
        if direction(k).is_some() {
            self.held = Some((k, now, now));
        }
        self.act(k, now, cues)
    }

    /// A key's doing; true: back to the menu.
    fn act(&mut self, k: i32, now: i64, cues: &mut Vec<i32>) -> bool {
        match (self.editing, k) {
            (_, 19 | 138) => self.up_down(-1, cues),
            (_, 20 | 139) => self.up_down(1, cues),
            (false, 18 | 137 | 13 | 134) => {
                if self.macro_at().is_some() {
                    self.editing = true;
                    self.note = None;
                    self.clamp_row();
                    cues.push(if matches!(k, 18 | 137) { 11 } else { 8 });
                } else if matches!(k, 13 | 134) {
                    self.defaults(cues);
                } else {
                    cues.push(9);
                }
            }
            (false, 27 | 132 | 8) => {
                self.held = None;
                return true;
            }
            (true, 27 | 132 | 8) => {
                self.editing = false;
                self.note = None;
                cues.push(8);
            }
            (true, 17 | 136) => self.change(-1, cues),
            (true, 18 | 137) => self.change(1, cues),
            (true, 140 | 96 | 21) => self.change(-10, cues),
            (true, 142 | 9 | 22) => self.change(10, cues),
            (true, 13 | 134) => self.select(now, cues),
            (true, 133 | 113 | 81) => self.take_off(cues),
            _ => {}
        }
        false
    }

    /// Up or down a row of the list or the macro: sound 11, or 9 at an end.
    fn up_down(&mut self, d: i32, cues: &mut Vec<i32>) {
        let n = if self.editing {
            let b = crate::settings::binds();
            self.macro_at().map_or(0, |m| lines(&b.macros[m]).len() as i32)
        } else {
            MACROS as i32 + 1
        };
        let at = if self.editing { &mut self.row } else { &mut self.list };
        let to = *at + d;
        if (0..n).contains(&to) {
            *at = to;
            self.note = None;
            cues.push(11);
        } else {
            cues.push(9);
        }
    }

    /// The macro's row within its rows (they are fewer with fewer triggers).
    fn clamp_row(&mut self) {
        let b = crate::settings::binds();
        if let Some(m) = self.macro_at() {
            self.row = self.row.clamp(0, lines(&b.macros[m]).len() as i32 - 1);
        }
    }

    /// Left or right by `d`: a value stepped (a slider stops at its ends with
    /// sound 9; Press To Toggle turns over), the grid's cursor moved.
    fn change(&mut self, d: i32, cues: &mut Vec<i32>) {
        let mut b = crate::settings::binds();
        let (Some(m), Some(line)) = (self.macro_at(), self.line(&b)) else {
            cues.push(9);
            return;
        };
        let mac = &mut b.macros[m];
        let moved = match line {
            Line::Toggle if d.abs() == 1 => {
                mac.toggle = !mac.toggle;
                true
            }
            Line::Frequency => {
                let top = i32::from(mac.frequency.max(FREQUENCY_PAGE_MOST));
                step(&mut mac.frequency, d, 0, top)
            }
            Line::Pressure => step(&mut mac.pressure, d, i32::from(PRESSURE_LEAST), 100),
            Line::Deadzone => step(&mut mac.deadzone, d, 0, 100),
            Line::Binds(_) if d.abs() == 1 => {
                let to = self.cell + d;
                let ok = (0..8).contains(&to);
                if ok {
                    self.cell = to;
                }
                cues.push(if ok { 11 } else { 9 });
                return;
            }
            _ => false,
        };
        if moved {
            crate::settings::set_binds(b);
            self.note = None;
        }
        cues.push(if moved { 11 } else { 9 });
    }

    /// ✕ in the macro: a trigger awaited (its own, or another), the triggers
    /// cleared, Press To Toggle turned, the grid's cell ticked; a slider
    /// does nothing (sound 9).
    fn select(&mut self, now: i64, cues: &mut Vec<i32>) {
        let mut b = crate::settings::binds();
        let (Some(m), Some(line)) = (self.macro_at(), self.line(&b)) else {
            cues.push(9);
            return;
        };
        let n = b.macros[m].triggers.len();
        match line {
            Line::Trigger(k) => self.await_trigger((k < n).then_some(k), now, cues),
            Line::Add => self.await_trigger(None, now, cues),
            Line::Clear => {
                let freed = b.clear_triggers(m);
                crate::settings::set_binds(b);
                self.note = Some(format!("Macro {} has no trigger now.{}", m + 1, freed_note(&freed)));
                self.clamp_row();
                cues.push(13);
            }
            Line::Toggle => {
                b.macros[m].toggle = !b.macros[m].toggle;
                crate::settings::set_binds(b);
                cues.push(13);
            }
            Line::Binds(g) => {
                let mac = &mut b.macros[m];
                match GRID[(g * 8 + self.cell) as usize] {
                    Bind::Button(bit) => mac.buttons ^= bit,
                    Bind::Way(w) => mac.ways ^= 1 << w,
                }
                crate::settings::set_binds(b);
                self.note = None;
                cues.push(13);
            }
            _ => cues.push(9),
        }
    }

    /// A trigger awaited: `slot` replaced, or another.
    fn await_trigger(&mut self, slot: Option<usize>, now: i64, cues: &mut Vec<i32>) {
        self.capture = Some(Capture { slot, parts: Vec::new(), since: now, stale: None });
        self.note = None;
        cues.push(8);
    }

    /// □: the trigger selected taken off.
    fn take_off(&mut self, cues: &mut Vec<i32>) {
        let mut b = crate::settings::binds();
        let (Some(m), Some(Line::Trigger(k))) = (self.macro_at(), self.line(&b)) else {
            cues.push(9);
            return;
        };
        if k >= b.macros[m].triggers.len() {
            cues.push(9);
            return;
        }
        let gone = binds::chord_text(&b.macros[m].triggers[k]);
        let freed = b.remove_trigger(m, k);
        crate::settings::set_binds(b);
        self.note = Some(format!("Macro {}: {gone} taken off.{}", m + 1, freed_note(&freed)));
        self.clamp_row();
        cues.push(13);
    }

    /// Defaults (✕ on it): every macro cleared.
    fn defaults(&mut self, cues: &mut Vec<i32>) {
        let mut b = crate::settings::binds();
        let freed = b.clear_macros();
        crate::settings::set_binds(b);
        self.note = Some(format!("Every macro is cleared.{}", freed_note(&freed)));
        cues.push(13);
    }

    /// A key pressed while a trigger is awaited (its name, `binds::name_of`):
    /// Escape gives up; Backspace takes off the trigger being bound; a key of
    /// the window's own, or one a row of the Controls page is on, is refused
    /// (the wait goes on); any other is held in the trigger.
    pub fn capture_key(&mut self, vui: &Vui, name: &str, cues: &mut Vec<i32>) {
        let Some(c) = self.capture.as_ref() else { return };
        if name == "escape" {
            self.capture = None;
            cues.push(8);
            return;
        }
        if name == "backspace" {
            let slot = c.slot;
            self.capture = None;
            match (slot, self.macro_at()) {
                (Some(_), Some(_)) => self.take_off(cues),
                _ => cues.push(8),
            }
            return;
        }
        let b = crate::settings::binds();
        match b.check_trigger(&[Input::Key(name.to_string())]) {
            Err(MacroKey::Reserved(k)) => {
                self.note = Some(format!("{} is not for binding: choose another key.", binds::display(&k)));
                cues.push(9);
            }
            Err(MacroKey::Row(k, r)) => {
                self.note = Some(format!("{} is {}'s: choose a free key, or move that row on the Controls page.", binds::display(&k), bindpage::label(vui, r)));
                cues.push(9);
            }
            _ => self.hold(Input::Key(name.to_string()), cues),
        }
    }

    /// A key let go while a trigger is awaited: one of it, and the trigger is
    /// kept.
    pub fn capture_key_up(&mut self, vui: &Vui, name: &str, cues: &mut Vec<i32>) {
        self.let_go(vui, &Input::Key(name.to_string()), cues);
    }

    /// A controller's button pressed while a trigger is awaited (its bit
    /// index, before any binding): held in the trigger.
    pub fn capture_button(&mut self, phys: u8, cues: &mut Vec<i32>) {
        if self.capture.is_some() {
            self.hold(Input::Button(phys & 15), cues);
        }
    }

    /// A controller's button let go while a trigger is awaited.
    pub fn capture_button_up(&mut self, vui: &Vui, phys: u8, cues: &mut Vec<i32>) {
        self.let_go(vui, &Input::Button(phys & 15), cues);
    }

    /// The controllers' sticks (rx, ry, lx, ly, +y down) while a trigger is
    /// awaited, once a frame: a way pushed past half is held in it, and let
    /// back under half it is kept; a way pushed as the wait began counts
    /// only once let go.
    pub fn capture_axes(&mut self, vui: &Vui, sticks: [f32; 4], cues: &mut Vec<i32>) {
        let Some(c) = self.capture.as_mut() else { return };
        let w = binds::ways(sticks);
        let pushed: u8 = (0..8).filter(|&i| w[i] >= CAPTURE_PUSH).fold(0, |a, i| a | 1 << i);
        let stale = c.stale.get_or_insert(pushed);
        *stale &= pushed;
        let stale = *stale;
        let held: Vec<u8> = c.parts.iter().filter_map(|p| if let Input::Stick(s) = p { Some(*s) } else { None }).collect();
        if let Some(&s) = held.iter().find(|&&s| pushed & (1 << s) == 0) {
            self.let_go(vui, &Input::Stick(s), cues);
            return;
        }
        for i in 0..8u8 {
            if pushed & stale & (1 << i) == 0 && pushed & (1 << i) != 0 && !held.contains(&i) {
                self.hold(Input::Stick(i), cues);
            }
        }
    }

    /// An input held in the trigger being bound (`CHORD_MOST` of them).
    fn hold(&mut self, i: Input, cues: &mut Vec<i32>) {
        let Some(c) = self.capture.as_mut() else { return };
        if !c.parts.contains(&i) && c.parts.len() < CHORD_MOST {
            c.parts.push(i);
            self.note = None;
            cues.push(11);
        }
    }

    /// An input let go while a trigger is awaited: one held in it, and the
    /// trigger is kept (PCSX2's: at the first release).
    fn let_go(&mut self, vui: &Vui, i: &Input, cues: &mut Vec<i32>) {
        let Some(c) = self.capture.as_ref() else { return };
        if !c.parts.contains(i) {
            return;
        }
        let Some(c) = self.capture.take() else { return };
        let Some(m) = self.macro_at() else { return };
        let mut b = crate::settings::binds();
        match b.set_trigger(m, c.slot, c.parts.clone()) {
            Ok(t) => {
                let mut note = format!("Macro {}: {}.", m + 1, binds::chord_text(&c.parts));
                for &(g, f) in &t.fields {
                    if let Some(r) = binds::ROWS.iter().position(|r| r.pad == binds::Pad::Button(1u16 << f)) {
                        note.push_str(&format!(" {} has no button now.", bindpage::label(vui, r)));
                    } else {
                        note.push_str(&format!(" {} is no game's button now.", binds::glyph(g)));
                    }
                }
                for &(_, r) in &t.rows {
                    note.push_str(&format!(" {} has no button now.", bindpage::label(vui, r)));
                }
                note.push_str(&freed_note(&t.freed));
                crate::settings::set_binds(b);
                self.note = Some(note);
                self.clamp_row();
                cues.push(13);
            }
            Err(MacroKey::Nothing) => {
                self.note = Some(format!("Macro {} keeps {} triggers: take one off first.", m + 1, TRIGGERS_MOST));
                cues.push(9);
            }
            Err(MacroKey::Reserved(k) | MacroKey::Row(k, _)) => {
                self.note = Some(format!("{} cannot be in a trigger: choose another key.", binds::display(&k)));
                cues.push(9);
            }
        }
    }

    /// A pointer at frame point `p`: the keys to press. While a trigger is
    /// awaited a click gives up (△).
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.capture.is_some() {
            return vec![key::TRIANGLE];
        }
        let Some(h) = self.hits.at(p) else { return Vec::new() };
        if h < ROW {
            let e = h - LIST;
            if !self.editing && e == self.list {
                return vec![key::CROSS];
            }
            self.editing = false;
            self.list = e;
            self.note = None;
            self.held = None;
            return vec![MOVED];
        }
        let (kind, k) = (h / 100 * 100, h % 100);
        if kind == CELL {
            let (g, c) = (k / 8, k % 8);
            let row = self.binds_row(g);
            let there = self.editing && self.row == row && self.cell == c;
            (self.editing, self.row, self.cell) = (true, row, c);
            self.note = None;
            return vec![if there { key::CROSS } else { MOVED }];
        }
        let there = self.editing && self.row == k;
        (self.editing, self.row) = (true, k);
        match kind {
            LEFT => vec![key::LEFT],
            RIGHT => vec![key::RIGHT],
            BAR => {
                if let Some(&(_, x0, x1)) = self.bars.iter().find(|b| b.0 == k) {
                    self.slide_to(((p[0] - x0 as f32) / (x1 - x0).max(1) as f32).clamp(0.0, 1.0));
                }
                vec![MOVED]
            }
            _ if !there => {
                self.note = None;
                vec![MOVED]
            }
            _ => {
                let b = crate::settings::binds();
                match self.line(&b) {
                    Some(Line::Toggle | Line::Frequency | Line::Pressure | Line::Deadzone) => vec![key::RIGHT],
                    _ => vec![key::CROSS],
                }
            }
        }
    }

    /// The macro's row of the grid's row `g`.
    fn binds_row(&self, g: i32) -> i32 {
        let b = crate::settings::binds();
        self.macro_at().and_then(|m| lines(&b.macros[m]).iter().position(|&l| l == Line::Binds(g))).map_or(0, |r| r as i32)
    }

    /// The slider selected put at `t` of its travel (a click on its bar).
    fn slide_to(&mut self, t: f32) {
        let mut b = crate::settings::binds();
        let (Some(m), Some(line)) = (self.macro_at(), self.line(&b)) else { return };
        let mac = &mut b.macros[m];
        let at = |lo: i32, hi: i32| lo + ((hi - lo) as f32 * t).round() as i32;
        match line {
            Line::Frequency => mac.frequency = at(0, i32::from(mac.frequency.max(FREQUENCY_PAGE_MOST))) as u16,
            Line::Pressure => mac.pressure = at(i32::from(PRESSURE_LEAST), 100) as u8,
            Line::Deadzone => mac.deadzone = at(0, 100) as u8,
            _ => return,
        }
        crate::settings::set_binds(b);
        self.note = None;
    }

    /// The page in window `w`: the list, the macro selected, the help and
    /// the legend. A direction held steps on; a trigger awaited too long is
    /// given up.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, w: &mut Window, cues: &mut Vec<i32>) {
        let now = vui.time;
        if self.capture.as_ref().is_some_and(|c| now - c.since >= CAPTURE_MS) {
            self.capture = None;
            self.note = Some("Nothing was bound: hold its keys or buttons, then let one go.".into());
            cues.push(8);
        }
        let b = crate::settings::binds();
        if let Some((k, since, last)) = self.held
            && self.capture.is_none()
        {
            let slide = self.editing && matches!(direction(k), Some(Dir::Side)) && matches!(self.line(&b), Some(Line::Frequency | Line::Pressure | Line::Deadzone));
            let every = if slide { REPEAT_SLIDE } else { REPEAT_EVERY };
            if now - since >= REPEAT_AFTER && now - last >= every {
                self.act(k, now, cues);
                self.held = Some((k, since, now));
            }
        }
        let b = crate::settings::binds();
        self.hits.clear();
        self.bars.clear();
        w.draw(vui, r);
        let rows = w.rows;
        let dim = UI_COLORS[2];
        // the list: the sixteen and Defaults, as many as fit above the help
        let n = MACROS as i32 + 1;
        let shown = (rows - 4).clamp(1, n);
        if self.list < self.top {
            self.top = self.list;
        } else if self.list >= self.top + shown {
            self.top = self.list - shown + 1;
        }
        self.top = self.top.clamp(0, n - shown);
        for e in self.top..self.top + shown {
            let at = 1 + e - self.top;
            let c = UI_COLORS[(e == self.list) as usize];
            if e == MACROS as i32 {
                w.text_box(vui, r, &wide("Defaults"), LIST_X, at, SUMMARY_X + SUMMARY_W - LIST_X, 1, c, 0);
            } else {
                let mac = &b.macros[e as usize];
                w.text_box(vui, r, &wide(&format!("Macro {}", e + 1)), LIST_X, at, SUMMARY_X - LIST_X, 1, c, 0);
                let (s, sc) = if mac.presses() { (mac.binds_text(), UI_COLORS[0]) } else { ("-".to_string(), dim) };
                w.text_box(vui, r, &wide(&s), SUMMARY_X, at, SUMMARY_W, 1, if mac.set_up() { sc } else { dim }, 2);
            }
            self.hits.push(rows_rect(w, vui, r, LIST_X, at, SUMMARY_X + SUMMARY_W - LIST_X, 1), LIST + e);
        }
        if (self.top..self.top + shown).contains(&self.list) {
            w.tick_r(vui, r, LIST_X - 4, 1 + self.list - self.top, !self.editing);
        }
        if n > shown {
            let h = vui.font_height(r) - 4;
            let top = w.row_center(vui, r, 1)[1] - 4;
            w.draw_vscroll(vui, r, [w.top_left()[0] + SUMMARY_X + SUMMARY_W + 4, top - 3], shown * h + 4, self.top as f32 / (n - shown) as f32);
        }
        // the macro selected
        if let Some(m) = self.macro_at() {
            self.draw_macro(vui, r, w, &b.macros[m], now);
        }
        // the help: what is awaited, the last change, or the row's
        let help = self.help(&b, now);
        w.text_box(vui, r, &wide(&help), LIST_X, rows - 3, w.size[0] - 80, 2, UI_COLORS[0], 2);
        let back = vui.words.get(217, vui.language);
        let legend = if self.editing { format!("|x Change   |V|v Adjust   |7|8 By ten   |s Take off   |t {back}") } else { format!("|x|v Change   |t {back}") };
        w.text(vui, r, &wide(&legend), LIST_X, rows - 1, UI_COLORS[0], 0);
    }

    /// A macro's rows on the right: each label, its value in its frame (a
    /// slider with its number before it), and the grid of what it presses.
    fn draw_macro(&mut self, vui: &Vui, r: &mut dyn Raster, w: &mut Window, mac: &Macro, now: i64) {
        let ls = lines(mac);
        let dim = UI_COLORS[2];
        let arrow = vui.entry(128).uv1[0] + 3;
        for (k, &line) in ls.iter().enumerate() {
            let k = k as i32;
            let at = line_row(&ls, k);
            let on = self.editing && self.row == k;
            let c = UI_COLORS[on as usize];
            let label = match line {
                Line::Trigger(0) => "Trigger".to_string(),
                Line::Trigger(_) => "or".to_string(),
                Line::Add => "Add a Trigger".to_string(),
                Line::Clear => "Clear Triggers".to_string(),
                Line::Toggle => "Press To Toggle".to_string(),
                Line::Frequency => "Frequency".to_string(),
                Line::Pressure => "Pressure".to_string(),
                Line::Deadzone => "Deadzone".to_string(),
                Line::Binds(_) => String::new(),
            };
            if let Line::Binds(g) = line {
                // the grid's row: its cells, ticked ones framed, the cursor's
                // with its arrows
                if g == 0 {
                    w.text_box(vui, r, &wide("Presses (all at once)"), EDIT_X, at - 1, LABEL_W + VALUE_W, 1, if self.editing && matches!(ls.get(self.row as usize), Some(Line::Binds(_))) { UI_COLORS[1] } else { UI_COLORS[0] }, 0);
                }
                for c8 in 0..8 {
                    let x = EDIT_X + c8 * CELL_W;
                    let (text, ticked) = match GRID[(g * 8 + c8) as usize] {
                        Bind::Button(b) => (binds::glyph(binds::bit_index(b).unwrap_or(0)).to_string(), mac.buttons & b != 0),
                        Bind::Way(i) => (WAYS[i as usize].1.to_string(), mac.ways & (1 << i) != 0),
                    };
                    let cursor = on && self.cell == c8;
                    if ticked || cursor {
                        w.draw_frame(vui, r, x + 4, at, CELL_W - 8, 1, if cursor { 0x60 } else { 0x40 });
                    }
                    w.text_box(vui, r, &wide(&text), x, at, CELL_W, 1, if ticked { UI_COLORS[0] } else { dim }, 8);
                    self.hits.push(rows_rect(w, vui, r, x + 20, at, CELL_W - 20, 1), CELL + g * 8 + c8);
                }
                continue;
            }
            w.text_box(vui, r, &wide(&label), EDIT_X, at, LABEL_W, 1, c, 2);
            self.hits.push(rows_rect(w, vui, r, EDIT_X, at, LABEL_W, 1), ROW + k);
            match line {
                Line::Trigger(t) => {
                    let text = match &self.capture {
                        Some(cap) if on => {
                            let left = (CAPTURE_MS - (now - cap.since)).max(0) / 1000 + 1;
                            if cap.parts.is_empty() { format!("Press... {left}") } else { format!("{} + ... {left}", binds::chord_text(&cap.parts)) }
                        }
                        _ => mac.triggers.get(t).map_or_else(|| "-".to_string(), |ch| binds::chord_text(ch)),
                    };
                    w.draw_frame(vui, r, VALUE_X, at, VALUE_W, 1, if on { 0x60 } else { 0x40 });
                    w.text_box(vui, r, &wide(&text), VALUE_X, at, VALUE_W, 1, UI_COLORS[0], 8 | 2);
                    self.hits.push(rows_rect(w, vui, r, VALUE_X + 20, at, VALUE_W - 20, 1), ROW + k);
                }
                Line::Add if on && self.capture.is_some() => {
                    let cap = self.capture.as_ref().map(|c| c.parts.clone()).unwrap_or_default();
                    let left = self.capture.as_ref().map_or(0, |c| (CAPTURE_MS - (now - c.since)).max(0) / 1000 + 1);
                    let text = if cap.is_empty() { format!("Press... {left}") } else { format!("{} + ... {left}", binds::chord_text(&cap)) };
                    w.draw_frame(vui, r, VALUE_X, at, VALUE_W, 1, 0x60);
                    w.text_box(vui, r, &wide(&text), VALUE_X, at, VALUE_W, 1, UI_COLORS[0], 8 | 2);
                }
                Line::Toggle => {
                    let v = vui.words.get(if mac.toggle { 551 } else { 552 }, vui.language).to_string();
                    w.draw_frame(vui, r, VALUE_X, at, VALUE_W, 1, if on { 0x60 } else { 0x40 });
                    w.text_box(vui, r, &wide(&v), VALUE_X, at, VALUE_W, 1, UI_COLORS[0], 8);
                    let left = w.top_left()[0] + VALUE_X;
                    let rr = rows_rect(w, vui, r, VALUE_X, at, VALUE_W, 1);
                    self.hits.push([left - 4, rr[1], left + VALUE_W / 2, rr[3]], LEFT + k);
                    self.hits.push([left + VALUE_W / 2, rr[1], left + VALUE_W + 8, rr[3]], RIGHT + k);
                }
                Line::Frequency | Line::Pressure | Line::Deadzone => {
                    let (t, number) = match line {
                        Line::Frequency => {
                            let top = mac.frequency.max(FREQUENCY_PAGE_MOST);
                            (f32::from(mac.frequency) / f32::from(top), if mac.frequency == 0 { "Off".to_string() } else { mac.frequency.to_string() })
                        }
                        Line::Pressure => (f32::from(mac.pressure - PRESSURE_LEAST) / f32::from(100 - PRESSURE_LEAST), format!("{}%", mac.pressure)),
                        _ => (f32::from(mac.deadzone) / 100.0, format!("{}%", mac.deadzone)),
                    };
                    w.draw_hscroll(vui, r, SLIDER_X, at, SLIDER_W, t, if on { 32 } else { 0 });
                    let end = SLIDER_X - arrow - 2;
                    w.text_box(vui, r, &wide(&number), VALUE_X - 8, at, end - (VALUE_X - 8), 1, c, 0x10);
                    let left = w.top_left()[0] + SLIDER_X;
                    let rr = rows_rect(w, vui, r, SLIDER_X, at, SLIDER_W, 1);
                    self.hits.push([left - 4, rr[1], left + SLIDER_W + 4, rr[3]], BAR + k);
                    self.bars.push((k, left, left + SLIDER_W));
                }
                _ => {}
            }
        }
        // the tick at the row selected (the grid's has its cursor)
        if self.editing && matches!(ls.get(self.row as usize), Some(l) if !matches!(l, Line::Binds(_))) {
            w.tick_r(vui, r, EDIT_X - 4, line_row(&ls, self.row), true);
        }
    }

    /// The help under the list: what a trigger being bound wants, the last
    /// change, or what the row selected is.
    fn help(&self, b: &binds::Binds, now: i64) -> String {
        if let Some(c) = &self.capture {
            let left = (CAPTURE_MS - (now - c.since)).max(0) / 1000 + 1;
            return format!(
                "Hold its keys, buttons or sticks (four at most, together for a chord), then let one go. Esc gives up{}. {left}",
                if c.slot.is_some() { "; Backspace takes it off" } else { "" }
            );
        }
        if let Some(n) = &self.note {
            return n.clone();
        }
        let Some(m) = self.macro_at() else {
            return "Clears every macro: no triggers, nothing pressed, PCSX2's settings.".into();
        };
        let mac = &b.macros[m];
        if !self.editing {
            return summary(mac, m);
        }
        match self.line(b) {
            Some(Line::Trigger(t)) if t < mac.triggers.len() => {
                format!("Runs Macro {} while held: {}. |x binds it again, |s takes it off.", m + 1, binds::chord_text(&mac.triggers[t]))
            }
            Some(Line::Trigger(_)) => format!("Nothing runs Macro {} yet: |x, then hold a key, a button or a stick (several for a chord) and let one go.", m + 1),
            Some(Line::Add) => "Another trigger: any one of them runs the macro (PCSX2's Shift+click).".into(),
            Some(Line::Clear) => "Every trigger off: their keys free, their buttons the game's again.".into(),
            Some(Line::Toggle) => "On: a press starts the macro and the next stops it. Off: it runs while a trigger is held.".into(),
            Some(Line::Frequency) => "Off: held while it runs. N: let go and pressed every N frames, 60 a second; the game, at 30, sees 2 as 15 presses a second, 4 as 7.5.".into(),
            Some(Line::Pressure) => "How far it pushes a stick: under 24% the game reads it at rest, from 24% a walk, from 60% a run. A button is pressed at any.".into(),
            Some(Line::Deadzone) => "How far a stick, or L2 or R2, in a trigger must go to count. A key or a button counts unless it is 100%.".into(),
            Some(Line::Binds(_)) => "What it presses, all at once: |x ticks the button or the stick's way under the cursor.".into(),
            None => String::new(),
        }
    }
}

/// The macro as the list's help says it: what it presses, and how.
fn summary(mac: &Macro, m: usize) -> String {
    if *mac == Macro::default() {
        return format!("Macro {} is not set: |x to set it.", m + 1);
    }
    let presses = if mac.presses() { mac.binds_text() } else { "nothing".into() };
    let on: Vec<String> = mac.triggers.iter().map(|c| binds::chord_text(c)).collect();
    let when = match (on.is_empty(), mac.toggle) {
        (true, _) => "with no trigger".to_string(),
        (false, false) => format!("while {} is held", on.join(" or ")),
        (false, true) => format!("from a press of {} to the next", on.join(" or ")),
    };
    let every = if mac.frequency == 0 { String::new() } else { format!(", let go and pressed every {} frames", mac.frequency) };
    format!("Macro {} presses {presses} {when}{every}. |x to change it.", m + 1)
}

/// The window's row of the macro's row `k`: a row down, and the grid a row
/// below its title.
fn line_row(ls: &[Line], k: i32) -> i32 {
    let grid = ls.iter().position(|l| matches!(l, Line::Binds(_))).map_or(i32::MAX, |g| g as i32);
    1 + k + (k >= grid) as i32
}

/// The buttons a change gave back, as a note says them.
fn freed_note(freed: &[u8]) -> String {
    if freed.is_empty() {
        return String::new();
    }
    let g: Vec<&str> = freed.iter().map(|&b| binds::glyph(b)).collect();
    format!(" {} the game's again.", if g.len() == 1 { format!("{} is", g[0]) } else { format!("{} are", g.join(" ")) })
}

/// A setting stepped by `d` within `lo..=hi`: whether it moved.
fn step<T: Copy + Into<i32> + TryFrom<i32>>(v: &mut T, d: i32, lo: i32, hi: i32) -> bool {
    let now: i32 = (*v).into();
    let to = (now + d).clamp(lo, hi.max(lo));
    if to == now {
        return false;
    }
    if let Ok(t) = T::try_from(to) {
        *v = t;
    }
    true
}

/// Which way a key goes, if it is a direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dir {
    Up,
    Down,
    Side,
}

fn direction(k: i32) -> Option<Dir> {
    match k {
        19 | 138 => Some(Dir::Up),
        20 | 139 => Some(Dir::Down),
        17 | 136 | 18 | 137 => Some(Dir::Side),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::bit;
    use crate::pointer::testing::{NullRaster, mid, vui};
    use crate::settings::{self, testing};

    /// The null raster at another font's height (ARIALUI's is 21).
    struct Font(i32);

    impl Raster for Font {
        fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16]) {}
        fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16], _: u32) {}
        fn string_width(&self, s: &[u16]) -> i32 {
            s.len() as i32 * 8
        }
        fn point_size(&self) -> i32 {
            self.0
        }
        fn viewport(&self) -> [i32; 4] {
            [0, 0, 640, 448]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    /// The Options window's for the page: 600 wide, as many rows as fit.
    fn window(vui: &Vui, r: &dyn Raster) -> Window {
        let mut w = Window::default();
        w.init(vui, r, [320, 224], 600, bindpage::ControlsPage::window_rows(vui.font_height(r)), 0);
        w.open(true);
        w
    }

    fn press(page: &mut MacrosPage, k: i32) -> bool {
        let mut cues = Vec::new();
        let back = page.key(k, true, 0, &mut cues);
        page.key(k, false, 0, &mut cues);
        back
    }

    fn bit_of(b: u16) -> u8 {
        binds::bit_index(b).unwrap()
    }

    /// By the keys: down the list to Macro 3, ✕ into it, ✕ on its trigger
    /// and L1 and Y held, Y let go: the chord kept, L1 off the game's field
    /// (said); Add a Trigger and R3; □ on it takes it off, R3 the game's
    /// again; Clear Triggers and L1 is too; △ back to the list; Defaults
    /// clears every macro; △ on the list back to the menu. The control: a
    /// key Talk is on is refused and the wait goes on.
    #[test]
    fn a_macro_is_set_by_the_keys() {
        testing::reset(false);
        let vui = vui();
        let mut w = window(&vui, &NullRaster);
        let mut page = MacrosPage::default();
        let mut cues = Vec::new();
        page.draw(&vui, &mut NullRaster, &mut w, &mut cues);
        press(&mut page, key::DOWN);
        press(&mut page, key::DOWN);
        assert_eq!((page.list, page.editing), (2, false));
        press(&mut page, key::CROSS);
        assert!(page.editing, "into Macro 3");
        press(&mut page, key::CROSS);
        assert!(page.capturing().is_some(), "its trigger awaited");
        page.capture_button(bit_of(bit::L1), &mut cues);
        page.capture_key(&vui, "q", &mut cues);
        assert!(page.note.as_deref().is_some_and(|n| n.starts_with("Q is Talk")), "{:?}", page.note);
        assert!(page.capturing().is_some(), "the control: refused, the wait goes on");
        page.capture_key(&vui, "y", &mut cues);
        page.capture_key_up(&vui, "y", &mut cues);
        let b = settings::binds();
        assert_eq!(b.macros[2].triggers, vec![vec![Input::Button(bit_of(bit::L1)), Input::Key("y".into())]]);
        assert_eq!((page.capturing(), b.remap(bit::L1)), (None, 0), "kept at the first let go; L1 the trigger's");
        assert!(page.note.as_deref().is_some_and(|n| n.starts_with("Macro 3: |7 + Y.") && n.contains("has no button now")), "{:?}", page.note);
        // Add a Trigger: R3
        press(&mut page, key::DOWN);
        press(&mut page, key::CROSS);
        page.capture_button(bit_of(bit::R3), &mut cues);
        page.capture_button_up(&vui, bit_of(bit::R3), &mut cues);
        assert_eq!(settings::binds().macros[2].triggers.len(), 2);
        // □ on it: taken off, R3 the game's again
        assert_eq!(lines(&settings::binds().macros[2])[1], Line::Trigger(1));
        press(&mut page, key::SQUARE);
        assert_eq!((settings::binds().macros[2].triggers.len(), settings::binds().remap(bit::R3)), (1, bit::R3));
        assert!(page.note.as_deref().is_some_and(|n| n.contains("taken off") && n.ends_with("|( is the game's again.")), "{:?}", page.note);
        // Clear Triggers (Trigger, Add a Trigger, Clear Triggers)
        press(&mut page, key::DOWN);
        assert_eq!(lines(&settings::binds().macros[2])[page.row as usize], Line::Clear);
        press(&mut page, key::CROSS);
        assert!(settings::binds().macros[2].triggers.is_empty());
        assert_eq!(settings::binds().remap(bit::L1), bit::L1, "L1 the game's again");
        // △: the list; Defaults clears them all
        settings::set_binds({
            let mut b = settings::binds();
            b.set_trigger(5, None, vec![Input::Button(bit_of(bit::L3))]).unwrap();
            b.macros[5].buttons = bit::CROSS;
            b
        });
        press(&mut page, key::TRIANGLE);
        assert!(!page.editing);
        for _ in 0..20 {
            press(&mut page, key::DOWN);
        }
        assert_eq!(page.list, MACROS as i32, "Defaults, the list's last");
        press(&mut page, key::CROSS);
        assert!(settings::binds().is_default(), "every macro cleared, L3 back");
        assert!(press(&mut page, key::TRIANGLE), "△ on the list: the menu");
        testing::reset(false);
    }

    /// A trigger is bound as PCSX2 binds one: a stick resting pushed as the
    /// wait began is no part of it until let go; a stick pushed past half is,
    /// as are keys and buttons, four at most; it is kept the moment one is
    /// let go (a stick back under half). Escape gives up, Backspace takes off
    /// the trigger being bound again, five seconds with nothing kept give up,
    /// a click gives up. The controls: a stick under half, and a key of the
    /// window's own (F11), are no part of it.
    #[test]
    fn a_trigger_is_bound_as_pcsx2_binds_one() {
        testing::reset(false);
        let mut vui = vui();
        let mut w = window(&vui, &NullRaster);
        let mut page = MacrosPage { editing: true, ..MacrosPage::default() };
        let mut cues = Vec::new();
        page.key(key::CROSS, true, 0, &mut cues);
        page.capture_axes(&vui, [0.0, 0.0, 0.8, 0.0], &mut cues);
        assert!(page.capture.as_ref().unwrap().parts.is_empty(), "L right, pushed as it began");
        page.capture_axes(&vui, [0.0, 0.0, 0.0, -0.4], &mut cues);
        assert!(page.capture.as_ref().unwrap().parts.is_empty(), "the control: L up under half");
        page.capture_axes(&vui, [0.0, 0.0, 0.0, -0.7], &mut cues);
        page.capture_key(&vui, "f11", &mut cues);
        page.capture_button(bit_of(bit::CROSS), &mut cues);
        page.capture_key(&vui, "u", &mut cues);
        page.capture_button(bit_of(bit::L2), &mut cues);
        page.capture_button(bit_of(bit::R1), &mut cues);
        let parts = page.capture.as_ref().unwrap().parts.clone();
        assert_eq!(parts, vec![Input::Stick(0), Input::Button(bit_of(bit::CROSS)), Input::Key("u".into()), Input::Button(bit_of(bit::L2))], "four, F11 and the fifth left out");
        page.capture_axes(&vui, [0.0, 0.0, 0.0, -0.3], &mut cues);
        assert_eq!((page.capture.is_none(), settings::binds().macros[0].triggers.clone()), (true, vec![parts]), "kept as L up came back");
        // Escape: given up, the trigger as it was
        let had = settings::binds();
        page.key(key::CROSS, true, 0, &mut cues);
        page.capture_button(bit_of(bit::SQUARE), &mut cues);
        page.capture_key(&vui, "escape", &mut cues);
        assert_eq!((page.capture.is_none(), settings::binds()), (true, had));
        // Backspace: the trigger being bound again taken off
        page.key(key::CROSS, true, 0, &mut cues);
        page.capture_key(&vui, "backspace", &mut cues);
        assert!(page.capture.is_none() && settings::binds().macros[0].triggers.is_empty());
        assert_eq!(settings::binds().remap(0xFFFF), 0xFFFF, "its buttons the game's again");
        // five seconds, nothing kept
        page.key(key::CROSS, true, 1000, &mut cues);
        vui.time = 5999;
        page.draw(&vui, &mut NullRaster, &mut w, &mut cues);
        assert!(page.capture.is_some(), "not yet");
        vui.time = 6000;
        page.draw(&vui, &mut NullRaster, &mut w, &mut cues);
        assert!(page.capture.is_none() && settings::binds().macros[0].triggers.is_empty(), "given up");
        // a click
        page.key(key::CROSS, true, 6000, &mut cues);
        assert_eq!(page.point([10.0, 10.0]), vec![key::TRIANGLE]);
        page.key(key::TRIANGLE, true, 6000, &mut cues);
        assert!(page.capture.is_none());
        testing::reset(false);
    }

    /// Each setting steps in PCSX2's range: Press To Toggle turns over (left,
    /// right, ✕); Frequency 0 to 60 by one, L1 and R1 by ten, stopped at its
    /// ends (sound 9), and one set past 60 by hand steps down from there;
    /// Pressure 1 to 100; Deadzone 0 to 100. A held direction steps on after
    /// 400 ms, a slider every 45. The control: a slider at its end does not
    /// move.
    #[test]
    fn each_setting_steps_in_pcsx2s_range() {
        testing::reset(false);
        let mut vui = vui();
        let mut w = window(&vui, &NullRaster);
        let mut page = MacrosPage { editing: true, ..MacrosPage::default() };
        let row = |l: Line| lines(&Macro::default()).iter().position(|&x| x == l).unwrap() as i32;
        let m = || settings::binds().macros[0].clone();
        page.row = row(Line::Toggle);
        press(&mut page, key::RIGHT);
        assert!(m().toggle);
        press(&mut page, key::CROSS);
        assert!(!m().toggle);
        page.row = row(Line::Frequency);
        let mut cues = Vec::new();
        page.key(key::LEFT, true, 0, &mut cues);
        assert_eq!((m().frequency, cues.last()), (0, Some(&9)), "the control: at its end");
        for _ in 0..3 {
            press(&mut page, key::RIGHT);
        }
        press(&mut page, key::R1);
        assert_eq!(m().frequency, 13);
        press(&mut page, key::L1);
        press(&mut page, key::L1);
        assert_eq!(m().frequency, 0);
        for _ in 0..7 {
            press(&mut page, key::R1);
        }
        assert_eq!(m().frequency, FREQUENCY_PAGE_MOST, "60 at most");
        settings::set_binds({
            let mut b = settings::binds();
            b.macros[0].frequency = 300;
            b
        });
        press(&mut page, key::RIGHT);
        assert_eq!(m().frequency, 300, "set by hand: no further");
        press(&mut page, key::LEFT);
        assert_eq!(m().frequency, 299);
        page.row = row(Line::Pressure);
        press(&mut page, key::RIGHT);
        assert_eq!(m().pressure, 100);
        for _ in 0..12 {
            press(&mut page, key::L1);
        }
        assert_eq!(m().pressure, PRESSURE_LEAST);
        page.row = row(Line::Deadzone);
        for _ in 0..12 {
            press(&mut page, key::R1);
        }
        assert_eq!(m().deadzone, 100);
        // held: Deadzone down from 100 after 400 ms, then every 45
        let mut cues = Vec::new();
        page.key(key::LEFT, true, 1000, &mut cues);
        for t in (1000..=1600).step_by(5) {
            vui.time = t;
            page.draw(&vui, &mut NullRaster, &mut w, &mut cues);
        }
        // 1 at the press, then 1400, 1445, ... 1580: 5 more
        assert_eq!(m().deadzone, 94);
        page.key(key::LEFT, false, 1600, &mut cues);
        vui.time = 2000;
        page.draw(&vui, &mut NullRaster, &mut w, &mut cues);
        assert_eq!(m().deadzone, 94, "let go");
        testing::reset(false);
    }

    /// The grid ticks what the macro presses, in PCSX2's order: ↑ the first,
    /// ✕ the seventh, the sticks' ways on the third row; ✕ again unticks
    /// (the control); its ends stop the cursor; the list says what it
    /// presses.
    #[test]
    fn the_grid_ticks_what_it_presses() {
        testing::reset(false);
        let vui = vui();
        let mut w = window(&vui, &NullRaster);
        let mut page = MacrosPage { editing: true, ..MacrosPage::default() };
        let ls = lines(&Macro::default());
        page.row = ls.iter().position(|&l| l == Line::Binds(0)).unwrap() as i32;
        press(&mut page, key::CROSS);
        for _ in 0..6 {
            press(&mut page, key::RIGHT);
        }
        press(&mut page, key::CROSS);
        press(&mut page, key::DOWN);
        press(&mut page, key::DOWN);
        press(&mut page, key::CROSS);
        let m = settings::binds().macros[0].clone();
        assert_eq!((m.buttons, m.ways), (bit::UP | bit::CROSS, 1 << 6), "↑, ✕ and R↓");
        assert_eq!(m.binds_text(), "|Z|x R\u{2193}");
        let mut cues = Vec::new();
        page.key(key::RIGHT, true, 0, &mut cues);
        page.key(key::RIGHT, true, 0, &mut cues);
        assert_eq!((page.cell, cues.last()), (7, Some(&9)), "the row's end");
        press(&mut page, key::LEFT);
        press(&mut page, key::CROSS);
        assert_eq!(settings::binds().macros[0].ways, 0, "the control: ticked again, off");
        page.draw(&vui, &mut NullRaster, &mut w, &mut cues);
        press(&mut page, key::TRIANGLE);
        assert!(summary(&settings::binds().macros[0], 0).starts_with("Macro 1 presses |Z|x with no trigger"), "{}", summary(&settings::binds().macros[0], 0));
        testing::reset(false);
    }

    /// A pointer: a macro's row selects it, and selected is ✕; a row of the
    /// macro selects it, and selected steps a value on; a value's halves are
    /// left and right; a click on a slider's bar puts its knob there; a cell
    /// of the grid moves the cursor, and there is ✕; while a trigger is
    /// awaited any click gives up. The control: with nothing drawn nothing
    /// is hit.
    #[test]
    fn a_pointer_on_the_macros_page() {
        testing::reset(false);
        let vui = vui();
        let r = NullRaster;
        let mut w = window(&vui, &r);
        let mut page = MacrosPage::default();
        assert!(page.point([320.0, 200.0]).is_empty(), "nothing drawn yet");
        let mut cues = Vec::new();
        page.draw(&vui, &mut NullRaster, &mut w, &mut cues);
        let list = |e: i32| mid(rows_rect(&w, &vui, &r, LIST_X, 1 + e, SUMMARY_X + SUMMARY_W - LIST_X, 1));
        assert_eq!(page.point(list(3)), vec![MOVED]);
        assert_eq!((page.list, page.editing), (3, false));
        assert_eq!(page.point(list(3)), vec![key::CROSS], "selected: ✕");
        let ls = lines(&Macro::default());
        let at = |l: Line| ls.iter().position(|&x| x == l).unwrap() as i32;
        let label = |l: Line| mid(rows_rect(&w, &vui, &r, EDIT_X, line_row(&ls, at(l)), LABEL_W, 1));
        assert_eq!(page.point(label(Line::Toggle)), vec![MOVED]);
        assert_eq!((page.editing, page.row), (true, at(Line::Toggle)));
        assert_eq!(page.point(label(Line::Toggle)), vec![key::RIGHT], "selected: stepped on");
        let y = |l: Line| w.row_center(&vui, &r, line_row(&ls, at(l)))[1] as f32;
        let left = (w.top_left()[0] + VALUE_X) as f32;
        assert_eq!(page.point([left + 10.0, y(Line::Toggle)]), vec![key::LEFT]);
        assert_eq!(page.point([left + VALUE_W as f32 - 10.0, y(Line::Toggle)]), vec![key::RIGHT]);
        // Pressure's bar, a quarter along: 1 + 99 x 0.25, 26%
        let bar = (w.top_left()[0] + SLIDER_X) as f32;
        assert_eq!(page.point([bar + SLIDER_W as f32 / 4.0, y(Line::Pressure)]), vec![MOVED]);
        assert_eq!((page.row, settings::binds().macros[3].pressure), (at(Line::Pressure), 26));
        // the grid's second row, third cell: L1
        let g1 = line_row(&ls, at(Line::Binds(1)));
        let cell = mid(rows_rect(&w, &vui, &r, EDIT_X + 2 * CELL_W + 20, g1, CELL_W - 20, 1));
        assert_eq!(page.point(cell), vec![MOVED]);
        assert_eq!((page.row, page.cell), (at(Line::Binds(1)), 2));
        assert_eq!(page.point(cell), vec![key::CROSS]);
        press(&mut page, key::CROSS);
        assert_eq!(settings::binds().macros[3].buttons, bit::L1);
        // a trigger awaited: any click gives up
        page.row = 0;
        press(&mut page, key::CROSS);
        assert_eq!(page.point(list(0)), vec![key::TRIANGLE]);
        testing::reset(false);
    }

    /// The page fits the window at the game's font (ARIALUI, 21 high: 21
    /// rows) and the tests' (16: 24 rows): the list's sixteen and Defaults,
    /// and a macro's rows at their most (four triggers, Clear, the four
    /// settings, the grid and its title) above the help's two rows, the
    /// legend on the last. The control: at a font so tall the window has
    /// twelve rows, the list scrolls to keep Defaults in view.
    #[test]
    fn the_page_fits_the_window() {
        testing::reset(false);
        let mut full = Macro::default();
        for i in 0..TRIGGERS_MOST {
            full.triggers.push(vec![Input::Stick(i as u8)]);
        }
        let ls = lines(&full);
        let lowest = line_row(&ls, ls.len() as i32 - 1);
        assert_eq!(lowest, 13, "the grid's last row");
        for h in [21, 16] {
            let vui = vui();
            let w = window(&vui, &Font(h));
            let help = w.rows - 3;
            assert!(MACROS as i32 + 1 < help && lowest < help, "{h}: rows {}", w.rows);
            let mut page = MacrosPage { list: MACROS as i32, ..MacrosPage::default() };
            let mut w = w;
            page.draw(&vui, &mut Font(h), &mut w, &mut Vec::new());
            assert_eq!(page.top, 0, "{h}: the whole list in view");
        }
        let vui = vui();
        let mut w = window(&vui, &Font(36));
        assert_eq!(w.rows, 12);
        let mut page = MacrosPage { list: MACROS as i32, ..MacrosPage::default() };
        page.draw(&vui, &mut Font(36), &mut w, &mut Vec::new());
        assert!(page.top > 0 && page.top + (w.rows - 4) > MACROS as i32, "the control: scrolled, Defaults in view");
        testing::reset(false);
    }
}
