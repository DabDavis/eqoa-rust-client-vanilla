//! The map's places (petition 1479, a player's: "Map should let you switch to
//! the other places even when there, i.e. in tunaria but want to see what's in
//! Oggok"; the server owner's call, 2026-11-11). The Places button (P on a
//! keyboard, Start on a pad) opens a list under the map's title: "Where I am"
//! first, then every world the map can draw (CLIENT's six with a name, and the
//! server's own worlds held here, `serverworlds`), the towns of the world shown
//! under it, by name (`markers::towns_of`: the cities, the coach towns). Up and
//! down (the arrows, W and S, the d-pad) move through it, ✕ (Enter, Space, a
//! click on a row) takes a row, △ (Escape, P, Start) closes the list.
//!
//! A world is drawn as the player's is (`Builder`, its zones read off the disc
//! the same way, kept in the same cache: a world the player has been in comes
//! back at once), in the map's own copy of its file (`Shown`), never the game's
//! library; the player's world's map waits meanwhile and comes back as it was
//! on "Where I am", with the map closed or when the player changes world. Its
//! marks are its own (the camps, the coaches, the community's markers and the
//! quest marks of that world, as `of_world` gives them); the player, the group
//! and the target are drawn only on the player's own world. A town centres the
//! view on it, close in.

use super::markers;

/// The rows shown at once; the list scrolls past them with the selection.
pub const SHOWN: usize = 12;

/// World units a window pixel a town is shown at (a town's streets and the
/// ground round it).
pub const TOWN_SCALE: f32 = 3.0;

/// How far from a town's middle (`markers_data::TOWNS`' `at`) the player is
/// in it, for Town Map First (Live petition 5650): the radius the towns were
/// found with (their spawns "about it", within 600 of the middle).
pub const TOWN_RADIUS: f32 = 600.0;

/// The town of world `world` the player at `p` (x, z) is in: the nearest
/// within `TOWN_RADIUS`, none outside every town.
pub fn town_at(world: i32, p: [f32; 2]) -> Option<usize> {
    let d2 = |t: &markers::Town| (t.at[0] - p[0]).powi(2) + (t.at[1] - p[1]).powi(2);
    super::markers_data::TOWNS
        .iter()
        .enumerate()
        .filter(|(_, t)| t.world == world && d2(t) <= TOWN_RADIUS * TOWN_RADIUS)
        .min_by(|a, b| d2(a.1).total_cmp(&d2(b.1)))
        .map(|(i, _)| i)
}

/// A row of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// back to the player's world, following the player
    Here,
    /// a world, by its `gWorldInfo` index (or the server's id past CLIENT's six)
    World(i32),
    /// a town (`markers_data::TOWNS`' index)
    Town(usize),
}

/// The list's state.
#[derive(Clone, Debug, Default)]
pub struct Picker {
    pub open: bool,
    /// the selected row and the first shown
    pub sel: usize,
    pub top: usize,
}

/// The rows: "Where I am", the worlds `worlds` (in order), the towns of world
/// `shown` under it by name.
pub fn rows(worlds: &[i32], shown: i32) -> Vec<Row> {
    let mut v = vec![Row::Here];
    for &w in worlds {
        v.push(Row::World(w));
        if w == shown {
            let mut towns: Vec<(usize, &str)> = markers::towns_of(w).map(|(i, t)| (i, t.name)).collect();
            towns.sort_by_key(|(_, n)| n.to_ascii_lowercase());
            v.extend(towns.into_iter().map(|(i, _)| Row::Town(i)));
        }
    }
    v
}

/// A town's world and place.
pub fn town(i: usize) -> Option<(i32, [f32; 2])> {
    super::markers_data::TOWNS.get(i).map(|t| (t.world, t.at))
}

/// A town's name.
pub fn town_name(i: usize) -> &'static str {
    super::markers_data::TOWNS.get(i).map_or("", |t| t.name)
}

impl Picker {
    /// The list opened on the row of what is shown (`at`: the world shown, or
    /// "Where I am" while it is the player's).
    pub fn show(&mut self, rows: &[Row], at: Row) {
        self.open = true;
        self.sel = rows.iter().position(|&r| r == at).unwrap_or(0);
        self.scroll(rows.len());
    }

    /// Up (`d` -1) or down (1) a row, stopping at the ends.
    pub fn step(&mut self, d: i32, n: usize) {
        if n == 0 {
            return;
        }
        self.sel = (self.sel as i32 + d).clamp(0, n as i32 - 1) as usize;
        self.scroll(n);
    }

    /// The row `r` selected (a world's taken: its towns come under it).
    pub fn select(&mut self, rows: &[Row], r: Row) {
        if let Some(i) = rows.iter().position(|&x| x == r) {
            self.sel = i;
        }
        self.scroll(rows.len());
    }

    /// The first row shown kept so the selection is in view.
    fn scroll(&mut self, n: usize) {
        self.sel = self.sel.min(n.saturating_sub(1));
        if self.sel < self.top {
            self.top = self.sel;
        }
        if self.sel >= self.top + SHOWN {
            self.top = self.sel + 1 - SHOWN;
        }
        self.top = self.top.min(n.saturating_sub(SHOWN));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "Where I am" first, every world, the shown world's towns under it by
    /// name (Oggok under Rathe, Freeport under Tunaria) and no other world's.
    /// The control: Tunaria shown, Rathe has no towns under it.
    #[test]
    fn the_rows_are_the_worlds_and_the_shown_worlds_towns() {
        let oggok = super::super::markers_data::TOWNS.iter().position(|t| t.name == "Oggok").unwrap();
        let freeport = super::super::markers_data::TOWNS.iter().position(|t| t.name == "Freeport").unwrap();
        let r = rows(&[0, 1, 2], 1);
        assert_eq!(r[..3], [Row::Here, Row::World(0), Row::World(1)]);
        assert!(r.contains(&Row::Town(oggok)) && !r.contains(&Row::Town(freeport)));
        assert_eq!(*r.last().unwrap(), Row::World(2));
        let t = rows(&[0, 1, 2], 0);
        let at = t.iter().position(|&x| x == Row::Town(freeport)).unwrap();
        assert!(at > 1 && at < t.iter().position(|&x| x == Row::World(1)).unwrap());
        assert!(!t.contains(&Row::Town(oggok)), "the control: Rathe's towns only under Rathe shown");
        let names: Vec<&str> = t.iter().filter_map(|r| if let Row::Town(i) = r { Some(town_name(*i)) } else { None }).collect();
        let mut sorted = names.clone();
        sorted.sort_by_key(|n| n.to_ascii_lowercase());
        assert_eq!(names, sorted, "by name");
    }

    /// In a town: Freeport's middle and a step inside its radius are
    /// Freeport, the nearest when two are near; the controls: past the radius
    /// nowhere, and Freeport's place in another world nothing.
    #[test]
    fn the_town_the_player_is_in() {
        let freeport = super::super::markers_data::TOWNS.iter().position(|t| t.name == "Freeport").unwrap();
        let at = super::super::markers_data::TOWNS[freeport].at;
        assert_eq!(town_at(0, at), Some(freeport));
        assert_eq!(town_at(0, [at[0] + 400.0, at[1] - 300.0]), Some(freeport), "500 from its middle");
        assert_eq!(town_at(0, [at[0] + 700.0, at[1]]), None, "the control: outside it");
        assert_eq!(town_at(1, at), None, "the control: another world");
        let good = super::super::markers_data::TOWNS.iter().position(|t| t.name == "Hazinak (Good)").unwrap();
        let g = super::super::markers_data::TOWNS[good].at;
        assert_eq!(town_at(0, [g[0] + 10.0, g[1] + 10.0]), Some(good), "the nearer of the two Hazinaks");
    }

    /// The selection stops at the ends and the list scrolls to keep it shown.
    #[test]
    fn the_list_scrolls_with_the_selection() {
        let r = rows(&[0, 1, 2], 0);
        let mut p = Picker::default();
        p.show(&r, Row::Here);
        p.step(-1, r.len());
        assert_eq!((p.sel, p.top), (0, 0));
        for _ in 0..SHOWN {
            p.step(1, r.len());
        }
        assert_eq!((p.sel, p.top), (SHOWN, 1));
        for _ in 0..100 {
            p.step(1, r.len());
        }
        assert_eq!(p.sel, r.len() - 1);
        assert_eq!(p.top, r.len() - SHOWN);
        p.select(&r, Row::World(1));
        assert!(p.top <= p.sel && p.sel < p.top + SHOWN);
    }
}
