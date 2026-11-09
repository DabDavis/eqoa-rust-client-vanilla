//! The touch screen: an on-screen controller laid over the game, one more
//! source on the pad's port (`input::Report`, OR'd with the controllers), so
//! everything a pad does in the game it does the same way. Its buttons are
//! the game's own glyphs (SUPPORT's atlas: the face buttons, the shoulders,
//! Start and Select, L3 and R3, the d-pad's arrows) over discs of its own.
//!
//! It is laid out as a DualShock 4 is (the user, 2026-10-27: "in landscape
//! have ps4 controller layout, aka dual analog"): two sticks, the d-pad above
//! and outside the left one, the face buttons above and outside the right
//! one, L2 L1 over the d-pad and R1 R2 over the face buttons (under the HUD's
//! compass and bars), Select and Start at the top in the middle, L3 and R3
//! just over their sticks. In a window taller than wide the same controller
//! lies along the bottom (`layout`).
//!
//! - each stick floats: a finger put down near it is its centre, and its
//!   push is the finger's way from there (the left one walks, the right one
//!   turns the camera, as a controller's do);
//! - the buttons are held while touched;
//! - a finger dragged anywhere else looks round (`mouse::look`: the
//!   orbit, in first person the right stick's push), two of them pinch the
//!   orbit's distance;
//! - a tap there targets the creature under it, a double tap talks to it
//!   (`mouse::Click`).
//!
//! Over a window, and on the game's screens before the world, every button
//! is still the pad's (drawn fainter) and a stick takes only a finger put
//! down inside its ring, so a finger anywhere else reaches the window, whose
//! rows, tabs and legends take a tap (`Play::point`, `Host::click`), and a
//! drag scrolls it.
//!
//! Shown from the first touch (in a page on a touch-first device from the
//! start); a key or a controller hides it.
//!
//! Its size is the Extended page's Touch Pad Size (`settings`'
//! `touch_size`, the player's ask of 2026-11-02: "Is there a way to make the
//! on screen controller size adjustable?"): the whole pad, 50% to 100% of
//! the most that fits, which is what it was before (`layout`).
//!
//! The client's own dialog over the game (the update's question) takes a
//! finger as the mouse's pointer: a finger there is a
//! [`Taps`]', and lifted where it came down it is a click there.

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
    /// a stick (`Layout::sticks`' index) and where the finger came down,
    /// its centre
    Stick { which: usize, centre: [f32; 2] },
    Button(u16),
    Look { start: [f32; 2], moved: bool },
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    id: u64,
    role: Role,
    at: [f32; 2],
    /// when it came down, and whether it was held still long enough to be a
    /// long press (`take_holds`), whose lift is then no tap
    since: Instant,
    held: bool,
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

/// A stick on the screen (window pixels).
#[derive(Clone, Copy, Debug)]
pub struct Stick {
    /// where it is drawn while no finger holds it
    pub home: [f32; 2],
    /// its ring's radius, and its reach: a finger this far from where it
    /// came down pushes the stick all the way
    pub r: f32,
    /// in play a finger put down in here (x0, y0, x1, y1) is the stick's
    /// too, as well as one inside its ring
    pub zone: [f32; 4],
}

impl Stick {
    /// Whether a finger put down at `at` is this stick's: one inside its
    /// ring, and in play (no `window`) one anywhere in its zone.
    fn takes(&self, at: [f32; 2], window: bool) -> bool {
        let z = self.zone;
        dist(self.home, at) <= self.r || (!window && at[0] >= z[0] && at[0] < z[2] && at[1] >= z[1] && at[1] < z[3])
    }
}

/// Where everything goes in a window of a size.
#[derive(Clone, Debug)]
pub struct Layout {
    /// the unit the buttons are sized and spaced in (the pad's size of the
    /// full pad's: `full`)
    pub u: f32,
    /// the full pad's unit, which a finger's own reaches are measured in (a
    /// tap's slop, a double tap's spread): a finger is no smaller on a
    /// smaller pad
    pub full: f32,
    pub buttons: Vec<Button>,
    /// the left stick (the walk: `Report::sticks` 2 and 3), then the right
    /// one (the camera: 0 and 1)
    pub sticks: [Stick; 2],
}

/// The smallest Touch Pad Size, of the full pad (`layout`).
pub const SMALLEST: f32 = 0.5;

/// The full pad's unit in a `size` window (`layout`): wider than tall, off
/// the window's height (a sixth and a half), less where the window is too
/// narrow for the two sticks to keep apart; taller than wide, off its width,
/// less where it is too short.
fn unit(size: (u32, u32)) -> f32 {
    let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
    if w >= h { (h / 6.5).min(w / 9.6) } else { (w / 6.6).min(h / 10.5) }
}

/// The controller in a `size` window, at `scale` of its full size (the
/// options' Touch Pad Size: `SMALLEST` to 1; more is the full pad,
/// which is the most that fits: grown by 8%, a shoulder meets
/// the face buttons' top on a phone held wide, and Select or ✕ meets a
/// stick on one held upright).
///
/// The full pad, wider than tall, as a DualShock 4 is held: the unit off
/// the window's height, as the HUD's is (so the shoulders sit under its
/// compass and bars, which reach 1.2 of the height's sixth and a half),
/// less where the window is too narrow for the two sticks to keep apart.
/// Taller than wide, along the bottom: the sticks lowest, the d-pad and the
/// face buttons over them, the shoulders over those, and Select, Start, L3
/// and R3 between the sticks. Each side is the other's mirror.
///
/// Smaller, it is the same pad drawn smaller toward where it is held: each
/// side (its shoulders, its cluster, its stick and, wider than tall, its
/// L3 or R3) toward its bottom corner, the middle's buttons (Select and
/// Start, and taller than wide L3 and R3) toward the middle of the edge
/// they sit by. Each part only shrinks toward a point no nearer any other
/// part, so nothing meets that did not meet at full size, and nothing
/// leaves the window or reaches up into the HUD.
pub fn layout(size: (u32, u32), scale: f32) -> Layout {
    let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let s = if scale.is_nan() { 1.0 } else { scale.clamp(SMALLEST, 1.0) };
    let u = unit(size);
    // the full pad's shoulders' height, the left side's d-pad (the face
    // buttons' mirror) and stick, Select's place (Start's mirror), L3's
    // (R3's mirror); and where the middle's buttons shrink toward, and
    // whether L3 and R3 go with the sides
    let (shoulders, cluster, stick, select, l3, middle, side_l3) = if w >= h {
        let v = h / 6.5;
        let stick = [3.35 * u, h - 1.45 * u];
        (1.75 * v, [1.75 * u, 3.4 * v], stick, [w / 2.0 - 0.75 * u, 0.62 * u], [stick[0], stick[1] - 1.35 * u], [w / 2.0, 0.0], true)
    } else {
        (h - 5.45 * u, [1.5 * u, h - 3.75 * u], [1.5 * u, h - 1.45 * u], [w / 2.0 - 0.5 * u, h - 2.15 * u], [w / 2.0 - 0.5 * u, h - 0.85 * u], [w / 2.0, h], false)
    };
    let mirror = |p: [f32; 2]| [w - p[0], p[1]];
    // a full-size place of the left side, of the middle (left of it), or of
    // either mirrored, at the pad's size
    let corner = [0.0, h];
    let toward = |a: [f32; 2], p: [f32; 2]| if s == 1.0 { p } else { [a[0] + s * (p[0] - a[0]), a[1] + s * (p[1] - a[1])] };
    let side = |p: [f32; 2]| toward(corner, p);
    let mid = |p: [f32; 2]| toward(middle, p);
    let mut buttons = Vec::new();
    let mut put = |bit: u16, at: [f32; 2], r: f32, glyph: i32| {
        buttons.push(Button { bit, at, r: s * r, glyph });
    };
    // the shoulders, the triggers outermost
    let sr = 0.38 * u;
    put(bit::L2, side([0.75 * u, shoulders]), sr, glyph::L2);
    put(bit::L1, side([1.75 * u, shoulders]), sr, glyph::L1);
    put(bit::R1, mirror(side([1.75 * u, shoulders])), sr, glyph::R1);
    put(bit::R2, mirror(side([0.75 * u, shoulders])), sr, glyph::R2);
    // the d-pad
    let (ds, dr) = (0.68 * u, 0.36 * u);
    for (b, o, g) in [(bit::UP, [0.0, -ds], glyph::UP), (bit::DOWN, [0.0, ds], glyph::DOWN), (bit::LEFT, [-ds, 0.0], glyph::LEFT), (bit::RIGHT, [ds, 0.0], glyph::RIGHT)] {
        put(b, side([cluster[0] + o[0], cluster[1] + o[1]]), dr, g);
    }
    // the face buttons
    let (fs, fr) = (0.76 * u, 0.4 * u);
    for (b, o, g) in [(bit::TRIANGLE, [0.0, -fs], glyph::TRIANGLE), (bit::CROSS, [0.0, fs], glyph::CROSS), (bit::SQUARE, [fs, 0.0], glyph::SQUARE), (bit::CIRCLE, [-fs, 0.0], glyph::CIRCLE)] {
        put(b, mirror(side([cluster[0] + o[0], cluster[1] + o[1]])), fr, g);
    }
    // Select and Start, L3 and R3
    let mr = 0.36 * u;
    put(bit::SELECT, mid(select), mr, glyph::SELECT);
    put(bit::START, mirror(mid(select)), mr, glyph::START);
    let tr = 0.3 * u;
    let l3 = if side_l3 { side(l3) } else { mid(l3) };
    put(bit::L3, l3, tr, glyph::L3);
    put(bit::R3, mirror(l3), tr, glyph::R3);
    // the sticks: in play a finger put down in the lower corner's reach of
    // one is its (up to the window's middle), below the d-pad's and the face
    // buttons' places (a button takes its finger first)
    let stick = side(stick);
    let r = s * 0.95 * u;
    let top = stick[1] - 1.25 * r;
    let left = Stick { home: stick, r, zone: [0.0, top, (stick[0] + 1.6 * r).min(w / 2.0), h] };
    let right = Stick { home: mirror(stick), r, zone: [(w - stick[0] - 1.6 * r).max(w / 2.0), top, w, h] };
    Layout { u: s * u, full: u, buttons, sticks: [left, right] }
}

impl Layout {
    /// The button under a finger at `at`, a little generously.
    fn button_at(&self, at: [f32; 2]) -> Option<&Button> {
        self.buttons.iter().filter(|b| dist(b.at, at) <= b.r * 1.25).min_by(|a, b| dist(a.at, at).total_cmp(&dist(b.at, at)))
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

pub struct TouchPad {
    fingers: Vec<Finger>,
    /// its size, of the full pad's (`layout`): the options' Touch Pad
    /// Size, which the viewer hands it before its fingers and its frames
    /// (`settings::touch_scale`)
    pub scale: f32,
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
    /// a window is up, or the game's screens before the world are (the
    /// viewer sets it each frame): the pad drawn fainter, and a stick takes
    /// only a finger put down inside its ring
    pub window: bool,
    /// a finger held still is a long press (`take_holds`; the viewer sets it
    /// each frame: the map is up, where it puts the group's waypoint)
    pub holds: bool,
}

impl Default for TouchPad {
    /// Hidden, unused, at its full size.
    fn default() -> Self {
        TouchPad {
            fingers: Vec::new(),
            scale: 1.0,
            shown: false,
            used: false,
            look: [0.0; 2],
            pinch: 0.0,
            last_tap: None,
            clicks: Vec::new(),
            size: (0, 0),
            tex: None,
            window: false,
            holds: false,
        }
    }
}

impl TouchPad {
    /// A pad, shown from the start or from the first touch.
    pub fn new(shown: bool) -> Self {
        TouchPad { shown, used: shown, ..Default::default() }
    }

    /// Where its buttons and sticks are in its window now.
    fn layout(&self) -> Layout {
        layout(self.size, self.scale)
    }

    /// A finger's event in a `size` window, at window pixels `at`.
    pub fn touch(&mut self, id: u64, phase: Phase, at: [f32; 2], now: Instant, size: (u32, u32)) {
        self.shown = true;
        self.used = true;
        self.size = size;
        let l = self.layout();
        match phase {
            Phase::Started => {
                let held = |k: usize| self.fingers.iter().any(|f| matches!(f.role, Role::Stick { which, .. } if which == k));
                let role = if let Some(b) = l.button_at(at) {
                    Role::Button(b.bit)
                } else if let Some(which) = (0..2).find(|&k| !held(k) && l.sticks[k].takes(at, self.window)) {
                    Role::Stick { which, centre: at }
                } else {
                    Role::Look { start: at, moved: false }
                };
                self.fingers.retain(|f| f.id != id);
                self.fingers.push(Finger { id, role, at, since: now, held: false });
            }
            Phase::Moved => {
                let looking = self.fingers.iter().filter(|f| matches!(f.role, Role::Look { .. })).count();
                let other = self.fingers.iter().find(|f| f.id != id && matches!(f.role, Role::Look { .. })).map(|f| f.at);
                let Some(f) = self.fingers.iter_mut().find(|f| f.id == id) else { return };
                let was = f.at;
                f.at = at;
                if let Role::Look { start, moved } = &mut f.role {
                    if !*moved && dist(*start, at) > 0.12 * l.full {
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
                if let (Role::Look { start, moved: false }, Phase::Ended, false) = (f.role, phase, f.held) {
                    let double = self.last_tap.is_some_and(|(p, t)| now.duration_since(t) < DOUBLE && dist(p, start) < 0.5 * l.full);
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

    /// The pad as the fingers hold it: the buttons touched, the sticks'
    /// pushes.
    pub fn report(&self) -> Report {
        let mut r = Report::default();
        let l = self.layout();
        for f in &self.fingers {
            match f.role {
                Role::Button(b) => r.buttons |= b,
                Role::Stick { which, centre } => {
                    let reach = l.sticks[which].r;
                    let (dx, dy) = ((f.at[0] - centre[0]) / reach, (f.at[1] - centre[1]) / reach);
                    let m = (dx * dx + dy * dy).sqrt().max(1.0);
                    // rx ry, then lx ly
                    let k = if which == 0 { 2 } else { 0 };
                    r.sticks[k] = dx / m;
                    r.sticks[k + 1] = dy / m;
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

    /// While long presses are wanted (`holds`: the map is up), a finger held
    /// still on the screen (no button's, not moved past a tap's slop, the
    /// only one looking) for `map::group_waypoint::HOLD` by `now`: where it
    /// came down, once; its lift is then no tap. None else.
    pub fn take_holds(&mut self, now: Instant) -> Vec<[f32; 2]> {
        let looking = self.fingers.iter().filter(|f| matches!(f.role, Role::Look { .. })).count();
        if !self.holds || looking != 1 {
            return Vec::new();
        }
        let hold = Duration::from_secs_f32(crate::map::group_waypoint::HOLD);
        let mut out = Vec::new();
        for f in &mut self.fingers {
            if let Role::Look { start, moved: false } = f.role
                && !f.held
                && now.saturating_duration_since(f.since) >= hold
            {
                f.held = true;
                out.push(start);
            }
        }
        out
    }

    /// A key or a controller in use: the overlay goes, every finger let go.
    pub fn hide(&mut self) {
        self.shown = false;
        self.fingers.clear();
    }

    /// The disc and the ring the buttons and the sticks are drawn on, made
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
    /// With `dim` (a window up, or the game's screens before the world)
    /// it is drawn a little fainter, a button held as brightly.
    pub fn draw(&self, ui: &mut Ui, size: (u32, u32), dim: bool, glyph: &dyn Fn(i32) -> Option<Glyph>) {
        let (Some((layer, disc, ring)), true) = (self.tex, self.shown) else { return };
        if layer != ui.serial() {
            return;
        }
        let l = layout(size, self.scale);
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
        let fade = if dim { 0.7 } else { 1.0 };
        for b in &l.buttons {
            let on = held & b.bit != 0;
            ui.push_whole(quad(disc, b.at, [b.r, b.r], full, [0.05, 0.05, 0.08, if on { 0.55 } else { 0.3 * fade }]));
            if let Some((tex, uv, px)) = glyph(b.glyph) {
                // the glyph inside the disc, its shape kept
                let k = 1.25 * b.r / px[0].max(px[1]);
                ui.push_whole(quad(tex, b.at, [px[0] * k / 2.0, px[1] * k / 2.0], uv, [1.0, 1.0, 1.0, if on { 1.0 } else { 0.75 * fade }]));
            }
        }
        // each stick: held, its ring round where the finger came down and
        // its knob under the finger (at most a reach out); else at home, its
        // ring and knob faint
        let knob = 0.45 * l.u;
        for (k, s) in l.sticks.iter().enumerate() {
            let at = self.fingers.iter().find_map(|f| match f.role {
                Role::Stick { which, centre } if which == k => Some((centre, f.at)),
                _ => None,
            });
            match at {
                Some((c, at)) => {
                    let (dx, dy) = (at[0] - c[0], at[1] - c[1]);
                    let m = (dx * dx + dy * dy).sqrt().max(s.r) / s.r;
                    ui.push_whole(quad(ring, c, [s.r, s.r], full, [1.0, 1.0, 1.0, 0.6]));
                    ui.push_whole(quad(disc, [c[0] + dx / m, c[1] + dy / m], [knob, knob], full, [0.85, 0.85, 0.9, 0.6]));
                }
                None => {
                    ui.push_whole(quad(ring, s.home, [s.r, s.r], full, [1.0, 1.0, 1.0, 0.22 * fade]));
                    ui.push_whole(quad(disc, s.home, [knob, knob], full, [0.05, 0.05, 0.08, 0.3 * fade]));
                }
            }
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
        for b in layout(size, touch.scale).buttons {
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

/// The fingers where a pointer is taken instead of the pad (the client's
/// own dialog: the update's question): a finger's tap is a click
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
/// looking finger on the pad may before it looks (at any pad size).
fn tap_slop(size: (u32, u32)) -> f32 {
    0.12 * unit(size)
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

    /// Over a window (and on the game's screens before the world) every
    /// button is still the pad's, the d-pad, Start, L3 and the shoulders as
    /// much as the face buttons; a stick takes only a finger put down inside
    /// its ring, and one put down in its zone off the ring is a tap for the
    /// window. The control: in play that finger is the stick's.
    #[test]
    fn over_a_window_every_button_is_the_pads_and_a_stick_only_its_ring() {
        let t = Instant::now();
        let l = layout(SIZE, 1.0);
        let at = |bit: u16| l.buttons.iter().find(|b| b.bit == bit).unwrap().at;
        let mut p = TouchPad::new(true);
        p.window = true;
        for (k, b) in [bit::UP, bit::START, bit::L3, bit::R1, bit::TRIANGLE].into_iter().enumerate() {
            p.touch(k as u64, Phase::Started, at(b), t, SIZE);
            assert_eq!(p.report().buttons, b, "{}", bit::name(b));
            p.touch(k as u64, Phase::Ended, at(b), t, SIZE);
        }
        assert!(p.take_clicks().is_empty(), "a button's finger is no tap");
        // in the left stick's zone, off its ring: the window's
        let s = l.sticks[0];
        let off = [s.zone[0] + 2.0, s.zone[3] - 2.0];
        assert!(dist(off, s.home) > s.r && s.takes(off, false) && l.button_at(off).is_none(), "a place in the zone, off the ring and the buttons");
        p.touch(10, Phase::Started, off, t, SIZE);
        p.touch(10, Phase::Moved, [off[0] + 1.0, off[1]], t, SIZE);
        assert_eq!(p.report(), Report::default(), "nothing held");
        p.touch(10, Phase::Ended, off, t, SIZE);
        assert_eq!(p.take_clicks(), vec![Click::Target(off[0] as f64, off[1] as f64)], "a tap for the window");
        // inside the ring: the stick's
        p.touch(11, Phase::Started, s.home, t, SIZE);
        p.touch(11, Phase::Moved, [s.home[0], s.home[1] - s.r], t, SIZE);
        assert!(near(p.report().sticks, [0.0, 0.0, 0.0, -1.0]), "{:?}", p.report().sticks);
        p.touch(11, Phase::Ended, s.home, t, SIZE);
        // the control: in play the zone's place is the stick's
        let later = t + Duration::from_secs(5);
        p.window = false;
        p.touch(12, Phase::Started, off, later, SIZE);
        p.touch(12, Phase::Moved, [off[0] + s.r, off[1]], later, SIZE);
        assert!(near(p.report().sticks, [0.0, 0.0, 1.0, 0.0]), "{:?}", p.report().sticks);
        p.touch(12, Phase::Ended, off, later, SIZE);
        assert!(p.take_clicks().is_empty());
    }

    fn near(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-4)
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

    /// With long presses wanted (the map up), a finger held still past the
    /// hold is one, once, where it came down, and its lift no tap; a finger
    /// lifted sooner is a tap, one that moved a drag, two at once a pinch.
    /// The control: not wanted, the same long hold is a tap at its lift.
    #[test]
    fn a_finger_held_still_on_the_map_is_a_long_press() {
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let at = [1000.0, 400.0];
        let mut p = TouchPad::new(true);
        p.window = true;
        p.holds = true;
        p.touch(1, Phase::Started, at, ms(0), SIZE);
        assert!(p.take_holds(ms(500)).is_empty(), "not yet");
        assert_eq!(p.take_holds(ms(650)), vec![at]);
        assert!(p.take_holds(ms(900)).is_empty(), "once");
        p.touch(1, Phase::Ended, at, ms(950), SIZE);
        assert!(p.take_clicks().is_empty(), "its lift no tap");
        // lifted sooner: a tap
        p.touch(2, Phase::Started, at, ms(2000), SIZE);
        p.touch(2, Phase::Ended, at, ms(2200), SIZE);
        assert!(p.take_holds(ms(2700)).is_empty());
        assert_eq!(p.take_clicks().len(), 1, "a tap");
        // moved past the slop: a drag
        p.touch(3, Phase::Started, at, ms(4000), SIZE);
        p.touch(3, Phase::Moved, [at[0] + 80.0, at[1]], ms(4100), SIZE);
        assert!(p.take_holds(ms(4800)).is_empty(), "a drag");
        p.touch(3, Phase::Ended, [at[0] + 80.0, at[1]], ms(4900), SIZE);
        // two fingers: a pinch's
        p.touch(4, Phase::Started, at, ms(6000), SIZE);
        p.touch(5, Phase::Started, [at[0] + 300.0, at[1]], ms(6010), SIZE);
        assert!(p.take_holds(ms(6800)).is_empty(), "two fingers");
        p.touch(4, Phase::Ended, at, ms(6900), SIZE);
        p.touch(5, Phase::Ended, [at[0] + 300.0, at[1]], ms(6900), SIZE);
        p.take_clicks();
        // the control: not wanted, held as long, it is a tap
        p.holds = false;
        p.touch(6, Phase::Started, at, ms(8000), SIZE);
        assert!(p.take_holds(ms(8700)).is_empty());
        p.touch(6, Phase::Ended, at, ms(8800), SIZE);
        assert_eq!(p.take_clicks().len(), 1, "the control: a tap");
    }

    /// Window shapes: phones and tablets either way up, desktop windows, a
    /// square and the shapes either side of it, an ultrawide, a small one.
    const SHAPES: [(u32, u32); 20] = [
        (2400, 1080),
        (1080, 2400),
        (2532, 1170),
        (1170, 2532),
        (844, 390),
        (390, 844),
        (2360, 1640),
        (1640, 2360),
        (1180, 820),
        (1920, 1080),
        (1280, 720),
        (1440, 900),
        (1024, 768),
        (768, 1024),
        (1000, 1000),
        (1000, 1001),
        (1001, 1000),
        (3440, 1440),
        (320, 240),
        (240, 320),
    ];

    /// The Touch Pad Size's steps (`settings`): 50% to 100% in fives.
    fn sizes() -> impl Iterator<Item = f32> {
        (10..=20).map(|k| k as f32 / 20.0)
    }

    /// Every shape keeps the whole controller on the screen with nothing
    /// overlapping, at every size: the sixteen buttons and both sticks'
    /// rings, each apart from every other, all inside the window.
    #[test]
    fn the_layout_keeps_its_buttons_apart_and_on_the_screen() {
        for (size, s) in SHAPES.into_iter().flat_map(|z| sizes().map(move |s| (z, s))) {
            let l = layout(size, s);
            let (w, h) = (size.0 as f32, size.1 as f32);
            assert_eq!(l.buttons.len(), 16);
            let all: u16 = l.buttons.iter().fold(0, |m, b| m | b.bit);
            assert_eq!(all, 0xFFFF, "{size:?}: every pad button has one");
            let mut discs: Vec<(String, [f32; 2], f32)> = l.buttons.iter().map(|b| (bit::name(b.bit).to_string(), b.at, b.r)).collect();
            discs.extend(l.sticks.iter().enumerate().map(|(k, s)| (format!("stick {k}"), s.home, s.r)));
            for (i, (n, at, r)) in discs.iter().enumerate() {
                assert!(at[0] - r >= 0.0 && at[0] + r <= w && at[1] - r >= 0.0 && at[1] + r <= h, "{size:?} at {s}: {n} at {at:?}, {r} round, off the screen");
                for (m, bt, br) in &discs[i + 1..] {
                    assert!(dist(*at, *bt) >= r + br, "{size:?} at {s}: {n} overlaps {m}");
                }
            }
        }
    }

    /// The controller as a DualShock 4 is laid out: a stick each side of the
    /// middle, the d-pad above the left one and the face buttons above the
    /// right one (outside them, or upright straight above), L2 L1 over the
    /// d-pad and R1 R2 over
    /// the face buttons with the triggers outermost, Select left of Start
    /// either side of the middle, L3 by the left stick and R3 by the right.
    /// Wider than tall the shoulders are under the HUD's compass and bars
    /// (1.2 of the height's sixth and a half down) and Select and Start over
    /// the d-pad's height; taller than wide all of it is in the window's
    /// lower part (the picture over it).
    fn ds4(l: &Layout, size: (u32, u32)) -> Result<(), String> {
        let (w, h) = (size.0 as f32, size.1 as f32);
        let b = |bit: u16| *l.buttons.iter().find(|x| x.bit == bit).unwrap();
        let mid = |bits: [u16; 4]| {
            let p = bits.map(|x| b(x).at);
            [p.iter().map(|p| p[0]).sum::<f32>() / 4.0, p.iter().map(|p| p[1]).sum::<f32>() / 4.0]
        };
        let dpad = mid([bit::UP, bit::DOWN, bit::LEFT, bit::RIGHT]);
        let face = mid([bit::TRIANGLE, bit::CROSS, bit::SQUARE, bit::CIRCLE]);
        let [ls, rs] = l.sticks.map(|s| s.home);
        let is = |ok: bool, what: &str| if ok { Ok(()) } else { Err(format!("{size:?}: {what}")) };
        is(ls[0] < w / 2.0 && rs[0] > w / 2.0, "a stick each side of the middle")?;
        let e = 0.01 * l.u;
        is(dpad[0] <= ls[0] + e && dpad[1] < ls[1], "the d-pad above the left stick, no further in")?;
        is(face[0] >= rs[0] - e && face[1] < rs[1], "the face buttons above the right stick, no further in")?;
        for (s, c) in [(bit::L2, dpad), (bit::L1, dpad), (bit::R1, face), (bit::R2, face)] {
            is(b(s).at[1] < c[1] && (b(s).at[0] < w / 2.0) == (c[0] < w / 2.0), &format!("{} over its side's cluster", bit::name(s)))?;
        }
        is(b(bit::L2).at[0] < b(bit::L1).at[0] && b(bit::R1).at[0] < b(bit::R2).at[0], "the triggers outermost")?;
        is(b(bit::SELECT).at[0] < w / 2.0 && b(bit::START).at[0] > w / 2.0, "Select left of Start, either side of the middle")?;
        let (l3, r3) = (b(bit::L3).at, b(bit::R3).at);
        is(dist(l3, ls) < dist(l3, rs) && dist(r3, rs) < dist(r3, ls), "L3 by the left stick, R3 by the right")?;
        if w >= h {
            let v = h / 6.5;
            for s in [bit::L2, bit::L1, bit::R1, bit::R2] {
                is(b(s).at[1] - b(s).r >= 1.2 * v, &format!("{} under the HUD's compass and bars", bit::name(s)))?;
            }
            is(b(bit::SELECT).at[1] < b(bit::UP).at[1] && b(bit::START).at[1] < b(bit::TRIANGLE).at[1], "Select and Start at the top")?;
        } else {
            for x in &l.buttons {
                is(x.at[1] - x.r >= 0.4 * h, &format!("{} in the lower part", bit::name(x.bit)))?;
            }
        }
        Ok(())
    }

    /// Every shape's layout is a DualShock 4's (`ds4`), at every size. The
    /// control: the same layouts mirrored left for right are none.
    #[test]
    fn the_layout_is_a_dualshock_4s() {
        for (size, s) in SHAPES.into_iter().flat_map(|z| sizes().map(move |s| (z, s))) {
            let l = layout(size, s);
            ds4(&l, size).map_err(|e| format!("{e} at {s}")).unwrap();
            let mut m = l.clone();
            for b in &mut m.buttons {
                b.at[0] = size.0 as f32 - b.at[0];
            }
            for s in &mut m.sticks {
                s.home[0] = size.0 as f32 - s.home[0];
            }
            assert!(ds4(&m, size).is_err(), "{size:?} at {s}: mirrored, still one");
        }
    }

    /// A smaller pad is the full one drawn smaller toward where it is held:
    /// each side's parts toward its bottom corner, Select and Start (and,
    /// taller than wide, L3 and R3) toward the middle of the edge they are
    /// by, every disc and ring that much smaller. The full pad is the most
    /// there is (more is the full pad, and so is a size that is no number),
    /// half of it the least. The controls: at full size every place is
    /// where the pad has always put it, and a finger's own reach (the
    /// slop, a double tap's spread) is the full pad's at any size.
    #[test]
    fn a_smaller_pad_is_the_full_one_drawn_toward_where_it_is_held() {
        for size in SHAPES {
            let (w, h) = (size.0 as f32, size.1 as f32);
            let full = layout(size, 1.0);
            for s in [0.5f32, 0.75, 0.95] {
                let l = layout(size, s);
                for (a, b) in full.buttons.iter().zip(&l.buttons) {
                    assert_eq!(a.bit, b.bit);
                    let middle = matches!(a.bit, bit::SELECT | bit::START) || (h > w && matches!(a.bit, bit::L3 | bit::R3));
                    let anchor = if middle {
                        [w / 2.0, if w >= h { 0.0 } else { h }]
                    } else if a.at[0] < w / 2.0 {
                        [0.0, h]
                    } else {
                        [w, h]
                    };
                    let want = [anchor[0] + s * (a.at[0] - anchor[0]), anchor[1] + s * (a.at[1] - anchor[1])];
                    assert!(dist(want, b.at) < 1e-3 && (b.r - s * a.r).abs() < 1e-4, "{size:?} at {s}: {} at {:?}, not {want:?}", bit::name(a.bit), b.at);
                }
                for (k, anchor) in [[0.0, h], [w, h]].into_iter().enumerate() {
                    let (a, b) = (full.sticks[k], l.sticks[k]);
                    let want = [anchor[0] + s * (a.home[0] - anchor[0]), anchor[1] + s * (a.home[1] - anchor[1])];
                    assert!(dist(want, b.home) < 1e-3 && (b.r - s * a.r).abs() < 1e-4, "{size:?} at {s}: stick {k}");
                }
                assert_eq!((l.u, l.full), (s * full.u, full.u), "{size:?} at {s}: a finger's unit the full pad's");
            }
            for (s, like) in [(1.5, 1.0), (f32::NAN, 1.0), (0.2, SMALLEST), (0.0, SMALLEST)] {
                let (a, b) = (layout(size, s), layout(size, like));
                assert!(a.buttons.iter().zip(&b.buttons).all(|(a, b)| a.at == b.at && a.r == b.r), "{size:?}: {s} is {like}");
            }
        }
        // the full pad where it has always been: a phone either way up
        let l = layout((2400, 1080), 1.0);
        let u = 1080.0 / 6.5;
        let at = |l: &Layout, bit: u16| l.buttons.iter().find(|b| b.bit == bit).unwrap().at;
        let was = |a: [f32; 2], b: [f32; 2]| assert!(dist(a, b) < 1e-3, "{a:?}, where it was {b:?}");
        was(at(&l, bit::L2), [0.75 * u, 1.75 * u]);
        was(at(&l, bit::START), [2400.0 - (1200.0 - 0.75 * u), 0.62 * u]);
        was(at(&l, bit::SQUARE), [2400.0 - 1.75 * u - 0.76 * u, 3.4 * u]);
        was(l.sticks[0].home, [3.35 * u, 1080.0 - 1.45 * u]);
        assert_eq!(l.sticks[0].r, 0.95 * u);
        let l = layout((1080, 2400), 1.0);
        let u = 1080.0 / 6.6;
        was(at(&l, bit::L3), [540.0 - 0.5 * u, 2400.0 - 0.85 * u]);
        was(at(&l, bit::CIRCLE), [1080.0 - 1.5 * u + 0.76 * u, 2400.0 - 3.75 * u]);
        assert_eq!(tap_slop((2400, 1080)), 0.12 * (1080.0f32 / 6.5));
    }

    /// The fingers go by the pad as it is drawn: on a half-size pad ✕ is
    /// where the half pad has it, and where the full pad's △ was is no
    /// button (a look). The control: the full pad's △ there.
    #[test]
    fn the_fingers_go_by_the_pads_size() {
        let t = Instant::now();
        let at = |l: &Layout, bit: u16| l.buttons.iter().find(|b| b.bit == bit).unwrap().at;
        let (half, full) = (layout(SIZE, 0.5), layout(SIZE, 1.0));
        let mut p = TouchPad::new(true);
        p.scale = 0.5;
        p.touch(1, Phase::Started, at(&half, bit::CROSS), t, SIZE);
        assert_eq!(p.report().buttons, bit::CROSS);
        p.touch(1, Phase::Ended, at(&half, bit::CROSS), t, SIZE);
        p.touch(2, Phase::Started, at(&full, bit::TRIANGLE), t, SIZE);
        assert_eq!(p.report(), Report::default(), "the full pad's △ is off the half pad");
        p.touch(2, Phase::Ended, at(&full, bit::TRIANGLE), t, SIZE);
        p.scale = 1.0;
        p.touch(3, Phase::Started, at(&full, bit::TRIANGLE), t + Duration::from_secs(1), SIZE);
        assert_eq!(p.report().buttons, bit::TRIANGLE, "the control");
    }

    /// A button is held while its finger is down, and each stick follows its
    /// own finger from where it came down: the left on lx ly, the right on
    /// rx ry. A second finger put down on a stick another holds looks.
    #[test]
    fn a_button_is_held_while_touched_and_each_stick_follows_its_finger() {
        let t = Instant::now();
        let l = layout(SIZE, 1.0);
        let mut p = TouchPad::default();
        let cross = l.buttons.iter().find(|b| b.bit == bit::CROSS).unwrap().at;
        p.touch(1, Phase::Started, cross, t, SIZE);
        assert_eq!(p.report().buttons, bit::CROSS);
        // the left stick: put down off its home, which is its centre then,
        // and pushed up and right a full reach from there
        let [ls, rs] = l.sticks;
        let c = [ls.home[0] - 20.0, ls.home[1] + 10.0];
        p.touch(2, Phase::Started, c, t, SIZE);
        assert_eq!(p.report().sticks, [0.0; 4], "where it came down is its centre");
        p.touch(2, Phase::Moved, [c[0] + ls.r, c[1] - ls.r], t, SIZE);
        // the right stick, half a reach left
        let d = [rs.home[0] + 5.0, rs.home[1]];
        p.touch(3, Phase::Started, d, t, SIZE);
        p.touch(3, Phase::Moved, [d[0] - rs.r / 2.0, d[1]], t, SIZE);
        let r = p.report();
        assert_eq!(r.buttons, bit::CROSS);
        let q = std::f32::consts::FRAC_1_SQRT_2;
        assert!(near(r.sticks, [-0.5, 0.0, q, -q]), "{:?}", r.sticks);
        // a second finger on the held left stick: a look
        p.touch(4, Phase::Started, [ls.home[0] + 30.0, ls.home[1]], t, SIZE);
        p.touch(4, Phase::Moved, [ls.home[0] + 80.0, ls.home[1]], t, SIZE);
        assert!(near(p.report().sticks, [-0.5, 0.0, q, -q]));
        assert_eq!(p.take_look(), (50.0, 0.0, 0.0));
        for (id, at) in [(1, cross), (2, c), (3, d), (4, ls.home)] {
            p.touch(id, Phase::Ended, at, t, SIZE);
        }
        assert_eq!(p.report(), Report::default());
        assert!(p.take_clicks().is_empty(), "none was a tap");
    }

    #[test]
    fn hidden_it_lets_go_of_every_finger_but_stays_used() {
        let t = Instant::now();
        let l = layout(SIZE, 1.0);
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
