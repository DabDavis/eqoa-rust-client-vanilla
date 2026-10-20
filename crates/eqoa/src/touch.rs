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
//! - a finger dragged anywhere else looks round as the mouse does
//!   (`mouse::look`), two of them pinch the orbit's distance;
//! - a tap there targets the creature under it, a double tap talks to it
//!   (`mouse::Click`).
//!
//! Shown from the first touch (in a page on a touch-first device from the
//! start); a key or a controller hides it.

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
    /// the disc and the ring, once on the layer
    tex: Option<(usize, usize)>,
}

impl TouchPad {
    /// A pad, shown from the start or from the first touch.
    pub fn new(shown: bool) -> Self {
        TouchPad { shown, used: shown, ..Default::default() }
    }

    /// A finger's event in a `size` window, at window pixels `at`.
    pub fn touch(&mut self, id: u64, phase: Phase, at: [f32; 2], now: Instant, size: (u32, u32)) {
        self.shown = true;
        self.used = true;
        self.size = size;
        let l = layout(size);
        match phase {
            Phase::Started => {
                let role = if let Some(b) = l.button_at(at) {
                    Role::Button(b.bit)
                } else if l.in_stick_zone(at) && !self.fingers.iter().any(|f| matches!(f.role, Role::Stick { .. })) {
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
    /// here (they are no game data) and put on the layer once.
    pub fn textures(&mut self, ui: &mut Ui, device: &render::wgpu::Device, queue: &render::wgpu::Queue) {
        if self.tex.is_some() {
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
        self.tex = Some((d, r));
    }

    /// The overlay's quads over a `size` window. `glyph` answers an atlas
    /// entry on the layer, once SUPPORT's atlas is there.
    /// With `dim`, a window is up: the pad steps back from it (the face
    /// buttons and the d-pad, which drive every window, a little; the rest
    /// more).
    pub fn draw(&self, ui: &mut Ui, size: (u32, u32), dim: bool, glyph: &dyn Fn(i32) -> Option<Glyph>) {
        let (Some((disc, ring)), true) = (self.tex, self.shown) else { return };
        let l = layout(size);
        let held = self.report().buttons;
        let menu_keys = bit::CROSS | bit::CIRCLE | bit::SQUARE | bit::TRIANGLE | bit::UP | bit::DOWN | bit::LEFT | bit::RIGHT;
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
            let fade = match (dim, b.bit & menu_keys != 0) {
                (false, _) => 1.0,
                (true, true) => 0.7,
                (true, false) => 0.3,
            };
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
            // where it floats from: a faint ring only
            None => ui.push_whole(quad(ring, l.stick_home, [l.stick_r, l.stick_r], full, [1.0, 1.0, 1.0, 0.18])),
        }
    }
}

/// `EQOA_TOUCHES`, for headless runs: `;`-separated finger events at a time
/// in ms, `down <id> <x> <y>@<ms>`, `move <id> <x> <y>@<ms>`, `up <id> <x>
/// <y>@<ms>` (window pixels), played through a `TouchPad` as the window's
/// would be.
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

    const SIZE: (u32, u32) = (2400, 1080);

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
}
