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

/// The bank's filter (Heirloom petition 3265: "filter the bank by equipment,
/// gems and so on"): which items its grids show. The client's own reading of
/// the item records, nothing asked of a server: equipment is what is worn
/// (`wear_slot`), a consumable what the game lists with Use (food, drink,
/// potions, the clickies: `infotext::is_usable`), a crafting item what a
/// server marks a component (go-eqoa-server's `craft`, the record's 23rd
/// word, which the game lists under Charged) or a gem, and a gem a crafting
/// stone by its name (`GEM_WORDS`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Equipment,
    Gems,
    Crafting,
    Consumables,
}

impl Filter {
    pub const ALL: [Filter; 5] = [Filter::All, Filter::Equipment, Filter::Gems, Filter::Crafting, Filter::Consumables];

    pub fn name(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Equipment => "Equipment",
            Filter::Gems => "Gems",
            Filter::Crafting => "Crafting",
            Filter::Consumables => "Consumables",
        }
    }

    /// The next (`d` > 0) or the one before, round.
    pub fn step(self, d: i32) -> Filter {
        let i = Filter::ALL.iter().position(|&f| f == self).unwrap_or(0) as i32;
        Filter::ALL[(i + d.signum()).rem_euclid(Filter::ALL.len() as i32) as usize]
    }

    /// Whether item `it` is shown under this filter.
    pub fn keeps(self, it: &NetItem) -> bool {
        let worn = it.item.wear_slot != -1;
        match self {
            Filter::All => true,
            Filter::Equipment => worn,
            Filter::Gems => !worn && !infotext::is_usable(it) && is_gem(&it.item.name),
            Filter::Crafting => !worn && !infotext::is_usable(it) && (it.item.p54 != 0 || is_gem(&it.item.name)),
            Filter::Consumables => !worn && infotext::is_usable(it),
        }
    }
}

/// The words a gem's name holds (the jewelcrafting stones' names, and "gem"
/// itself): a whole word of the name, any case.
const GEM_WORDS: [&str; 53] = [
    "gem", "jewel", "diamond", "ruby", "sapphire", "emerald", "pearl", "opal", "jade", "jasper", "onyx", "amber", "topaz", "garnet", "amethyst", "ametrine",
    "citrine", "quartz", "peridot", "bloodstone", "carnelian", "malachite", "hematite", "lapis", "lazuli", "lazulite", "moonstone", "nephrite", "agate",
    "beryl", "aquamarine", "heliodor", "achroite", "amblygonite", "apatite", "dravite", "enstatite", "idocrase", "indicolite", "jacinth", "kyanite",
    "leucite", "pectolite", "rhodizite", "rubellite", "schorl", "sinhalite", "turquoise", "zircon", "tourmaline", "spinel", "sunstone", "starstone",
];

/// Whether an item's name names a gem (`GEM_WORDS`, whole words).
pub fn is_gem(name: &str) -> bool {
    name.split(|c: char| !c.is_ascii_alphabetic()).any(|w| GEM_WORDS.iter().any(|g| w.eq_ignore_ascii_case(g)))
}

/// `order`, then only what `filter` keeps.
pub fn order_filtered(inv: &ItemList, sort: Sort, filter: Filter) -> Vec<usize> {
    order(inv, sort).into_iter().filter(|&k| filter.keeps(&inv.items[k])).collect()
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

/// A grid's place and shape: its first cell's top left (frame points), its
/// columns and the rows shown at once. The item menu's is `PACK`; the bank's
/// two (`bank`, Heirloom petition 3265) are narrower.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    pub origin: [i32; 2],
    pub cols: i32,
    pub rows: i32,
}

/// The item menu's grid.
pub const PACK: Grid = Grid { origin: ORIGIN, cols: COLS, rows: ROWS };

impl Grid {
    /// Cell `cell`'s rectangle with row `top` at the grid's top, `None` when
    /// it is not shown.
    pub fn cell_rect(&self, cell: i32, top: i32) -> Option<[i32; 4]> {
        let (row, col) = (cell / self.cols - top, cell % self.cols);
        if !(0..self.rows).contains(&row) || cell < 0 {
            return None;
        }
        let (x, y) = (self.origin[0] + col * PITCH[0], self.origin[1] + row * PITCH[1]);
        Some([x, y, x + CELL[0], y + CELL[1]])
    }

    /// The grid's whole rectangle (its frame's inside).
    pub fn rect(&self) -> [i32; 4] {
        [self.origin[0], self.origin[1], self.origin[0] + self.cols * PITCH[0] - 4, self.origin[1] + self.rows * PITCH[1] - 4]
    }

    /// The cells for `items` in a room of `room`: the room, or what is held
    /// past it, to whole rows.
    pub fn cells(&self, items: usize, room: usize) -> i32 {
        let n = items.max(room) as i32;
        (n + self.cols - 1) / self.cols * self.cols
    }
}

/// The cells: the pack's room (`inventory::pack_cap`), or what it holds
/// past it, to whole rows.
pub fn cells(items: usize) -> i32 {
    PACK.cells(items, crate::inventory::pack_cap())
}

/// Cell `cell`'s rectangle with row `top` at the grid's top, `None` when it
/// is not shown.
pub fn cell_rect(cell: i32, top: i32) -> Option<[i32; 4]> {
    PACK.cell_rect(cell, top)
}

/// The grid's whole rectangle (its frame's inside).
pub fn grid_rect() -> [i32; 4] {
    PACK.rect()
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
        self.step_in(&PACK, dx, dy, cells)
    }

    /// `step` over grid `g`.
    pub fn step_in(&mut self, g: &Grid, dx: i32, dy: i32, cells: i32) -> i32 {
        let to = self.sel + dx + dy * g.cols;
        if cells <= 0 || to < 0 || to >= cells || (dy == 0 && dx == 0) {
            return 9;
        }
        self.sel = to;
        self.show_in(g, cells);
        11
    }

    /// The top row put where the selection is shown, and inside the cells.
    pub fn show(&mut self, cells: i32) {
        self.show_in(&PACK, cells);
    }

    /// `show` over grid `g`.
    pub fn show_in(&mut self, g: &Grid, cells: i32) {
        let rows = (cells + g.cols - 1) / g.cols;
        self.sel = self.sel.clamp(0, (cells - 1).max(0));
        let row = self.sel / g.cols;
        if row < self.top {
            self.top = row;
        } else if row >= self.top + g.rows {
            self.top = row - g.rows + 1;
        }
        self.top = self.top.clamp(0, (rows - g.rows).max(0));
    }

    /// The cell under frame point `p`.
    pub fn cell_at(&self, p: [f32; 2], cells: i32) -> Option<i32> {
        self.cell_at_in(&PACK, p, cells)
    }

    /// `cell_at` over grid `g`.
    pub fn cell_at_in(&self, g: &Grid, p: [f32; 2], cells: i32) -> Option<i32> {
        (0..g.rows * g.cols).map(|k| self.top * g.cols + k).filter(|&c| c < cells).find(|&c| {
            g.cell_rect(c, self.top).is_some_and(|r| p[0] >= r[0] as f32 && p[0] < r[2] as f32 && p[1] >= r[1] as f32 && p[1] < r[3] as f32)
        })
    }

    /// The rows scrolled by the wheel (`d` rows, up negative), the selection
    /// kept in view.
    pub fn scroll(&mut self, d: i32, cells: i32) {
        self.scroll_in(&PACK, d, cells);
    }

    /// `scroll` over grid `g`.
    pub fn scroll_in(&mut self, g: &Grid, d: i32, cells: i32) {
        let rows = (cells + g.cols - 1) / g.cols;
        self.top = (self.top + d).clamp(0, (rows - g.rows).max(0));
        let row = self.sel / g.cols;
        if row < self.top {
            self.sel += (self.top - row) * g.cols;
        } else if row >= self.top + g.rows {
            self.sel -= (row - self.top - g.rows + 1) * g.cols;
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
    draw_grid(vui, r, &PACK, bag, cells, pulse, true, 0, hits);
}

/// A grid's cells (`draw`'s, and the bank's two): the backdrop, each cell
/// shown (an icon or an empty slot, a stack's count at its bottom right, a
/// worn item's gold frame, a red wash on what cannot be worn), the hovered
/// cell, the selection's pulsing frame (cyan while an item is carried; only
/// while `active`), and the scroll bar's thumb when there are more rows than
/// are shown. Each cell's rectangle goes in `hits` as `base` and its number.
#[allow(clippy::too_many_arguments)]
pub fn draw_grid(vui: &Vui, r: &mut dyn Raster, grid: &Grid, bag: &Bag, cells: &[Option<Cell>], pulse: f32, active: bool, base: i32, hits: &mut crate::pointer::Hits) {
    let (white, gold) = ([1.0; 4], [0.95, 0.82, 0.45, 1.0]);
    let g = grid.rect();
    rect(r, [0.02, 0.02, 0.015, 0.72], [g[0] - 8, g[1] - 8], [g[2] + 8, g[3] + 8]);
    outline(r, [0.62, 0.52, 0.28, 1.0], [g[0] - 8, g[1] - 8, g[2] + 8, g[3] + 8], 1);
    let (cols, shown) = (grid.cols, grid.rows);
    let n = cells.len() as i32;
    for c in bag.top * cols..(bag.top * cols + shown * cols).min(n) {
        let Some(rc) = grid.cell_rect(c, bag.top) else { continue };
        hits.push(rc, base + c);
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
    if let Some(rc) = bag.hover.filter(|&h| h != bag.sel || !active).and_then(|h| grid.cell_rect(h, bag.top)) {
        outline(r, [1.0, 1.0, 1.0, 0.5], rc, 1);
    }
    if let Some(rc) = grid.cell_rect(bag.sel, bag.top).filter(|_| active) {
        let c = if bag.carry.is_some() { [0.3, 0.9, 1.0, pulse] } else { [1.0, 1.0, 0.2, pulse] };
        outline(r, c, rc, 2);
    }
    let rows = (n + cols - 1) / cols;
    if rows > shown {
        let track = [g[2] + 3, g[1], g[2] + 6, g[3]];
        rect(r, [0.3, 0.27, 0.2, 0.6], [track[0], track[1]], [track[2], track[3]]);
        let h = (track[3] - track[1]) * shown / rows;
        let y = track[1] + (track[3] - track[1] - h) * bag.top / (rows - shown);
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

    /// The bank's filter: each kind keeps its own (a helm Equipment, bread
    /// and a portal stone Consumables, a component Crafting, a sapphire and a
    /// "Polished Gem" Gems and Crafting), All keeps everything, and a worn
    /// "Pearl Earring" is equipment, not a gem; the control: ore is crafting
    /// but no gem, and "Pearlescent" no pearl.
    #[test]
    fn the_filter_keeps_its_own() {
        let mut ore = item("iron ore", -1, -1, 0, 1);
        ore.item.max_stack = 999;
        let sapphire = item("Sapphire", -1, -1, 0, 1);
        let polished = item("Polished Gem", -1, -1, 0, 0);
        let helm = item("Iron Helm", 1, -1, 0, 0);
        let earring = item("Pearl Earring", 9, -1, 0, 0);
        let bread = item("Bread", -1, -1, 2, 0);
        let stone = item("Portal Stone", -1, -1, 7, 0);
        let shiny = item("Pearlescent Shard", -1, -1, 0, 0);
        let all = [&ore, &sapphire, &polished, &helm, &earring, &bread, &stone, &shiny];
        let kept = |f: Filter| all.iter().filter(|it| f.keeps(it)).map(|it| it.item.name.as_str()).collect::<Vec<_>>();
        assert_eq!(kept(Filter::All).len(), all.len());
        assert_eq!(kept(Filter::Equipment), vec!["Iron Helm", "Pearl Earring"]);
        assert_eq!(kept(Filter::Gems), vec!["Sapphire", "Polished Gem"]);
        assert_eq!(kept(Filter::Crafting), vec!["iron ore", "Sapphire", "Polished Gem"]);
        assert_eq!(kept(Filter::Consumables), vec!["Bread", "Portal Stone"]);
        assert!(!is_gem("Pearlescent Shard") && is_gem("Black Star Gem") && is_gem("cat's eye agate"));
        let ale = item("Amber Ale", -1, -1, 2, 0);
        assert!(!Filter::Gems.keeps(&ale) && Filter::Consumables.keeps(&ale), "a drink named for a stone");
        assert_eq!(Filter::Consumables.step(1), Filter::All);
        assert_eq!(Filter::All.step(-1), Filter::Consumables);
        let inv = ItemList { p04: 0, items: vec![ore.clone(), helm.clone(), sapphire.clone()] };
        assert_eq!(order_filtered(&inv, Sort::Name, Filter::Crafting), vec![0, 2]);
        assert_eq!(order_filtered(&inv, Sort::Name, Filter::All), vec![1, 0, 2], "the sort kept");
    }

    /// A grid of its own shape (the bank's two columns): its cells, its
    /// moves and the cell under a point; the control: the item menu's grid
    /// is `PACK`'s, as before.
    #[test]
    fn a_grid_of_another_shape() {
        let g = Grid { origin: [460, 60], cols: 2, rows: 7 };
        assert_eq!(g.cells(3, 40), 40);
        assert_eq!(g.cells(41, 40), 42);
        assert_eq!(g.cell_rect(3, 0), Some([540, 94, 616, 124]));
        assert_eq!(g.rect(), [460, 60, 616, 294]);
        let mut b = Bag::default();
        assert_eq!(b.step_in(&g, 0, 1, 40), 11);
        assert_eq!(b.sel, 2, "a row down is two cells");
        for _ in 0..7 {
            b.step_in(&g, 0, 1, 40);
        }
        assert_eq!((b.sel, b.top), (16, 2));
        let r = g.cell_rect(16, b.top).unwrap();
        assert_eq!(b.cell_at_in(&g, [(r[0] + 1) as f32, (r[1] + 1) as f32], 40), Some(16));
        assert_eq!(cell_rect(0, 0), Some([ORIGIN[0], ORIGIN[1], ORIGIN[0] + CELL[0], ORIGIN[1] + CELL[1]]), "the control");
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
