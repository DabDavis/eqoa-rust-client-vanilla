//! The HUD's pieces where the player put them (the server owner, 2026-11-11:
//! "lets add click to drag HUD UI elements and also reset button"). The
//! client's own: the game lays its HUD out once, for a 4:3 picture, and a
//! wider or a taller window keeps each piece at its own side (`render::ui::
//! Anchor`, `Vertical`); here each piece can also be moved, and is drawn
//! moved by `Ui::shift` from where the game puts it. Nothing moved, every
//! quad is where it was, byte for byte.
//!
//! - Moving is a mode, Move HUD (Extended > HUD Layout, or `/hud`): while it
//!   is on and the world is up with no window over it, every piece drawn has
//!   an outline and its name, a bar at the top says so with Lock and Reset,
//!   and a press (the mouse's left button, or a finger) on a piece drags it.
//!   Locked, a click on the HUD is what it always was (targeting from the
//!   frame and the bars, the minimap's map, the tracker's journal, the
//!   chat box's lines and its selection, the meter's buttons), and a drag
//!   anywhere is the camera's: a modifier on the mouse would leave a touch
//!   screen without a way, and a press that both clicks and drags would
//!   target the player each time it picked up the frame. A controller needs
//!   none (the pad cannot point).
//! - A piece is kept as a distance from the side of the screen nearest it,
//!   across and up and down (`Place`: the left, the middle or the right, the
//!   top, the middle or the bottom, by where its middle lies in thirds of the
//!   screen), so a piece put at the right stays at the right when the window
//!   is made wider, and one at the middle stays in the middle. Drawn, it is
//!   kept on the screen (`clamp`): a window made smaller brings it in.
//! - Kept the moment a drag ends, for this window (`hud.opt`, window n's
//!   `hud-<n>.opt`, `settings::hud_layout`; in a page the origin's storage),
//!   and read at the next start. Reset HUD (Extended, the bar's Reset, `/hud
//!   reset`) puts every piece back at once and keeps that.

use render::ui::{SCREEN_H, SCREEN_W, Ui};
use std::cell::RefCell;

/// The HUD's pieces that move, in the order they are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    /// the portrait's frame, the compass, the health, power and experience gauges
    Player,
    /// the group's bars, or a raid's list
    Group,
    /// the pet's bar and its effects' row
    Pet,
    /// the target's bar
    Target,
    /// the minimap (`map::mini`)
    Minimap,
    /// the quest tracker (`tracker`)
    Quests,
    /// the DPS meter (`dpsmeter`)
    Meter,
    /// the player's effects (`influence`)
    Effects,
    /// the chat box (`chat`)
    Chat,
    /// the breath gauge, under water
    Breath,
    /// the quick menus (`hudmenu`)
    QuickMenus,
}

/// Each piece, in `Piece`'s order.
pub const PIECES: [Piece; 11] = [Piece::Player, Piece::Group, Piece::Pet, Piece::Target, Piece::Minimap, Piece::Quests, Piece::Meter, Piece::Effects, Piece::Chat, Piece::Breath, Piece::QuickMenus];
const N: usize = PIECES.len();

/// Where on an axis a piece is measured from: the screen's left (top), its
/// middle, its right (bottom).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Start,
    Middle,
    End,
}

impl Edge {
    /// How far the screen's `self` is from the frame's, in frame pixels,
    /// `beyond` whole frame pixels showing past each of the frame's sides
    /// (`render::ui::side`, `top`).
    fn reference(self, beyond: f32) -> f32 {
        match self {
            Edge::Start => -beyond,
            Edge::Middle => 0.0,
            Edge::End => beyond,
        }
    }

    fn key(self, across: bool) -> &'static str {
        match (self, across) {
            (Edge::Start, true) => "l",
            (Edge::End, true) => "r",
            (Edge::Start, false) => "t",
            (Edge::End, false) => "b",
            (Edge::Middle, _) => "m",
        }
    }

    fn from_key(s: &str, across: bool) -> Option<Edge> {
        Some(match (s, across) {
            ("l", true) | ("t", false) => Edge::Start,
            ("r", true) | ("b", false) => Edge::End,
            ("m", _) => Edge::Middle,
            _ => return None,
        })
    }
}

impl Piece {
    fn index(self) -> usize {
        PIECES.iter().position(|&p| p == self).expect("every piece is listed")
    }

    /// Its name in `hud.opt`.
    pub fn key(self) -> &'static str {
        match self {
            Piece::Player => "player",
            Piece::Group => "group",
            Piece::Pet => "pet",
            Piece::Target => "target",
            Piece::Minimap => "minimap",
            Piece::Quests => "quests",
            Piece::Meter => "dps",
            Piece::Effects => "effects",
            Piece::Chat => "chat",
            Piece::Breath => "breath",
            Piece::QuickMenus => "quickbars",
        }
    }

    /// Its name on its outline while the HUD moves.
    pub fn label(self) -> &'static str {
        match self {
            Piece::Player => "Character",
            Piece::Group => "Group",
            Piece::Pet => "Pet",
            Piece::Target => "Target",
            Piece::Minimap => "Minimap",
            Piece::Quests => "Quest Tracker",
            Piece::Meter => "DPS Meter",
            Piece::Effects => "Effects",
            Piece::Chat => "Chat",
            Piece::Breath => "Breath",
            Piece::QuickMenus => "Quick Bars",
        }
    }

    /// The sides the game keeps it at (`Hud::draw`, `Play::draw_hud`):
    /// across a wider window and up or down a taller one.
    pub fn own(self) -> (Edge, Edge) {
        match self {
            Piece::Player | Piece::Group | Piece::Pet | Piece::Effects => (Edge::Start, Edge::Start),
            Piece::Target => (Edge::Start, Edge::End),
            Piece::Chat => (Edge::End, Edge::End),
            Piece::Minimap | Piece::Quests | Piece::Meter | Piece::Breath | Piece::QuickMenus => (Edge::End, Edge::Start),
        }
    }
}

/// Where a moved piece is: across, `dx` frame pixels from where the game would
/// put it were it kept at the screen's `h` (rather than its own side); up and
/// down the same by `v` and `dy`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub h: Edge,
    pub dx: f32,
    pub v: Edge,
    pub dy: f32,
}

/// The screen beyond the frame: whole frame pixels past each side, above and
/// below (`render::ui::side`, `top`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Beyond {
    pub side: f32,
    pub top: f32,
}

impl Beyond {
    pub fn of(size: (u32, u32)) -> Beyond {
        Beyond { side: render::ui::side(size), top: render::ui::top(size) }
    }

    /// The screen, in frame pixels.
    pub fn screen(self) -> [f32; 4] {
        [-self.side, -self.top, SCREEN_W + self.side, SCREEN_H + self.top]
    }
}

impl Place {
    /// How far `piece` placed here goes from where the game puts it, on a
    /// screen `b`.
    pub fn shift(self, piece: Piece, b: Beyond) -> [f32; 2] {
        let (h, v) = piece.own();
        [self.dx + self.h.reference(b.side) - h.reference(b.side), self.dy + self.v.reference(b.top) - v.reference(b.top)]
    }

    /// The place of `piece` moved `shift` on screen `b`, where it covers
    /// `rect` (frame pixels, as drawn there): measured from the screen's side
    /// nearest its middle, in thirds.
    pub fn of(piece: Piece, shift: [f32; 2], rect: [f32; 4], b: Beyond) -> Place {
        let s = b.screen();
        let third = |lo: f32, hi: f32, mid: f32| {
            let w = (hi - lo) / 3.0;
            if mid < lo + w {
                Edge::Start
            } else if mid > hi - w {
                Edge::End
            } else {
                Edge::Middle
            }
        };
        let h = third(s[0], s[2], (rect[0] + rect[2]) / 2.0);
        let v = third(s[1], s[3], (rect[1] + rect[3]) / 2.0);
        let (oh, ov) = piece.own();
        Place { h, dx: shift[0] - h.reference(b.side) + oh.reference(b.side), v, dy: shift[1] - v.reference(b.top) + ov.reference(b.top) }
    }
}

/// `shift` brought in so that `natural` (a piece's rectangle where the game
/// puts it) moved by it stays on screen `b`, in whole frame pixels; a piece
/// larger than the screen keeps its left and top on it.
pub fn clamp(shift: [f32; 2], natural: [f32; 4], b: Beyond) -> [f32; 2] {
    let s = b.screen();
    let axis = |d: f32, lo: f32, hi: f32, slo: f32, shi: f32| {
        let d = d.min(shi - hi);
        d.max(slo - lo).round()
    };
    [axis(shift[0], natural[0], natural[2], s[0], s[2]), axis(shift[1], natural[1], natural[3], s[1], s[3])]
}

/// Every moved piece's place (none for one where the game puts it).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Layout {
    pub places: [Option<Place>; N],
}

impl Layout {
    pub fn get(&self, p: Piece) -> Option<Place> {
        self.places[p.index()]
    }

    pub fn set(&mut self, p: Piece, place: Option<Place>) {
        self.places[p.index()] = place;
    }

    /// As `hud.opt` keeps it: a line a moved piece, `<piece> <h> <dx> <v> <dy>`.
    pub fn to_text(self) -> String {
        let mut s = String::new();
        for p in PIECES {
            if let Some(at) = self.get(p) {
                s.push_str(&format!("{} {} {} {} {}\n", p.key(), at.h.key(true), at.dx, at.v.key(false), at.dy));
            }
        }
        s
    }

    /// `to_text` read back; a line that does not read is left out, its piece
    /// where the game puts it.
    pub fn from_text(s: &str) -> Layout {
        let mut l = Layout::default();
        for line in s.lines() {
            let mut it = line.split_whitespace();
            let parsed = (|| {
                let name = it.next()?;
                let p = PIECES.into_iter().find(|p| p.key() == name)?;
                let h = Edge::from_key(it.next()?, true)?;
                let dx: f32 = it.next()?.parse().ok()?;
                let v = Edge::from_key(it.next()?, false)?;
                let dy: f32 = it.next()?.parse().ok()?;
                // far past any screen: nothing a drag made
                let sane = |d: f32| d.is_finite() && d.abs() < 10_000.0;
                (it.next().is_none() && sane(dx) && sane(dy)).then_some((p, Place { h, dx, v, dy }))
            })();
            if let Some((p, at)) = parsed {
                l.set(p, Some(at));
            }
        }
        l
    }
}

/// A drag under way: the piece, where the press was and how far the piece
/// was moved then (frame pixels).
#[derive(Clone, Copy, Debug)]
struct Drag {
    piece: Piece,
    from: [f32; 2],
    shift: [f32; 2],
}

/// What the bar at the top while the HUD moves has: its buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Lock,
    Reset,
}

#[derive(Default)]
struct State {
    /// the window's place the layout was read for (`multibox`)
    read_for: Option<usize>,
    layout: Layout,
    unlocked: bool,
    /// each piece's rectangle as drawn in the last HUD frame (frame pixels,
    /// moved), and where the game put it (unmoved); those of the frame being
    /// made
    drawn: [Option<[f32; 4]>; N],
    natural: [Option<[f32; 4]>; N],
    making: [Option<[f32; 4]>; N],
    making_natural: [Option<[f32; 4]>; N],
    drag: Option<Drag>,
    /// a press Move HUD took, not let go yet
    pressed: bool,
    /// the bar's buttons where they were drawn
    buttons: Vec<([f32; 4], Button)>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let me = crate::multibox::place();
        if s.read_for != Some(me) {
            s.read_for = Some(me);
            s.layout = Layout::from_text(&crate::settings::hud_layout());
        }
        f(&mut s)
    })
}

/// The layout now.
#[cfg(test)]
pub fn layout() -> Layout {
    with(|s| s.layout)
}

/// Whether Move HUD is on.
pub fn unlocked() -> bool {
    with(|s| s.unlocked)
}

/// Move HUD on or off; off ends a drag (keeping where it got to).
pub fn set_unlocked(on: bool) {
    let ended = with(|s| {
        s.unlocked = on;
        !on && s.drag.take().is_some()
    });
    if ended {
        keep();
    }
}

/// Every piece where the game puts it, kept so.
pub fn reset() {
    with(|s| {
        s.layout = Layout::default();
        s.drag = None;
    });
    keep();
}

/// The layout kept for this window (`settings::save_hud_layout`).
fn keep() {
    let text = with(|s| s.layout.to_text());
    crate::settings::save_hud_layout(&text);
}

/// How far `piece` is drawn from where the game puts it now, on screen `b`:
/// its place's, kept on the screen by where it was last drawn.
pub fn shift(piece: Piece, b: Beyond) -> [f32; 2] {
    with(|s| shift_in(s, piece, b))
}

fn shift_in(s: &State, piece: Piece, b: Beyond) -> [f32; 2] {
    let Some(at) = s.layout.get(piece) else { return [0.0; 2] };
    let raw = at.shift(piece, b);
    match s.natural[piece.index()] {
        Some(n) => clamp(raw, n, b),
        None => [raw[0].round(), raw[1].round()],
    }
}

/// A piece begun on `ui` (`end`): what it draws is moved by its shift.
pub struct Mark {
    piece: Piece,
    from: usize,
    shift: [f32; 2],
    before: [f32; 2],
}

/// `piece` is drawn next on `ui`, moved where the player put it.
pub fn begin(ui: &mut Ui, piece: Piece) -> Mark {
    let b = Beyond { side: ui.side(), top: ui.top() };
    let shift = shift(piece, b);
    let before = std::mem::replace(&mut ui.shift, shift);
    Mark { piece, from: ui.quads.len(), shift, before }
}

/// The piece `m` drawn: the rectangle its quads cover noted (beside what the
/// frame noted of it already) and `ui`'s shift as it was.
pub fn end(ui: &mut Ui, m: Mark) {
    ui.shift = m.before;
    let quads = &ui.quads[m.from.min(ui.quads.len())..];
    let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for q in quads {
        for p in &q.pos {
            r = [r[0].min(p[0]), r[1].min(p[1]), r[2].max(p[0]), r[3].max(p[1])];
        }
    }
    if r[0] <= r[2] {
        note(m.piece, r, m.shift);
    }
}

/// `piece` covers frame rectangle `r` as drawn this frame, moved `shift`
/// (what it draws elsewhere: the minimap's own layer).
pub fn note(piece: Piece, r: [f32; 4], shift: [f32; 2]) {
    with(|s| {
        let i = piece.index();
        let join = |a: Option<[f32; 4]>, b: [f32; 4]| Some(a.map_or(b, |a| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]));
        s.making[i] = join(s.making[i], r);
        s.making_natural[i] = join(s.making_natural[i], [r[0] - shift[0], r[1] - shift[1], r[2] - shift[0], r[3] - shift[1]]);
    });
}

/// A HUD frame begun: nothing drawn of it yet.
pub fn frame_begin() {
    with(|s| {
        s.making = [None; N];
        s.making_natural = [None; N];
    });
}

/// A HUD frame made: what it drew is where the pieces are (`at`), and where
/// the game put them keeps the next frame's on the screen (`shift`). A piece
/// being dragged keeps where it was last seen, if it is not drawn now.
pub fn frame_end() {
    with(|s| {
        for i in 0..N {
            let dragged = s.drag.is_some_and(|d| d.piece.index() == i);
            if s.making[i].is_some() || !dragged {
                s.drawn[i] = s.making[i];
                s.natural[i] = s.making_natural[i].or(if dragged { s.natural[i] } else { None });
            }
        }
    });
}

/// Where `piece` was drawn in the last HUD frame (frame pixels).
#[cfg(test)]
pub fn drawn(piece: Piece) -> Option<[f32; 4]> {
    with(|s| s.drawn[piece.index()])
}

/// The pieces drawn in the last HUD frame and where.
pub fn drawn_all() -> Vec<(Piece, [f32; 4])> {
    with(|s| PIECES.iter().filter_map(|&p| s.drawn[p.index()].map(|r| (p, r))).collect())
}

/// How near a piece a press picks it up (frame pixels): a thin one (the
/// breath gauge) is still easy to take.
const REACH: f32 = 3.0;

/// The piece at frame point `p` while the HUD moves: of those drawn there
/// the smallest (the pet's bar inside a raid list's reach).
fn piece_at_in(s: &State, p: [f32; 2]) -> Option<Piece> {
    let inside = |r: [f32; 4]| p[0] >= r[0] - REACH && p[0] <= r[2] + REACH && p[1] >= r[1] - REACH && p[1] <= r[3] + REACH;
    PIECES
        .iter()
        .filter_map(|&k| s.drawn[k.index()].filter(|&r| inside(r)).map(|r| (k, (r[2] - r[0]) * (r[3] - r[1]))))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(k, _)| k)
}

/// What a press at frame point `p` on screen `b` took while the HUD moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// nothing: the press is the world's as ever
    None,
    /// a piece, now dragged
    Piece(Piece),
    /// a button of the bar's, done
    Button(Button),
}

/// A press at frame point `p` while the HUD moves: a button of the bar's is
/// done at once (Lock, Reset), a piece is picked up.
pub fn press(p: [f32; 2], b: Beyond) -> Press {
    let hit = with(|s| {
        if !s.unlocked {
            return Press::None;
        }
        if let Some(&(_, k)) = s.buttons.iter().find(|(r, _)| p[0] >= r[0] && p[0] <= r[2] && p[1] >= r[1] && p[1] <= r[3]) {
            return Press::Button(k);
        }
        let Some(piece) = piece_at_in(s, p) else { return Press::None };
        let shift = shift_in(s, piece, b);
        s.drag = Some(Drag { piece, from: p, shift });
        Press::Piece(piece)
    });
    if hit != Press::None {
        with(|s| s.pressed = true);
    }
    match hit {
        Press::Button(Button::Lock) => set_unlocked(false),
        Press::Button(Button::Reset) => reset(),
        _ => {}
    }
    hit
}

/// Whether a piece is being dragged.
pub fn dragging() -> bool {
    with(|s| s.drag.is_some())
}

/// The pointer at frame point `p` on screen `b`: the piece dragged goes
/// with it, kept on the screen.
pub fn drag_to(p: [f32; 2], b: Beyond) {
    with(|s| {
        let Some(d) = s.drag else { return };
        let i = d.piece.index();
        let want = [d.shift[0] + p[0] - d.from[0], d.shift[1] + p[1] - d.from[1]];
        let Some(n) = s.natural[i] else { return };
        let to = clamp(want, n, b);
        let rect = [n[0] + to[0], n[1] + to[1], n[2] + to[0], n[3] + to[1]];
        let place = (to != [0.0; 2]).then(|| Place::of(d.piece, to, rect, b));
        s.layout.set(d.piece, place);
    });
}

/// The press let go: a piece dragged stays where it is, kept. False when
/// Move HUD took no press (the release is the world's).
pub fn release() -> bool {
    let (pressed, dragged) = with(|s| (std::mem::take(&mut s.pressed), s.drag.take().is_some()));
    if dragged {
        keep();
    }
    pressed || dragged
}

/// The bar's buttons drawn over `r` (frame pixels) this frame, for `press`.
pub fn set_buttons(b: Vec<([f32; 4], Button)>) {
    with(|s| s.buttons = b);
}

/// The piece being dragged, if any.
pub fn dragged() -> Option<Piece> {
    with(|s| s.drag.map(|d| d.piece))
}

/// `/hud`: Move HUD turned (no word), on (`move`, `unlock`), off (`lock`), or
/// every piece back (`reset`); the line said in the chat box.
pub fn command(arg: &str) -> &'static str {
    match arg.trim().to_ascii_lowercase().as_str() {
        "" => {
            let on = !unlocked();
            set_unlocked(on);
            if on { MOVING } else { LOCKED }
        }
        "move" | "unlock" | "on" => {
            set_unlocked(true);
            MOVING
        }
        "lock" | "off" | "done" => {
            set_unlocked(false);
            LOCKED
        }
        "reset" => {
            reset();
            "The HUD is back where the game puts it."
        }
        _ => "Type /hud to move the HUD's pieces (drag them; /hud again to lock them), /hud reset to put them back.",
    }
}

const MOVING: &str = "Move HUD: drag the HUD's pieces where you want them; /hud or Lock at the top when done.";
const LOCKED: &str = "The HUD is locked.";

/// The words' height on the outlines and the bar (frame pixels).
const TEXT_H: f32 = 11.0;
const FILL: [f32; 4] = [1.0, 0.85, 0.25, 0.10];
const FILL_HELD: [f32; 4] = [1.0, 0.85, 0.25, 0.25];
const EDGE: [f32; 4] = [1.0, 0.85, 0.25, 0.9];
const WORDS: [f32; 4] = [1.0, 0.95, 0.7, 1.0];
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 0.85];
const BAR: [f32; 4] = [0.0, 0.0, 0.0, 0.7];
const BUTTON: [f32; 4] = [0.35, 0.3, 0.12, 0.95];

/// The bar's words.
const BAR_TEXT: &str = "Move HUD: drag a piece to move it";

/// Move HUD's look, over the HUD drawn this frame (`frame_end` first): each
/// piece's outline, a light fill (the one held fuller) and its name, and at
/// the top of the screen the bar with its two buttons, noted for `press`.
/// Placed and cut to the whole screen, past the frame's sides and above and
/// below it.
pub fn draw_moving(r: &mut crate::ui::UiRaster) {
    let (Some(white), Some(font)) = (r.assets.white, r.assets.font) else { return };
    let ui = &mut *r.ui;
    let (anchor, vertical, shift) = (ui.anchor, ui.vertical, ui.shift);
    ui.anchor = render::ui::Anchor::Middle;
    ui.vertical = render::ui::Vertical::Middle;
    ui.shift = [0.0; 2];
    let b = Beyond { side: ui.side(), top: ui.top() };
    let held = dragged();
    let fill = |ui: &mut Ui, r: [f32; 4], color: [f32; 4]| {
        ui.push_whole(render::ui::Quad { tex: white, blend: 3, pos: [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]], uv: [[0.5, 0.5]; 4], color, clip: None });
    };
    let scale = TEXT_H / ui.fonts[font].height.max(1) as f32;
    let width = |ui: &Ui, s: &str| ui.fonts[font].string_width(&crate::ui::wide(s)) as f32 * scale;
    let words = |ui: &mut Ui, x: f32, y: f32, s: &str, color: [f32; 4]| {
        let w = crate::ui::wide(s);
        let from = ui.quads.len();
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
            ui.text_scaled(font, x + dx, y + dy, scale, BLACK, &w);
        }
        ui.text_scaled(font, x, y, scale, color, &w);
        for q in &mut ui.quads[from..] {
            q.clip = Some(render::ui::WHOLE);
        }
    };
    for (piece, at) in drawn_all() {
        fill(ui, at, if held == Some(piece) { FILL_HELD } else { FILL });
        for e in [[at[0], at[1], at[2], at[1] + 1.0], [at[0], at[3] - 1.0, at[2], at[3]], [at[0], at[1], at[0] + 1.0, at[3]], [at[2] - 1.0, at[1], at[2], at[3]]] {
            fill(ui, e, EDGE);
        }
        words(ui, at[0] + 3.0, at[1] + 2.0, piece.label(), WORDS);
    }
    // the bar: its words, Reset and Lock, in the middle of the screen's top
    let pad = 4.0;
    let (lock_w, reset_w) = (width(ui, "Lock") + pad * 4.0, width(ui, "Reset") + pad * 4.0);
    let text_w = width(ui, BAR_TEXT);
    let w = pad * 3.0 + text_w + pad * 2.0 + reset_w + pad + lock_w;
    let h = TEXT_H + pad * 2.0;
    let (x0, y0) = (320.0 - w / 2.0, -b.top + 4.0);
    fill(ui, [x0, y0, x0 + w, y0 + h], BAR);
    words(ui, x0 + pad * 3.0, y0 + pad, BAR_TEXT, WORDS);
    let mut x = x0 + pad * 3.0 + text_w + pad * 2.0;
    let mut buttons = Vec::new();
    for (label, bw, k) in [("Reset", reset_w, Button::Reset), ("Lock", lock_w, Button::Lock)] {
        let rr = [x, y0 + 2.0, x + bw, y0 + h - 2.0];
        fill(ui, rr, BUTTON);
        words(ui, x + pad * 2.0, y0 + pad, label, [1.0; 4]);
        buttons.push((rr, k));
        x += bw + pad;
    }
    set_buttons(buttons);
    ui.anchor = anchor;
    ui.vertical = vertical;
    ui.shift = shift;
}

#[cfg(test)]
pub(crate) fn testing_reset() {
    STATE.with(|s| *s.borrow_mut() = State::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDE: Beyond = Beyond { side: 106.0, top: 0.0 };
    const TALL: Beyond = Beyond { side: 0.0, top: 85.0 };
    const FOUR: Beyond = Beyond { side: 0.0, top: 0.0 };

    /// A place keeps the distance from the side it was measured from: the
    /// chat box (the right's) put at the left of a wide screen is measured
    /// from the left, so a wider screen leaves it as far from the left, where
    /// a shift kept from its own side would carry it into the middle. The
    /// control: on the screen it was placed on, the shift is the drag's.
    #[test]
    fn a_place_keeps_its_distance_from_the_side_nearest_it() {
        let chat_natural = [200.0 + WIDE.side, 331.0, 640.0 + WIDE.side, 448.0];
        let to = [-400.0, -100.0];
        let rect = [chat_natural[0] + to[0], chat_natural[1] + to[1], chat_natural[2] + to[0], chat_natural[3] + to[1]];
        let at = Place::of(Piece::Chat, to, rect, WIDE);
        assert_eq!((at.h, at.v), (Edge::Start, Edge::Middle), "its middle is in the left third, the middle third");
        assert_eq!(at.shift(Piece::Chat, WIDE), to, "the control: where it was dropped");
        // a wider screen: the chat's own side (the right) went 50 out, the
        // piece stays as far from the left (50 out the other way)
        let wider = Beyond { side: 156.0, top: 0.0 };
        let s = at.shift(Piece::Chat, wider);
        let left = chat_natural[0] + 50.0 + s[0];
        assert_eq!(left - (-wider.side), rect[0] - (-WIDE.side), "as far from the left");
        // a piece of the left's at its own side: no matter the width, its shift is the drag's
        let p = Place::of(Piece::Player, [5.0, 5.0], [0.0, 20.0, 190.0, 80.0], WIDE);
        assert_eq!(p.h, Edge::Start);
        assert_eq!(p.shift(Piece::Player, wider), [5.0, 5.0]);
        assert_eq!(p.shift(Piece::Player, TALL), [5.0, 5.0]);
    }

    /// A piece is kept on the screen, whole pixels; a window made smaller
    /// brings it in. The control: one already on the screen is left alone.
    #[test]
    fn a_piece_is_kept_on_the_screen() {
        let n = [10.0, 15.0, 200.0, 80.0];
        assert_eq!(clamp([20.4, 30.6], n, FOUR), [20.0, 31.0], "the control: on the screen");
        assert_eq!(clamp([600.0, 0.0], n, FOUR), [440.0, 0.0], "its right at the screen's");
        assert_eq!(clamp([-50.0, -40.0], n, FOUR), [-10.0, -15.0], "its left and top at the screen's");
        assert_eq!(clamp([-200.0, 0.0], n, WIDE), [-116.0, 0.0], "a wide screen's left is past the frame's");
        assert_eq!(clamp([0.0, 500.0], n, TALL), [0.0, 448.0 + 85.0 - 80.0], "a tall screen's bottom");
        // larger than the screen: its left and top on it
        assert_eq!(clamp([30.0, 0.0], [0.0, 0.0, 700.0, 10.0], FOUR), [0.0, 0.0]);
    }

    /// What `hud.opt` keeps reads back the same; a line that does not read
    /// leaves its piece where the game puts it. The control: an empty file is
    /// every piece unmoved.
    #[test]
    fn the_layout_reads_back_what_it_kept() {
        assert_eq!(Layout::from_text(""), Layout::default(), "the control");
        let mut l = Layout::default();
        l.set(Piece::Chat, Some(Place { h: Edge::Start, dx: -12.5, v: Edge::Middle, dy: 40.0 }));
        l.set(Piece::Minimap, Some(Place { h: Edge::End, dx: 3.0, v: Edge::End, dy: -7.0 }));
        let text = l.to_text();
        assert_eq!(text, "minimap r 3 b -7\nchat l -12.5 m 40\n");
        assert_eq!(Layout::from_text(&text), l);
        let odd = "chat l -12.5 m 40\nnothing r 1 t 1\npet x 1 t 1\npet l 1 t\npet l nan t 1\npet l 1e9 t 1\ngroup l 1 t 1 more\n";
        let back = Layout::from_text(odd);
        assert_eq!(back.get(Piece::Chat), l.get(Piece::Chat));
        assert!(PIECES.iter().filter(|&&p| p != Piece::Chat).all(|&p| back.get(p).is_none()), "the rest unmoved: {back:?}");
    }

    /// Move HUD on: a press on a piece drawn picks it up (the smallest under
    /// it), the drag moves it (kept on the screen), the release keeps it; the
    /// next frame draws it there. Reset puts it back. Locked, a press is the
    /// world's. The control: a press off every piece takes nothing.
    #[test]
    fn a_piece_is_dragged_kept_and_reset() {
        testing_reset();
        crate::settings::testing::reset(false);
        let frame = |shift_of: &dyn Fn(Piece) -> [f32; 2]| {
            frame_begin();
            for (p, r) in [(Piece::Player, [10.0, 15.0, 200.0, 80.0]), (Piece::Chat, [200.0, 331.0, 640.0, 448.0]), (Piece::Pet, [20.0, 60.0, 194.0, 106.0])] {
                let s = shift_of(p);
                note(p, [r[0] + s[0], r[1] + s[1], r[2] + s[0], r[3] + s[1]], s);
            }
            frame_end();
        };
        frame(&|_| [0.0; 2]);
        assert_eq!(press([100.0, 40.0], FOUR), Press::None, "locked: the world's");
        set_unlocked(true);
        assert_eq!(press([5.0, 300.0], FOUR), Press::None, "the control: off every piece");
        assert_eq!(press([100.0, 70.0], FOUR), Press::Piece(Piece::Pet), "the pet's bar over the frame: the smaller");
        assert!(release());
        assert_eq!(press([300.0, 400.0], FOUR), Press::Piece(Piece::Chat));
        drag_to([100.0, 300.0], FOUR);
        assert_eq!(shift(Piece::Chat, FOUR), [-200.0, -100.0]);
        // past the screen's left: kept on it
        drag_to([-500.0, 300.0], FOUR);
        assert_eq!(shift(Piece::Chat, FOUR), [-200.0, -100.0]);
        drag_to([0.0, 600.0], FOUR);
        assert_eq!(shift(Piece::Chat, FOUR), [-200.0, 0.0], "its bottom at the screen's");
        assert!(release());
        assert!(!dragging() && !release());
        assert!(crate::settings::hud_layout().starts_with("chat "), "kept: {:?}", crate::settings::hud_layout());
        // read again (the next start): the same place
        testing_reset();
        assert_eq!(shift(Piece::Chat, FOUR), [-200.0, 0.0], "read back");
        frame(&|p| shift(p, FOUR));
        assert_eq!(drawn(Piece::Chat), Some([0.0, 331.0, 440.0, 448.0]), "drawn there");
        // the bar's Reset
        set_unlocked(true);
        set_buttons(vec![([270.0, 2.0, 320.0, 16.0], Button::Lock), ([330.0, 2.0, 380.0, 16.0], Button::Reset)]);
        assert_eq!(press([350.0, 8.0], FOUR), Press::Button(Button::Reset));
        assert_eq!(layout(), Layout::default());
        assert_eq!(crate::settings::hud_layout(), "", "kept so");
        assert_eq!(press([300.0, 8.0], FOUR), Press::Button(Button::Lock));
        assert!(!unlocked());
    }

    /// `/hud`'s words.
    #[test]
    fn the_hud_command() {
        testing_reset();
        crate::settings::testing::reset(false);
        command("");
        assert!(unlocked(), "turned on");
        command("");
        assert!(!unlocked(), "and off");
        command("move");
        assert!(unlocked());
        command("LOCK");
        assert!(!unlocked());
        let mut l = Layout::default();
        l.set(Piece::Pet, Some(Place { h: Edge::Start, dx: 1.0, v: Edge::Start, dy: 1.0 }));
        with(|s| s.layout = l);
        command("reset");
        assert_eq!(layout(), Layout::default());
        assert!(command("what").starts_with("Type /hud"));
    }
}
