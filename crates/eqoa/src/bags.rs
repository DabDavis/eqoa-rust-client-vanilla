//! Inventory Style at Bags (Heirloom petition 3118, Mythilles: "A bag system
//! more like EQ or a typical MMO would be preferred"): the item menu's pack
//! as a grid of the items' own icons, a stack's count on its icon, laid out
//! by Bag Sort, in place of the game's four tabbed lists. Vanilla's own.
//!
//! Nothing here acts: a cell's item opens the very popup its tab's row opens
//! in the game's list (`itemmenu`'s built menus: use or equip, inspect, quick
//! slot, move, destroy, Link Item, Give to Merc, Hide Look), so every action
//! goes out as the list sends it. This is the grid's layout, its order and
//! its moves over the cells, and its drawing.

use crate::infotext;
use net::dump::{ItemList, NetItem};
use vi::ui::{Raster, Vui, wide};

/// Bag Sort: the order the grid lays the pack out in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    /// the pack's own order (the one Move changes)
    #[default]
    Pack,
    /// worn first by slot, then equipment by where it is worn, then the rest
    Slot,
    /// the game's tabs' order (usable, equipment, charged, misc), by name
    Type,
    Name,
}

impl Sort {
    pub const ALL: [Sort; 4] = [Sort::Pack, Sort::Slot, Sort::Type, Sort::Name];

    pub fn name(self) -> &'static str {
        match self {
            Sort::Pack => "Pack Order",
            Sort::Slot => "Slot",
            Sort::Type => "Type",
            Sort::Name => "Name",
        }
    }

    /// Its word in the options' file.
    pub fn key(self) -> &'static str {
        match self {
            Sort::Pack => "pack",
            Sort::Slot => "slot",
            Sort::Type => "type",
            Sort::Name => "name",
        }
    }

    pub fn from_key(s: &str) -> Option<Sort> {
        Sort::ALL.into_iter().find(|k| k.key() == s)
    }

    /// The next (`d` > 0) or the one before.
    pub fn step(self, d: i32) -> Sort {
        let i = Sort::ALL.iter().position(|&s| s == self).unwrap_or(0) as i32;
        Sort::ALL[(i + d.signum()).rem_euclid(Sort::ALL.len() as i32) as usize]
    }
}

/// The game's tab an item's popup is built in (`itemmenu::build_linked`): a
/// worn item that is used (a clicky) its Usable tab's, other equipment its
/// Equipment tab's, then Usable, Charged, Misc; `None` for one in no tab.
pub fn tab_of(it: &NetItem) -> Option<usize> {
    if it.equip_slot != -1 && infotext::is_usable(it) {
        return Some(0);
    }
    [(1, infotext::is_wearable as fn(&NetItem) -> bool), (0, infotext::is_usable), (2, infotext::is_charged), (3, infotext::is_misc)]
        .into_iter()
        .find(|(_, keep)| keep(it))
        .map(|(t, _)| t)
}

/// The pack's indices in the grid's order.
pub fn order(inv: &ItemList, sort: Sort) -> Vec<usize> {
    let mut v: Vec<usize> = (0..inv.items.len()).collect();
    let it = |k: usize| &inv.items[k];
    let name = |k: usize| it(k).item.name.to_lowercase();
    match sort {
        Sort::Pack => {}
        Sort::Slot => v.sort_by_key(|&k| {
            let i = it(k);
            let worn = if i.equip_slot != -1 { 0 } else { 1 };
            let place = if i.equip_slot != -1 { i.equip_slot } else if i.item.wear_slot != -1 { i.item.wear_slot } else { 1000 + tab_of(i).unwrap_or(4) as i64 };
            (worn, place, k)
        }),
        Sort::Type => v.sort_by_key(|&k| (tab_of(it(k)).unwrap_or(4), name(k), k)),
        Sort::Name => v.sort_by_key(|&k| (name(k), k)),
    }
    v
}

/// The grid's columns, and the rows shown at once.
pub const COLS: i32 = 5;
pub const ROWS: i32 = 8;
/// A cell: an icon at two thirds of the list's 114 by 44.
pub const CELL: [i32; 2] = [76, 30];
const PITCH: [i32; 2] = [CELL[0] + 4, CELL[1] + 4];
/// The first cell's top left (frame points, the item menu's own).
pub const ORIGIN: [i32; 2] = [28, 52];

/// The cells: the pack's room (`inventory::pack_cap`), or what it holds
/// past it, to whole rows.
pub fn cells(items: usize) -> i32 {
    let n = items.max(crate::inventory::pack_cap()) as i32;
    (n + COLS - 1) / COLS * COLS
}

/// Cell `cell`'s rectangle with row `top` at the grid's top, `None` when it
/// is not shown.
pub fn cell_rect(cell: i32, top: i32) -> Option<[i32; 4]> {
    let (row, col) = (cell / COLS - top, cell % COLS);
    if !(0..ROWS).contains(&row) || cell < 0 {
        return None;
    }
    let (x, y) = (ORIGIN[0] + col * PITCH[0], ORIGIN[1] + row * PITCH[1]);
    Some([x, y, x + CELL[0], y + CELL[1]])
}

/// The grid's whole rectangle (its frame's inside).
pub fn grid_rect() -> [i32; 4] {
    [ORIGIN[0], ORIGIN[1], ORIGIN[0] + COLS * PITCH[0] - 4, ORIGIN[1] + ROWS * PITCH[1] - 4]
}

/// The Sort button over the grid (frame points).
pub const SORT_BUTTON: [i32; 4] = [ORIGIN[0] + 200, 12, ORIGIN[0] + COLS * PITCH[0] - 4, 34];

/// The grid's moves: the selected cell, the row at its top, the item
/// carried by Move (its pack index), and the cell under the pointer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bag {
    pub sel: i32,
    pub top: i32,
    pub carry: Option<usize>,
    pub hover: Option<i32>,
}

impl Bag {
    /// A step of the d-pad (`dx` -1 left, 1 right; `dy` rows) over `cells`
    /// cells: left and right go on into the row before or after; at an edge
    /// nothing moves. The sound: 11 moved, 9 not.
    pub fn step(&mut self, dx: i32, dy: i32, cells: i32) -> i32 {
        let to = self.sel + dx + dy * COLS;
        if cells <= 0 || to < 0 || to >= cells || (dy == 0 && dx == 0) {
            return 9;
        }
        self.sel = to;
        self.show(cells);
        11
    }

    /// The top row put where the selection is shown, and inside the cells.
    pub fn show(&mut self, cells: i32) {
        let rows = (cells + COLS - 1) / COLS;
        self.sel = self.sel.clamp(0, (cells - 1).max(0));
        let row = self.sel / COLS;
        if row < self.top {
            self.top = row;
        } else if row >= self.top + ROWS {
            self.top = row - ROWS + 1;
        }
        self.top = self.top.clamp(0, (rows - ROWS).max(0));
    }

    /// The cell under frame point `p`.
    pub fn cell_at(&self, p: [f32; 2], cells: i32) -> Option<i32> {
        (0..ROWS * COLS).map(|k| self.top * COLS + k).filter(|&c| c < cells).find(|&c| {
            cell_rect(c, self.top).is_some_and(|r| p[0] >= r[0] as f32 && p[0] < r[2] as f32 && p[1] >= r[1] as f32 && p[1] < r[3] as f32)
        })
    }

    /// The rows scrolled by the wheel (`d` rows, up negative), the selection
    /// kept in view.
    pub fn scroll(&mut self, d: i32, cells: i32) {
        let rows = (cells + COLS - 1) / COLS;
        self.top = (self.top + d).clamp(0, (rows - ROWS).max(0));
        let row = self.sel / COLS;
        if row < self.top {
            self.sel += (self.top - row) * COLS;
        } else if row >= self.top + ROWS {
            self.sel -= (row - self.top - ROWS + 1) * COLS;
        }
    }
}

/// Where a carried item goes when let go on cell `cell` of the grid in
/// `order`: the pack index of the item there (`SwapItems`, 0x003D: the item
/// moved there, the ones between shifting), the last item's past them;
/// `None` for no move.
pub fn drop_target(order: &[usize], cell: i32, carried: usize) -> Option<usize> {
    let to = match order.get(cell.max(0) as usize) {
        Some(&k) => k,
        None => *order.iter().max()?,
    };
    (to != carried).then_some(to)
}

/// An item's icon at `at`, `size` big (the game's 114 by 44 squeezed), or
/// where the cache has none the list's empty row and its "?".
pub fn draw_icon(vui: &Vui, r: &mut dyn Raster, icon: i32, at: [i32; 2], size: [i32; 2], alpha: f32) {
    if icon != 0 && icon != -1 && r.has(icon as u32) {
        r.stretch_blit(icon as u32, 0, at, size, [0, 0], [114, 44], if alpha < 1.0 { 3 } else { 0 }, alpha);
    } else {
        vui.blit_size(r, 1, at, size);
        let q = wide("?");
        let w = r.string_width(&q);
        r.draw_string([at[0] + (size[0] - w) / 2, at[1] + (size[1] - vui.font_height(r)) / 2], [1.0, 1.0, 1.0, alpha], &q);
    }
}

fn rect(r: &mut dyn Raster, c: [f32; 4], a: [i32; 2], b: [i32; 2]) {
    r.fill_fan(c, [a, [b[0], a[1]], b, [a[0], b[1]]]);
}

/// A frame `w` thick round `rc`.
pub fn outline(r: &mut dyn Raster, c: [f32; 4], rc: [i32; 4], w: i32) {
    let [l, t, rr, b] = rc;
    rect(r, c, [l - w, t - w], [rr + w, t]);
    rect(r, c, [l - w, b], [rr + w, b + w]);
    rect(r, c, [l - w, t], [l, b]);
    rect(r, c, [rr, t], [rr + w, b]);
}

/// What one cell shows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cell {
    pub icon: i32,
    pub quantity: i64,
    pub worn: bool,
    /// equipment the player cannot wear (the list's red bar, `icon2` 140)
    pub unusable: bool,
    /// the item carried by Move
    pub carried: bool,
}

/// The grid: the backdrop, each cell shown (an icon or an empty slot, a
/// stack's count at its bottom right, a worn item's gold frame, a red wash
/// on what cannot be worn), the selection's pulsing frame (cyan while an
/// item is carried), the scroll bar's thumb when the pack has more rows
/// than are shown, and the header (the room taken, the Sort button). The
/// cells' rectangles, for the pointer, go in `hits`.
pub fn draw(vui: &Vui, r: &mut dyn Raster, bag: &Bag, cells: &[Option<Cell>], used: usize, room: usize, sort: Sort, pulse: f32, hits: &mut crate::pointer::Hits) {
    let g = grid_rect();
    rect(r, [0.02, 0.02, 0.015, 0.72], [g[0] - 8, g[1] - 8], [g[2] + 8, g[3] + 8]);
    outline(r, [0.62, 0.52, 0.28, 1.0], [g[0] - 8, g[1] - 8, g[2] + 8, g[3] + 8], 1);
    // the header: "Bags 23/40" at the left, the Sort button at the right
    let white = [1.0; 4];
    let gold = [0.95, 0.82, 0.45, 1.0];
    let title = wide(&format!("Bags  {used}/{room}"));
    vui.draw_string_ex(r, &title, ORIGIN[0], 16, gold, true);
    let b = SORT_BUTTON;
    rect(r, [0.12, 0.1, 0.06, 0.9], [b[0], b[1]], [b[2], b[3]]);
    outline(r, [0.62, 0.52, 0.28, 1.0], b, 1);
    let label = wide(&format!("Sort: {}", sort.name()));
    let w = r.string_width(&label);
    // the Controller interface's: □ before it, the button that turns it
    // (a glyph, which a pointer presses as its button)
    let glyph = if crate::settings::pc_mode() { 0 } else { vui.entry(71).uv1[0] + 4 };
    let x = b[0] + (b[2] - b[0] - w - glyph) / 2;
    if glyph > 0 {
        vui.blit_ex(r, 71, [x, b[1] + (b[3] - b[1] - vui.entry(71).uv1[1]) / 2], 3, 1.0, 0);
    }
    vui.draw_string_ex(r, &label, x + glyph, b[1] + 3, white, true);
    hits.clear();
    hits.push(b, -2);
    let n = cells.len() as i32;
    for c in bag.top * COLS..(bag.top * COLS + ROWS * COLS).min(n) {
        let Some(rc) = cell_rect(c, bag.top) else { continue };
        hits.push(rc, c);
        let at = [rc[0], rc[1]];
        match cells[c as usize] {
            None => {
                vui.blit_size(r, 1, at, CELL);
                rect(r, [0.0, 0.0, 0.0, 0.35], at, [rc[2], rc[3]]);
            }
            Some(cell) => {
                draw_icon(vui, r, cell.icon, at, CELL, if cell.carried { 0.45 } else { 1.0 });
                if cell.unusable {
                    rect(r, [0.8, 0.05, 0.05, 0.3], at, [rc[2], rc[3]]);
                }
                if cell.worn {
                    outline(r, [0.95, 0.78, 0.3, 1.0], rc, 1);
                    vui.draw_string_ex(r, &wide("W"), rc[0] + 2, rc[1] + 1, gold, true);
                }
                if cell.quantity >= 2 {
                    let s = wide(&cell.quantity.to_string());
                    let w = r.string_width(&s);
                    let y = rc[3] - vui.font_height(r) + 2;
                    vui.draw_string_ex(r, &s, rc[2] - w - 2, y, white, true);
                }
            }
        }
    }
    // the hovered cell, then the selection over it
    if let Some(rc) = bag.hover.filter(|&h| h != bag.sel).and_then(|h| cell_rect(h, bag.top)) {
        outline(r, [1.0, 1.0, 1.0, 0.5], rc, 1);
    }
    if let Some(rc) = cell_rect(bag.sel, bag.top) {
        let c = if bag.carry.is_some() { [0.3, 0.9, 1.0, pulse] } else { [1.0, 1.0, 0.2, pulse] };
        outline(r, c, rc, 2);
    }
    let rows = (n + COLS - 1) / COLS;
    if rows > ROWS {
        let track = [g[2] + 3, g[1], g[2] + 6, g[3]];
        rect(r, [0.3, 0.27, 0.2, 0.6], [track[0], track[1]], [track[2], track[3]]);
        let h = (track[3] - track[1]) * ROWS / rows;
        let y = track[1] + (track[3] - track[1] - h) * bag.top / (rows - ROWS);
        rect(r, gold, [track[0], y], [track[2], y + h]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::dump::Item;

    fn item(name: &str, wear: i64, equip: i64, p4c: i64, p54: i64) -> NetItem {
        NetItem { quantity: 1, equip_slot: equip, quick_slot: -1, item: Item { name: name.into(), wear_slot: wear, p4c, p54, ..Default::default() }, ..Default::default() }
    }

    fn pack() -> ItemList {
        ItemList {
            p04: 0,
            items: vec![
                item("zinc ore", -1, -1, 0, 0),    // misc
                item("Iron Helm", 1, -1, 0, 0),    // equipment
                item("Bread", -1, -1, 2, 0),       // usable
                item("Sword", 12, 14, 0, 0),       // worn primary
                item("arrows", -1, -1, 0, 5),      // charged
                item("Cloak", 7, 8, 0, 0),         // worn cloak
            ],
        }
    }

    /// Each sort's order: the pack's own; worn by slot, then equipment by
    /// wear slot, then the rest by tab; by tab then name; by name, case
    /// ignored.
    #[test]
    fn the_sorts_order_the_pack() {
        let inv = pack();
        assert_eq!(order(&inv, Sort::Pack), vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(order(&inv, Sort::Slot), vec![5, 3, 1, 2, 4, 0], "worn cloak (8) and sword (14), the helm, then usable, charged, misc");
        assert_eq!(order(&inv, Sort::Type), vec![2, 5, 1, 3, 4, 0], "usable, equipment by name, charged, misc");
        assert_eq!(order(&inv, Sort::Name), vec![4, 2, 5, 1, 3, 0]);
        assert_eq!(tab_of(&inv.items[4]), Some(2));
        let mut clicky = item("Wand", 18, 19, 7, 0);
        assert_eq!(tab_of(&clicky), Some(0), "a worn clicky is used from Usable");
        clicky.equip_slot = -1;
        assert_eq!(tab_of(&clicky), Some(1), "and put on from Equipment");
    }

    /// The sorts' names and keys round trip, and step both ways round.
    #[test]
    fn sort_keys_and_steps() {
        for s in Sort::ALL {
            assert_eq!(Sort::from_key(s.key()), Some(s));
        }
        assert_eq!(Sort::from_key("size"), None);
        assert_eq!(Sort::Name.step(1), Sort::Pack);
        assert_eq!(Sort::Pack.step(-1), Sort::Name);
        assert_eq!(Sort::Slot.step(1), Sort::Type);
    }

    /// The grid: five across, eight rows shown; the d-pad's steps (left and
    /// right run on into the next row; nothing past the cells), the top row
    /// following the selection, the cell under a point, the wheel.
    #[test]
    fn the_grid_moves_and_scrolls() {
        crate::inventory::with_caps([60, 40], || {
            let n = cells(3);
            assert_eq!(n, 60, "the pack's room, not its items");
            let mut b = Bag::default();
            assert_eq!(b.step(-1, 0, n), 9, "nothing before the first");
            assert_eq!(b.step(1, 0, n), 11);
            assert_eq!(b.sel, 1);
            b.sel = 4;
            assert_eq!(b.step(1, 0, n), 11);
            assert_eq!(b.sel, 5, "on into the next row");
            assert_eq!(b.step(0, 1, n), 11);
            assert_eq!(b.sel, 10);
            for _ in 0..7 {
                b.step(0, 1, n);
            }
            assert_eq!((b.sel, b.top), (45, 2), "row 9 shown: the top two rows on");
            assert_eq!(b.step(0, 3, n), 9, "past the last cell");
            let r = cell_rect(45, b.top).unwrap();
            let mid = [(r[0] + r[2]) as f32 / 2.0, (r[1] + r[3]) as f32 / 2.0];
            assert_eq!(b.cell_at(mid, n), Some(45));
            assert_eq!(b.cell_at([2.0, 2.0], n), None);
            assert!(cell_rect(5, b.top).is_none(), "row 1 is above the grid now");
            b.scroll(-5, n);
            assert_eq!(b.top, 0);
            assert_eq!(b.sel / COLS, 7, "the selection kept in view");
            b.scroll(9, n);
            assert_eq!(b.top, 4, "no further than the last rows");
        });
        assert_eq!(cells(41), 45, "a pack past its room, to whole rows");
    }

    /// Move's drop: on another item, that item's place; on an empty cell,
    /// past the last; on itself, nothing.
    #[test]
    fn a_carried_item_drops_on_a_cell() {
        let order = vec![2, 0, 1];
        assert_eq!(drop_target(&order, 1, 2), Some(0));
        assert_eq!(drop_target(&order, 0, 2), None);
        assert_eq!(drop_target(&order, 7, 0), Some(2));
        assert_eq!(drop_target(&[], 0, 0), None);
    }
}
