//! The Stats Box (the user, 2026-10-26: "make your stats show on the screen
//! in a box like the mini map? Makes it so nice for buffs/debuffs, a toggle
//! off by default, with button toggle"): the client's own, the player's stats
//! as the Character menu's Summary has them (`trainer`: the seven it trains,
//! then its Details' armour class, hit points, power and six resists), each
//! under the game's own short word for it (`sStatWords`, `FindStatWord`: Str,
//! Sta, ... AC, HPMax, POW, FR, CR, LR, AR, PR, DR), in a box framed as the
//! minimap is (`map::mini`). It sits under the effects' icons (`influence`,
//! from (143, 15)) and right of the player's gauges and the group's bars, out
//! at a wide window's left and up a tall one's top with the HUD's left side.
//!
//! A value is the stat channel's (0x42, `net::dump::Stats::stat`: what the
//! server counts, its effects' share in it), a resist derived as the Summary
//! derives one (`trainer::Data::derived`: a seventh of wisdom and 40 on, 315
//! at most). Against it stands what the stat is without the effects: the base
//! stat and the worn items' (`GetStatTotal`), or that and the masteries' (the
//! Summary's Base adds them to the channel's base, as go-eqoa-server leaves
//! them out of it; Sandstorm's base hit points hold them), the seven within
//! their cap (`GetP44Total`, as the server keeps them). A value above both is
//! drawn blue and one below both red, as the Summary's list of items marks a
//! bonus and a malus (`DrawItems`), made light to read on the dark box; the
//! rest in white. So a buff or a debuff shows at once, and one that changes
//! nothing (a stat at its cap) shows nothing. Stamina's hit points are the
//! base maximum's on the channel already (a stamina buff moves HPMax in
//! white); a spell's own hit points are blue. POW is never marked: the
//! server's maximum power is worked out of every stat, worn ones too, and
//! its base of the base stats alone (go-eqoa-server's and Sandstorm's
//! alike), so the two differ by what the worn items' stats give as well.
//!
//! Extended's UI page's Stats Box turns it on (off by default), and so does B
//! (`binds`' "stats_box", a controller's button can take it on the Controls
//! page), kept at once. It is up where the minimap could be
//! (`Play::stats_box_view`: in play, no window up, no player search, not with
//! the map up or behind the loading screen), never under Hide UI. A click or a
//! tap on it opens the Summary (`hud::Pick::Stats`).

use crate::ui::{Layout as _, UiRaster, Vui, wide};
use net::dump::{ItemList, Stats};
use render::ui::{Anchor, Vertical};

/// Its frame in the HUD's 640x448 frame (`Anchor::Left`, `Vertical::Top`, as
/// the HUD's left side): from under the effects' icons (their bar, 34 high
/// from y 15, `influence`) and right of the player's gauges (to x ~120) and
/// the group's bars (to 194, `hud`'s `GROUP_AT`), above the death notice's bar
/// (y 229) and the manual camera's crosshair (208), and clear of the right
/// side's minimap, quest tracker and target's details (from x 380).
pub const FRAME: [i32; 4] = [200, 54, 348, 186];

/// The frame art's border as it is drawn (frame pixels: its bars 9 thick at
/// the top and the bottom, 7 at the sides) and the rows' room inside it.
const BORDER: [f32; 2] = [7.0, 9.0];
const PAD: [f32; 2] = [3.0, 2.0];
/// The least gap between the two columns, frame pixels, and between the
/// rows' words (of the font's height).
const GAP: f32 = 6.0;
const LEADING: f32 = 0.08;

/// The words' size, of the HUD's font, at most: less when the rows would not
/// fit the frame, eight high and the widest word and value of each column
/// across (the language's words: `layout`). The disc's ArialUI is 21 high,
/// and the English words come out at 0.6.
pub const SCALE: f32 = 0.6;
/// The widest value a column makes room for.
const ROOM: &str = "8888";

/// The rows, two columns of eight: a stat's word (`sStatWords`) and its id.
/// The seven trainable stats and the armour class; the maximum hit points
/// (HPMax, the channel's 8), the maximum power (POW, 10) and the resists in
/// the Summary's order (fire, cold, lightning, arcane, poison, disease).
pub const ROWS: [[(i32, i32); 8]; 2] = [
    [(226, 0), (227, 1), (228, 2), (229, 3), (230, 4), (231, 5), (232, 6), (242, 14)],
    [(234, 8), (496, 10), (447, 24), (451, 25), (450, 26), (452, 27), (448, 22), (449, 23)],
];

/// A row's label, a value as it stands, one an effect raises, one it lowers,
/// and the words' shadow; the box's backing.
pub const LABEL: [f32; 4] = [0.95, 0.82, 0.5, 1.0];
pub const SAME: [f32; 4] = [0.96, 0.95, 0.9, 1.0];
pub const RAISED: [f32; 4] = [0.45, 0.78, 1.0, 1.0];
pub const LOWERED: [f32; 4] = [1.0, 0.45, 0.4, 1.0];
const SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.85];
const BACK: [f32; 4] = [0.05, 0.06, 0.08, 0.85];

/// How the effects leave a stat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shift {
    Same,
    Raised,
    Lowered,
}

/// A row: its stat's word, its value, and what it is without the effects:
/// between the base's and the worn items' alone and that with the masteries'
/// (none for POW, whose effects' share cannot be told).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Row {
    pub word: i32,
    pub value: i32,
    pub without: Option<(i32, i32)>,
}

impl Row {
    pub fn shift(&self) -> Shift {
        match self.without {
            Some((_, high)) if self.value > high => Shift::Raised,
            Some((low, _)) if self.value < low => Shift::Lowered,
            _ => Shift::Same,
        }
    }

    pub fn color(&self) -> [f32; 4] {
        match self.shift() {
            Shift::Same => SAME,
            Shift::Raised => RAISED,
            Shift::Lowered => LOWERED,
        }
    }
}

/// What the box shows: `ROWS`' rows, column by column.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub rows: [[Row; 8]; 2],
}

/// What the box reads off the client: the stat channel, the inventory (its
/// worn items), the masteries' stats and stat-cap bonuses
/// (`VIClientPart4EC58`, the dump's and 0x1402's) and the stat cap
/// (`m_player.m_p44`, none when 0).
pub struct Source<'a> {
    pub stats: &'a Stats,
    pub inv: &'a ItemList,
    pub mastery: &'a [i64; 29],
    pub bonuses: &'a [i64; 7],
    pub p44: i32,
}

/// The rows as they stand, or none before the stat channel has come (no
/// maximum hit points yet).
pub fn view(s: &Source) -> Option<View> {
    if s.stats.stat[8] <= 0 {
        return None;
    }
    // each stat without the effects: the base and the worn items, and that
    // with the masteries, the seven within their cap
    let (mut low, mut high) = ([0i32; 29], [0i32; 29]);
    for i in 0..29 {
        let gear = crate::infotext::stat_total(&s.stats.base, s.inv, i as i64) as i32;
        let m = crate::trainer::mastery_stat(s.mastery, i as i32);
        let cap = |v: i32| if i < 7 && s.p44 > 0 { v.min(crate::trainer::stat_cap(s.p44, s.bonuses, i as i32)) } else { v };
        (low[i], high[i]) = (cap(gear.min(gear + m)), cap(gear.max(gear + m)));
    }
    let mut out = View::default();
    for (col, rows) in ROWS.iter().enumerate() {
        for (k, &(word, stat)) in rows.iter().enumerate() {
            let derived = |v: &[i32; 29]| crate::trainer::Data::derived(v, stat);
            let without = (stat != 10).then(|| (derived(&low), derived(&high)));
            out.rows[col][k] = Row { word, value: derived(&s.stats.stat), without };
        }
    }
    Some(out)
}

/// B (`binds`' "stats_box"): the box shown or hidden, kept (Extended's
/// Stats Box).
pub fn toggle() {
    let on = !crate::settings::stats_box();
    if let Err(e) = crate::settings::set_stats_box(on) {
        crate::say(&format!("stats box {}, not kept: {e}", if on { "shown" } else { "hidden" }));
    }
    crate::say(&format!("stats box {}", if on { "shown" } else { "hidden" }));
}

/// Where the rows go inside the frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rows {
    /// each column's labels' left edge and its values' right edge
    pub lefts: [f32; 2],
    pub rights: [f32; 2],
    /// the first row's top and the step to the next
    pub top: f32,
    pub step: f32,
    /// the words' scale, of the font's
    pub scale: f32,
}

/// The rows' place inside the frame for a font `font_h` high whose two
/// columns are `wide[c]` across at its own size (the widest label, a space
/// and `ROOM`): the largest scale up to `SCALE` at which eight rows fit the
/// frame's height and both columns its width with `GAP` between them, the
/// columns at the frame's two sides (the room left over between them).
pub fn layout(font_h: f32, wide: [f32; 2]) -> Rows {
    let (x0, y0, x1, y1) = (FRAME[0] as f32, FRAME[1] as f32, FRAME[2] as f32, FRAME[3] as f32);
    let (left, right) = (x0 + BORDER[0] + PAD[0], x1 - BORDER[0] - PAD[0]);
    let (top, bottom) = (y0 + BORDER[1] + PAD[1], y1 - BORDER[1] - PAD[1]);
    let font_h = font_h.max(1.0);
    let by_height = (bottom - top) / (8.0 * font_h * (1.0 + LEADING));
    let by_width = (right - left - GAP) / (wide[0] + wide[1]).max(1.0);
    let scale = SCALE.min(by_height).min(by_width);
    let step = (bottom - top) / 8.0;
    Rows { lefts: [left, right - wide[1] * scale], rights: [left + wide[0] * scale, right], top: top + (step - font_h * scale) / 2.0, step, scale }
}

/// Each column's width at the font's own size by `width` (a string's, as the
/// font measures it): its widest label, a space and `ROOM`.
pub fn widths(labels: &[[Vec<u16>; 8]; 2], width: impl Fn(&[u16]) -> f32) -> [f32; 2] {
    let room = width(&wide(&format!(" {ROOM}")));
    std::array::from_fn(|c| labels[c].iter().map(|l| width(l)).fold(0.0, f32::max) + room)
}

/// Draws the box in the HUD's frame: the backing, the minimap's border round
/// it, the rows.
pub fn draw(vui: &Vui, ui: &mut UiRaster, v: &View) {
    ui.ui.anchor = Anchor::Left;
    ui.ui.vertical = Vertical::Top;
    let f = FRAME;
    let corners = |r: [i32; 4]| [[r[0] as f32, r[1] as f32], [r[2] as f32, r[1] as f32], [r[2] as f32, r[3] as f32], [r[0] as f32, r[3] as f32]];
    // the backing inside the border, a dark wash under the game's panel art
    // (the map's cards'), then the border, a window's frame as the minimap's
    let inside = [f[0] + 2, f[1] + 2, f[2] - 2, f[3] - 2];
    ui.solid(3, BACK, corners(inside));
    vui.stretch_blit_ex(ui, 61, [inside[0], inside[1]], [inside[2] - inside[0], inside[3] - inside[1]], 3, 0.6);
    vui.blit_frame(ui, [f[0], f[1]], [f[2] - f[0], f[3] - f[1]], &vi::ui::frame_sets()[0], 3, 1.0, true);
    let Some(font) = ui.assets.font else { return };
    let labels: [[Vec<u16>; 8]; 2] = std::array::from_fn(|c| std::array::from_fn(|k| wide(vui.words.get(v.rows[c][k].word, vui.language))));
    let l = {
        let f = &ui.ui.fonts[font];
        layout(f.height as f32, widths(&labels, |s| f.string_width(s) as f32))
    };
    for (col, rows) in v.rows.iter().enumerate() {
        for (k, row) in rows.iter().enumerate() {
            let y = (l.top + k as f32 * l.step).round();
            let value = wide(&row.value.to_string());
            let w = ui.ui.fonts[font].string_width(&value) as f32 * l.scale;
            let x = (l.rights[col] - w).round();
            for (s, at, c) in [(&labels[col][k], l.lefts[col].round(), LABEL), (&value, x, row.color())] {
                ui.ui.text_scaled(font, at + 1.0, y + 1.0, l.scale, SHADOW, s);
                ui.ui.text_scaled(font, at, y, l.scale, c, s);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::dump::{Item, ItemStat, NetItem};

    /// A character's channel: every base stat 50, the totals as the server
    /// counts them from the base, the worn items and `fx` (the effects' share
    /// by stat), the resists' raw 10, 300 base hit points and 200 power.
    fn channel(worn: &[(i64, i64)], fx: &[(usize, i32)]) -> Stats {
        let mut s = Stats::default();
        for i in 0..7 {
            s.base[i] = 50;
        }
        s.base[8] = 300;
        s.base[10] = 200;
        for i in 22..28 {
            s.base[i] = 10;
        }
        s.stat = s.base;
        for &(id, v) in worn {
            s.stat[id as usize] += v as i32;
        }
        for &(i, v) in fx {
            s.stat[i] += v;
        }
        s
    }

    /// An inventory of one worn item giving `stats`, and one carried (not
    /// worn) giving the same, which counts for nothing.
    fn gear(stats: &[(i64, i64)]) -> ItemList {
        let it = |slot| NetItem { equip_slot: slot, item: Item { stats: stats.iter().map(|&(id, value)| ItemStat { id, value }).collect(), ..Default::default() }, ..Default::default() };
        ItemList { items: vec![it(3), it(-1)], ..Default::default() }
    }

    fn of(v: &View, stat: i32) -> Row {
        let k = ROWS.iter().flatten().position(|&(_, s)| s == stat).expect("a row");
        v.rows[k / 8][k % 8]
    }

    /// Worn items alone leave every value white; a buff of 10 strength is
    /// blue and a debuff of 5 agility red, by the amount the channel moved;
    /// the resists derive as the Summary's (a seventh of wisdom and 40 on).
    /// The control: counting no worn items, the item's strength would read as
    /// an effect's.
    #[test]
    fn an_effect_is_marked_and_gear_is_not() {
        let inv = gear(&[(0, 7), (14, 40), (24, 5)]);
        let src = |s| Source { stats: s, inv: &inv, mastery: &[0; 29], bonuses: &[0; 7], p44: 0 };
        let plain = channel(&[(0, 7), (14, 40), (24, 5)], &[]);
        let v = view(&src(&plain)).expect("a view");
        assert!(v.rows.iter().flatten().all(|r| r.shift() == Shift::Same), "gear is no effect: {v:?}");
        assert_eq!((of(&v, 0).value, of(&v, 14).value), (57, 40));
        assert_eq!(of(&v, 24).value, 50 / 7 + 15 + 40, "fire: a seventh of wisdom, 40 and the raw 15");
        let buffed = channel(&[(0, 7), (14, 40), (24, 5)], &[(0, 10), (2, -5)]);
        let v = view(&src(&buffed)).expect("a view");
        assert_eq!((of(&v, 0).value, of(&v, 0).without, of(&v, 0).shift()), (67, Some((57, 57)), Shift::Raised));
        assert_eq!((of(&v, 2).value, of(&v, 2).shift(), of(&v, 2).color()), (45, Shift::Lowered, LOWERED));
        assert_eq!(of(&v, 1).shift(), Shift::Same, "stamina untouched");
        // wisdom raised: every resist with it (its seventh), each marked
        let wise = channel(&[], &[(4, 14)]);
        let v = view(&Source { stats: &wise, inv: &ItemList::default(), mastery: &[0; 29], bonuses: &[0; 7], p44: 0 }).expect("a view");
        assert!([24, 25, 26, 27, 22, 23].iter().all(|&s| of(&v, s).shift() == Shift::Raised && Some(of(&v, s).value - 2) == of(&v, s).without.map(|w| w.1)));
        // the control: the worn item not counted
        let v = view(&Source { stats: &plain, inv: &ItemList::default(), mastery: &[0; 29], bonuses: &[0; 7], p44: 0 }).expect("a view");
        assert_eq!(of(&v, 0).shift(), Shift::Raised, "the control must fail");
    }

    /// A stat at its cap: a buff past it moves nothing and is not marked
    /// (the server keeps the seven within `GetP44Total`), one from below it
    /// is; a mastery's share counts as the base's. The control: with no cap
    /// the capped stat would read as lowered.
    #[test]
    fn the_cap_and_the_masteries_count_as_the_base() {
        let inv = gear(&[(0, 30)]);
        // 50 base + 30 worn = 80, the cap 70 (60 and a mastery's 10)
        let mut bonuses = [0; 7];
        bonuses[0] = 10;
        let capped = channel(&[(0, 20)], &[]);
        let src = |s, p44| Source { stats: s, inv: &inv, mastery: &[0; 29], bonuses: &bonuses, p44 };
        let v = view(&src(&capped, 60)).expect("a view");
        assert_eq!((of(&v, 0).value, of(&v, 0).without, of(&v, 0).shift()), (70, Some((70, 70)), Shift::Same));
        // the control: no cap
        assert_eq!(view(&src(&capped, 0)).expect("a view").rows[0][0].shift(), Shift::Lowered, "the control must fail");
        // a mastery's 5 stamina (slot 1) is the base's, whether the server
        // counts it on top (go-eqoa-server) or in its base (as Sandstorm its
        // hit points'); 6 more is an effect's
        let mut m = [0; 29];
        m[1] = 5;
        let none = ItemList::default();
        let with = |s: &Stats, m: &[i64; 29]| view(&Source { stats: s, inv: &none, mastery: m, bonuses: &[0; 7], p44: 0 }).expect("a view");
        let on_top = channel(&[], &[(1, 5)]);
        assert_eq!(of(&with(&on_top, &m), 1).shift(), Shift::Same);
        let mut in_base = channel(&[], &[(1, 5)]);
        in_base.base[1] += 5;
        assert_eq!(of(&with(&in_base, &m), 1).shift(), Shift::Same, "counted in the base");
        let buffed = channel(&[], &[(1, 11)]);
        assert_eq!(of(&with(&buffed, &m), 1).shift(), Shift::Raised);
        assert_eq!(of(&with(&on_top, &[0; 29]), 1).shift(), Shift::Raised, "the control: without the mastery, an effect's");
        // POW is never marked: worn stats move the server's maximum power off
        // its base too
        let mut pow = channel(&[], &[(10, 37)]);
        pow.stat[9] = 0;
        assert_eq!((of(&with(&pow, &[0; 29]), 10).value, of(&with(&pow, &[0; 29]), 10).shift()), (237, Shift::Same));
        assert_eq!(of(&with(&channel(&[], &[(8, 37)]), &[0; 29]), 8).shift(), Shift::Raised, "the control: HPMax's own share is");
        // nothing before the channel
        assert!(view(&Source { stats: &Stats::default(), inv: &none, mastery: &m, bonuses: &[0; 7], p44: 0 }).is_none());
        // a mastery that takes away (an alchemist's Pow -100 kind): the band
        // the other way round
        let mut minus = [0; 29];
        minus[14] = -20;
        let hp = channel(&[], &[(8, -20)]);
        assert_eq!(of(&with(&hp, &minus), 8).shift(), Shift::Same, "{:?}", of(&with(&hp, &minus), 8));
    }

    /// The frame clears the HUD's left side (the player's gauges, the
    /// effects' bar, the group's bars) and the right's (the minimap, the
    /// tracker, the target's details), and the rows fit it: eight a column,
    /// two columns side by side. The control: a frame from x 190 would be on
    /// the group's bars.
    #[test]
    fn it_fits_between_the_huds_pieces() {
        let f = FRAME;
        assert!(f[0] >= 194 + 4, "right of the group's bars (to 194)");
        assert!(f[1] >= 15 + 34 + 4, "below the effects' bar (to 49)");
        assert!(f[3] < 208, "above the crosshair and the death notice");
        assert!(f[2] < crate::tracker::COLUMN[0] && f[2] < crate::map::mini::FRAME[0], "left of the right side's pieces");
        // a font 21 high with columns 86 and 113 across (ArialUI's English)
        let l = layout(21.0, [86.0, 113.0]);
        assert!(l.lefts[0] > f[0] as f32 && l.rights[1] < f[2] as f32 && l.rights[0] + GAP <= l.lefts[1] + 1e-3, "{l:?}");
        assert!(l.top >= f[1] as f32 && l.top + 7.0 * l.step + 21.0 * l.scale <= f[3] as f32, "eight rows inside: {l:?}");
        assert!(l.scale <= SCALE && 21.0 * l.scale < l.step);
        // a wider language's words: smaller, still inside; a small font: the cap
        let w = layout(21.0, [140.0, 160.0]);
        assert!(w.scale < l.scale && w.lefts[0] >= l.lefts[0] && w.rights[0] + GAP <= w.lefts[1] + 1e-3 && w.rights[1] <= l.rights[1] + 1e-3, "{w:?}");
        assert_eq!(layout(8.0, [20.0, 20.0]).scale, SCALE, "never past SCALE");
        let reaching = [190, f[1], f[2], f[3]];
        assert!(reaching[0] < 194, "the control: on the bars");
    }

    /// The disc's own font (SKIPs without run/eqoa.iso): each column's widest
    /// label and a four-digit value fit its column with a space between them.
    #[test]
    fn the_widest_row_fits_its_column() {
        let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&iso_path) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let id = vi::ui::font_id() as i32;
        let c = esf::Container::open(disc.read("ARIALUI.CSF").expect("ARIALUI.CSF")).expect("a container");
        let font = (0..c.nodes.len()).filter(|&i| c.nodes[i].ty == 0x7000).find_map(|i| esf::font::read(&c, i).filter(|f| f.id == id)).expect("ArialUI");
        // the game's words for the rows (WORDS.TXT's English column)
        let words = [["Str", "Sta", "Agi", "Dex", "Wis", "Int", "Cha", "AC"], ["HPMax", "POW", "FR", "CR", "LR", "AR", "PR", "DR"]];
        let labels: [[Vec<u16>; 8]; 2] = std::array::from_fn(|c| std::array::from_fn(|k| wide(words[c][k])));
        let width = |s: &[u16]| font.width(&String::from_utf16_lossy(s)) as f32;
        let l = layout(font.height as f32, widths(&labels, width));
        eprintln!("ArialUI {} high: scale {}, rows {} apart", font.height, l.scale, l.step);
        assert!(l.scale >= 0.5, "readable: {l:?}");
        for col in 0..2 {
            let label = words[col].iter().map(|w| font.width(w)).max().unwrap() as f32 * l.scale;
            let value = font.width(ROOM) as f32 * l.scale;
            let room = l.rights[col] - l.lefts[col];
            assert!(label + value + font.width(" ") as f32 * l.scale <= room + 1e-3, "column {col}: {label} + {value} in {room}");
        }
        assert!(l.rights[0] + GAP <= l.lefts[1] + 1e-3, "the columns apart: {l:?}");
        // the control: a five-digit value with HPMax would not fit its column
        assert!((font.width("HPMax") + font.width(" 88888")) as f32 * l.scale > l.rights[1] - l.lefts[1] + 1e-3, "the control must fail");
    }
}
