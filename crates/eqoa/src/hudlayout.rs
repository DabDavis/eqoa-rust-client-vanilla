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
//! - Sized from a corner's grip while the HUD moves: every piece in the PC
//!   interface, the minimap alone in the Controller's (Heirloom petition
//!   3065), each interface's sizes in its own layout (`sizable`).

use render::ui::{HudScale, SCREEN_H, SCREEN_W, Ui};
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
    Chat1,
    Chat2,
    Chat3,
    Settings,
    /// the breath gauge, under water
    Breath,
    /// the quick menus (`hudmenu`)
    QuickMenus,
    /// the Stacked Spell Bars (`hudmenu::draw_stacked`): over the chat box
    /// unmoved, but a piece of their own (a GM's petition 5229: they moved
    /// with the chat box)
    SpellBars,
    /// the Utility Bars (`hudmenu::HudMenus::draw_utility`: the item quick
    /// slots and the weapon sets, while the spell bars are stacked), at the
    /// upper left or on the chat box as Utility Bar Layout has them
    UtilityBars,
    /// The PC weapon-set bar, with its own placement and pin.
    EquipmentBar,
    /// The details window opened by inspecting a target.
    TargetInfo,
    /// Loot rolls and ready checks share a stack of non-modal raid prompts.
    RaidPrompts,
    /// The Stat Window (`statwnd`), at the left.
    Stats,
    /// The focus target's window (`focus`), above the target box.
    Focus,
}

/// Each piece, in `Piece`'s order.
pub const PIECES: [Piece; 22] = [
    Piece::Player, Piece::Group, Piece::Pet, Piece::Target, Piece::Minimap, Piece::Quests,
    Piece::Meter, Piece::Effects, Piece::Chat, Piece::Chat1, Piece::Chat2, Piece::Chat3,
    Piece::Settings, Piece::Breath, Piece::QuickMenus, Piece::SpellBars, Piece::UtilityBars,
    Piece::EquipmentBar, Piece::TargetInfo, Piece::RaidPrompts, Piece::Stats, Piece::Focus,
];
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
    fn chat_tab(self) -> Option<usize> {
        match self {
            Piece::Chat => Some(0),
            Piece::Chat1 => Some(1),
            Piece::Chat2 => Some(2),
            Piece::Chat3 => Some(3),
            _ => None,
        }
    }

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
            Piece::Chat1 => "chat1",
            Piece::Chat2 => "chat2",
            Piece::Chat3 => "chat3",
            Piece::Settings => "settings",
            Piece::Breath => "breath",
            Piece::QuickMenus => "quickbars",
            Piece::SpellBars => "spellbars",
            Piece::UtilityBars => "utilitybars",
            Piece::EquipmentBar => "equipment",
            Piece::TargetInfo => "targetinfo",
            Piece::RaidPrompts => "raidprompts",
            Piece::Stats => "stats",
            Piece::Focus => "focus",
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
            Piece::Chat1 => "Chat Tab 1",
            Piece::Chat2 => "Chat Tab 2",
            Piece::Chat3 => "Chat Tab 3",
            Piece::Settings => "Settings",
            Piece::Breath => "Breath",
            Piece::QuickMenus => "Quick Bars",
            Piece::SpellBars => "Spell Bars",
            Piece::UtilityBars => "Utility Bars",
            Piece::EquipmentBar => "Equipment Switch",
            Piece::TargetInfo => "Target Details",
            Piece::RaidPrompts => "Raid Prompts",
            Piece::Stats => "Stat Window",
            Piece::Focus => "Focus Target",
        }
    }

    /// The sides the game keeps it at (`Hud::draw`, `Play::draw_hud`):
    /// across a wider window and up or down a taller one.
    pub fn own(self) -> (Edge, Edge) {
        match self {
            Piece::Player | Piece::Group | Piece::Pet | Piece::Effects => (Edge::Start, Edge::Start),
            Piece::Target | Piece::Focus => (Edge::Start, Edge::End),
            Piece::Stats => (Edge::Start, Edge::Middle),
            Piece::TargetInfo => (Edge::End, Edge::Middle),
            Piece::RaidPrompts => (Edge::Middle, Edge::Middle),
            Piece::Chat | Piece::Chat1 | Piece::Chat2 | Piece::Chat3 | Piece::Settings | Piece::SpellBars | Piece::EquipmentBar => (Edge::End, Edge::End),
            Piece::Minimap | Piece::Quests | Piece::Meter | Piece::Breath | Piece::QuickMenus => (Edge::End, Edge::Start),
            // the side Utility Bar Layout draws them at (`Play::draw_hud`)
            Piece::UtilityBars => match crate::settings::utility_layout() {
                crate::hudmenu::UtilityLayout::UpperLeft => (Edge::Start, Edge::Start),
                crate::hudmenu::UtilityLayout::Below => (Edge::End, Edge::End),
            },
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub places: [Option<Place>; N],
    pinned: [bool; N],
    percent: [u16; N],
}

impl Default for Layout {
    fn default() -> Self {
        Self { places: [None; N], pinned: [false; N], percent: [100; N] }
    }
}

fn size_step(percent: u16) -> u16 {
    (percent.clamp(50, 200) + 2) / 5 * 5
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
            if self.pinned[p.index()] { s.push_str(&format!("pin {}\n", p.key())); }
            if self.percent[p.index()] != 100 {
                s.push_str(&format!("size {} {}\n", p.key(), self.percent[p.index()]));
            }
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
            if it.clone().next() == Some("size") {
                it.next();
                if let (Some(name), Some(value), None) = (it.next(), it.next(), it.next())
                    && let Some(piece) = PIECES.into_iter().find(|p| p.key() == name)
                    && let Ok(percent @ 50..=200) = value.parse::<u16>()
                {
                    l.percent[piece.index()] = size_step(percent);
                }
                continue;
            }
            if it.clone().next() == Some("pin") {
                it.next();
                if let Some(name) = it.next() && it.next().is_none()
                    && let Some(piece) = PIECES.into_iter().find(|p| p.key() == name) {
                    l.pinned[piece.index()] = true;
                }
                continue;
            }
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
    kind: DragKind,
}

#[derive(Clone, Copy, Debug)]
enum DragKind {
    Move,
    Chat(ChatResize),
    Scale { rect: [f32; 4], direction: [f32; 2], percent: u16, place: Option<Place> },
}

#[derive(Clone, Copy)]
struct ScaleGrip {
    piece: Piece,
    rect: [f32; 4],
    direction: [f32; 2],
}

#[derive(Clone, Copy, Debug)]
struct ChatResize {
    initial: [u8; 2],
    next: [u8; 2],
    /// Screen distance per percentage point of width and per row of height.
    unit: [f32; 2],
    /// A resize by the upper-left handle holds the opposite corner still,
    /// even when the PC preset originally aligned the box to the left edge.
    place: Option<Place>,
}

impl ChatResize {
    fn delta(self) -> [f32; 2] {
        std::array::from_fn(|i| (self.next[i] as f32 - self.initial[i] as f32) * self.unit[i])
    }

    fn update(&mut self, dx: f32, dy: f32) {
        let width = self.initial[0] as f32 + dx / self.unit[0];
        self.next = [(width / 5.0).round().clamp(10.0, 25.0) as u8 * 5,
            (self.initial[1] as f32 + dy / self.unit[1]).round().clamp(3.0, 12.0) as u8];
    }
}

/// What the bar at the top while the HUD moves has: its buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Lock,
    Reset,
    Pin(Piece),
}

#[derive(Default)]
struct State {
    /// the window's place the layout was read for (`multibox`)
    read_for: Option<(usize, bool)>,
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
    /// Visible chat resize handles and the screen height of one row.
    chat_grips: Vec<(Piece, [f32; 4], f32)>,
    scale_grips: Vec<ScaleGrip>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let me = crate::settings::hud_cache_key();
        if s.read_for != Some(me) {
            *s = State { read_for: Some(me), layout: Layout::from_text(&crate::settings::hud_layout()), ..State::default() };
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

/// Whether `piece` has a size of its own in this interface: every piece in the
/// PC's, the minimap alone in the Controller's (Heirloom petition 3065: "allow
/// the controller mini-map to be able to have its size increased/decreased the
/// same way"), each interface's kept in its own layout (`hud` or `hud-pc`).
fn sizable(pc: bool, piece: Piece) -> bool {
    pc || piece == Piece::Minimap
}

/// An individual piece's size relative to the player's global HUD scale
/// (`sizable`; 100 for one that has none in this interface).
pub fn piece_scale_percent(piece: Piece) -> u16 {
    with(|s| if sizable(s.read_for.is_some_and(|(_, pc)| pc), piece) { s.layout.percent[piece.index()] } else { 100 })
}

/// Used by the Spell Bar Size option. Its center stays put across edits even
/// for a manually placed bar, which is drawn from the retail bottom-right corner.
pub fn set_piece_scale_percent(piece: Piece, percent: u16) {
    let changed = with(|s| {
        let i = piece.index();
        let next = size_step(percent);
        if !sizable(s.read_for.is_some_and(|(_, pc)| pc), piece) || s.layout.percent[i] == next { return false; }
        if piece == Piece::SpellBars && let Some(pos) = s.layout.places[i].as_mut() {
            let global = crate::settings::client().display_ui().hud_scale as f32 / 100.0;
            pos.dx += (SCREEN_W / 2.0) * global * (f32::from(next) - f32::from(s.layout.percent[i])) / 100.0;
        }
        s.layout.percent[i] = next;
        s.natural[i] = None;
        true
    });
    if changed { keep(); }
}

/// Move HUD on or off; off ends a drag (keeping where it got to).
pub fn set_unlocked(on: bool) {
    let ended = with(|s| {
        s.unlocked = on;
        if on { None } else { s.drag.take() }
    });
    finish_drag(ended);
}

/// Every piece where the game puts it, kept so.
pub fn reset() {
    with(|s| {
        s.layout = Layout::default();
        s.drag = None;
    });
    keep();
}

/// One piece where the game (or the client, for its own) puts it, kept so:
/// Utility Bar Layout turned puts the Utility Bars where it has them.
pub fn reset_piece(piece: Piece) {
    with(|s| {
        s.layout.set(piece, None);
        if s.drag.is_some_and(|d| d.piece == piece) {
            s.drag = None;
        }
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
    let raw = match s.layout.get(piece) {
        Some(at) => at.shift(piece, b),
        None if s.read_for.is_some_and(|(_, pc)| pc) => match pc_offset(piece, b, &s.layout) {
            Some(offset) => offset,
            None if s.layout.percent[piece.index()] != 100 => [0.0; 2],
            None => return [0.0; 2],
        },
        // At the original Controller positions the minimap's roller ends
        // exactly where the carousel's vertical frame starts (x = 589).
        None if piece == Piece::Minimap && crate::settings::minimap_scroll()
            && s.layout.get(Piece::QuickMenus).is_none() && s.drawn[Piece::QuickMenus.index()].is_some() => [-4.0, 0.0],
        None => return [0.0; 2],
    };
    match s.natural[piece.index()] {
        Some(n) if s.read_for.is_some_and(|(_, pc)| pc) && s.layout.get(piece).is_none() && matches!(piece, Piece::Chat1 | Piece::Chat2 | Piece::Chat3) => {
            place_detached(s, piece, b, n, raw)
        }
        Some(n) => clamp(raw, n, b),
        None => [raw[0].round(), raw[1].round()],
    }
}

/// An unplaced pane keeps its preferred corner when there is room. Otherwise
/// try the edges of the visible HUD pieces. Earlier detached panes take
/// priority so two panes never keep stepping around each other across frames.
fn place_detached(s: &State, piece: Piece, b: Beyond, n: [f32; 4], preferred: [f32; 2]) -> [f32; 2] {
    let mut obstacles = s.drawn;
    for p in [Piece::Chat1, Piece::Chat2, Piece::Chat3] {
        if p == piece || (p.index() > piece.index() && s.layout.get(p).is_none()) {
            obstacles[p.index()] = None;
        }
    }
    let gap = 4.0;
    let covered = |[x, y]: [f32; 2]| -> f32 { obstacles.iter().flatten().map(|r| {
        let w = (n[2] + x).min(r[2] + gap) - (n[0] + x).max(r[0] - gap);
        let h = (n[3] + y).min(r[3] + gap) - (n[1] + y).max(r[1] - gap);
        w.max(0.0) * h.max(0.0)
    }).sum() };
    let wanted = clamp(preferred, n, b);
    let mut cover = covered(wanted);
    if cover == 0.0 { return wanted; }
    let screen = b.screen();
    let mut best = wanted;
    let mut distance = 0.0;
    for r in obstacles.iter().flatten() {
        for x in [wanted[0], screen[0] - n[0] + gap, screen[2] - n[2] - gap, r[2] - n[0] + gap + 1.0, r[0] - n[2] - gap - 1.0] {
            for y in [wanted[1], r[3] - n[1] + gap + 1.0, r[1] - n[3] - gap - 1.0] {
                let at = clamp([x, y], n, b);
                let d = (at[0] - wanted[0]).abs() + (at[1] - wanted[1]).abs();
                if cover == 0.0 && d >= distance { continue; }
                let overlap = covered(at);
                if overlap < cover || (overlap == cover && d < distance) {
                    best = at;
                    cover = overlap;
                    distance = d;
                }
            }
        }
    }
    best
}

/// The PC action bars are centered, the target above them and chat in two
/// columns at the sides. Lift main chat clear of the bars on narrower displays.
fn pc_offset(piece: Piece, b: Beyond, layout: &Layout) -> Option<[f32; 2]> {
    let c = crate::settings::client();
    let k = c.hud_scale as f32 * 0.01;
    let factor = |p: Piece| k * f32::from(layout.percent[p.index()]) / 100.0;
    let spell = factor(Piece::SpellBars);
    let chat = factor(Piece::Chat);
    let width = SCREEN_W + 2.0 * b.side;
    let chat_width = (crate::chat::ChatSize::BASE_WIDTH as f32 * c.chat_width as f32 / 100.0 * chat).min(width);
    let side_room = (width - 340.0 * spell) * 0.5;
    // Reserve one extra row for a character's packed special abilities.
    let spell_height = (29.0 * (c.pc_action_rows.clamp(1, 4) as f32 + 1.0) + 8.0) * spell;
    let main_y = if chat_width + 10.0 > side_room { -spell_height } else { -4.0 };
    let left = -width + chat_width - 25.0 * chat + 8.0;
    let pane_height = (c.chat_rows as f32 * 17.0 + 76.0) * chat;
    let gear_lift = if 171.0 * factor(Piece::EquipmentBar) + 8.0 > side_room { (spell_height - 40.0 * k).max(0.0) } else { 0.0 };
    let right_y = -122.0 * k - gear_lift;
    Some(match piece {
        // Beside the group, with room for two raid columns at the default size.
        Piece::Pet => [212.0 * k, 0.0],
        Piece::SpellBars => [-b.side - 320.0 * (1.0 - spell), -3.0],
        Piece::Target => [320.0 + b.side - 107.0 * factor(piece), 18.0 - SCREEN_H - 2.0 * b.top + 100.0 * factor(piece)],
        Piece::Chat => [left, main_y],
        Piece::Chat1 => {
            let [pane_width, rows] = c.chat_pane_sizes[0].unwrap_or([c.chat_width, c.chat_rows]);
            let height = (rows as f32 * 17.0 + 76.0) * factor(piece);
            // Main chat rises above four spell bars at 4:3. When that leaves
            // too little space below the player frame, use the other column.
            if SCREEN_H + 2.0 * b.top + main_y - pane_height - height < 90.0 * k {
                return Some([-8.0, right_y]);
            }
            let dx = (crate::chat::ChatSize::BASE_WIDTH as f32 * pane_width as f32 / 100.0 * factor(piece)).min(width) - chat_width;
            [left + dx, main_y - pane_height]
        }
        Piece::Chat2 => [-8.0, right_y],
        Piece::Chat3 => [-8.0, right_y - (c.chat_pane_sizes[1].map_or(c.chat_rows, |s| s[1]) as f32 * 17.0 + 76.0) * factor(Piece::Chat2)],
        Piece::EquipmentBar => [-8.0, 94.0 * factor(piece) - gear_lift],
        Piece::UtilityBars if c.utility_layout == crate::hudmenu::UtilityLayout::Below => [-8.0, 94.0 * factor(piece) - gear_lift],
        _ => return None,
    })
}

/// The piece's frame corner and how far widescreen or portrait moves that corner.
fn anchor(piece: Piece, b: Beyond) -> ([f32; 2], [f32; 2]) {
    let (h, v) = piece.own();
    let edge = |e, span| match e { Edge::Start => 0.0, Edge::Middle => span * 0.5, Edge::End => span };
    ([edge(h, SCREEN_W), edge(v, SCREEN_H)], [h.reference(b.side), v.reference(b.top)])
}

/// The HUD Scale (Interface Size, either interface; the Controller's since
/// petition 2912) times the piece's own size (the PC's), about the piece's
/// own corner. The Controller interface draws the raid prompts and the target
/// info as windows, outside `begin` (`Play`'s frame), so they are not scaled
/// there and their hit tests (`local_point`) must not be either.
pub fn scale(piece: Piece, b: Beyond) -> HudScale {
    let (corner, offset) = anchor(piece, b);
    let windowed = matches!(piece, Piece::RaidPrompts | Piece::TargetInfo) && !crate::settings::pc_mode();
    let global = if windowed { 1.0 } else { crate::settings::client().display_ui().hud_scale as f32 / 100.0 };
    HudScale { origin: [corner[0] + offset[0], corner[1] + offset[1]], factor: global * (f32::from(piece_scale_percent(piece)) / 100.0) }
}

/// A pointer in frame pixels, expressed in the piece's original drawing coordinates.
pub fn local_point(piece: Piece, p: [f32; 2], b: Beyond) -> [f32; 2] {
    let moved = shift(piece, b);
    let (_, offset) = anchor(piece, b);
    let q = scale(piece, b).inverse([p[0] - moved[0], p[1] - moved[1]]);
    [q[0] - offset[0], q[1] - offset[1]]
}

/// Where a piece's drawing coordinates land, including its saved move. Used by
/// layers drawn outside the main HUD pass.
pub fn frame_point(piece: Piece, p: [f32; 2], b: Beyond) -> [f32; 2] {
    let moved = shift(piece, b);
    let (_, offset) = anchor(piece, b);
    let q = scale(piece, b).point([p[0] + offset[0], p[1] + offset[1]]);
    [q[0] + moved[0], q[1] + moved[1]]
}

/// A piece begun on `ui` (`end`): what it draws is scaled and moved together.
pub struct Mark {
    piece: Piece,
    from: usize,
    shift: [f32; 2],
    before: [f32; 2],
    before_scale: HudScale,
}

/// `piece` is drawn next on `ui`, moved where the player put it.
pub fn begin(ui: &mut Ui, piece: Piece) -> Mark {
    let b = Beyond { side: ui.side(), top: ui.top() };
    let shift = shift(piece, b);
    let before = std::mem::replace(&mut ui.shift, shift);
    let before_scale = std::mem::replace(&mut ui.hud_scale, scale(piece, b));
    Mark { piece, from: ui.quads.len(), shift, before, before_scale }
}

/// The piece `m` drawn: the rectangle its quads cover noted (beside what the
/// frame noted of it already) and `ui`'s shift as it was.
pub fn end(ui: &mut Ui, m: Mark) {
    ui.shift = m.before;
    ui.hud_scale = m.before_scale;
    if let Some(r) = covered(&ui.quads[m.from.min(ui.quads.len())..]) {
        note(m.piece, r, m.shift);
    }
}

/// The piece `m` drawn so far noted as `end` notes it, and the mark for what
/// follows: drawn at the piece's shift but not counted in it, as the name over
/// the stacked bars, which may run past the screen's side. Counted, `clamp`
/// moved the whole block in from the edge it was placed against while such a
/// name showed (petition 2547, "UI Stacked shifting when placed against the
/// edge - Shifts when you select a spell that would show its name off the
/// screen"). End the mark it gives with `end`.
pub fn seal(ui: &mut Ui, m: Mark) -> Mark {
    if let Some(r) = covered(&ui.quads[m.from.min(ui.quads.len())..]) {
        note(m.piece, r, m.shift);
    }
    Mark { from: usize::MAX, ..m }
}

/// The screen's left and right as the piece of `m` draws (its frame pixels
/// before its shift), whole pixels within it.
pub fn screen_x(ui: &Ui, m: &Mark) -> [i32; 2] {
    let b = Beyond { side: ui.side(), top: ui.top() };
    let s = b.screen();
    [local_point(m.piece, [s[0], 0.0], b)[0].ceil() as i32, local_point(m.piece, [s[2], 0.0], b)[0].floor() as i32]
}

/// The screen as the piece of `m` draws (`screen_x`, and its top and
/// bottom): left, top, right, bottom, whole pixels within it.
pub fn screen_rect(ui: &Ui, m: &Mark) -> [i32; 4] {
    let b = Beyond { side: ui.side(), top: ui.top() };
    let s = b.screen();
    let (a, z) = (local_point(m.piece, [s[0], s[1]], b), local_point(m.piece, [s[2], s[3]], b));
    [a[0].ceil() as i32, a[1].ceil() as i32, z[0].floor() as i32, z[1].floor() as i32]
}

/// The frame rectangle `quads` show: each one's bounds within its cut, a quad
/// cut to nothing left out. A name scrolled in its box (`TextScroll`: the quick
/// bar's selected ability, 158 pixels wide) is drawn whole and cut to the box;
/// its whole length counted, a long name ran the piece off the screen's side
/// and `clamp` moved the bar with every step of the scroll (the server owner's
/// skeleton shadow knight, 2026-11-11: "a long spell name is making the spell
/// bar move from left to right across the screen").
fn covered(quads: &[render::ui::Quad]) -> Option<[f32; 4]> {
    let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for q in quads {
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for p in &q.pos {
            b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])];
        }
        if let Some([x0, y0, x1, y1]) = q.clip {
            b = [b[0].max(x0 as f32), b[1].max(y0 as f32), b[2].min(x1 as f32), b[3].min(y1 as f32)];
        }
        if b[0] <= b[2] && b[1] <= b[3] {
            r = [r[0].min(b[0]), r[1].min(b[1]), r[2].max(b[2]), r[3].max(b[3])];
        }
    }
    (r[0] <= r[2]).then_some(r)
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

/// A detached pane can acquire a new automatic position as neighboring pieces
/// change. Start an edit at the position the player actually saw and clicked.
fn drawn_shift(s: &State, piece: Piece, b: Beyond) -> [f32; 2] {
    let i = piece.index();
    match (s.drawn[i], s.natural[i]) {
        (Some(rect), Some(natural)) => [rect[0] - natural[0], rect[1] - natural[1]],
        _ => shift_in(s, piece, b),
    }
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
        if let Some(&(_, k)) = s.buttons.iter().rev().find(|(r, _)| p[0] >= r[0] && p[0] <= r[2] && p[1] >= r[1] && p[1] <= r[3]) {
            return Press::Button(k);
        }
        if let Some(&(piece, _, row)) = s.chat_grips.iter().rev().find(|(piece, r, _)| !s.layout.pinned[piece.index()]
            && p[0] >= r[0] && p[0] < r[2] && p[1] >= r[1] && p[1] < r[3])
            && let Some(tab) = piece.chat_tab()
        {
            let c = crate::settings::client().display_ui();
            let (w, h) = crate::settings::chat_pane_size(tab);
            let initial = [w, h];
            let width = crate::chat::ChatSize::BASE_WIDTH as f32 * c.hud_scale as f32 / 10_000.0
                * (f32::from(s.layout.percent[piece.index()]) / 100.0);
            let place = s.drawn[piece.index()].map(|rect| Place::of(piece, drawn_shift(s, piece, b), rect, b));
            let resize = ChatResize { initial, next: initial, unit: [width, row], place };
            s.drag = Some(Drag { piece, from: p, shift: [0.0; 2], kind: DragKind::Chat(resize) });
            return Press::Piece(piece);
        }
        if let Some(&grip) = s.scale_grips.iter().rev().find(|g| {
            let [l, t, r, b] = g.rect;
            !s.layout.pinned[g.piece.index()] && l <= p[0] && p[0] < r && t <= p[1] && p[1] < b
        }) && let Some(rect) = s.drawn[grip.piece.index()] {
            let piece = grip.piece;
            let kind = DragKind::Scale { rect, direction: grip.direction, percent: s.layout.percent[piece.index()], place: s.layout.get(piece) };
            s.drag = Some(Drag { piece, from: p, shift: drawn_shift(s, piece, b), kind });
            return Press::Piece(piece);
        }
        let Some(piece) = piece_at_in(s, p) else { return Press::None };
        if s.layout.pinned[piece.index()] { return Press::Piece(piece); }
        let shift = drawn_shift(s, piece, b);
        s.drag = Some(Drag { piece, from: p, shift, kind: DragKind::Move });
        Press::Piece(piece)
    });
    if hit != Press::None {
        with(|s| s.pressed = true);
    }
    match hit {
        Press::Button(Button::Lock) => set_unlocked(false),
        Press::Button(Button::Reset) => reset(),
        Press::Button(Button::Pin(p)) => {
            with(|s| {
                let pin = !s.layout.pinned[p.index()];
                if pin && s.layout.get(p).is_none() && let Some(rect) = s.drawn[p.index()] {
                    let at = Place::of(p, drawn_shift(s, p, b), rect, b);
                    s.layout.set(p, Some(at));
                }
                s.layout.pinned[p.index()] = pin;
            });
            keep();
        }
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
        match d.kind {
            DragKind::Chat(mut resize) => {
                resize.update(d.from[0] - p[0], d.from[1] - p[1]);
                s.drag = Some(Drag { kind: DragKind::Chat(resize), ..d });
                return;
            }
            DragKind::Scale { rect, direction, percent, place } => {
                let deltas: [f32; 2] = std::array::from_fn(|a| {
                    direction[a] * (p[a] - d.from[a]) / (rect[a+2] - rect[a]).max(32.0)
                });
                let delta = if deltas[0].abs() >= deltas[1].abs() { deltas[0] } else { deltas[1] };
                let next = size_step((f32::from(percent) * (1.0 + delta)).round() as u16);
                if s.layout.percent[i] == next { return; }
                s.layout.percent[i] = next;
                // Reproject the original bounds, not the last mouse event, so
                // rounding never accumulates. Keep the opposite corner stationary.
                let (corner, offset) = anchor(d.piece, b);
                let ratio = f32::from(next) / f32::from(percent);
                let natural: [f32; 4] = std::array::from_fn(|j| {
                    let a = j % 2;
                    let origin = corner[a] + offset[a];
                    origin + (rect[j] - d.shift[a] - origin) * ratio
                });
                s.natural[i] = Some(natural);
                if next == percent { s.layout.set(d.piece, place); return; }
                let want = std::array::from_fn(|a| {
                    let fixed = if direction[a] > 0.0 { a } else { a + 2 };
                    rect[fixed] - natural[fixed]
                });
                let to = clamp(want, natural, b);
                let drawn = std::array::from_fn(|j| natural[j] + to[j % 2]);
                s.layout.set(d.piece, Some(Place::of(d.piece, to, drawn, b)));
                return;
            }
            DragKind::Move => {}
        }
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
    let (pressed, drag) = with(|s| (std::mem::take(&mut s.pressed), s.drag.take()));
    finish_drag(drag);
    pressed || drag.is_some()
}

fn finish_drag(drag: Option<Drag>) {
    let Some(d) = drag else { return };
    if let DragKind::Chat(size) = d.kind {
        if size.initial != size.next {
            if let Some(place) = size.place {
                with(|s| s.layout.set(d.piece, Some(place)));
                keep();
            }
            if let Some(tab) = d.piece.chat_tab() {
                crate::settings::keep_chat_pane_size(tab, size.next[0], size.next[1]);
            }
        }
    } else {
        keep();
    }
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
pub fn command(arg: &str) -> String {
    // the profiles (petition 2947): /hud save|load|delete <name>, /hud list
    let (word, name) = arg.trim().split_once(char::is_whitespace).map_or((arg.trim(), ""), |(w, n)| (w, n.trim()));
    match word.to_ascii_lowercase().as_str() {
        "save" => return save_profile(name),
        "load" => return load_profile(name),
        "delete" | "remove" => return delete_profile(name),
        "list" | "profiles" => return list_profiles(),
        _ => {}
    }
    command_word(arg).to_string()
}

/// `/hud`'s words that are not a profile's.
fn command_word(arg: &str) -> &'static str {
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
        _ => "Type /hud to move the HUD's pieces (drag them; /hud again to lock them), /hud reset to put them back; /hud save, load or delete <name> and /hud list for named layouts.",
    }
}

// ---- named layouts (Heirloom petition 2947) --------------------------------

/// Named layouts at most, and a name's longest (characters).
pub const PROFILES_MOST: usize = 20;
const PROFILE_NAME_MOST: usize = 24;

/// The profiles file read (`settings::hud_profiles`): each `profile <name>`
/// line, then the layout's lines (`Layout::to_text`, and `hudscale <n>`) up
/// to the next; in the order they were first saved.
fn profiles() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in crate::settings::hud_profiles().lines() {
        if let Some(name) = line.strip_prefix("profile ") {
            out.push((name.trim().to_string(), String::new()));
        } else if let Some((_, body)) = out.last_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    out
}

fn keep_profiles(list: &[(String, String)]) -> bool {
    let text: String = list.iter().map(|(n, b)| format!("profile {n}\n{b}")).collect();
    crate::settings::save_hud_profiles(&text)
}

/// A profile's name as typed: its words, one space between, letters,
/// digits, `-` and `_`; none when empty, too long or holding anything else.
fn profile_name(name: &str) -> Option<String> {
    let n = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let ok = !n.is_empty() && n.chars().count() <= PROFILE_NAME_MOST && n.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_'));
    ok.then_some(n)
}

/// The profile of `name` (any case) in `list`.
fn find(list: &[(String, String)], name: &str) -> Option<usize> {
    list.iter().position(|(n, _)| n.eq_ignore_ascii_case(name))
}

/// The names kept, for the options' list.
pub fn profile_names() -> Vec<String> {
    profiles().into_iter().map(|(n, _)| n).collect()
}

/// The HUD as it is now (every piece's place and size, and the interface's
/// HUD Scale) kept under `name`, over one of the name.
pub fn save_profile(name: &str) -> String {
    let Some(name) = profile_name(name) else {
        return format!("Name the layout: /hud save <name> (letters, digits, - and _, {PROFILE_NAME_MOST} at most).");
    };
    let body = format!("{}hudscale {}\n", with(|s| s.layout.to_text()), crate::settings::hud_scale_percent());
    let mut list = profiles();
    match find(&list, &name) {
        Some(i) => list[i] = (name.clone(), body),
        None if list.len() >= PROFILES_MOST => return format!("There are {PROFILES_MOST} HUD layouts already: /hud delete one first."),
        None => list.push((name.clone(), body)),
    }
    if keep_profiles(&list) { format!("HUD layout \"{name}\" saved: /hud load {name} puts it back.") } else { "The HUD layout could not be saved.".to_string() }
}

/// The profile `name` put on the HUD: its places, sizes and HUD Scale,
/// kept as this window's layout (as a drag keeps one).
pub fn load_profile(name: &str) -> String {
    let list = profiles();
    let Some(i) = find(&list, name.trim()) else {
        return if name.trim().is_empty() { "Which layout? /hud load <name> (/hud list names them).".to_string() } else { format!("No HUD layout is named \"{}\" (/hud list names them).", name.trim()) };
    };
    let (name, body) = &list[i];
    with(|s| {
        s.layout = Layout::from_text(body);
        s.drag = None;
        s.natural = [None; N];
    });
    keep();
    if let Some(v) = body.lines().find_map(|l| l.strip_prefix("hudscale ")).and_then(|v| v.trim().parse::<u8>().ok()) {
        crate::settings::set_hud_scale_percent(v);
    }
    format!("HUD layout \"{name}\" loaded.")
}

/// The profile `name` let go.
pub fn delete_profile(name: &str) -> String {
    let mut list = profiles();
    let Some(i) = find(&list, name.trim()) else {
        return format!("No HUD layout is named \"{}\".", name.trim());
    };
    let (gone, _) = list.remove(i);
    if keep_profiles(&list) { format!("HUD layout \"{gone}\" deleted.") } else { "The HUD layouts could not be saved.".to_string() }
}

thread_local! {
    /// The profile Extended's HUD Layouts row shows (an index of `profile_names`).
    static PICKED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Extended's HUD Layouts row moved `d` through the profiles' names.
pub fn step_picked(d: i32) {
    let n = profile_names().len();
    if n > 0 {
        PICKED.with(|p| p.set((p.get() as i64 + i64::from(d.signum())).rem_euclid(n as i64) as usize));
    }
}

/// The profile Extended's HUD Layouts row shows: none with none saved.
pub fn picked() -> Option<String> {
    let names = profile_names();
    let i = PICKED.with(|p| p.get());
    names.get(i).or(names.first()).cloned()
}

/// The profiles' names in a line.
fn list_profiles() -> String {
    let names = profile_names();
    if names.is_empty() { "No HUD layouts saved: /hud save <name> keeps the HUD as it is now.".to_string() } else { format!("HUD layouts: {} (/hud load <name>)", names.join(", ")) }
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
const BAR_TEXT: &str = "Move HUD: drag a piece to move it, the minimap's corner to resize it";
const PC_BAR_TEXT: &str = "Move HUD: drag a piece or resize from a corner";

/// Diagonal grips face the screen's center and its opposite corner, so even a
/// piece flush against two edges has room to grow. Keep clear of title controls.
fn grips(piece: Piece, r: [f32; 4], b: Beyond) -> [ScaleGrip; 2] {
    const W: f32 = 11.0;
    let screen = b.screen();
    let inward: [f32; 2] = std::array::from_fn(|a| {
        if r[a] + r[a+2] <= screen[a] + screen[a+2] { 1.0 } else { -1.0 }
    });
    [inward, [-inward[0], -inward[1]]].map(|direction| {
        let x = if direction[0] < 0.0 { r[0] } else { r[2] - W };
        let y = if direction[1] < 0.0 { r[1] - W } else { r[3] - 2.0 };
        let x = x.clamp(screen[0], screen[2] - W);
        let y = y.clamp(screen[1], screen[3] - W);
        ScaleGrip { piece, rect: [x, y, x + W, y + W], direction }
    })
}

#[cfg(test)]
pub(crate) fn scale_grips(piece: Piece) -> Vec<([f32; 4], [f32; 2])> {
    with(|s| s.scale_grips.iter().filter(|g| g.piece == piece).map(|g| (g.rect, g.direction)).collect())
}

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
    let drag = with(|s| s.drag);
    let mut chat_grips = Vec::new();
    let mut scale_grips = Vec::new();
    let mut buttons = Vec::new();
    let pc = crate::settings::pc_mode();
    let pins = with(|s| s.layout.pinned);
    for (piece, mut at) in drawn_all() {
        if held == Some(piece) && let Some(Drag { kind: DragKind::Chat(size), .. }) = drag {
            let [dx, dy] = size.delta();
            at[0] -= dx;
            at[1] -= dy;
        }
        fill(ui, at, if held == Some(piece) { FILL_HELD } else { FILL });
        for e in [[at[0], at[1], at[2], at[1] + 1.0], [at[0], at[3] - 1.0, at[2], at[3]], [at[0], at[1], at[0] + 1.0, at[3]], [at[2] - 1.0, at[1], at[2], at[3]]] {
            fill(ui, e, EDGE);
        }
        if pc {
            // Keep the edit handles readable over a pane's tabs and the
            // action bar's own icons while the player is arranging the HUD.
            fill(ui, [at[0] + 1.0, at[1] + 1.0, at[2] - 1.0, (at[1] + TEXT_H + 5.0).min(at[3])], [0.075, 0.065, 0.04, 0.97]);
        }
        let mut x = at[0] + 3.0;
        if pc && piece.chat_tab().is_some() && !pins[piece.index()] {
            let right = x + width(ui, "Resize") + 6.0;
            let grip = [at[0], at[1], right, at[1] + TEXT_H + 5.0];
            fill(ui, grip, BUTTON);
            words(ui, x, at[1] + 2.0, "Resize", [1.0; 4]);
            let row = (ui.fonts[font].height as f32 - 4.0).max(1.0) * crate::hudlayout::scale(piece, b).factor;
            chat_grips.push((piece, grip, row));
            x = right + 3.0;
        }
        words(ui, x, at[1] + 2.0, piece.label(), WORDS);
        // its corner grips: every piece unpinned in the PC interface, the
        // minimap in the Controller's (`sizable`: petition 3065)
        let resizable = if pc {
            let label = if pins[piece.index()] { "Unpin" } else { "Pin" };
            let w = width(ui, label) + 8.0;
            let left = (at[2] - w).max(x + width(ui, piece.label()) + 3.0).clamp(-b.side, SCREEN_W + b.side - w);
            let top = at[1].clamp(-b.top, SCREEN_H + b.top - TEXT_H - 5.0);
            let rect = [left, top, left + w, top + TEXT_H + 5.0];
            fill(ui, rect, BUTTON);
            words(ui, left + 4.0, top + 2.0, label, [1.0; 4]);
            buttons.push((rect, Button::Pin(piece)));
            !pins[piece.index()]
        } else {
            sizable(false, piece)
        };
        if resizable {
            for grip in grips(piece, at, b) {
                let [l, t, r, bottom] = grip.rect;
                let [dx, dy] = grip.direction;
                let (cx, cy) = ((l + r) * 0.5, (t + bottom) * 0.5);
                // On a short piece at the display edge, keep Pin and the
                // chat reflow grip exposed; the opposite corner still works.
                if buttons.iter().any(|(btn, _)| btn[0] <= cx && cx < btn[2] && btn[1] <= cy && cy < btn[3])
                    || chat_grips.iter().any(|(_, btn, _)| btn[0] <= cx && cx < btn[2] && btn[1] <= cy && cy < btn[3]) { continue; }
                fill(ui, grip.rect, BUTTON);
                for step in [-2.5, -0.5, 1.5] {
                    let (x, y) = (cx + dx * step, cy + dy * step);
                    fill(ui, [x - 1., y - 1., x + 1., y + 1.], WORDS);
                }
                let (x, y) = (cx + dx * 3., cy + dy * 3.);
                fill(ui, [x.min(cx) - 0.8, y - 0.8, x.max(cx) + 0.8, y + 0.8], WORDS);
                fill(ui, [x - 0.8, y.min(cy) - 0.8, x + 0.8, y.max(cy) + 0.8], WORDS);
                scale_grips.push(grip);
            }
        }
    }
    with(|s| { s.chat_grips = chat_grips; s.scale_grips = scale_grips; });
    // the bar: its words, Reset and Lock, in the middle of the screen's top
    let pad = 4.0;
    let (lock_w, reset_w) = (width(ui, "Lock") + pad * 4.0, width(ui, "Reset") + pad * 4.0);
    let size_label;
    let title = if let Some(Drag { piece, kind: DragKind::Scale { .. }, .. }) = drag {
        size_label = format!("{}  {}%", piece.label(), piece_scale_percent(piece));
        size_label.as_str()
    } else if pc { PC_BAR_TEXT } else { BAR_TEXT };
    let text_w = width(ui, title);
    let w = pad * 3.0 + text_w + pad * 2.0 + reset_w + pad + lock_w;
    let h = TEXT_H + pad * 2.0;
    let (x0, y0) = (320.0 - w / 2.0, -b.top + 4.0);
    fill(ui, [x0, y0, x0 + w, y0 + h], BAR);
    words(ui, x0 + pad * 3.0, y0 + pad, title, WORDS);
    let mut x = x0 + pad * 3.0 + text_w + pad * 2.0;
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

    #[test]
    fn corner_drags_size_each_piece_keep_the_far_corner_and_restore_the_saved_profile() {
        testing_reset();
        crate::settings::testing::reset(false);
        crate::settings::set_ui_mode(true).unwrap();
        set_unlocked(true);
        for piece in PIECES {
            let rect = [230.0, 150.0, 330.0, 214.0];
            // Put the piece in clear space so the screen edge does not limit its growth.
            with(|s| s.layout.set(piece, Some(Place::of(piece, [0.0; 2], rect, WIDE))));
            frame_begin();
            note(piece, rect, [0.0; 2]);
            frame_end();
            let grip = grips(piece, rect, WIDE)[0];
            with(|s| s.scale_grips = vec![grip]);
            let start = [(grip.rect[0] + grip.rect[2]) * 0.5, (grip.rect[1] + grip.rect[3]) * 0.5];
            let far = std::array::from_fn(|a| rect[if grip.direction[a] > 0.0 { a } else { a + 2 }]);
            let fixed = local_point(piece, far, WIDE);
            assert_eq!(press(start, WIDE), Press::Piece(piece));
            drag_to([start[0] + 25.0 * grip.direction[0], start[1] + 16.0 * grip.direction[1]], WIDE);
            assert!(release());
            assert_eq!(piece_scale_percent(piece), 125, "{piece:?}");
            let after = frame_point(piece, fixed, WIDE);
            assert!((far[0] - after[0]).abs() <= 0.51 && (far[1] - after[1]).abs() <= 0.51, "{piece:?}: {far:?} -> {after:?}");
        }
        crate::settings::set_ui_mode(false).unwrap();
        assert!(PIECES.into_iter().all(|p| piece_scale_percent(p) == 100));
        crate::settings::set_ui_mode(true).unwrap();
        assert!(PIECES.into_iter().all(|p| piece_scale_percent(p) == 125));
        reset();
        testing_reset();
        assert_eq!(layout(), Layout::default());
    }

    /// Heirloom petition 3065: the Controller interface's minimap is resized
    /// from its corner as the PC's pieces are, kept in the Controller's own
    /// layout; its other pieces keep their native sizes.
    #[test]
    fn controller_minimap_resizes_from_its_corner_and_keeps_its_size() {
        testing_reset();
        crate::settings::testing::reset(false);
        crate::settings::set_ui_mode(false).unwrap();
        set_unlocked(true);
        assert!(sizable(false, Piece::Minimap) && !sizable(false, Piece::Target) && !sizable(false, Piece::Chat));
        let piece = Piece::Minimap;
        let rect = [230.0, 150.0, 330.0, 214.0];
        with(|s| s.layout.set(piece, Some(Place::of(piece, [0.0; 2], rect, WIDE))));
        frame_begin();
        note(piece, rect, [0.0; 2]);
        frame_end();
        let grip = grips(piece, rect, WIDE)[0];
        with(|s| s.scale_grips = vec![grip]);
        let start = [(grip.rect[0] + grip.rect[2]) * 0.5, (grip.rect[1] + grip.rect[3]) * 0.5];
        let far = std::array::from_fn(|a| rect[if grip.direction[a] > 0.0 { a } else { a + 2 }]);
        let fixed = local_point(piece, far, WIDE);
        assert_eq!(press(start, WIDE), Press::Piece(piece));
        drag_to([start[0] + 25.0 * grip.direction[0], start[1] + 16.0 * grip.direction[1]], WIDE);
        assert!(release());
        assert_eq!(piece_scale_percent(piece), 125);
        assert_eq!(scale(piece, WIDE).factor, crate::settings::client().display_ui().hud_scale as f32 / 100.0 * 1.25);
        let after = frame_point(piece, fixed, WIDE);
        assert!((far[0] - after[0]).abs() <= 0.51 && (far[1] - after[1]).abs() <= 0.51, "{far:?} -> {after:?}");
        assert!(layout().to_text().contains("size minimap 125\n"));
        // the Controller's other pieces keep the game's sizes
        set_piece_scale_percent(Piece::Target, 150);
        assert_eq!(piece_scale_percent(Piece::Target), 100);
        // the PC's layout is its own, and the Controller's comes back with its size
        crate::settings::set_ui_mode(true).unwrap();
        assert_eq!(piece_scale_percent(piece), 100);
        crate::settings::set_ui_mode(false).unwrap();
        assert_eq!(piece_scale_percent(piece), 125);
        reset();
        assert_eq!(piece_scale_percent(piece), 100, "Reset HUD puts it back");
        testing_reset();
    }

    #[test]
    fn size_records_can_be_added_to_old_placements_without_losing_valid_records() {
        let old = "pin target\ntarget l 5 b -8\n";
        let before = Layout::from_text(old);
        assert_eq!(before.percent, [100; N]);
        let mut expected = before;
        expected.percent[Piece::Target.index()] = 135;
        let config = format!("{old}size target 136\nsize target 500\nsize typo 150\nsize chat -5\nsize minimap 55 extra\n");
        assert_eq!(Layout::from_text(&config), expected);
        assert_eq!(Layout::from_text(&expected.to_text()), expected);
    }

    #[test]
    fn scaled_hud_pointers_follow_each_piece_and_its_saved_move() {
        testing_reset();
        crate::settings::testing::reset(false);
        crate::settings::set_client(crate::settings::ClientOptions { pc_mode: true, hud_scale: 75, ..Default::default() });
        let local = [400.0, 360.0];
        for b in [FOUR, WIDE, TALL] {
            for piece in PIECES {
                with(|s| s.layout.set(piece, Some(Place { h: Edge::Middle, v: Edge::Middle, dx: 12.0, dy: -20.0 })));
                let shown = frame_point(piece, local, b);
                assert_eq!(local_point(piece, shown, b), local, "{piece:?}, {b:?}");
            }
        }
        with(|s| {
            s.layout = Layout::default();
            s.layout.set(Piece::Chat, Some(Place { h: Edge::End, v: Edge::End, dx: 0.0, dy: 0.0 }));
        });
        assert_eq!(frame_point(Piece::Chat, local, WIDE), [566.0, 382.0]);
        assert_eq!(frame_point(Piece::Player, [40.0, 20.0], TALL), [30.0, -70.0]);
    }

    /// The Controller interface's HUD Scale (petition 2912): every HUD piece
    /// scaled about its own corner and pointed at through the same scale,
    /// but the raid prompts and the target info, which it draws as windows;
    /// an older file's `hud_scale` (the PC's) does not reach it. The control:
    /// at its default the game's HUD, every point where it was.
    #[test]
    fn controller_hud_scale_sizes_and_points_at_every_piece() {
        testing_reset();
        crate::settings::testing::reset(false);
        let legacy = crate::settings::ClientOptions { pc_mode: false, hud_scale: 75, ..Default::default() };
        crate::settings::set_client(legacy);
        let local = [40.0, 20.0];
        for b in [FOUR, WIDE, TALL] {
            for piece in PIECES {
                assert_eq!(scale(piece, b).factor, 1.0, "{piece:?}: the PC's 75 is not the Controller's");
                assert_eq!(frame_point(piece, local, b), [local[0] + anchor(piece, b).1[0], local[1] + anchor(piece, b).1[1]], "{piece:?}: where it was");
            }
        }
        crate::settings::set_client(crate::settings::ClientOptions { controller_hud_scale: 50, ..legacy });
        for b in [FOUR, WIDE, TALL] {
            for piece in PIECES {
                let windowed = matches!(piece, Piece::RaidPrompts | Piece::TargetInfo);
                assert_eq!(scale(piece, b).factor, if windowed { 1.0 } else { 0.5 }, "{piece:?}, {b:?}");
                let shown = frame_point(piece, [400.0, 360.0], b);
                assert_eq!(local_point(piece, shown, b), [400.0, 360.0], "{piece:?}, {b:?}: pointed at where drawn");
            }
        }
        // the player's frame, at the top left, half its distance from there
        assert_eq!(frame_point(Piece::Player, local, FOUR), [20.0, 10.0]);
        // the PC interface keeps its own scale
        crate::settings::set_client(crate::settings::ClientOptions { pc_mode: true, controller_hud_scale: 50, ..legacy });
        assert_eq!(scale(Piece::Player, FOUR).factor, 0.75);
        crate::settings::testing::reset(false);
    }

    #[test]
    fn chat_grip_resizes_on_release_and_keeps_only_its_settings() {
        testing_reset();
        crate::settings::testing::reset(false);
        crate::settings::set_client(crate::settings::ClientOptions { pc_mode: true, nameplate_scale: 150, ..Default::default() });
        set_unlocked(true);
        with(|s| s.chat_grips.push((Piece::Chat, [200.0, 330.0, 245.0, 347.0], 17.0)));
        assert_eq!(press([210.0, 338.0], FOUR), Press::Piece(Piece::Chat));
        drag_to([325.5, 304.0], FOUR);
        assert_eq!(crate::settings::client().chat_width, 100, "reflow on release");
        assert!(release());
        let c = crate::settings::client();
        assert_eq!((c.chat_width, c.chat_rows, c.nameplate_scale), (75, 7, 150));
        let saved = String::from_utf8(crate::settings::testing::kept("client.opt").unwrap()).unwrap();
        assert!(saved.contains("chat_width=75") && saved.contains("chat_rows=7"));
        assert!(!saved.contains("nameplate_scale"), "other unsaved changes stay pending");
        assert_eq!(layout(), Layout::default());
        with(|s| s.chat_grips.push((Piece::Chat2, [165.0, 220.0, 210.0, 236.0], 17.0)));
        assert_eq!(press([170.0, 228.0], FOUR), Press::Piece(Piece::Chat2));
        drag_to([245.0, 262.0], FOUR);
        assert!(release());
        assert_eq!(crate::settings::chat_pane_size(2), (60, 5));
        assert_eq!(crate::settings::chat_pane_size(0), (75, 7));
    }

    #[test]
    fn controller_minimap_leaves_a_gutter_beside_the_stock_carousel() {
        crate::settings::testing::reset(false);
        testing_reset();
        assert_eq!(shift(Piece::Minimap, WIDE), [0.0; 2]);
        with(|s| s.drawn[Piece::QuickMenus.index()] = Some([536.0, 13.0, 741.0, 186.0]));
        assert_eq!(shift(Piece::Minimap, WIDE), [-4.0, 0.0]);
        let moved = Place { h: Edge::End, v: Edge::Start, dx: -62.0, dy: 12.0 };
        with(|s| s.layout.set(Piece::Minimap, Some(moved)));
        assert_eq!(shift(Piece::Minimap, WIDE), [-62.0, 12.0]);
        with(|s| {
            s.layout = Layout::default();
            s.layout.set(Piece::QuickMenus, Some(moved));
        });
        assert_eq!(shift(Piece::Minimap, WIDE), [0.0; 2]);
    }

    #[test]
    fn a_detached_pc_pane_clears_the_player_frame_at_four_by_three() {
        testing_reset();
        crate::settings::testing::reset(false);
        crate::settings::set_ui_mode(true).unwrap();
        crate::settings::set_chat_detached(1, true).unwrap();
        crate::settings::keep_chat_pane_size(1, 70, 5);
        // The 640x480 renderer's measured frame rectangles, with four spell rows.
        let fixed = [
            (Piece::Player, [8.5, 12.75, 109.65, 71.4]), (Piece::Target, [246.0, 18.0, 393.9, 69.85]),
            (Piece::Minimap, [496.35, 38.25, 596.65, 141.95]), (Piece::Chat, [6.55, 164.15, 244.55, 318.85]),
            (Piece::SpellBars, [175.5, 346.4, 464.5, 445.0]), (Piece::UtilityBars, [486.65, 358.85, 603.1, 383.5]),
            (Piece::EquipmentBar, [486.65, 389.45, 632.0, 414.1]), (Piece::Settings, [551.6, 418.25, 631.5, 440.35]),
        ];
        let natural = [385.0, 323.05, 642.55, 448.85];
        let bounds = |[x, y]: [f32; 2]| [natural[0] + x, natural[1] + y, natural[2] + x, natural[3] + y];
        let clear = |a: [f32; 4], b: [f32; 4]| a[2] <= b[0] || b[2] <= a[0] || a[3] <= b[1] || b[3] <= a[1];
        let first = bounds(shift(Piece::Chat1, FOUR));
        assert!(fixed.iter().all(|(_, r)| clear(first, *r)), "clear on the first draw, before the renderer measures the pane");
        with(|s| {
            for (p, r) in fixed { s.drawn[p.index()] = Some(r); }
            s.natural[Piece::Chat1.index()] = Some(natural);
            s.natural[Piece::Chat2.index()] = Some(natural);
        });
        let pane = bounds(shift(Piece::Chat1, FOUR));
        assert!(pane[0] > 300.0 && pane[2] <= 640.0);
        assert!(fixed.iter().all(|(_, r)| clear(pane, *r)));
        with(|s| s.drawn[Piece::Chat1.index()] = Some(pane));
        let second = bounds(shift(Piece::Chat2, FOUR));
        assert_ne!(second, pane, "another detached tab finds a separate place");
        set_unlocked(true);
        set_buttons(vec![([2.0, 2.0, 24.0, 20.0], Button::Pin(Piece::Chat1))]);
        press([8.0, 8.0], FOUR);
        release();
        assert!(layout().get(Piece::Chat1).is_some(), "pin captures the automatically chosen location");
        with(|s| s.drawn[Piece::Minimap.index()] = Some(pane));
        assert_eq!(bounds(shift(Piece::Chat1, FOUR)), pane, "new obstacles do not move a pinned pane");
        assert!(pc_offset(Piece::Chat1, WIDE, &layout()).unwrap()[0] < -100.0, "a wide display keeps Tells above Main on the left");
        crate::settings::testing::reset(false);
        testing_reset();
    }

    #[test]
    fn pc_pins_and_placements_survive_mode_switches() {
        crate::settings::testing::reset(false);
        testing_reset();
        crate::settings::set_ui_mode(true).unwrap();
        let piece = Piece::Chat1;
        for b in [FOUR, WIDE, TALL] {
            let mid = frame_point(Piece::SpellBars, [320.0, 400.0], b);
            assert!((mid[0] - 320.0).abs() <= 0.51);
            let main = frame_point(Piece::Chat, [400.0, 400.0], b);
            let above = frame_point(piece, [400.0, 400.0], b);
            if b == FOUR {
                assert!(above[0] > main[0] + 100.0, "the narrow layout uses the right column");
            } else {
                assert!(above[1] < main[1] - 90.0 && (above[0] - main[0]).abs() < 0.1);
            }
        }
        let place = Place { h: Edge::Start, dx: -130.0, v: Edge::Start, dy: -40.0 };
        with(|s| s.layout.set(piece, Some(place)));
        let button = [4.0, 4.0, 26.0, 22.0];
        set_unlocked(true);
        set_buttons(vec![(button, Button::Pin(piece))]);
        assert_eq!(press([10.0; 2], FOUR), Press::Button(Button::Pin(piece)));
        release();
        with(|s| s.drawn[piece.index()] = Some([30.0, 30.0, 120.0, 100.0]));
        assert_eq!(press([80.0; 2], FOUR), Press::Piece(piece));
        assert!(!dragging());
        release();
        crate::settings::set_ui_mode(false).unwrap();
        assert!(!unlocked() && !layout().pinned[piece.index()] && layout().get(piece).is_none());
        crate::settings::set_ui_mode(true).unwrap();
        assert_eq!(layout().get(piece), Some(place));
        assert!(layout().pinned[piece.index()]);
        set_unlocked(true);
        set_buttons(vec![(button, Button::Pin(piece))]);
        press([10.0; 2], FOUR);
        release();
        with(|s| s.drawn[piece.index()] = Some([30.0, 30.0, 120.0, 100.0]));
        press([80.0; 2], FOUR);
        assert!(dragging());
        release();
    }

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

    /// Petition 5229: the Stacked Spell Bars are a piece of their own. Drawn
    /// just over the chat box, a press on them picks them up (the smaller),
    /// and dragged they move alone: the chat box stays; the chat box dragged
    /// leaves them. The control: a press on the chat box below them is the
    /// chat box's.
    #[test]
    fn the_stacked_spell_bars_move_apart_from_the_chat_box() {
        testing_reset();
        crate::settings::testing::reset(false);
        let frame = || {
            frame_begin();
            for (p, r) in [(Piece::Chat, [200.0, 331.0, 640.0, 448.0]), (Piece::SpellBars, [400.0, 260.0, 640.0, 330.0])] {
                let s = shift(p, FOUR);
                note(p, [r[0] + s[0], r[1] + s[1], r[2] + s[0], r[3] + s[1]], s);
            }
            frame_end();
        };
        frame();
        set_unlocked(true);
        assert_eq!(press([300.0, 400.0], FOUR), Press::Piece(Piece::Chat), "the control: the chat box");
        assert!(release());
        assert_eq!(press([500.0, 300.0], FOUR), Press::Piece(Piece::SpellBars));
        drag_to([300.0, 200.0], FOUR);
        assert!(release());
        assert_eq!(shift(Piece::SpellBars, FOUR), [-200.0, -100.0]);
        assert_eq!(shift(Piece::Chat, FOUR), [0.0, 0.0], "the chat box stays");
        frame();
        assert_eq!(press([300.0, 400.0], FOUR), Press::Piece(Piece::Chat));
        drag_to([250.0, 380.0], FOUR);
        assert!(release());
        assert_eq!(shift(Piece::Chat, FOUR), [-50.0, -20.0]);
        assert_eq!(shift(Piece::SpellBars, FOUR), [-200.0, -100.0], "the bars stay");
        assert!(crate::settings::hud_layout().contains("spellbars "), "kept: {:?}", crate::settings::hud_layout());
        set_unlocked(false);
        reset();
    }

    /// The Utility Bars (2026-11-18, "free move these in freemove mode like
    /// the others"): a piece of their own, picked up and moved apart from the
    /// spell bars, to the screen's left for one, and kept; their side is
    /// Utility Bar Layout's (the upper left's, or the chat box's below);
    /// `reset_piece` puts them back alone, the spell bars where they were put
    /// (the control).
    #[test]
    fn the_utility_bars_move_on_their_own() {
        use crate::hudmenu::UtilityLayout;
        testing_reset();
        crate::settings::testing::reset(false);
        assert_eq!(Piece::UtilityBars.own(), (Edge::Start, Edge::Start), "upper left");
        crate::settings::set_client(crate::settings::ClientOptions { utility_layout: UtilityLayout::Below, ..crate::settings::client() });
        assert_eq!(Piece::UtilityBars.own(), (Edge::End, Edge::End), "below, on the chat box");
        let frame = || {
            frame_begin();
            for (p, r) in [(Piece::SpellBars, [216.0, 230.0, 390.0, 290.0]), (Piece::UtilityBars, [216.0, 292.0, 390.0, 330.0])] {
                let s = shift(p, FOUR);
                note(p, [r[0] + s[0], r[1] + s[1], r[2] + s[0], r[3] + s[1]], s);
            }
            frame_end();
        };
        frame();
        set_unlocked(true);
        assert_eq!(press([300.0, 250.0], FOUR), Press::Piece(Piece::SpellBars), "the control: the spell bars");
        drag_to([300.0, 200.0], FOUR);
        assert!(release());
        frame();
        assert_eq!(press([300.0, 310.0], FOUR), Press::Piece(Piece::UtilityBars));
        drag_to([100.0, 110.0], FOUR);
        assert!(release());
        assert_eq!(shift(Piece::UtilityBars, FOUR), [-200.0, -200.0], "to the screen's left");
        assert_eq!(shift(Piece::SpellBars, FOUR), [0.0, -50.0], "the spell bars stay");
        assert!(crate::settings::hud_layout().contains("utilitybars "), "kept: {:?}", crate::settings::hud_layout());
        reset_piece(Piece::UtilityBars);
        assert_eq!(shift(Piece::UtilityBars, FOUR), [0.0, 0.0], "back where their layout puts them");
        assert_eq!(shift(Piece::SpellBars, FOUR), [0.0, -50.0], "the spell bars where they were put");
        set_unlocked(false);
        reset();
        crate::settings::testing::reset(false);
    }

    /// The quick bar moved, its selected ability's name scrolled in its box
    /// (`HudMenu::draw`: 158 wide, centred at 511): the bar stays where it was put, the
    /// name however long and wherever the scroll has it. The name's quads are
    /// drawn whole and cut to the box; counted whole, a long name ran the bar
    /// off the screen's right and `clamp` carried it left and back with the
    /// scroll (the control: the same quads uncut). A quad cut to nothing is
    /// not counted.
    #[test]
    fn a_long_name_scrolled_in_its_box_leaves_the_quick_bar_where_it_is() {
        use render::ui::Quad;
        let quad = |x0: f32, x1: f32, y0: f32, y1: f32, clip: Option<[i32; 4]>| Quad { tex: 0, blend: 3, pos: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]], uv: [[0.0; 2]; 4], color: [1.0; 4], clip };
        let bar = [430.0, 25.0, 640.0, 120.0];
        let to = [0.0, 60.0];
        let frame = |offset: f32, tw: f32, cut: bool| -> [f32; 2] {
            testing_reset();
            with(|s| s.layout.set(Piece::QuickMenus, Some(Place::of(Piece::QuickMenus, to, bar, FOUR))));
            let mut sh = [0.0; 2];
            for _ in 0..3 {
                sh = shift(Piece::QuickMenus, FOUR);
                let mv = |r: [f32; 4]| [r[0] + sh[0], r[1] + sh[1], r[2] + sh[0], r[3] + sh[1]];
                let b = mv(bar);
                let clip = [432 + sh[0] as i32, 30 + sh[1] as i32, 591 + sh[0] as i32, 45 + sh[1] as i32];
                let name = mv([442.0 + offset, 32.0, 442.0 + offset + tw, 44.0]);
                let quads = [quad(b[0], b[2], b[1], b[3], None), quad(name[0], name[2], name[1], name[3], cut.then_some(clip))];
                frame_begin();
                if let Some(r) = covered(&quads) {
                    note(Piece::QuickMenus, r, sh);
                }
                frame_end();
            }
            sh
        };
        let short = frame(0.0, 80.0, true);
        assert_eq!(short, to, "the control: a short name, where it was put");
        for offset in [0.0, -40.0, -142.0] {
            assert_eq!(frame(offset, 300.0, true), to, "a long name scrolled {offset}: the bar stays");
        }
        assert_ne!(frame(0.0, 300.0, false), frame(-142.0, 300.0, false), "the control: counted uncut, the bar moves with the scroll");
        let gone = quad(700.0, 900.0, 32.0, 44.0, Some([432, 30, 591, 45]));
        assert_eq!(covered(&[gone]), None, "cut to nothing");
        assert_eq!(covered(&[quad(400.0, 900.0, 32.0, 44.0, Some([432, 30, 591, 45]))]), Some([432.0, 32.0, 591.0, 44.0]));
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

    /// Named layouts (petition 2947): saved with the HUD Scale, listed,
    /// loaded back over another layout (any case), saved again over the
    /// same name, deleted; a bad or missing name refused. The control: a
    /// load of a name never saved changes nothing.
    #[test]
    fn hud_profiles_save_load_delete() {
        testing_reset();
        crate::settings::testing::reset(false);
        let mut wide = Layout::default();
        wide.set(Piece::Pet, Some(Place { h: Edge::Start, dx: 40.0, v: Edge::Start, dy: 7.0 }));
        wide.percent[Piece::SpellBars.index()] = 80;
        with(|s| s.layout = wide);
        crate::settings::set_hud_scale_percent(70);
        assert!(command("save Ultra Wide").contains("saved"));
        with(|s| s.layout = Layout::default());
        crate::settings::set_hud_scale_percent(100);
        assert!(command("save dualbox").contains("saved"));
        assert_eq!(profile_names(), ["Ultra Wide", "dualbox"]);
        assert!(command("list").contains("Ultra Wide, dualbox"));
        assert!(command("load nothing").starts_with("No HUD layout"));
        assert_eq!(layout(), Layout::default(), "the control: nothing loaded");
        assert!(command("load ultra wide").contains("loaded"));
        assert_eq!(layout(), wide);
        assert_eq!(crate::settings::hud_scale_percent(), 70);
        assert_eq!(Layout::from_text(&crate::settings::hud_layout()), wide, "kept as the window's layout");
        assert!(command("load DUALBOX").contains("loaded"));
        assert_eq!((layout(), crate::settings::hud_scale_percent()), (Layout::default(), 100));
        assert!(command("save dualbox").contains("saved"));
        assert_eq!(profile_names().len(), 2, "saved over its own name");
        assert!(command("delete ultra wide").contains("deleted"));
        assert_eq!(profile_names(), ["dualbox"]);
        assert!(command("save").starts_with("Name the layout"));
        assert!(command("save a|b").starts_with("Name the layout"));
        assert!(command("load").starts_with("Which layout"));
    }
}
