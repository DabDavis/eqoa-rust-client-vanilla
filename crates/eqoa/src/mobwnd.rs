//! A creature's details and loot chart (Heirloom petition 3163: "view a mob's
//! loot chart: right-click a mob to view its details like stats and loot
//! table"): the client's own window, as the game has none, on a server that
//! offers it (go-eqoa-server's `world.mob_details`, `net::mobdetails`).
//!
//! - A right click on a creature (a monster: not a player, a pet, a
//!   townsperson or a corpse) targets it and asks the server
//!   (`Play::mob_details_click`); the window opens at once, "..." until the
//!   answer. Elsewhere a right click is what it was (a talk, a corpse opened).
//! - The window ([`MobWnd`]): one at a time, at the top middle in the band the
//!   item links' windows keep to; the name in yellow and its X, then its level,
//!   class and con, its hit points, and what its corpse can hold with each
//!   item's chance a kill, as the server read it off its loot model: the words
//!   (every kill, common, uncommon, rare) and the percent. Long, the wheel
//!   scrolls it; Escape or △ closes it (before the item links' windows).

use net::mobdetails::{Answer, Details, Row, Source, Status};
use vi::ui::{FrameParam, Raster, Vui, Window, icon_id, wide};

/// The window's width, and the band at the top of the 640 by 448 frame it
/// keeps to (as the item links' windows: right of the gauges, left of the
/// minimap, over the chat box).
const W: i32 = 260;
const BAND: [i32; 2] = [134, 470];
const TOP: i32 = 52;
const BOTTOM: i32 = 318;
const ROWS_MOST: i32 = 16;

/// The open window.
#[derive(Clone, Debug, Default)]
pub struct Wnd {
    pub id: u32,
    /// the name the entity bears (until the answer)
    name: String,
    answer: Option<Answer>,
    /// where it was drawn last, and its X (frame x0 y0 x1 y1)
    rect: [i32; 4],
    x_hit: [i32; 4],
    /// the text's first row shown, its rows in all and the rows shown
    top: i32,
    lines: i32,
    rows: i32,
}

/// The creature details' window, none open or one.
#[derive(Default)]
pub struct MobWnd {
    pub wnd: Option<Wnd>,
}

/// A chance a kill (hundredths of a percent) as the window says it: every
/// kill, else its word (common from 25%, uncommon from 5%, rare below) and the
/// percent.
pub fn chance_text(basis: u16) -> String {
    if basis >= 10000 {
        return "every kill".into();
    }
    let word = if basis >= 2500 { "common" } else if basis >= 500 { "uncommon" } else { "rare" };
    // whole from 10%, a tenth from 1%, else hundredths; cut, never rounded up
    // past a word's edge (4.99% is rare 4.9%, not 5.0%)
    let n = if basis >= 1000 {
        format!("{}", basis / 100)
    } else if basis >= 100 {
        format!("{}.{}", basis / 100, basis / 10 % 10)
    } else {
        format!("0.{:02}", basis)
    };
    format!("{word} {n}%")
}

/// A consider colour's word (0x0010's: 0 red .. 5 green).
pub fn con_word(con: u8) -> &'static str {
    match con {
        0 => "red",
        1 => "yellow",
        2 => "white",
        3 => "dark blue",
        4 => "light blue",
        5 => "green",
        _ => "unknown",
    }
}

/// A row's line: its text and its chance.
fn row_line(r: &Row) -> String {
    format!("  {} - {}", r.text, chance_text(r.chance))
}

/// The window's text for a creature's details: its level, class and con, its
/// hit points, then its loot by where it comes from (what it carries, its loot
/// tables, its region's drops, quest items).
pub fn describe(d: &Details) -> String {
    let mut out = vec![format!("Level {} {}, con {}", d.level, d.class, con_word(d.con))];
    if d.max_hp > 0 {
        out.push(format!("Hit points {}/{} ({}%)", d.hp, d.max_hp, (d.hp as u64 * 100 / d.max_hp as u64).min(100)));
    }
    out.extend(d.lines.iter().cloned());
    let groups: [(&str, &[Source]); 4] = [
        ("Carries:", &[Source::Own, Source::Held]),
        ("Loot tables:", &[Source::Table, Source::Every]),
        ("Its region's drops:", &[Source::Category]),
        ("Quest items:", &[Source::Quest]),
    ];
    let mut any = false;
    for (head, sources) in groups {
        let rows: Vec<&Row> = d.rows.iter().filter(|r| sources.contains(&r.source)).collect();
        if rows.is_empty() {
            continue;
        }
        any = true;
        out.push(head.into());
        out.extend(rows.into_iter().map(row_line));
    }
    let other: Vec<&Row> = d.rows.iter().filter(|r| matches!(r.source, Source::Other(_))).collect();
    if !other.is_empty() {
        any = true;
        out.push("Other:".into());
        out.extend(other.into_iter().map(row_line));
    }
    if !any && d.lines.is_empty() {
        out.push("No loot is known for it.".into());
    }
    if d.left_out {
        out.push("(more not shown)".into());
    }
    out.join("\n")
}

impl MobWnd {
    pub fn is_open(&self) -> bool {
        self.wnd.is_some()
    }

    /// The window opened on the creature the client knows by `id`, named
    /// `name` until the answer comes (one already open goes).
    pub fn open(&mut self, id: u32, name: &str) {
        self.wnd = Some(Wnd { id, name: name.to_string(), ..Default::default() });
    }

    /// The server's answer: into the window when it is about the creature it
    /// is open on (a late answer about another is dropped). Whether it was.
    pub fn record(&mut self, a: Answer) -> bool {
        match &mut self.wnd {
            Some(w) if w.id == a.id => {
                w.answer = Some(a);
                w.top = 0;
                true
            }
            _ => false,
        }
    }

    /// Escape or △: closed; false with none open.
    pub fn close(&mut self) -> bool {
        self.wnd.take().is_some()
    }

    /// Whether frame point `p` is on the window (a click there is its).
    pub fn covers(&self, p: [f32; 2]) -> bool {
        self.wnd.as_ref().is_some_and(|w| inside(w.rect, p))
    }

    /// A click at frame point `p`: on the X it closes (true); anywhere else on
    /// the window nothing.
    pub fn point(&mut self, p: [f32; 2]) -> bool {
        if self.wnd.as_ref().is_some_and(|w| inside(w.rect, p) && inside(w.x_hit, p)) {
            self.wnd = None;
            return true;
        }
        false
    }

    /// The wheel's `lines` (up positive) over the window: its text scrolled a
    /// row a line. Whether it was over it.
    pub fn wheel(&mut self, p: [f32; 2], lines: f32) -> bool {
        let Some(w) = self.wnd.as_mut().filter(|w| inside(w.rect, p)) else { return false };
        let by = -(lines.round() as i32);
        w.top = (w.top + by).clamp(0, (w.lines - w.rows).max(0));
        true
    }

    /// The window's text now.
    fn text(w: &Wnd) -> String {
        match &w.answer {
            None => "...".into(),
            Some(Answer { status: Status::Shown, details: Some(d), .. }) => describe(d),
            Some(Answer { status: Status::TooSoon, .. }) => "Asked too often: try again in a moment.".into(),
            Some(Answer { status: Status::NotACreature, .. }) => "Nothing is known of it.".into(),
            Some(Answer { status: Status::Off, .. }) => "This server shows no creature details.".into(),
            Some(_) => "Nothing is known of it.".into(),
        }
    }

    /// The window at the band's middle (`side`: the frame pixels shown beyond
    /// each side, which widen the band), its place and its X noted.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, side: i32) {
        let Some(w) = self.wnd.as_mut() else { return };
        let (white, yellow) = ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 0.0, 1.0]);
        let step = vui.font_height(r) - 4;
        let fp = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 1), br: (106, 0), bottom: (102, 2), bl: (105, 0), left: (104, 0) };
        let band = [BAND[0] - side.max(0), BAND[1] + side.max(0)];
        let at = [(band[0] + band[1]) / 2 - W / 2, TOP];
        let shown = match &w.answer {
            Some(Answer { details: Some(d), .. }) => d.name.clone(),
            _ => w.name.clone(),
        };
        let text = wide(&Self::text(w));
        let head = 6 + step + 6;
        let room_rows = ((BOTTOM - TOP - head - 8) / step).clamp(1, ROWS_MOST);
        let lines = Window::default().count_lines(vui, r, &text, W - 20).max(1);
        let more = lines > room_rows;
        let rows = if more { (room_rows - 1).max(1) } else { lines };
        (w.lines, w.rows) = (lines, rows);
        w.top = w.top.clamp(0, (lines - rows).max(0));
        let h = head + (rows + more as i32) * step + 8;
        w.rect = [at[0], at[1], at[0] + W, at[1] + h];
        r.blit(icon_id("black"), 0, at, [0, 0], [W, h], 3, 0.8);
        vui.blit_frame(r, at, [W, h], &fp, 3, 1.0, true);
        rows_at(vui, r, at, W, at[1] + 6 + step / 2, 1).text_box(vui, r, &wide(&shown), 10, 0, W - 32, 1, yellow, 3);
        let xx = at[0] + W - 16;
        vui.draw_string_ex(r, &wide("x"), xx, at[1] + 5, white, true);
        w.x_hit = [xx - 6, at[1], at[0] + W, at[1] + 24];
        rows_at(vui, r, at, W, at[1] + head + step / 2, rows).text_box(vui, r, &text, 10, 0, W - 20, rows, white, ((w.top as u32) << 24) | 3);
        if more {
            let last = (w.top + rows).min(lines);
            let hint = format!("Wheel for more  {last}/{lines}");
            rows_at(vui, r, at, W, at[1] + head + rows * step + step / 2, 1).text_box(vui, r, &wide(&hint), 10, 0, W - 20, 1, [0.85, 0.85, 0.85, 1.0], 3);
        }
    }
}

/// A window `w` wide from `at`'s x whose row 0 is centred on frame line `y`,
/// `rows` rows (as `itemlink`'s).
fn rows_at(vui: &Vui, r: &dyn Raster, at: [i32; 2], w: i32, y: i32, rows: i32) -> Window {
    let mut win = Window::default();
    win.init(vui, r, [0, 0], w, rows, 1);
    let top = y - 23;
    win.center = [at[0] + w / 2, top + win.size[1] / 2];
    win
}

fn inside(r: [i32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] as f32 && p[0] < r[2] as f32 && p[1] >= r[1] as f32 && p[1] < r[3] as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, vui};

    fn details() -> Details {
        Details {
            name: "moss snake".into(),
            level: 15,
            class: "Ranger".into(),
            con: 5,
            hp: 30,
            max_hp: 40,
            lines: Vec::new(),
            rows: vec![
                Row { source: Source::Own, chance: 10000, pattern: 1, text: "Rusty Sword".into() },
                Row { source: Source::Table, chance: 5100, pattern: 1, text: "Rusty Sword".into() },
                Row { source: Source::Table, chance: 1900, pattern: 2, text: "Fishing Bait".into() },
                Row { source: Source::Category, chance: 40, pattern: 0, text: "rare: one of 120 items".into() },
                Row { source: Source::Quest, chance: 5000, pattern: 3, text: "slashed belt (your quest)".into() },
            ],
            left_out: false,
        }
    }

    /// The words and percents: every kill, common from 25%, uncommon from
    /// 5%, rare below; the percent to a hundredth below 1%.
    #[test]
    fn chances_read_as_words_and_percents() {
        assert_eq!(chance_text(10000), "every kill");
        assert_eq!(chance_text(5100), "common 51%");
        assert_eq!(chance_text(2500), "common 25%");
        assert_eq!(chance_text(1900), "uncommon 19%");
        assert_eq!(chance_text(500), "uncommon 5.0%");
        assert_eq!(chance_text(499), "rare 4.9%", "cut, not rounded up past uncommon's 5%");
        assert_eq!(chance_text(40), "rare 0.40%");
        assert_eq!(chance_text(1), "rare 0.01%");
    }

    /// The text: its level, class, con and hit points, then the loot by where
    /// it comes from, each with its chance; a creature with none says so.
    #[test]
    fn the_text_groups_the_loot() {
        let t = describe(&details());
        assert_eq!(t, "Level 15 Ranger, con green\nHit points 30/40 (75%)\nCarries:\n  Rusty Sword - every kill\n\
            Loot tables:\n  Rusty Sword - common 51%\n  Fishing Bait - uncommon 19%\nIts region's drops:\n  \
            rare: one of 120 items - rare 0.40%\nQuest items:\n  slashed belt (your quest) - common 50%");
        let none = Details { name: "rat".into(), level: 1, class: "Warrior".into(), ..Default::default() };
        assert_eq!(describe(&none), "Level 1 Warrior, con red\nNo loot is known for it.");
    }

    /// Open on a creature, "..." until the answer about it; an answer about
    /// another is dropped. Drawn in the band, its X closes it, a click elsewhere
    /// on it is its own and nothing; the wheel scrolls a long text within it.
    /// The control: before a frame drew it nothing is there.
    #[test]
    fn the_window_opens_answers_and_closes() {
        let (v, mut r) = (vui(), NullRaster);
        let mut m = MobWnd::default();
        m.open(9, "moss snake");
        assert_eq!(MobWnd::text(m.wnd.as_ref().unwrap()), "...");
        assert!(!m.covers([300.0, 60.0]), "not drawn yet");
        assert!(!m.record(Answer { id: 8, status: Status::Shown, details: Some(details()) }), "another's answer");
        let mut long = details();
        for i in 0..40 {
            long.rows.push(Row { source: Source::Table, chance: 100, pattern: 10 + i, text: format!("item {i}") });
        }
        assert!(m.record(Answer { id: 9, status: Status::Shown, details: Some(long) }));
        m.draw(&v, &mut r, 0);
        let rect = m.wnd.as_ref().unwrap().rect;
        assert!(rect[0] >= BAND[0] && rect[2] <= BAND[1] && rect[1] == TOP && rect[3] <= BOTTOM, "{rect:?}");
        let mid = [(rect[0] + rect[2]) as f32 / 2.0, (rect[1] + rect[3]) as f32 / 2.0];
        assert!(m.covers(mid) && !m.point(mid), "the window, not its X");
        assert!(m.wheel(mid, -3.0));
        assert_eq!(m.wnd.as_ref().unwrap().top, 3, "scrolled three rows");
        assert!(!m.wheel([600.0, 400.0], -3.0), "off the window");
        let x = m.wnd.as_ref().unwrap().x_hit;
        assert!(m.point([(x[0] + x[2]) as f32 / 2.0, (x[1] + x[3]) as f32 / 2.0]));
        assert!(!m.is_open() && !m.close());
    }
}
