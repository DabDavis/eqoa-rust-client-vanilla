//! The touch screen: an on-screen DualShock 2 laid over the game, one more
//! source on the pad's port (`input::Report`, OR'd with the controllers), so
//! everything a pad does in the game it does the same way. Its buttons are
//! the game's own glyphs (SUPPORT's atlas: the face buttons, the shoulders,
//! Start and Select, L3 and R3, the d-pad's arrows) over discs of its own.
//!
//! - the left stick floats: a finger put down in the lower left is its
//!   centre, and its push is the finger's way from there;
//! - the buttons are held while touched: the face buttons at the lower
//!   right, the d-pad above the stick (the quick menus, every window),
//!   L2 L1 L3 Select Start R3 R1 R2 along the top;
//! - a finger dragged anywhere else looks round (`mouse::look`: the
//!   orbit, in first person the right stick's push), two of them pinch the
//!   orbit's distance;
//! - a tap there targets the creature under it, a double tap talks to it
//!   (`mouse::Click`).
//!
//! While a window is up the pad keeps only its face buttons (△ backs out of
//! every window); the d-pad, the top row and the stick step aside, so a
//! finger reaches the window, whose rows, tabs and legends take a tap
//! (`Play::point`), and a drag scrolls it.
//!
//! Shown from the first touch (in a page on a touch-first device from the
//! start); a key or a controller hides it.
//!
//! Before the world there is no pad on the screen: the game's screens take
//! a finger as the mouse's pointer (`pointer`), and so do the client's own
//! dialogs over the game (the report's, the update's question). A finger
//! there is a [`Taps`]': lifted where it came down, it is a click there.

use crate::input::{Report, bit};
use crate::mouse::Click;
use render::ui::{Quad, Ui};
use web_time::{Duration, Instant};

/// Two taps this close in time are a double tap.
const DOUBLE: Duration = Duration::from_millis(400);

/// The atlas entries the buttons wear.
mod glyph {
    pub const TRIANGLE: i32 = 73;
    pub const CROSS: i32 = 74;
    pub const SQUARE: i32 = 75;
    pub const CIRCLE: i32 = 76;
    pub const RIGHT: i32 = 85;
    pub const LEFT: i32 = 86;
    pub const DOWN: i32 = 87;
    pub const UP: i32 = 90;
    pub const L1: i32 = 197;
    pub const L2: i32 = 198;
    pub const R1: i32 = 199;
    pub const R2: i32 = 200;
    pub const SELECT: i32 = 201;
    pub const START: i32 = 202;
    pub const L3: i32 = 218;
    pub const R3: i32 = 219;
}

/// An atlas entry on the layer: its texture, its texture rectangle (u0, v0,
/// u1, v1) and its size in texels.
pub type Glyph = (usize, [f32; 4], [f32; 2]);

/// A finger's phase, as winit reports it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

/// What a finger is doing.
#[derive(Clone, Copy, Debug)]
enum Role {
    Stick { centre: [f32; 2] },
    Button(u16),
    Look { start: [f32; 2], moved: bool },
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    id: u64,
    role: Role,
    at: [f32; 2],
}

/// A button on the screen: its pad bit, its centre and radius (window
/// pixels), its glyph.
#[derive(Clone, Copy, Debug)]
pub struct Button {
    pub bit: u16,
    pub at: [f32; 2],
    pub r: f32,
    pub glyph: i32,
}

/// Where everything goes in a window of a size.
#[derive(Clone, Debug)]
pub struct Layout {
    /// the unit: a sixth and a half of the window's shorter side
    pub u: f32,
    pub buttons: Vec<Button>,
    /// where the stick is drawn while no finger holds it, its reach
    pub stick_home: [f32; 2],
    pub stick_r: f32,
    /// a finger put down in here (x0, y0, x1, y1) is the stick
    pub stick_zone: [f32; 4],
}

pub fn layout(size: (u32, u32)) -> Layout {
    let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let u = w.min(h) / 6.5;
    let mut buttons = Vec::new();
    // the face buttons, lower right
    let f = [w - 1.6 * u, h - 1.6 * u];
    let (s, r) = (0.78 * u, 0.4 * u);
    for (b, d, g) in [
        (bit::TRIANGLE, [0.0, -s], glyph::TRIANGLE),
        (bit::CROSS, [0.0, s], glyph::CROSS),
        (bit::SQUARE, [-s, 0.0], glyph::SQUARE),
        (bit::CIRCLE, [s, 0.0], glyph::CIRCLE),
    ] {
        buttons.push(Button { bit: b, at: [f[0] + d[0], f[1] + d[1]], r, glyph: g });
    }
    // the d-pad, left, above the stick
    let d = [1.45 * u, h - 3.9 * u];
    let (ds, dr) = (0.66 * u, 0.34 * u);
    for (b, o, g) in [(bit::UP, [0.0, -ds], glyph::UP), (bit::DOWN, [0.0, ds], glyph::DOWN), (bit::LEFT, [-ds, 0.0], glyph::LEFT), (bit::RIGHT, [ds, 0.0], glyph::RIGHT)] {
        buttons.push(Button { bit: b, at: [d[0] + o[0], d[1] + o[1]], r: dr, glyph: g });
    }
    // along the top, in the middle: one row, or two in a narrow window
    let row = [(bit::L2, glyph::L2), (bit::L1, glyph::L1), (bit::L3, glyph::L3), (bit::SELECT, glyph::SELECT), (bit::START, glyph::START), (bit::R3, glyph::R3), (bit::R1, glyph::R1), (bit::R2, glyph::R2)];
    let rows: Vec<Vec<(u16, i32)>> = if w >= 9.5 * u {
        vec![row.to_vec()]
    } else {
        vec![vec![row[0], row[1], row[6], row[7]], vec![row[2], row[3], row[4], row[5]]]
    };
    for (j, row) in rows.iter().enumerate() {
        let step = (1.1 * u).min((w - 4.0 * u) / row.len() as f32);
        let x0 = w / 2.0 - step * (row.len() as f32 - 1.0) / 2.0;
        for (k, (b, g)) in row.iter().enumerate() {
            buttons.push(Button { bit: *b, at: [x0 + step * k as f32, (0.5 + 0.9 * j as f32) * u], r: (0.42 * step).min(0.4 * u), glyph: *g });
        }
    }
    Layout { u, buttons, stick_home: [2.7 * u, h - 1.6 * u], stick_r: 0.9 * u, stick_zone: [0.0, h - 2.85 * u, 0.42 * w, h] }
}

impl Layout {
    /// The button under a finger at `at`, a little generously.
    fn button_at(&self, at: [f32; 2]) -> Option<&Button> {
        self.buttons.iter().filter(|b| dist(b.at, at) <= b.r * 1.25).min_by(|a, b| dist(a.at, at).total_cmp(&dist(b.at, at)))
    }

    fn in_stick_zone(&self, at: [f32; 2]) -> bool {
        let z = self.stick_zone;
        at[0] >= z[0] && at[0] < z[2] && at[1] >= z[1] && at[1] < z[3]
    }
}

/// A looking finger's drag down `dy` window pixels in a `size` window, as
/// the wheel's lines (up positive) a window up takes it as: a fifteenth of
/// the window's height a line, so a list follows the finger.
pub fn drag_lines(dy: f32, size: (u32, u32)) -> f32 {
    dy / (size.1.max(1) as f32 / 15.0)
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

#[derive(Default)]
pub struct TouchPad {
    fingers: Vec<Finger>,
    /// drawn, and its fingers read
    pub shown: bool,
    /// shown at least once: from then on it stays a source on the port,
    /// hidden or not, as a pad left plugged in does (else a key that hides
    /// it unplugs it, and the game says the controller was disconnected)
    pub used: bool,
    /// the looking fingers' drag since the last frame, and the pinch (in
    /// the wheel's lines)
    look: [f32; 2],
    pinch: f32,
    last_tap: Option<([f32; 2], Instant)>,
    clicks: Vec<Click>,
    size: (u32, u32),
    /// the disc and the ring, once on the layer, and which layer that was
    /// (`Ui::serial`): a session's end takes the HUD's layer with it, and the
    /// discs kept from it named textures past the next one's end (five crash
    /// reports of 2026-10-23 from a touch screen: "the len is 475 but the
    /// index is 475")
    tex: Option<(u64, usize, usize)>,
    /// a window is up (the viewer sets it each frame): only the face
    /// buttons are the pad's
    pub window: bool,
    /// with a window up the d-pad stays the pad's too (the viewer sets it
    /// each frame): a quick-chat menu has the keys, whose choices are the
    /// d-pad's directions, tapped or held
    pub dpad: bool,
}

/// The face buttons, the only ones a window up leaves on the screen.
const FACE: u16 = bit::CROSS | bit::CIRCLE | bit::SQUARE | bit::TRIANGLE;

/// The d-pad's arms.
const DPAD: u16 = bit::UP | bit::DOWN | bit::LEFT | bit::RIGHT;

impl TouchPad {
    /// A pad, shown from the start or from the first touch.
    pub fn new(shown: bool) -> Self {
        TouchPad { shown, used: shown, ..Default::default() }
    }

    /// The buttons a window up leaves the pad: the face buttons, and the
    /// d-pad over the quick-chat menus.
    fn kept(&self) -> u16 {
        FACE | if self.dpad { DPAD } else { 0 }
    }

    /// A finger's event in a `size` window, at window pixels `at`.
    pub fn touch(&mut self, id: u64, phase: Phase, at: [f32; 2], now: Instant, size: (u32, u32)) {
        self.shown = true;
        self.used = true;
        self.size = size;
        let l = layout(size);
        match phase {
            Phase::Started => {
                let role = if let Some(b) = l.button_at(at).filter(|b| !self.window || b.bit & self.kept() != 0) {
                    Role::Button(b.bit)
                } else if !self.window && l.in_stick_zone(at) && !self.fingers.iter().any(|f| matches!(f.role, Role::Stick { .. })) {
                    Role::Stick { centre: at }
                } else {
                    Role::Look { start: at, moved: false }
                };
                self.fingers.retain(|f| f.id != id);
                self.fingers.push(Finger { id, role, at });
            }
            Phase::Moved => {
                let looking = self.fingers.iter().filter(|f| matches!(f.role, Role::Look { .. })).count();
                let other = self.fingers.iter().find(|f| f.id != id && matches!(f.role, Role::Look { .. })).map(|f| f.at);
                let Some(f) = self.fingers.iter_mut().find(|f| f.id == id) else { return };
                let was = f.at;
                f.at = at;
                if let Role::Look { start, moved } = &mut f.role {
                    if !*moved && dist(*start, at) > 0.12 * l.u {
                        *moved = true;
                    }
                    if *moved {
                        match (looking, other) {
                            // two fingers: apart zooms in, together out
                            (2.., Some(o)) => {
                                let (d0, d1) = (dist(was, o).max(1.0), dist(at, o).max(1.0));
                                self.pinch += (d0 / d1).ln() / 0.9f32.ln();
                            }
                            _ => {
                                self.look[0] += at[0] - was[0];
                                self.look[1] += at[1] - was[1];
                            }
                        }
                    }
                }
            }
            Phase::Ended | Phase::Cancelled => {
                let Some(k) = self.fingers.iter().position(|f| f.id == id) else { return };
                let f = self.fingers.remove(k);
                if let (Role::Look { start, moved: false }, Phase::Ended) = (f.role, phase) {
                    let double = self.last_tap.is_some_and(|(p, t)| now.duration_since(t) < DOUBLE && dist(p, start) < 0.5 * l.u);
                    let (x, y) = (start[0] as f64, start[1] as f64);
                    if double {
                        self.last_tap = None;
                        self.clicks.push(Click::Interact(x, y));
                    } else {
                        self.last_tap = Some((start, now));
                        self.clicks.push(Click::Target(x, y));
                    }
                }
            }
        }
    }

    /// The pad as the fingers hold it: the buttons touched, the stick's push.
    pub fn report(&self) -> Report {
        let mut r = Report::default();
        let reach = layout(self.size).stick_r;
        for f in &self.fingers {
            match f.role {
                Role::Button(b) => r.buttons |= b,
                Role::Stick { centre } => {
                    let (dx, dy) = ((f.at[0] - centre[0]) / reach, (f.at[1] - centre[1]) / reach);
                    let m = (dx * dx + dy * dy).sqrt().max(1.0);
                    r.sticks[2] = dx / m;
                    r.sticks[3] = dy / m;
                }
                Role::Look { .. } => {}
            }
        }
        r
    }

    /// The looking fingers' drag since the last frame (window pixels) and
    /// the pinch (the wheel's lines).
    pub fn take_look(&mut self) -> (f32, f32, f32) {
        let l = std::mem::take(&mut self.look);
        (l[0], l[1], std::mem::take(&mut self.pinch))
    }

    /// The taps since the last frame.
    pub fn take_clicks(&mut self) -> Vec<Click> {
        std::mem::take(&mut self.clicks)
    }

    /// A key or a controller in use: the overlay goes, every finger let go.
    pub fn hide(&mut self) {
        self.shown = false;
        self.fingers.clear();
    }

    /// The disc and the ring the buttons and the stick are drawn on, made
    /// here (they are no game data) and put on the layer once (on each new
    /// layer once: the next session's HUD draws on another).
    pub fn textures(&mut self, ui: &mut Ui, device: &render::wgpu::Device, queue: &render::wgpu::Queue) {
        if self.tex.is_some_and(|(layer, ..)| layer == ui.serial()) {
            return;
        }
        let n = 64u32;
        let mut disc = vec![0u8; (n * n * 4) as usize];
        let mut ring = disc.clone();
        for y in 0..n {
            for x in 0..n {
                let d = ((x as f32 + 0.5 - n as f32 / 2.0).powi(2) + (y as f32 + 0.5 - n as f32 / 2.0).powi(2)).sqrt() / (n as f32 / 2.0);
                // a soft edge a pixel and a half wide
                let edge = |r: f32| ((r - d) * n as f32 / 3.0).clamp(0.0, 1.0);
                let i = ((y * n + x) * 4) as usize;
                disc[i..i + 4].copy_from_slice(&[255, 255, 255, (edge(1.0) * 255.0) as u8]);
                let a = edge(1.0) * (1.0 - edge(0.86));
                ring[i..i + 4].copy_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
            }
        }
        let d = ui.add_texture(device, queue, n, n, &disc);
        let r = ui.add_texture(device, queue, n, n, &ring);
        self.tex = Some((ui.serial(), d, r));
    }

    /// The overlay's quads over a `size` window. `glyph` answers an atlas
    /// entry on the layer, once SUPPORT's atlas is there.
    /// With `dim`, a window is up: only the face buttons, a little faded
    /// (a button still held is drawn while it is).
    pub fn draw(&self, ui: &mut Ui, size: (u32, u32), dim: bool, glyph: &dyn Fn(i32) -> Option<Glyph>) {
        let (Some((layer, disc, ring)), true) = (self.tex, self.shown) else { return };
        if layer != ui.serial() {
            return;
        }
        let l = layout(size);
        let held = self.report().buttons;
        let quad = |tex: usize, c: [f32; 2], half: [f32; 2], uv: [f32; 4], color: [f32; 4]| {
            let p = |x: f32, y: f32| render::ui::frame_from_window(size, [x, y]);
            Quad {
                tex,
                blend: 3,
                pos: [p(c[0] - half[0], c[1] - half[1]), p(c[0] + half[0], c[1] - half[1]), p(c[0] + half[0], c[1] + half[1]), p(c[0] - half[0], c[1] + half[1])],
                uv: [[uv[0], uv[1]], [uv[2], uv[1]], [uv[2], uv[3]], [uv[0], uv[3]]],
                color,
                clip: None,
            }
        };
        let full = [0.0, 0.0, 1.0, 1.0];
        for b in &l.buttons {
            let on = held & b.bit != 0;
            if dim && b.bit & self.kept() == 0 && !on {
                continue;
            }
            let fade = if dim { 0.7 } else { 1.0 };
            ui.push_whole(quad(disc, b.at, [b.r, b.r], full, [0.05, 0.05, 0.08, if on { 0.55 } else { 0.3 * fade }]));
            if let Some((tex, uv, px)) = glyph(b.glyph) {
                // the glyph inside the disc, its shape kept
                let k = 1.25 * b.r / px[0].max(px[1]);
                ui.push_whole(quad(tex, b.at, [px[0] * k / 2.0, px[1] * k / 2.0], uv, [1.0, 1.0, 1.0, if on { 1.0 } else { 0.75 * fade }]));
            }
        }
        // the stick: where a finger holds it, else at home, faint
        let held_stick = self.fingers.iter().find_map(|f| match f.role {
            Role::Stick { centre } => Some((centre, f.at)),
            _ => None,
        });
        match held_stick {
            Some((c, at)) => {
                let (dx, dy) = (at[0] - c[0], at[1] - c[1]);
                let m = (dx * dx + dy * dy).sqrt().max(l.stick_r) / l.stick_r;
                ui.push_whole(quad(ring, c, [l.stick_r, l.stick_r], full, [1.0, 1.0, 1.0, 0.6]));
                ui.push_whole(quad(disc, [c[0] + dx / m, c[1] + dy / m], [0.42 * l.u, 0.42 * l.u], full, [0.85, 0.85, 0.9, 0.6]));
            }
            // where it floats from: a faint ring only (none over a window)
            None if !dim => ui.push_whole(quad(ring, l.stick_home, [l.stick_r, l.stick_r], full, [1.0, 1.0, 1.0, 0.18])),
            None => {}
        }
    }
}

/// The pad over a `size` window on `ui`, its buttons wearing SUPPORT's
/// glyphs once the game's UI (`parts`: its words and its atlas on this
/// layer) is in; `dim` with a window up (`TouchPad::draw`). The discs are
/// put on the layer first (`TouchPad::textures`).
pub fn draw_pad(ui: &mut Ui, device: &render::wgpu::Device, queue: &render::wgpu::Queue, touch: &mut TouchPad, size: (u32, u32), dim: bool, parts: Option<(&vi::ui::Vui, &crate::ui::UiAssets)>) {
    touch.textures(ui, device, queue);
    let mut glyphs: Vec<(i32, Glyph)> = Vec::new();
    if let Some((vui, assets)) = parts {
        for b in layout(size).buttons {
            let e = vui.entry(b.glyph);
            let Some(tex) = assets.surface(vui.entry_id(&e)) else { continue };
            let (w, h) = ui.texture_size(tex);
            let (w, h) = (w as f32, h as f32);
            let uv = [e.uv0[0] as f32 / w, e.uv0[1] as f32 / h, (e.uv0[0] + e.uv1[0]) as f32 / w, (e.uv0[1] + e.uv1[1]) as f32 / h];
            glyphs.push((b.glyph, (tex, uv, [e.uv1[0] as f32, e.uv1[1] as f32])));
        }
    }
    touch.draw(ui, size, dim, &|id| glyphs.iter().find(|g| g.0 == id).map(|g| g.1));
}

/// The fingers where a pointer is taken instead of the pad (the screens
/// before the world, the client's own dialogs): a finger's tap is a click
/// where it came down, at its lift (a mouse's is at its press, but a finger
/// that moves on is no click, and only the lift can tell). Each finger on
/// its own, as a looking finger on the pad is (`TouchPad::touch`): one that
/// moves farther than its slop (a drag, a pinch) or that the system cancels
/// clicks nothing.
#[derive(Clone, Debug, Default)]
pub struct Taps {
    /// the fingers down: id, where it came down, whether it moved on
    fingers: Vec<(u64, [f32; 2], bool)>,
}

/// Fingers kept down at most: one whose lift never came (the window lost
/// it) goes when more than this come down after it.
const TAP_FINGERS: usize = 10;

impl Taps {
    /// A finger's event in a `size` window at window pixels `at`: where it
    /// came down, when this is a tap's lift.
    pub fn touch(&mut self, id: u64, phase: Phase, at: [f32; 2], size: (u32, u32)) -> Option<[f32; 2]> {
        match phase {
            Phase::Started => {
                self.fingers.retain(|f| f.0 != id);
                if self.fingers.len() >= TAP_FINGERS {
                    self.fingers.remove(0);
                }
                self.fingers.push((id, at, false));
            }
            Phase::Moved => {
                if let Some(f) = self.fingers.iter_mut().find(|f| f.0 == id)
                    && dist(f.1, at) > tap_slop(size)
                {
                    f.2 = true;
                }
            }
            Phase::Ended | Phase::Cancelled => {
                let k = self.fingers.iter().position(|f| f.0 == id)?;
                let (_, start, moved) = self.fingers.remove(k);
                return (phase == Phase::Ended && !moved).then_some(start);
            }
        }
        None
    }

    /// Every finger down forgotten (the window lost the focus): their lifts
    /// click nothing.
    pub fn forget(&mut self) {
        self.fingers.clear();
    }
}

/// How far (window pixels) a finger may move and still tap: as far as a
/// looking finger on the pad may before it looks.
fn tap_slop(size: (u32, u32)) -> f32 {
    0.12 * layout(size).u
}

/// `EQOA_TOUCHES`, for headless runs: `;`-separated finger events at a time
/// in ms, `down <id> <x> <y>@<ms>`, `move <id> <x> <y>@<ms>`, `up <id> <x>
/// <y>@<ms>` (window pixels), played through a `TouchPad` as the window's
/// would be (in a playshot at the script's time; in a window at the
/// client's clock, the pregame's taps too).
pub fn script() -> Vec<(i64, u64, Phase, [f32; 2])> {
    let Ok(s) = std::env::var("EQOA_TOUCHES") else { return Vec::new() };
    s.split(';')
        .filter_map(|p| {
            let (what, ms) = p.trim().rsplit_once('@')?;
            let w: Vec<&str> = what.split_whitespace().collect();
            let phase = match *w.first()? {
                "down" => Phase::Started,
                "move" => Phase::Moved,
                "up" => Phase::Ended,
                _ => return None,
            };
            let n = |i: usize| w.get(i)?.parse::<f32>().ok();
            Some((ms.trim().parse::<i64>().ok()? / 16 * 16, w.get(1)?.parse().ok()?, phase, [n(2)?, n(3)?]))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With a window up only the face buttons are the pad's: a finger on
    /// the d-pad, the top row or the stick's corner is a tap for the window.
    #[test]
    fn a_window_takes_what_the_pad_steps_away_from() {
        let t = Instant::now();
        let l = layout(SIZE);
        let at = |bit: u16| l.buttons.iter().find(|b| b.bit == bit).unwrap().at;
        let mut p = TouchPad::new(true);
        p.window = true;
        for (k, a) in [at(bit::UP), at(bit::START), [l.stick_zone[0] + 50.0, l.stick_zone[3] - 50.0]].into_iter().enumerate() {
            p.touch(k as u64, Phase::Started, a, t, SIZE);
            assert_eq!(p.report(), Report::default(), "finger {k} holds nothing");
            p.touch(k as u64, Phase::Ended, a, t + Duration::from_secs(k as u64 + 1), SIZE);
        }
        assert_eq!(p.take_clicks().len(), 3, "three taps for the window");
        p.touch(9, Phase::Started, at(bit::TRIANGLE), t, SIZE);
        assert_eq!(p.report().buttons, bit::TRIANGLE, "△ is still the pad's");
        // negative control: no window, the d-pad is the pad's
        p.window = false;
        p.touch(10, Phase::Started, at(bit::UP), t, SIZE);
        assert_eq!(p.report().buttons, bit::TRIANGLE | bit::UP);
    }

    /// Over the quick-chat menus the d-pad stays the pad's (a direction is
    /// held there to edit its choice); the top row is still the window's.
    #[test]
    fn the_quick_chat_keeps_the_d_pad() {
        let t = Instant::now();
        let l = layout(SIZE);
        let at = |bit: u16| l.buttons.iter().find(|b| b.bit == bit).unwrap().at;
        let mut p = TouchPad::new(true);
        p.window = true;
        p.dpad = true;
        p.touch(1, Phase::Started, at(bit::LEFT), t, SIZE);
        assert_eq!(p.report().buttons, bit::LEFT);
        p.touch(2, Phase::Started, at(bit::START), t, SIZE);
        assert_eq!(p.report().buttons, bit::LEFT, "Start is the window's");
        p.touch(1, Phase::Ended, at(bit::LEFT), t, SIZE);
        // control: another window
        p.dpad = false;
        p.touch(3, Phase::Started, at(bit::DOWN), t, SIZE);
        assert_eq!(p.report().buttons, 0);
    }

    const SIZE: (u32, u32) = (2400, 1080);

    /// Before the world a finger lifted where it came down is a click there
    /// (where it came down, its wobble inside the slop); one that moved on
    /// (a drag, a pinch's two) or was cancelled is none, and each finger is
    /// its own (a thumb resting on the screen takes nothing from a tap
    /// beside it). The controls: the same finger moved just past the slop
    /// clicks nothing, and just inside it clicks.
    #[test]
    fn a_tap_is_a_click_where_the_finger_came_down() {
        let slop = tap_slop(SIZE);
        assert!(slop > 5.0 && slop < 40.0, "a fingertip's wobble on a 1080-high screen: {slop}");
        let at = [700.0, 400.0];
        let mut t = Taps::default();
        assert_eq!(t.touch(1, Phase::Started, at, SIZE), None, "nothing at the press");
        assert_eq!(t.touch(1, Phase::Moved, [at[0] + slop - 0.5, at[1]], SIZE), None);
        assert_eq!(t.touch(1, Phase::Ended, [at[0] + slop - 0.5, at[1]], SIZE), Some(at), "inside the slop: a tap where it came down");
        // the control: just past the slop, a drag
        t.touch(2, Phase::Started, at, SIZE);
        t.touch(2, Phase::Moved, [at[0], at[1] + slop + 0.5], SIZE);
        assert_eq!(t.touch(2, Phase::Ended, at, SIZE), None, "moved past the slop and back: no tap");
        // a thumb resting while another finger taps: the tap is a tap
        t.touch(3, Phase::Started, [20.0, 900.0], SIZE);
        t.touch(4, Phase::Started, at, SIZE);
        assert_eq!(t.touch(4, Phase::Ended, at, SIZE), Some(at), "the tap beside the resting thumb");
        // a pinch: both move, neither taps
        t.touch(5, Phase::Started, [600.0, 500.0], SIZE);
        t.touch(3, Phase::Moved, [20.0, 960.0], SIZE);
        t.touch(5, Phase::Moved, [540.0, 500.0], SIZE);
        assert_eq!(t.touch(5, Phase::Ended, [540.0, 500.0], SIZE), None);
        assert_eq!(t.touch(3, Phase::Ended, [20.0, 960.0], SIZE), None);
        // a cancelled finger, and forgotten ones
        t.touch(6, Phase::Started, at, SIZE);
        assert_eq!(t.touch(6, Phase::Cancelled, at, SIZE), None);
        assert_eq!(t.touch(6, Phase::Ended, at, SIZE), None, "its lift after the cancel");
        t.touch(7, Phase::Started, at, SIZE);
        t.forget();
        assert_eq!(t.touch(7, Phase::Ended, at, SIZE), None, "forgotten");
        // a lift with no press seen (the finger came down before the window had it)
        assert_eq!(t.touch(8, Phase::Ended, at, SIZE), None);
        // lifts lost: the oldest fingers go, a new one still taps
        for id in 100..100 + TAP_FINGERS as u64 + 3 {
            t.touch(id, Phase::Started, [1.0, 1.0], SIZE);
        }
        assert_eq!(t.fingers.len(), TAP_FINGERS);
        t.touch(9, Phase::Started, [10.0, 20.0], SIZE);
        assert_eq!(t.touch(9, Phase::Ended, [10.0, 20.0], SIZE), Some([10.0, 20.0]));
    }

    #[test]
    fn the_layout_keeps_its_buttons_apart_and_on_the_screen() {
        for size in [SIZE, (1080, 2400), (1024, 768), (1280, 720)] {
            let l = layout(size);
            assert_eq!(l.buttons.len(), 16);
            for (i, a) in l.buttons.iter().enumerate() {
                assert!(a.at[0] - a.r >= 0.0 && a.at[0] + a.r <= size.0 as f32 && a.at[1] - a.r >= 0.0 && a.at[1] + a.r <= size.1 as f32, "{size:?}: {a:?} off the screen");
                for b in &l.buttons[i + 1..] {
                    assert!(dist(a.at, b.at) >= a.r + b.r, "{size:?}: {a:?} overlaps {b:?}");
                }
            }
            // every pad button has one
            let all: u16 = l.buttons.iter().fold(0, |m, b| m | b.bit);
            assert_eq!(all, 0xFFFF, "{size:?}");
        }
    }

    #[test]
    fn a_button_is_held_while_touched_and_the_stick_follows_its_finger() {
        let t = Instant::now();
        let l = layout(SIZE);
        let mut p = TouchPad::default();
        let cross = l.buttons.iter().find(|b| b.bit == bit::CROSS).unwrap().at;
        p.touch(1, Phase::Started, cross, t, SIZE);
        assert_eq!(p.report().buttons, bit::CROSS);
        // the stick: put down in the lower left, pushed up and right a full reach
        let c = [l.stick_home[0] - 20.0, l.stick_home[1]];
        p.touch(2, Phase::Started, c, t, SIZE);
        p.touch(2, Phase::Moved, [c[0] + l.stick_r, c[1] - l.stick_r], t, SIZE);
        let r = p.report();
        assert_eq!(r.buttons, bit::CROSS);
        let d = std::f32::consts::FRAC_1_SQRT_2;
        assert!((r.sticks[2] - d).abs() < 1e-3 && (r.sticks[3] + d).abs() < 1e-3, "{:?}", r.sticks);
        p.touch(1, Phase::Ended, cross, t, SIZE);
        p.touch(2, Phase::Ended, c, t, SIZE);
        assert_eq!(p.report(), Report::default());
        assert!(p.take_clicks().is_empty(), "neither was a tap");
    }

    #[test]
    fn hidden_it_lets_go_of_every_finger_but_stays_used() {
        let t = Instant::now();
        let l = layout(SIZE);
        let mut p = TouchPad::new(false);
        assert!(!p.used, "never touched: no source");
        let cross = l.buttons.iter().find(|b| b.bit == bit::CROSS).unwrap().at;
        p.touch(1, Phase::Started, cross, t, SIZE);
        assert_eq!(p.report().buttons, bit::CROSS);
        p.hide();
        assert_eq!((p.shown, p.used, p.report()), (false, true, Report::default()));
        assert!(TouchPad::new(true).used, "shown from the start: a source at once");
    }

    #[test]
    fn elsewhere_a_drag_looks_two_pinch_and_a_tap_targets() {
        let t = Instant::now();
        let mut p = TouchPad::default();
        let at = [1300.0, 500.0];
        p.touch(1, Phase::Started, at, t, SIZE);
        p.touch(1, Phase::Moved, [1302.0, 501.0], t, SIZE);
        assert_eq!(p.take_look(), (0.0, 0.0, 0.0), "inside the slop");
        p.touch(1, Phase::Moved, [1352.0, 491.0], t, SIZE);
        assert_eq!(p.take_look(), (50.0, -10.0, 0.0));
        // a second finger: the two pinch
        p.touch(2, Phase::Started, [1552.0, 491.0], t, SIZE);
        p.touch(2, Phase::Moved, [1752.0, 491.0], t, SIZE);
        let (dx, dy, pinch) = p.take_look();
        assert_eq!((dx, dy), (0.0, 0.0));
        assert!(pinch > 0.0, "apart zooms in, as the wheel up does ({pinch})");
        p.touch(1, Phase::Ended, at, t, SIZE);
        p.touch(2, Phase::Ended, at, t, SIZE);
        assert!(p.take_clicks().is_empty());
        // a tap, and another soon after: a target, then a talk
        p.touch(3, Phase::Started, at, t, SIZE);
        p.touch(3, Phase::Ended, at, t, SIZE);
        p.touch(4, Phase::Started, at, t + Duration::from_millis(200), SIZE);
        p.touch(4, Phase::Ended, at, t + Duration::from_millis(250), SIZE);
        assert_eq!(p.take_clicks(), vec![Click::Target(1300.0, 500.0), Click::Interact(1300.0, 500.0)]);
        // a cancelled touch is no tap
        p.touch(5, Phase::Started, at, t + Duration::from_secs(2), SIZE);
        p.touch(5, Phase::Cancelled, at, t + Duration::from_secs(2), SIZE);
        assert!(p.take_clicks().is_empty());
    }

    /// The pad's discs are made on the layer they are drawn on. A session's
    /// end takes the HUD's layer with it, and the next session's has as many
    /// textures before the pad's, so the discs kept from the first named
    /// textures past its end: five crash reports of 2026-10-23 from a touch
    /// screen ("index out of bounds: the len is 475 but the index is 475").
    /// The control: drawn on the new layer before the pad has made them there,
    /// nothing is drawn, where the kept discs were. Needs a graphics adapter;
    /// without one it says so and passes.
    #[test]
    fn the_discs_are_made_again_on_the_next_sessions_layer() {
        let _gpu = crate::GPU_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let instance = render::wgpu::Instance::new(render::wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&render::wgpu::RequestAdapterOptions::default())) else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        // a session's layer: the HUD's textures first (one here), then the pad's
        let layer = || {
            let mut ui = Ui::new(&device, render::TARGET_FORMAT, render::DEPTH_FORMAT);
            ui.add_texture(&device, &queue, 1, 1, &[255; 4]);
            ui
        };
        let mut p = TouchPad::new(true);
        let mut first = layer();
        p.textures(&mut first, &device, &queue);
        let (_, disc, ring) = p.tex.unwrap();
        let mut next = layer();
        assert_ne!(first.serial(), next.serial());
        assert!(disc.max(ring) >= next.texture_count(), "the first layer's discs are past the next one's end");
        next.begin(SIZE);
        p.draw(&mut next, SIZE, false, &|_| None);
        assert!(next.quads.is_empty(), "a disc made on another layer is not drawn");
        p.textures(&mut next, &device, &queue);
        next.begin(SIZE);
        p.draw(&mut next, SIZE, false, &|_| None);
        assert!(!next.quads.is_empty());
        for q in &next.quads {
            assert!(q.tex < next.texture_count() && next.texture_size(q.tex) == (64, 64), "texture {} of {}", q.tex, next.texture_count());
        }
        let n = next.texture_count();
        p.textures(&mut next, &device, &queue);
        assert_eq!(next.texture_count(), n, "once on a layer");
    }
}
