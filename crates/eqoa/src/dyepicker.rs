//! The dyers' colour picker (the server owner, 2026-11-17: "lets upgrade the
//! dyer NPCs to show a RGB color picker, add to client if needed. then we
//! have any rgb combo"): the client's own window, as the game has none, opened
//! by go-eqoa-server's Equipment Dyers, Pet Groomers and Weapon Enchanters
//! when "Pick any colour..." is chosen (`net::dyepick`, `dyeplay`).
//!
//! A saturation and value square for the hue the strip beside it picks, the
//! hue, saturation, value, red, green and blue (and the alpha, where the
//! server says it is the player's: a pet's dye) as sliders with their numbers,
//! the colour as hex, the new colour beside the old, the colours picked last
//! as swatches, and OK and Cancel. The colour is previewed on the character
//! or the pet while it is picked (`dyeplay`). No preset colours (the server
//! owner, 2026-11-18: "lets remove the color presets from the dyer simplify
//! with the rgb picker"): the dyer's palette the server still sends is not
//! shown.
//!
//! - The pad (the server owner, 2026-11-18: "the rgb dye selector only works
//!   for mouse, lets make controller friendly"): the d-pad or the left stick,
//!   up and down a row (held, repeating), left and right the row's value by
//!   one (held, repeating and faster; the stick pushed further, faster), L1
//!   and R1 (and L2, R2) by ten; the row lit is framed in yellow, and the hue
//!   strip or the square too while its row is; on the recent colours left and
//!   right a swatch and ✕ takes it; ✕ anywhere else (or Start) picks, Cancel's
//!   ✕, △ or ○ cancel. A legend along the bottom says so in the pad's glyphs.
//!   The keyboard's arrows, Enter and Escape as those, Tab a row down,
//!   Page Up and Page Down by ten, and hex digits typed anywhere the colour
//!   ("3FA7D6", Backspace to take one back).
//! - A pointer (the mouse, a finger): a press on the square, the strip or a
//!   slider sets it there and a drag goes on setting it; a swatch, OK and
//!   Cancel act where they are let go.

use crate::pointer::Hits;
use vi::ui::{FrameParam, Raster, Vui, icon_id, wide};

/// The window's top left and size, in the game's 640x448 frame.
pub const AT: [i32; 2] = [116, 76];
pub const W: i32 = 408;
pub const H: i32 = 276;

/// The square, the hue strip, the right column, the rows and the swatches,
/// from the window's top left.
const SQUARE: [i32; 3] = [12, 32, 128];
const STRIP: [i32; 3] = [148, 32, 14];
const COLUMN: i32 = 174;
const SWATCH_NEW: [i32; 4] = [COLUMN, 32, 92, 28];
const SWATCH_OLD: [i32; 4] = [COLUMN + 94, 32, 40, 28];
const HEX_X: i32 = COLUMN + 142;
const ROWS_Y: i32 = 70;
const ROW_H: i32 = 18;
const BAR: [i32; 2] = [COLUMN + 18, 140];
const CELL: i32 = 14;
const SWATCHES_Y: i32 = 204;
const LEGEND_Y: i32 = H - 46;
const BUTTONS_Y: i32 = H - 22;
const BUTTON_X: [i32; 2] = [W / 2 - 60, W / 2 + 60];

/// How many colours picked last are kept.
pub const RECENT: usize = 10;

/// The cells a gradient is drawn in.
const SQUARE_CELLS: i32 = 32;
const BAR_CELLS: i32 = 24;

/// A slider's row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slider {
    Hue,
    Sat,
    Val,
    Red,
    Green,
    Blue,
    Alpha,
}

impl Slider {
    const ALL: [Slider; 7] = [Slider::Hue, Slider::Sat, Slider::Val, Slider::Red, Slider::Green, Slider::Blue, Slider::Alpha];

    fn label(self) -> &'static str {
        match self {
            Slider::Hue => "H",
            Slider::Sat => "S",
            Slider::Val => "V",
            Slider::Red => "R",
            Slider::Green => "G",
            Slider::Blue => "B",
            Slider::Alpha => "A",
        }
    }

    /// Its value's most (hue degrees, saturation and value percent, the
    /// channels 255).
    fn most(self) -> i32 {
        match self {
            Slider::Hue => 359,
            Slider::Sat | Slider::Val => 100,
            _ => 255,
        }
    }
}

/// A row the pad is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Slider(Slider),
    Swatches,
    Buttons,
}

/// What the picker tells its parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerUp {
    /// OK: the colour, 0xRRGGBBAA
    Pick(u32),
    Cancel,
}

/// HSV (hue 0..360, saturation and value 0..1) to RGB, rounded.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let (s, v) = (s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r, g, b].map(|u| ((u + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

/// RGB to HSV (hue 0..360, 0 for a grey; saturation and value 0..1).
pub fn rgb_to_hsv(rgb: [u8; 3]) -> (f32, f32, f32) {
    let [r, g, b] = rgb.map(|c| c as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h, s, max)
}

/// A colour as hex: "3FA7D6", its alpha after it ("3FA7D680") when `alpha`.
pub fn to_hex(rgba: u32, alpha: bool) -> String {
    if alpha { format!("{rgba:08X}") } else { format!("{:06X}", rgba >> 8) }
}

/// Hex typed ("#3fa7d6", "3FA7D6", "3FA7D680", "#abc"): the colour,
/// 0xRRGGBBAA, its alpha FF where none was given. None when it is not six or
/// eight hex digits (or three, each doubled).
pub fn parse_hex(s: &str) -> Option<u32> {
    let s = s.trim().trim_start_matches('#');
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match s.len() {
        3 => {
            let d: Vec<u32> = s.chars().map(|c| c.to_digit(16).unwrap_or(0) * 17).collect();
            Some(d[0] << 24 | d[1] << 16 | d[2] << 8 | 0xFF)
        }
        6 => u32::from_str_radix(s, 16).ok().map(|v| v << 8 | 0xFF),
        8 => u32::from_str_radix(s, 16).ok(),
        _ => None,
    }
}

/// A colour's channels, 0..1, for the raster.
pub fn rgba_f(rgba: u32) -> [f32; 4] {
    [(rgba >> 24) as u8, (rgba >> 16) as u8, (rgba >> 8) as u8, rgba as u8].map(|c| c as f32 / 255.0)
}

fn pack(rgb: [u8; 3], a: u8) -> u32 {
    u32::from(rgb[0]) << 24 | u32::from(rgb[1]) << 16 | u32::from(rgb[2]) << 8 | u32::from(a)
}

/// What a press holds while it is dragged, or is let go on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Held {
    Square,
    Strip,
    Slider(Slider),
    Swatch(usize),
    Button(usize),
}

// the hit notes' kinds
const HIT_SQUARE: i32 = 1;
const HIT_STRIP: i32 = 2;
const HIT_SLIDER: i32 = 10;
const HIT_SWATCH: i32 = 100;
const HIT_BUTTON: i32 = 400;
const HIT_WINDOW: i32 = 500;

/// The picker.
#[derive(Clone, Debug, Default)]
pub struct Picker {
    pub open: bool,
    pub title: String,
    /// whether the alpha is the player's (its row shown)
    pub alpha_on: bool,
    /// the colour now: hue degrees, saturation and value 0..1, the channels
    /// they make and the alpha
    hue: f32,
    sat: f32,
    val: f32,
    rgb: [u8; 3],
    alpha: u8,
    /// the colour it opened on (0: none), drawn beside the new
    pub old: u32,
    /// the swatches: the colours picked last
    pub recent: Vec<u32>,
    /// the pad's row, the swatch it is on, the button it is on
    pub row: Row,
    swatch: usize,
    button: usize,
    /// hex digits typed
    pub typed: String,
    /// a key held (its code, when it went down, when it last stepped)
    held_key: Option<(i32, i64, i64)>,
    /// the frames (`tick`s) the key has been held: a repeat waits for a few,
    /// so one slow frame between a press and its release is no hold
    held_ticks: u32,
    /// the left stick's way as a d-pad key (136-139) while it is pushed
    /// past half, and how far (0.5..1) it is pushed
    stick_key: Option<i32>,
    stick_push: f32,
    /// a pointer press held
    held: Option<Held>,
    hits: Hits,
}

impl Default for Row {
    fn default() -> Self {
        Row::Slider(Slider::Hue)
    }
}

impl Picker {
    /// Opened on colour `current` (0 none: white), the alpha the player's
    /// when `alpha_on`, with the recent colours' swatches.
    pub fn open(&mut self, title: &str, current: u32, alpha_on: bool, recent: Vec<u32>) {
        *self = Picker { open: true, title: title.to_string(), alpha_on, old: current, recent, ..Default::default() };
        let c = if current == 0 { 0xFFFF_FFFF } else { current };
        self.set_rgba(c);
        if !alpha_on {
            self.alpha = 0xFF;
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.held = None;
        self.held_key = None;
        self.stick_key = None;
        self.hits.clear();
    }

    /// The colour now, 0xRRGGBBAA.
    pub fn rgba(&self) -> u32 {
        pack(self.rgb, if self.alpha_on { self.alpha } else { 0xFF })
    }

    /// Set to `rgba`: its channels, and the HSV they are (the hue kept for a
    /// grey, the saturation for black).
    pub fn set_rgba(&mut self, rgba: u32) {
        let rgb = [(rgba >> 24) as u8, (rgba >> 16) as u8, (rgba >> 8) as u8];
        self.set_rgb(rgb);
        self.alpha = if self.alpha_on { (rgba as u8).max(1) } else { 0xFF };
    }

    fn set_rgb(&mut self, rgb: [u8; 3]) {
        let (h, s, v) = rgb_to_hsv(rgb);
        self.rgb = rgb;
        if v > 0.0 {
            if s > 0.0 {
                self.hue = h;
            }
            self.sat = s;
        }
        self.val = v;
    }

    fn set_hsv(&mut self, h: f32, s: f32, v: f32) {
        self.hue = h.clamp(0.0, 359.999);
        self.sat = s.clamp(0.0, 1.0);
        self.val = v.clamp(0.0, 1.0);
        self.rgb = hsv_to_rgb(self.hue, self.sat, self.val);
    }

    /// The rows the pad walks, top to bottom.
    fn rows(&self) -> Vec<Row> {
        let mut out: Vec<Row> = Slider::ALL.iter().filter(|&&s| s != Slider::Alpha || self.alpha_on).map(|&s| Row::Slider(s)).collect();
        if !self.recent.is_empty() {
            out.push(Row::Swatches);
        }
        out.push(Row::Buttons);
        out
    }

    fn sliders(&self) -> Vec<Slider> {
        Slider::ALL.iter().copied().filter(|&s| s != Slider::Alpha || self.alpha_on).collect()
    }

    /// A slider's value as its row shows it.
    pub fn value(&self, s: Slider) -> i32 {
        match s {
            Slider::Hue => self.hue.round() as i32 % 360,
            Slider::Sat => (self.sat * 100.0).round() as i32,
            Slider::Val => (self.val * 100.0).round() as i32,
            Slider::Red => self.rgb[0] as i32,
            Slider::Green => self.rgb[1] as i32,
            Slider::Blue => self.rgb[2] as i32,
            Slider::Alpha => self.alpha as i32,
        }
    }

    /// A slider set to `v` (held to its range; the hue wraps).
    pub fn set_value(&mut self, s: Slider, v: i32) {
        let c = |v: i32| v.clamp(0, 255) as u8;
        match s {
            Slider::Hue => self.set_hsv(v.rem_euclid(360) as f32, self.sat, self.val),
            Slider::Sat => self.set_hsv(self.hue, v.clamp(0, 100) as f32 / 100.0, self.val),
            Slider::Val => self.set_hsv(self.hue, self.sat, v.clamp(0, 100) as f32 / 100.0),
            Slider::Red => self.set_rgb([c(v), self.rgb[1], self.rgb[2]]),
            Slider::Green => self.set_rgb([self.rgb[0], c(v), self.rgb[2]]),
            Slider::Blue => self.set_rgb([self.rgb[0], self.rgb[1], c(v)]),
            Slider::Alpha => self.alpha = v.clamp(1, 255) as u8,
        }
    }

    fn step(&mut self, s: Slider, by: i32) {
        let v = self.value(s) + by;
        let v = if s == Slider::Hue { v } else { v.clamp(0, s.most()) };
        self.set_value(s, v);
    }

    /// Every swatch: the recent colours.
    fn swatches(&self) -> Vec<u32> {
        self.recent.clone()
    }

    fn row_step(&mut self, by: i32) {
        let rows = self.rows();
        let i = rows.iter().position(|&r| r == self.row).unwrap_or(0) as i32;
        self.row = rows[(i + by).clamp(0, rows.len() as i32 - 1) as usize];
    }

    /// The row's left or right by `by` (a slider's value, a swatch, a button).
    fn sideways(&mut self, by: i32) {
        match self.row {
            Row::Slider(s) => self.step(s, by),
            Row::Swatches => {
                let n = self.swatches().len();
                if n > 0 {
                    self.swatch = (self.swatch as i32 + by.signum()).clamp(0, n as i32 - 1) as usize;
                }
            }
            Row::Buttons => self.button = (self.button as i32 + by.signum()).clamp(0, 1) as usize,
        }
    }

    /// A key (the game's codes; the keyboard's arrows, Enter and Escape as
    /// the pad's) at `now` (ms): what to tell the parent, if anything. Held
    /// left and right repeat in `tick`.
    pub fn key(&mut self, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<PickerUp> {
        if !self.open {
            return None;
        }
        if !down {
            if self.held_key.is_some_and(|h| h.0 == key) {
                self.held_key = None;
            }
            return None;
        }
        match key {
            136 | 17 | 137 | 18 => {
                self.sideways(if matches!(key, 136 | 17) { -1 } else { 1 });
                self.held_key = Some((key, now, now));
                self.held_ticks = 0;
                cues.push(11);
            }
            // L1, R1 (Page Up, Page Down) and L2, R2: by ten
            140 | 142 | 141 | 143 => {
                self.sideways(if matches!(key, 140 | 141) { -10 } else { 10 });
                self.held_key = Some((key, now, now));
                self.held_ticks = 0;
                cues.push(11);
            }
            138 | 19 | 139 | 20 | 9 => {
                self.row_step(if matches!(key, 138 | 19) { -1 } else { 1 });
                if key != 9 {
                    self.held_key = Some((key, now, now));
                    self.held_ticks = 0;
                }
                cues.push(11);
            }
            134 | 13 => match self.row {
                Row::Swatches => {
                    if let Some(&c) = self.swatches().get(self.swatch) {
                        self.set_rgba(c | if self.alpha_on { 0 } else { 0xFF });
                        cues.push(13);
                    }
                }
                Row::Buttons if self.button == 1 => {
                    cues.push(8);
                    return self.finish(false);
                }
                // ✕ on OK or on a slider: the colour picked
                Row::Buttons | Row::Slider(_) => {
                    cues.push(13);
                    return self.finish(true);
                }
            },
            144 => {
                cues.push(13);
                return self.finish(true);
            }
            // △, ○ and Escape: cancelled
            132 | 135 | 27 => {
                cues.push(8);
                return self.finish(false);
            }
            _ => {}
        }
        None
    }

    /// The left stick (`lx`, `ly`: -1..1, +y down) at `now` (ms), a frame:
    /// pushed past half one way it is that way's d-pad key held (a row up or
    /// down, the value left or right, repeating as the d-pad's), let back it
    /// is let go. The further it is pushed sideways the faster the value
    /// runs (`tick`).
    pub fn stick(&mut self, lx: f32, ly: f32, now: i64, cues: &mut Vec<i32>) {
        if !self.open {
            self.stick_key = None;
            return;
        }
        let (ax, ay) = (lx.abs(), ly.abs());
        let way = if ax.max(ay) < 0.5 {
            None
        } else if ay > ax {
            Some(if ly < 0.0 { 138 } else { 139 })
        } else {
            Some(if lx < 0.0 { 136 } else { 137 })
        };
        self.stick_push = ax.max(ay);
        if way == self.stick_key {
            return;
        }
        if let Some(k) = self.stick_key.take() {
            self.key(k, false, now, cues);
        }
        if let Some(k) = way {
            self.key(k, true, now, cues);
            self.stick_key = Some(k);
        }
    }

    /// A held left, right, L1 or R1 (or up or down, or the stick) at `now`
    /// (ms), a frame: stepped again after 300 ms and four frames, every 50 ms
    /// (a row every 150), by five after a second and a half (the stick
    /// pushed all the way, by five at once).
    pub fn tick(&mut self, now: i64) {
        let Some((key, at, last)) = self.held_key else { return };
        self.held_ticks = self.held_ticks.saturating_add(1);
        let rows = matches!(key, 138 | 19 | 139 | 20);
        let every = if rows { 150 } else { 50 };
        if now - at < 300 || now - last < every || self.held_ticks < 4 {
            return;
        }
        if rows {
            self.row_step(if matches!(key, 138 | 19) { -1 } else { 1 });
            self.held_key = Some((key, at, now));
            return;
        }
        let pushed = self.stick_key == Some(key) && self.stick_push > 0.95;
        let mut by = if now - at > 1500 || pushed { 5 } else { 1 };
        if matches!(key, 140 | 142 | 141 | 143) {
            by = 10;
        }
        self.sideways(if matches!(key, 136 | 17 | 140 | 141) { -by } else { by });
        self.held_key = Some((key, at, now));
    }

    /// A hex digit typed (or Backspace, '\u{8}'): six (eight with the alpha)
    /// set the colour; a '#' starts again.
    pub fn type_char(&mut self, c: char) {
        match c {
            '#' => self.typed.clear(),
            '\u{8}' => {
                self.typed.pop();
            }
            c if c.is_ascii_hexdigit() => {
                let most = if self.alpha_on { 8 } else { 6 };
                if self.typed.len() >= most {
                    self.typed.clear();
                }
                self.typed.push(c.to_ascii_uppercase());
                if self.typed.len() == 6 || self.typed.len() == 8 {
                    if let Some(v) = parse_hex(&self.typed) {
                        let a = if self.typed.len() == 8 { v as u8 } else { if self.alpha_on { self.alpha } else { 0xFF } };
                        self.set_rgba(v & 0xFFFF_FF00 | u32::from(a));
                    }
                }
            }
            _ => {}
        }
    }

    /// OK (the colour picked, kept among the recent) or Cancel: closed.
    fn finish(&mut self, ok: bool) -> Option<PickerUp> {
        let c = self.rgba();
        self.close();
        Some(if ok { PickerUp::Pick(c) } else { PickerUp::Cancel })
    }

    /// Whether frame point `p` is on the window.
    pub fn covers(&self, p: [f32; 2]) -> bool {
        self.open && p[0] >= AT[0] as f32 && p[0] < (AT[0] + W) as f32 && p[1] >= AT[1] as f32 && p[1] < (AT[1] + H) as f32
    }

    /// A press (the left button, a finger) at frame point `p`: true when it
    /// was the window's (anywhere on it). The square, the strip and the
    /// sliders are set there at once.
    pub fn press(&mut self, p: [f32; 2]) -> bool {
        if !self.covers(p) {
            return false;
        }
        self.held = match self.hits.at(p) {
            Some(HIT_SQUARE) => Some(Held::Square),
            Some(HIT_STRIP) => Some(Held::Strip),
            Some(k) if (HIT_SLIDER..HIT_SLIDER + 7).contains(&k) => Some(Held::Slider(Slider::ALL[(k - HIT_SLIDER) as usize])),
            Some(k) if (HIT_SWATCH..HIT_BUTTON).contains(&k) => Some(Held::Swatch((k - HIT_SWATCH) as usize)),
            Some(k) if (HIT_BUTTON..HIT_BUTTON + 2).contains(&k) => Some(Held::Button((k - HIT_BUTTON) as usize)),
            _ => None,
        };
        self.drag(p);
        true
    }

    /// The pointer at `p` while pressed: the square, strip or slider held is
    /// set there. False with nothing dragged.
    pub fn drag(&mut self, p: [f32; 2]) -> bool {
        let Some(h) = self.held else { return false };
        let (x, y) = (p[0] - AT[0] as f32, p[1] - AT[1] as f32);
        let frac = |v: f32, from: i32, len: i32| ((v - from as f32) / len as f32).clamp(0.0, 1.0);
        match h {
            Held::Square => {
                let s = frac(x, SQUARE[0], SQUARE[2]);
                let v = 1.0 - frac(y, SQUARE[1], SQUARE[2]);
                self.set_hsv(self.hue, s, v);
            }
            Held::Strip => self.set_hsv(frac(y, STRIP[1], SQUARE[2]) * 359.999, self.sat, self.val),
            Held::Slider(s) => {
                let f = frac(x, BAR[0], BAR[1]);
                let lo = if s == Slider::Alpha { 1 } else { 0 };
                self.set_value(s, (lo as f32 + f * (s.most() - lo) as f32).round() as i32);
                self.row = Row::Slider(s);
            }
            Held::Swatch(_) | Held::Button(_) => {}
        }
        true
    }

    /// The press let go at `p`: on the swatch or button it came down on, that
    /// one taken or pressed. What to tell the parent, if anything.
    pub fn release(&mut self, p: [f32; 2], cues: &mut Vec<i32>) -> Option<PickerUp> {
        let h = self.held.take()?;
        let at = self.hits.at(p);
        match h {
            Held::Swatch(i) if at == Some(HIT_SWATCH + i as i32) => {
                if let Some(&c) = self.swatches().get(i) {
                    self.set_rgba(c | if self.alpha_on { 0 } else { 0xFF });
                    self.row = Row::Swatches;
                    self.swatch = i;
                    cues.push(13);
                }
                None
            }
            Held::Button(b) if at == Some(HIT_BUTTON + b as i32) => {
                cues.push(if b == 0 { 13 } else { 8 });
                self.finish(b == 0)
            }
            _ => None,
        }
    }

    /// Whether a press is held (the window's drag).
    pub fn pressed(&self) -> bool {
        self.held.is_some()
    }

    /// The window drawn, each part's place noted.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster) {
        self.hits.clear();
        if !self.open {
            return;
        }
        let (white, yellow, grey) = ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 0.0, 1.0], [0.6, 0.6, 0.6, 1.0]);
        let at = AT;
        let fp = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 1), br: (106, 0), bottom: (102, 2), bl: (105, 0), left: (104, 0) };
        r.blit(icon_id("black"), 0, at, [0, 0], [W, H], 3, 0.85);
        vui.blit_frame(r, at, [W, H], &fp, 3, 1.0, true);
        self.hits.push([at[0], at[1], at[0] + W, at[1] + H], HIT_WINDOW);
        let title = if self.title.is_empty() { "Pick a colour".to_string() } else { self.title.clone() };
        vui.draw_string_ex(r, &wide(&title), at[0] + 12, at[1] + 8, white, true);

        // the square: saturation across, value up, at the hue
        let [sx, sy, sz] = [at[0] + SQUARE[0], at[1] + SQUARE[1], SQUARE[2]];
        let cell = sz / SQUARE_CELLS;
        for j in 0..SQUARE_CELLS {
            for i in 0..SQUARE_CELLS {
                let s = (i as f32 + 0.5) / SQUARE_CELLS as f32;
                let v = 1.0 - (j as f32 + 0.5) / SQUARE_CELLS as f32;
                let c = hsv_to_rgb(self.hue, s, v);
                fill(r, rgb_f(c), [sx + i * cell, sy + j * cell, cell, cell]);
            }
        }
        self.hits.push([sx, sy, sx + sz, sy + sz], HIT_SQUARE);
        // the pad's row on the square (saturation, value) framed with it
        if matches!(self.row, Row::Slider(Slider::Sat | Slider::Val)) {
            frame(r, yellow, [sx - 2, sy - 2, sz + 4, sz + 4]);
        }
        let (cx, cy) = (sx + (self.sat * sz as f32) as i32, sy + ((1.0 - self.val) * sz as f32) as i32);
        ring(r, cx, cy, if self.val > 0.5 && self.sat < 0.5 { [0.0, 0.0, 0.0, 1.0] } else { white });

        // the hue strip
        let [hx, hy, hw] = [at[0] + STRIP[0], at[1] + STRIP[1], STRIP[2]];
        let hcell = sz / SQUARE_CELLS;
        for j in 0..SQUARE_CELLS {
            let c = hsv_to_rgb((j as f32 + 0.5) / SQUARE_CELLS as f32 * 360.0, 1.0, 1.0);
            fill(r, rgb_f(c), [hx, hy + j * hcell, hw, hcell]);
        }
        self.hits.push([hx - 3, hy, hx + hw + 3, hy + sz], HIT_STRIP);
        if self.row == Row::Slider(Slider::Hue) {
            frame(r, yellow, [hx - 2, hy - 2, hw + 4, sz + 4]);
        }
        let my = hy + (self.hue / 360.0 * sz as f32) as i32;
        fill(r, [0.0, 0.0, 0.0, 1.0], [hx - 3, my - 2, hw + 6, 4]);
        fill(r, white, [hx - 2, my - 1, hw + 4, 2]);

        // the new colour beside the old, and the hex
        let new = self.rgba();
        swatch(r, [at[0] + SWATCH_NEW[0], at[1] + SWATCH_NEW[1], SWATCH_NEW[2], SWATCH_NEW[3]], new, true);
        swatch(r, [at[0] + SWATCH_OLD[0], at[1] + SWATCH_OLD[1], SWATCH_OLD[2], SWATCH_OLD[3]], if self.old == 0 { 0xFFFF_FFFF } else { self.old }, false);
        let hex = if self.typed.is_empty() { format!("#{}", to_hex(new, self.alpha_on)) } else { format!("#{}_", self.typed) };
        vui.draw_string_ex(r, &wide(&hex), at[0] + HEX_X, at[1] + SWATCH_NEW[1] + 6, if self.typed.is_empty() { white } else { yellow }, true);

        // the sliders
        for (k, s) in self.sliders().into_iter().enumerate() {
            let y = at[1] + ROWS_Y + k as i32 * ROW_H;
            let lit = self.row == Row::Slider(s);
            let x = at[0] + COLUMN;
            vui.draw_string_ex(r, &wide(s.label()), x, y, if lit { yellow } else { white }, true);
            if lit {
                vui.blit_ex(r, 89, [x - 22, y + 2], 3, 1.0, 0);
            }
            let (bx, by) = (at[0] + BAR[0], y + 5);
            if lit {
                frame(r, yellow, [bx - 3, by - 4, BAR[1] + 6, 16]);
            }
            for i in 0..BAR_CELLS {
                let f = (i as f32 + 0.5) / BAR_CELLS as f32;
                fill(r, self.slider_colour(s, f), [bx + i * BAR[1] / BAR_CELLS, by, BAR[1] / BAR_CELLS + 1, 8]);
            }
            let lo = if s == Slider::Alpha { 1 } else { 0 };
            let f = (self.value(s) - lo) as f32 / (s.most() - lo) as f32;
            let mx = bx + (f.clamp(0.0, 1.0) * BAR[1] as f32) as i32;
            fill(r, [0.0, 0.0, 0.0, 1.0], [mx - 2, by - 3, 5, 14]);
            fill(r, white, [mx - 1, by - 2, 3, 12]);
            self.hits.push([bx - 4, y, bx + BAR[1] + 4, y + ROW_H], HIT_SLIDER + Slider::ALL.iter().position(|&a| a == s).unwrap_or(0) as i32);
            let num = wide(&format!("{}", self.value(s)));
            let nw = vui.string_width(r, &num);
            vui.draw_string_ex(r, &num, at[0] + W - 12 - nw, y, if lit { yellow } else { white }, true);
        }

        // the swatches: the recent colours on their row
        let sw_lit = self.row == Row::Swatches;
        let y = at[1] + SWATCHES_Y;
        if !self.recent.is_empty() {
            vui.draw_string_ex(r, &wide("Recent"), at[0] + 12, y - 2, if sw_lit { yellow } else { grey }, true);
        }
        for (k, c) in self.recent.clone().into_iter().enumerate() {
            let x = at[0] + 72 + k as i32 * (CELL + 2);
            self.swatch_cell(r, [x, y], c, k, sw_lit);
        }

        // the pad's legend, in its glyphs
        let legend = wide(legend(self.row, self.button));
        let mut win = vi::ui::Window::default();
        let lw = win.text_width(vui, r, &legend);
        win.init(vui, r, at, W, 1, 0);
        win.open(true);
        win.draw_text_internal(vui, r, &legend, at[0] + (W - lw) / 2, at[1] + LEGEND_Y, grey, 1);

        // OK and Cancel
        for (b, label) in ["OK", "Cancel"].iter().enumerate() {
            let (x, y) = (at[0] + BUTTON_X[b], at[1] + BUTTONS_Y);
            let s = wide(label);
            let lit = self.row == Row::Buttons && self.button == b;
            let half = vui.string_width(r, &s) / 2;
            let pressed = self.held == Some(Held::Button(b));
            fill(r, if pressed { [0.35, 0.35, 0.2, 1.0] } else { [0.18, 0.18, 0.18, 1.0] }, [x - 44, y - 10, 88, 20]);
            vui.draw_centered_string_ex(r, &s, x, y, if lit { yellow } else { white }, 3, true);
            if lit {
                vui.blit_ex(r, 89, [x - half - 20, y - 5], 3, 1.0, 0);
            }
            self.hits.push([x - 44, y - 10, x + 44, y + 10], HIT_BUTTON + b as i32);
        }
    }

    fn swatch_cell(&mut self, r: &mut dyn Raster, p: [i32; 2], c: u32, k: usize, lit_row: bool) {
        let lit = lit_row && self.swatch == k;
        let edge = if lit { [1.0, 1.0, 0.0, 1.0] } else { [0.25, 0.25, 0.25, 1.0] };
        fill(r, edge, [p[0] - 1, p[1] - 1, CELL + 2, CELL + 2]);
        fill(r, rgba_f(c | 0xFF), [p[0], p[1], CELL, CELL]);
        self.hits.push([p[0] - 1, p[1] - 1, p[0] + CELL + 1, p[1] + CELL + 1], HIT_SWATCH + k as i32);
    }

    /// The colour a slider's bar shows at fraction `f` of it: the colour now
    /// with that row's value there.
    fn slider_colour(&self, s: Slider, f: f32) -> [f32; 4] {
        let (h, sa, v) = (self.hue, self.sat, self.val);
        let c = |x: f32| (x * 255.0).round() as u8;
        let rgb = match s {
            Slider::Hue => hsv_to_rgb(f * 360.0, 1.0, 1.0),
            Slider::Sat => hsv_to_rgb(h, f, v),
            Slider::Val => hsv_to_rgb(h, sa, f),
            Slider::Red => [c(f), self.rgb[1], self.rgb[2]],
            Slider::Green => [self.rgb[0], c(f), self.rgb[2]],
            Slider::Blue => [self.rgb[0], self.rgb[1], c(f)],
            Slider::Alpha => {
                let g = 0.2 + 0.8 * f;
                return [self.rgb[0] as f32 / 255.0 * g, self.rgb[1] as f32 / 255.0 * g, self.rgb[2] as f32 / 255.0 * g, 1.0];
            }
        };
        rgb_f(rgb)
    }
}

fn rgb_f(c: [u8; 3]) -> [f32; 4] {
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0]
}

/// A rectangle (x, y, w, h) in one colour.
fn fill(r: &mut dyn Raster, c: [f32; 4], [x, y, w, h]: [i32; 4]) {
    r.fill_fan(c, [[x, y], [x + w, y], [x + w, y + h], [x, y + h]]);
}

/// A swatch with an edge (the new colour's lighter).
/// A rectangle's edge, two pixels wide (the pad's focus).
fn frame(r: &mut dyn Raster, c: [f32; 4], [x, y, w, h]: [i32; 4]) {
    fill(r, c, [x, y, w, 2]);
    fill(r, c, [x, y + h - 2, w, 2]);
    fill(r, c, [x, y, 2, h]);
    fill(r, c, [x + w - 2, y, 2, h]);
}

/// The legend along the bottom for the pad's row (and button): what ✕ does
/// there, the steps, the cancel.
pub fn legend(row: Row, button: usize) -> &'static str {
    match row {
        Row::Slider(_) => "|7|8 By 10   |x OK   |t Cancel",
        Row::Swatches => "|x Take   |t Cancel",
        Row::Buttons if button == 1 => "|x Cancel",
        Row::Buttons => "|x OK   |t Cancel",
    }
}

fn swatch(r: &mut dyn Raster, [x, y, w, h]: [i32; 4], c: u32, bright: bool) {
    fill(r, if bright { [0.9, 0.9, 0.9, 1.0] } else { [0.4, 0.4, 0.4, 1.0] }, [x - 1, y - 1, w + 2, h + 2]);
    fill(r, rgba_f(c | 0xFF), [x, y, w, h]);
}

/// The square's cursor: a ring of four bars round (x, y).
fn ring(r: &mut dyn Raster, x: i32, y: i32, c: [f32; 4]) {
    for b in [[x - 5, y - 5, 10, 2], [x - 5, y + 3, 10, 2], [x - 5, y - 5, 2, 10], [x + 3, y - 5, 2, 10]] {
        fill(r, c, b);
    }
}

/// The recent colours kept (the file's words, hex), newest first.
pub fn read_recent(text: &str) -> Vec<u32> {
    text.split_whitespace().filter_map(|w| u32::from_str_radix(w, 16).ok()).take(RECENT).collect()
}

/// `c` put first among the recent, once.
pub fn remember(recent: &mut Vec<u32>, c: u32) {
    recent.retain(|&x| x != c);
    recent.insert(0, c);
    recent.truncate(RECENT);
}

/// The recent colours as the file keeps them.
pub fn write_recent(recent: &[u32]) -> String {
    recent.iter().map(|c| format!("{c:08X}")).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, vui};

    /// HSV and RGB are each other's: the primaries and greys exactly, every
    /// 24-bit colour on a lattice back to itself.
    #[test]
    fn hsv_and_rgb_round_trip() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), [255, 0, 0]);
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), [0, 255, 0]);
        assert_eq!(hsv_to_rgb(240.0, 1.0, 1.0), [0, 0, 255]);
        assert_eq!(hsv_to_rgb(60.0, 1.0, 1.0), [255, 255, 0]);
        assert_eq!(hsv_to_rgb(300.0, 0.0, 0.5), [128, 128, 128]);
        assert_eq!(hsv_to_rgb(360.0, 1.0, 1.0), [255, 0, 0], "360 is 0");
        assert_eq!(rgb_to_hsv([255, 0, 0]), (0.0, 1.0, 1.0));
        assert_eq!(rgb_to_hsv([0, 0, 0]), (0.0, 0.0, 0.0));
        let (h, s, v) = rgb_to_hsv([0x3F, 0xA7, 0xD6]);
        assert!((h - 198.68).abs() < 0.01 && (s - 0.706).abs() < 0.001 && (v - 0.839).abs() < 0.001, "{h} {s} {v}");
        for r in (0..=255).step_by(15) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(51) {
                    let (h, s, v) = rgb_to_hsv([r as u8, g as u8, b as u8]);
                    assert_eq!(hsv_to_rgb(h, s, v), [r as u8, g as u8, b as u8]);
                }
            }
        }
    }

    /// Hex: six digits (a '#' or not, either case), eight with the alpha,
    /// three doubled; anything else none.
    #[test]
    fn hex_reads_and_writes() {
        assert_eq!(parse_hex("#3fa7d6"), Some(0x3FA7_D6FF));
        assert_eq!(parse_hex("3FA7D680"), Some(0x3FA7_D680));
        assert_eq!(parse_hex("#abc"), Some(0xAABB_CCFF));
        assert_eq!(parse_hex("3FA7D"), None);
        assert_eq!(parse_hex("3FA7DG"), None);
        assert_eq!(to_hex(0x3FA7_D680, false), "3FA7D6");
        assert_eq!(to_hex(0x3FA7_D680, true), "3FA7D680");
    }

    /// A slider sets its value and the others follow: the channels from the
    /// HSV and the HSV from the channels; the alpha is FF unless the
    /// player's, and never 0; the hue is kept through a grey.
    #[test]
    fn the_sliders_move_the_colour() {
        let mut p = Picker::default();
        p.open("Dye chest", 0x3FA7_D6FF, false, vec![]);
        assert_eq!(p.rgba(), 0x3FA7_D6FF);
        assert_eq!((p.value(Slider::Hue), p.value(Slider::Sat), p.value(Slider::Val)), (199, 71, 84));
        p.set_value(Slider::Red, 255);
        assert_eq!(p.rgba(), 0xFFA7_D6FF);
        p.set_value(Slider::Sat, 0);
        assert_eq!(p.rgba() >> 8, 0xFFFFFF, "no saturation: a grey at the value");
        let hue = p.value(Slider::Hue);
        p.set_value(Slider::Sat, 100);
        assert_eq!(p.value(Slider::Hue), hue, "the hue kept through the grey");
        p.set_value(Slider::Alpha, 0x40);
        assert_eq!(p.rgba() & 0xFF, 0xFF, "the alpha is not the player's");
        let mut q = Picker::default();
        q.open("Dye your pet", 0x3FA7_D680, true, vec![]);
        assert_eq!(q.rgba(), 0x3FA7_D680);
        q.set_value(Slider::Alpha, 0);
        assert_eq!(q.rgba() & 0xFF, 1, "never 0");
        q.set_value(Slider::Hue, 400);
        assert_eq!(q.value(Slider::Hue), 40, "the hue wraps");
        let mut w = Picker::default();
        w.open("", 0, false, vec![]);
        assert_eq!(w.rgba(), 0xFFFF_FFFF, "none: white");
    }

    /// The pad: down the rows (held, repeating), a slider stepped and held,
    /// L1/R1 and L2/R2 by ten, the recent swatches taken (no palette's), ✕ on
    /// OK or a slider picks, △, ○ and Cancel's ✕ cancel; hex typed sets it.
    #[test]
    fn the_pad_and_the_keyboard() {
        let mut cues = Vec::new();
        let mut p = Picker::default();
        p.open("Dye chest", 0x8000_00FF, false, vec![0x3FA7_D6FF, 0xD100_00FF]);
        assert_eq!(p.row, Row::Slider(Slider::Hue));
        for _ in 0..3 {
            p.key(139, true, 0, &mut cues);
            p.key(139, false, 0, &mut cues);
        }
        assert_eq!(p.row, Row::Slider(Slider::Red));
        p.key(137, true, 0, &mut cues);
        assert_eq!(p.rgba(), 0x8100_00FF);
        p.tick(100);
        p.tick(200);
        assert_eq!(p.rgba(), 0x8100_00FF, "not yet repeating");
        p.tick(250);
        p.tick(320);
        p.tick(380);
        assert_eq!(p.rgba(), 0x8300_00FF, "held: repeating");
        p.key(137, false, 400, &mut cues);
        p.tick(500);
        assert_eq!(p.rgba(), 0x8300_00FF, "let go");
        p.key(140, true, 600, &mut cues);
        p.tick(1500);
        p.key(140, false, 1510, &mut cues);
        assert_eq!(p.rgba(), 0x7900_00FF, "L1: ten less; one slow frame before its release is no hold");
        p.key(143, true, 1600, &mut cues);
        p.key(143, false, 1610, &mut cues);
        assert_eq!(p.rgba(), 0x8300_00FF, "R2: ten more");
        // up held: a row, then a row every 150 ms after 300 ms and four frames
        p.key(138, true, 2000, &mut cues);
        assert_eq!(p.row, Row::Slider(Slider::Val));
        for t in [2100, 2200, 2250, 2320] {
            p.tick(t);
        }
        assert_eq!(p.row, Row::Slider(Slider::Sat), "held up: repeating");
        p.tick(2400);
        assert_eq!(p.row, Row::Slider(Slider::Sat), "not before 150 ms");
        p.tick(2480);
        assert_eq!(p.row, Row::Slider(Slider::Hue));
        p.key(138, false, 2500, &mut cues);
        for _ in 0..9 {
            p.key(139, true, 0, &mut cues);
        }
        assert_eq!(p.row, Row::Buttons, "the alpha's row is not there: the bottom");
        p.key(138, true, 0, &mut cues);
        assert_eq!(p.row, Row::Swatches, "the recent colours");
        p.key(134, true, 0, &mut cues);
        assert_eq!(p.rgba(), 0x3FA7_D6FF, "the first swatch");
        p.key(137, true, 0, &mut cues);
        p.key(137, true, 0, &mut cues);
        p.key(134, true, 0, &mut cues);
        assert_eq!(p.rgba(), 0xD100_00FF, "the second, the last: no further");
        for c in "#00ff7f".chars() {
            p.type_char(c);
        }
        assert_eq!(p.rgba(), 0x00FF_7FFF);
        p.type_char('1');
        p.type_char('\u{8}');
        assert_eq!(p.typed, "", "a digit typed and taken back");
        p.key(139, true, 0, &mut cues);
        assert_eq!(p.key(134, true, 0, &mut cues), Some(PickerUp::Pick(0x00FF_7FFF)), "✕ on OK");
        assert!(!p.open);
        p.open("x", 0x8000_00FF, false, vec![]);
        assert_eq!(p.key(134, true, 0, &mut cues), Some(PickerUp::Pick(0x8000_00FF)), "✕ on a slider: picked");
        p.open("x", 0, false, vec![]);
        assert_eq!(p.rows().last(), Some(&Row::Buttons));
        assert!(!p.rows().contains(&Row::Swatches), "no recent colours: no swatches' row");
        assert_eq!(p.key(132, true, 0, &mut cues), Some(PickerUp::Cancel), "△");
        p.open("x", 0, false, vec![]);
        assert_eq!(p.key(135, true, 0, &mut cues), Some(PickerUp::Cancel), "○");
        p.open("x", 0, false, vec![]);
        for _ in 0..7 {
            p.key(139, true, 0, &mut cues);
        }
        p.key(137, true, 0, &mut cues);
        assert_eq!(p.key(134, true, 0, &mut cues), Some(PickerUp::Cancel), "✕ on Cancel");
        p.open("x", 0, false, vec![]);
        assert_eq!(p.key(144, true, 0, &mut cues), Some(PickerUp::Pick(0xFFFF_FFFF)), "Start: OK");
        assert_eq!(p.key(134, true, 0, &mut cues), None, "closed: nothing");
        // the legend: what ✕ does where the pad is
        assert_eq!(legend(Row::Slider(Slider::Red), 0), "|7|8 By 10   |x OK   |t Cancel");
        assert_eq!(legend(Row::Swatches, 0), "|x Take   |t Cancel");
        assert_eq!(legend(Row::Buttons, 1), "|x Cancel");
    }

    /// The left stick as the d-pad: pushed past half up or down a row, left
    /// or right the value, held repeating (all the way, by five), let back
    /// let go; the control: within half, nothing.
    #[test]
    fn the_left_stick_as_the_d_pad() {
        let mut cues = Vec::new();
        let mut p = Picker::default();
        p.open("x", 0x8000_00FF, false, vec![]);
        p.stick(0.3, -0.4, 0, &mut cues);
        assert_eq!((p.row, p.rgba()), (Row::Slider(Slider::Hue), 0x8000_00FF), "within half: nothing");
        for t in [0, 16, 33] {
            p.stick(0.0, 0.8, t, &mut cues);
        }
        assert_eq!(p.row, Row::Slider(Slider::Sat), "down: one row while held a moment");
        p.stick(0.0, 0.0, 50, &mut cues);
        p.stick(0.0, 0.8, 60, &mut cues);
        p.stick(0.0, 0.0, 70, &mut cues);
        p.stick(0.0, 0.8, 80, &mut cues);
        p.stick(0.0, 0.0, 90, &mut cues);
        assert_eq!(p.row, Row::Slider(Slider::Red), "pushed again: a row each");
        p.stick(0.9, 0.1, 100, &mut cues);
        assert_eq!(p.rgba(), 0x8100_00FF, "right: one up");
        p.stick(-1.0, 0.0, 200, &mut cues);
        assert_eq!(p.rgba(), 0x8000_00FF, "left: one down");
        for t in [250, 320, 400, 460, 520] {
            p.stick(-1.0, 0.0, t, &mut cues);
            p.tick(t);
        }
        assert_eq!(p.rgba(), 0x7B00_00FF, "all the way left, held: by five");
        p.stick(0.0, 0.0, 540, &mut cues);
        p.tick(600);
        p.tick(700);
        assert_eq!(p.rgba(), 0x7B00_00FF, "let back: let go");
    }

    #[test]
    fn a_pointer_drags_and_clicks_where_it_was_drawn() {
        let (vui, mut nr) = (vui(), NullRaster);
        let mut cues = Vec::new();
        let mut p = Picker::default();
        p.open("Dye chest", 0xFF00_00FF, false, vec![0x1010_10FF]);
        let sq = |fx: f32, fy: f32| [(AT[0] + SQUARE[0]) as f32 + fx * SQUARE[2] as f32, (AT[1] + SQUARE[1]) as f32 + fy * SQUARE[2] as f32];
        assert!(p.press(sq(0.5, 0.5)), "on the window");
        assert_eq!(p.rgba(), 0xFF00_00FF, "not drawn: nothing set");
        p.release(sq(0.5, 0.5), &mut cues);
        p.draw(&vui, &mut nr);
        assert!(!p.press([10.0, 10.0]), "off the window");
        assert!(p.press(sq(0.999, 0.0)));
        assert_eq!(p.rgba(), 0xFF00_00FF, "full saturation and value: red");
        p.drag(sq(0.0, 0.0));
        assert_eq!(p.rgba(), 0xFFFF_FFFF, "dragged to the left: white");
        p.drag(sq(1.0, 1.0));
        assert_eq!(p.rgba() >> 8, 0, "to the foot: black");
        p.drag(sq(1.0, 0.5));
        assert_eq!(p.release(sq(1.0, 0.5), &mut cues), None);
        assert_eq!(p.rgba(), 0x8000_00FF, "half the value");
        let strip = [(AT[0] + STRIP[0] + 5) as f32, (AT[1] + STRIP[1]) as f32 + SQUARE[2] as f32 / 3.0];
        p.press(strip);
        p.release(strip, &mut cues);
        assert_eq!(p.value(Slider::Hue), 120, "a third down the strip: green");
        // the blue slider, its row the fifth after the hue
        let bar_y = (AT[1] + ROWS_Y + 5 * ROW_H + 8) as f32;
        let bar = |f: f32| [(AT[0] + BAR[0]) as f32 + f * BAR[1] as f32, bar_y];
        p.press(bar(1.0));
        p.drag(bar(0.5));
        p.release(bar(0.5), &mut cues);
        assert_eq!(p.value(Slider::Blue), 128);
        assert_eq!(p.row, Row::Slider(Slider::Blue));
        p.draw(&vui, &mut nr);
        // the recent swatch: let go elsewhere, nothing; on it, taken
        let cell = [(AT[0] + 72) as f32 + 3.0, (AT[1] + SWATCHES_Y) as f32 + 3.0];
        p.press(cell);
        p.release([cell[0] + 100.0, cell[1]], &mut cues);
        assert_ne!(p.rgba(), 0x1010_10FF, "let go off it");
        p.press(cell);
        p.release(cell, &mut cues);
        assert_eq!(p.rgba(), 0x1010_10FF);
        let ok = [(AT[0] + BUTTON_X[0]) as f32, (AT[1] + BUTTONS_Y) as f32];
        p.draw(&vui, &mut nr);
        p.press(ok);
        assert_eq!(p.release(ok, &mut cues), Some(PickerUp::Pick(0x1010_10FF)));
        assert!(!p.open);
    }

    /// The recent colours: newest first, once each, ten at most, and the
    /// file's words back.
    #[test]
    fn the_recent_colours_kept() {
        let mut r = Vec::new();
        for c in 0..12u32 {
            remember(&mut r, c << 8 | 0xFF);
        }
        remember(&mut r, 5 << 8 | 0xFF);
        assert_eq!(r.len(), RECENT);
        assert_eq!(r[0], 0x5FF);
        assert_eq!(r.iter().filter(|&&c| c == 0x5FF).count(), 1);
        assert_eq!(read_recent(&write_recent(&r)), r);
        assert_eq!(read_recent("zz 3FA7D6FF"), vec![0x3FA7_D6FF]);
    }
}
