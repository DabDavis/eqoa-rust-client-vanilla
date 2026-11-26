//! Compare Gear (Heirloom petition 3118, Mythilles: "A huge list and you
//! cannot compare gear"): an equippable item in the pack set beside what it
//! would take off if worn, stat by stat, with the difference in green (more)
//! or red (less). Vanilla's own, not the game's.
//!
//! What it is compared with is exactly what wearing it would do: the item put
//! on a copy of the list where `EquipSlotFor` puts it, by `Msg003F`'s own
//! rule (`inventory::equip`, which also clears the slots it covers: a
//! two-handed weapon takes off the primary and the shield), and the worn
//! items that came off are the ones it replaces. A ring with a free finger
//! replaces nothing, and shows only what it adds.

use crate::{infotext, inventory};
use net::dump::{ItemList, NetItem};

/// One line of the comparison: a stat (or the weapon's damage), the
/// candidate's value and what the items it replaces give together.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// the stat's id, or `DAMAGE`
    pub stat: i64,
    pub new: i64,
    pub worn: i64,
}

impl Line {
    /// What wearing the candidate changes: more is better for every line.
    pub fn diff(&self) -> i64 {
        self.new - self.worn
    }
}

/// Not a stat: the weapon's damage line.
pub const DAMAGE: i64 = -1;

/// A comparison of item `index` of the pack.
#[derive(Clone, Debug, PartialEq)]
pub struct Compare {
    pub index: usize,
    /// the pack's indices of the worn items it would take off
    pub replaced: Vec<usize>,
    pub lines: Vec<Line>,
    /// whether the player can wear it (`VIPlayer::CanUse`)
    pub usable: bool,
}

/// The worn items wearing item `index` would take off (none for an item
/// already worn, one that is not worn anywhere, or one that does not fit).
pub fn replaced(inv: &ItemList, index: usize, class: i64, level: i64) -> Vec<usize> {
    let Some(it) = inv.items.get(index).filter(|it| it.equip_slot == -1) else { return Vec::new() };
    let slot = infotext::equip_slot_for(inv, it, class, level);
    if slot < 0 {
        return Vec::new();
    }
    let mut l = inv.clone();
    if inventory::equip(&mut l, index, slot).is_none() {
        return Vec::new();
    }
    (0..inv.items.len()).filter(|&k| k != index && inv.items[k].equip_slot != -1 && l.items[k].equip_slot == -1).collect()
}

/// The order a stat is listed in: the game's own (`sStatWords`), any other
/// after them by id.
fn rank(stat: i64) -> (usize, i64) {
    if stat == DAMAGE {
        return (0, 0);
    }
    (infotext::STAT_WORDS.iter().position(|e| e.1 == stat).map_or(usize::MAX, |k| k + 1), stat)
}

fn stat_sum<'a>(items: impl Iterator<Item = &'a NetItem>, stat: i64) -> i64 {
    items.flat_map(|it| it.item.stats.iter()).filter(|s| s.id == stat).map(|s| s.value).sum()
}

/// Item `index` against what it would replace: `None` for an item that is
/// not worn equipment, or one worn already.
pub fn compare(inv: &ItemList, index: usize, class: i64, race: i64, level: i64) -> Option<Compare> {
    let it = inv.items.get(index)?;
    if !infotext::is_wearable(it) || it.equip_slot != -1 || it.item.wear_slot == -1 {
        return None;
    }
    let replaced = replaced(inv, index, class, level);
    let worn: Vec<&NetItem> = replaced.iter().map(|&k| &inv.items[k]).collect();
    let mut stats: Vec<i64> = it.item.stats.iter().map(|s| s.id).chain(worn.iter().flat_map(|w| w.item.stats.iter().map(|s| s.id))).collect();
    stats.sort_by_key(|&s| rank(s));
    stats.dedup();
    let mut lines = Vec::new();
    let worn_damage = worn.iter().map(|w| w.item.damage).max().unwrap_or(0);
    if it.item.damage > 0 || worn_damage > 0 {
        lines.push(Line { stat: DAMAGE, new: it.item.damage, worn: worn_damage });
    }
    for s in stats {
        lines.push(Line { stat: s, new: stat_sum(std::iter::once(it), s), worn: stat_sum(worn.iter().copied(), s) });
    }
    Some(Compare { index, replaced, lines, usable: inventory::can_use(it, class, race, level) })
}

/// A line's label: the game's word for the stat (`FindStatWord`), its
/// damage's (493), or the id.
pub fn label(words: &vi::strtab::StringTable, language: i32, stat: i64) -> String {
    let w = if stat == DAMAGE { Some(493) } else { infotext::stat_word(stat) };
    match w {
        Some(w) => words.get(w, language).to_string(),
        None => format!("Stat {stat}"),
    }
}

/// The colour of a difference: green more, red less, grey none.
pub fn colour(diff: i64, alpha: f32) -> [f32; 4] {
    match diff.signum() {
        1 => [0.35, 1.0, 0.35, alpha],
        -1 => [1.0, 0.35, 0.3, alpha],
        _ => [0.7, 0.7, 0.7, alpha],
    }
}

/// A difference as it is drawn: "+3", "-2", "0".
pub fn signed(diff: i64) -> String {
    if diff > 0 { format!("+{diff}") } else { diff.to_string() }
}

/// The panel's width and the columns' places in it.
pub const WIDTH: i32 = 224;
const COL_NEW: i32 = 118;
const COL_WORN: i32 = 172;
const COL_DIFF: i32 = 218;
/// The panel's width beside the item menu's grid (Heirloom petition 3267:
/// over the grid it hid the cells round the one compared), in the column
/// right of it.
pub const WIDTH_BESIDE: i32 = 200;

/// The value columns' right edges (new, worn, difference) in a panel
/// `width` wide: `WIDTH`'s own, or a narrower one's closer together.
pub fn columns(width: i32) -> [i32; 3] {
    if width >= WIDTH {
        return [COL_NEW + width - WIDTH, COL_WORN + width - WIDTH, COL_DIFF + width - WIDTH];
    }
    let diff = width - 6;
    [diff - 80, diff - 40, diff]
}
/// A line's height.
pub const ROW: i32 = 15;

/// The panel's height for `c`.
pub fn height(c: &Compare) -> i32 {
    // each replaced name (or "an empty slot"), the column heads, the lines,
    // the margins
    let names = c.replaced.len().max(1) as i32;
    ROW * (2 + names + c.lines.len().max(1) as i32) + 10 + if c.usable { 0 } else { ROW }
}

/// The comparison drawn at `at` (its top left, frame points), `WIDTH` wide:
/// a dark panel with a gold rule, the replaced items' names, then a line a
/// stat with the candidate's value, the worn value and the difference.
pub fn draw(vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster, c: &Compare, inv: &ItemList, at: [i32; 2], alpha: f32) {
    draw_w(vui, r, c, inv, at, WIDTH, alpha);
}

/// `draw`, the panel `width` wide (`columns`).
#[allow(clippy::too_many_arguments)]
pub fn draw_w(vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster, c: &Compare, inv: &ItemList, at: [i32; 2], width: i32, alpha: f32) {
    use vi::ui::wide;
    let [col_new, col_worn, col_diff] = columns(width);
    let h = height(c);
    let [x, y] = at;
    r.fill_fan([0.04, 0.035, 0.025, 0.88 * alpha], [[x, y], [x + width, y], [x + width, y + h], [x, y + h]]);
    let gold = [0.78, 0.66, 0.35, alpha];
    for (a, b) in [([x, y], [x + width, y + 1]), ([x, y + h - 1], [x + width, y + h]), ([x, y], [x + 1, y + h]), ([x + width - 1, y], [x + width, y + h])] {
        r.fill_fan(gold, [a, [b[0], a[1]], b, [a[0], b[1]]]);
    }
    let white = [1.0, 1.0, 1.0, alpha];
    let grey = [0.75, 0.75, 0.7, alpha];
    let text = |r: &mut dyn vi::ui::Raster, s: &str, px: i32, py: i32, colour: [f32; 4]| r.draw_string([px, py], colour, &wide(s));
    let right = |r: &mut dyn vi::ui::Raster, s: &str, px: i32, py: i32, colour: [f32; 4]| {
        let w = wide(s);
        let width = r.string_width(&w);
        r.draw_string([px - width, py], colour, &w);
    };
    let clip = |r: &dyn vi::ui::Raster, s: &str, room: i32| -> String {
        let mut out = s.to_string();
        while !out.is_empty() && r.string_width(&wide(&out)) > room {
            out.pop();
        }
        if out.len() < s.len() && out.len() > 1 {
            out.pop();
            out.push('.');
        }
        out
    };
    let mut py = y + 5;
    if !c.usable {
        text(r, "You cannot wear this", x + 5, py, [1.0, 0.35, 0.3, alpha]);
        py += ROW;
    }
    if c.replaced.is_empty() {
        text(r, "Fills an empty slot", x + 5, py, gold);
        py += ROW;
    } else {
        for (n, &k) in c.replaced.iter().enumerate() {
            let worn = inv.items.get(k).map_or("", |it| it.item.name.as_str());
            let lead = if n == 0 { "Replaces " } else { "and " };
            let s = clip(r, &format!("{lead}{worn}"), width - 10);
            text(r, &s, x + 5, py, gold);
            py += ROW;
        }
    }
    right(r, "New", x + col_new, py, grey);
    right(r, "Worn", x + col_worn, py, grey);
    right(r, "+/-", x + col_diff, py, grey);
    py += ROW;
    if c.lines.is_empty() {
        text(r, "No stats", x + 5, py, grey);
    }
    for l in &c.lines {
        let d = l.diff();
        let s = clip(r, &label(&vui.words, vui.language, l.stat), col_new - 45);
        text(r, &s, x + 5, py, white);
        right(r, &l.new.to_string(), x + col_new, py, white);
        right(r, &l.worn.to_string(), x + col_worn, py, grey);
        right(r, &signed(d), x + col_diff, py, colour(d, alpha));
        py += ROW;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::dump::{Item, ItemStat};

    pub(crate) fn gear(name: &str, wear: i64, equip: i64, damage: i64, stats: &[(i64, i64)]) -> NetItem {
        NetItem {
            quantity: 1,
            equip_slot: equip,
            quick_slot: -1,
            item: Item {
                name: name.into(),
                wear_slot: wear,
                damage,
                attack_type: if damage > 0 { 1 } else { 0 },
                class_mask: -1,
                race_mask: -1,
                stats: stats.iter().map(|&(id, value)| ItemStat { id, value }).collect(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn list(items: Vec<NetItem>) -> ItemList {
        let mut l = ItemList::default();
        for (k, mut it) in items.into_iter().enumerate() {
            it.inv_index = k as u32 + 1;
            l.items.push(it);
        }
        l
    }

    /// A helm against the worn helm: each stat of either, the candidate's,
    /// the worn one's and the difference; the order the game lists stats in
    /// (`sStatWords`: STR 0 before STA 2 before HP 12).
    #[test]
    fn a_helm_against_the_worn_helm() {
        let inv = list(vec![gear("Old Cap", 1, 0, 0, &[(12, 10), (0, 2)]), gear("Iron Helm", 1, -1, 0, &[(0, 5), (2, 3)])]);
        let c = compare(&inv, 1, 0, 0, 10).unwrap();
        assert_eq!(c.replaced, vec![0]);
        let got: Vec<(i64, i64, i64, i64)> = c.lines.iter().map(|l| (l.stat, l.new, l.worn, l.diff())).collect();
        assert_eq!(got, vec![(0, 5, 2, 3), (2, 3, 0, 3), (12, 0, 10, -10)]);
        assert!(c.usable);
        // the worn helm itself: nothing to compare
        assert!(compare(&inv, 0, 0, 0, 10).is_none());
    }

    /// A two-handed weapon takes off the primary and the shield (`Msg003F`'s
    /// rule): both are what it is compared with, their stats added, the
    /// damage the better of the two.
    #[test]
    fn a_two_hander_against_the_sword_and_the_shield() {
        // wear 12: a primary (14); 15: two-handed (16), clearing 14 and 15;
        // 13: a shield (17)
        let inv = list(vec![gear("Sword", 12, 14, 7, &[(0, 2)]), gear("Shield", 13, 17, 0, &[(12, 20)]), gear("Greatsword", 15, -1, 15, &[(0, 4)])]);
        let c = compare(&inv, 2, 0, 0, 10).unwrap();
        assert_eq!(c.replaced, vec![0, 1]);
        assert_eq!(c.lines[0], Line { stat: DAMAGE, new: 15, worn: 7 });
        assert_eq!(c.lines.iter().find(|l| l.stat == 12).unwrap().diff(), -20, "the shield's HP lost");
        assert_eq!(c.lines.iter().find(|l| l.stat == 0).unwrap().diff(), 2);
    }

    /// A ring with a finger free replaces nothing: everything it has is
    /// gained; with both fingers taken it replaces the second's.
    #[test]
    fn a_ring_takes_the_free_finger_first() {
        let free = list(vec![gear("Ring A", 6, 6, 0, &[(1, 3)]), gear("Ring B", 6, -1, 0, &[(1, 1)])]);
        let c = compare(&free, 1, 0, 0, 10).unwrap();
        assert!(c.replaced.is_empty());
        assert_eq!(c.lines, vec![Line { stat: 1, new: 1, worn: 0 }]);
        let full = list(vec![gear("Ring A", 6, 6, 0, &[(1, 3)]), gear("Ring C", 6, 7, 0, &[(1, 2)]), gear("Ring B", 6, -1, 0, &[(1, 1)])]);
        let c = compare(&full, 2, 0, 0, 10).unwrap();
        assert_eq!(c.replaced, vec![1], "the second finger's");
        assert_eq!(c.lines[0].diff(), -1);
    }

    /// The columns: `WIDTH`'s own as they were; a narrower panel's inside it,
    /// in order, the labels' room (to the new column less a number's) still
    /// most of a word.
    #[test]
    fn the_columns_fit_the_width() {
        assert_eq!(columns(WIDTH), [COL_NEW, COL_WORN, COL_DIFF], "the control: as before");
        let c = columns(WIDTH_BESIDE);
        assert!(c[0] < c[1] && c[1] < c[2] && c[2] < WIDTH_BESIDE, "{c:?}");
        assert!(c[0] - 45 >= 60, "a label's room: {}", c[0] - 45);
    }

    /// The difference's sign and colour, and what is not compared (an item
    /// that is not worn equipment); a class that cannot wear it is said.
    #[test]
    fn signs_colours_and_what_is_not_compared() {
        assert_eq!((signed(3), signed(-2), signed(0)), ("+3".to_string(), "-2".to_string(), "0".to_string()));
        assert_eq!(colour(1, 1.0)[1], 1.0);
        assert_eq!(colour(-1, 1.0)[0], 1.0);
        assert_eq!(colour(0, 1.0), [0.7, 0.7, 0.7, 1.0]);
        let mut food = gear("Bread", -1, -1, 0, &[]);
        food.item.p4c = 2;
        let mut helm = gear("Helm", 1, -1, 0, &[(0, 1)]);
        helm.item.class_mask = 1 << 3;
        let inv = list(vec![food, helm]);
        assert!(compare(&inv, 0, 0, 0, 10).is_none(), "bread");
        assert!(compare(&inv, 9, 0, 0, 10).is_none(), "past the end");
        assert!(!compare(&inv, 1, 0, 0, 10).unwrap().usable, "a warrior cannot wear a shadowknight's helm");
        assert!(compare(&inv, 1, 3, 0, 10).unwrap().usable);
    }
}
