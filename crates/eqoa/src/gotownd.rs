//! `VIWndGoto` (CLIENT `viwndgoto.cpp`): the GM's teleporter, `VIWndGame`'s
//! mode 4 (TEST > Navigation > Goto Zone, `OpenGoto`). A box at (100, 100)
//! with the world's name, the grid square ("Col n, Row n", words 982 and
//! 983) and the zone's name there (`ZoneName`, 641 outside the world's
//! bounds). Up and down change the column, left and right the row (both
//! held to `gWorldInfo`'s bounds), Square and Circle the world (0..5, the
//! square back to 0, 0); Enter hands the three to `SetP50308` (the client
//! moves there at the frame's end, `MoveToGrid`) and closes, Triangle
//! closes. The focus (295) starts it on the player's world at 0, 0.
//!
//! A pointer (`pointer`, the client's own): the world's line, left half the
//! previous world, right half the next; the rest of the box goes (Enter).

use crate::pointer::{Hits, key};
use vi::ui::{Raster, Vui, wide};

/// What the teleporter tells the game window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GotoUp {
    /// Enter: `SetP50308(world, column, row)`, then 260
    Go { world: i32, col: i32, row: i32 },
    /// 260
    Close,
}

/// `gWorldInfo` as the window reads it: each world's name word and grid
/// bounds (columns, then rows).
#[derive(Clone, Copy, Debug, Default)]
pub struct Worlds {
    pub words: [i32; 6],
    pub bounds: [Option<[i32; 4]>; 6],
}

#[derive(Default)]
pub struct Goto {
    /// `m_y`: the row
    pub y: i32,
    /// `m_x`: the column
    pub x: i32,
    /// `m_world`: -1 until the focus
    pub world: i32,
    pub visible: bool,
    /// the world's line: 0 its left half, 1 its right; 2 the rest
    hits: Hits,
}

impl Goto {
    /// The constructor: no world yet.
    pub fn new() -> Self {
        Goto { world: -1, ..Default::default() }
    }

    /// `OnMessage` 295: the player's world, at 0, 0.
    pub fn focus(&mut self, world: i32) {
        self.world = world;
        self.y = 0;
        self.x = 0;
        self.visible = true;
    }

    /// `OnKeyDown`.
    pub fn key(&mut self, w: &Worlds, key: i32) -> Option<GotoUp> {
        match key {
            13 | 134 => return Some(GotoUp::Go { world: self.world, col: self.x, row: self.y }),
            19 | 138 => self.change_column(w, 1),
            20 | 139 => self.change_column(w, -1),
            17 | 136 => self.change_row(w, -1),
            18 | 137 => self.change_row(w, 1),
            22 | 133 => self.change_world(-1),
            21 | 135 => self.change_world(1),
            27 | 132 => return Some(GotoUp::Close),
            _ => {}
        }
        None
    }

    /// `gWorldInfo[m_world]`'s bounds (a world the table lacks has none, and
    /// nothing holds the square: the console reads past the table there).
    fn bounds(&self, w: &Worlds) -> Option<[i32; 4]> {
        usize::try_from(self.world).ok().and_then(|k| w.bounds.get(k).copied().flatten())
    }

    /// `ChangeRow(d)`: held to rows `m_p14`..`m_p18`.
    fn change_row(&mut self, w: &Worlds, d: i32) {
        self.y = self.y.wrapping_add(d);
        if let Some(b) = self.bounds(w) {
            let y = if self.y < b[2] { b[2] } else { self.y };
            self.y = if y > b[3] { b[3] } else { y };
        }
    }

    /// `ChangeColumn(d)`: held to columns `m_p0C`..`m_p10`.
    fn change_column(&mut self, w: &Worlds, d: i32) {
        self.x = self.x.wrapping_add(d);
        if let Some(b) = self.bounds(w) {
            let x = if self.x < b[0] { b[0] } else { self.x };
            self.x = if x > b[1] { b[1] } else { x };
        }
    }

    /// `ChangeWorld(d)`: 0..5, the square back to 0, 0.
    fn change_world(&mut self, d: i32) {
        self.x = 0;
        self.y = 0;
        self.world = self.world.wrapping_add(d).clamp(0, 5);
    }

    /// A pointer at frame point `p`: the keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.hits.at(p) {
            Some(0) => vec![key::SQUARE],
            Some(1) => vec![key::CIRCLE],
            Some(_) => vec![key::CROSS],
            None => Vec::new(),
        }
    }

    /// `ZoneName(world, row, col)`: 641 out of the world's bounds.
    pub fn zone_name(&self, w: &Worlds) -> u16 {
        let Some(b) = self.bounds(w) else { return 641 };
        if self.x < b[0] || b[1] < self.x || self.y < b[2] || b[3] < self.y {
            return 641;
        }
        crate::zones::zone(self.world, self.y, self.x).unwrap_or(641)
    }

    /// `OnDrawSelf`: the box, and for a world of the table its three lines
    /// (`DrawStringEx`, shadowed).
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, w: &Worlds) {
        let white = [1.0; 4];
        let (at, size) = ([100, 100], [330, 70]);
        vui.stretch_blit_ex(r, 61, at, size, 3, 0.75);
        self.hits.clear();
        self.hits.push([at[0], at[1], at[0] + size[0], at[1] + size[1]], 2);
        let mut pos = [at[0] + 10, at[1] + 6];
        if !(0..6).contains(&self.world) {
            return;
        }
        let word = |id: i32| wide(vui.words.get(id, vui.language));
        let h = vui.font_height(r) - 4;
        self.hits.push([at[0], pos[1], at[0] + size[0] / 2, pos[1] + h], 0);
        self.hits.push([at[0] + size[0] / 2, pos[1], at[0] + size[0], pos[1] + h], 1);
        vui.draw_string_ex(r, &word(w.words[self.world as usize]), pos[0], pos[1], white, true);
        pos[1] += h;
        let line = format!("{} {}, {} {}", vui.words.get(982, vui.language), self.x, vui.words.get(983, vui.language), self.y);
        vui.draw_string_ex(r, &wide(&line), pos[0], pos[1], white, true);
        pos[1] += h;
        vui.draw_string_ex(r, &word(self.zone_name(w) as i32), pos[0], pos[1], white, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worlds() -> Worlds {
        let mut w = Worlds::default();
        w.bounds[0] = Some([-1, 3, -2, 4]);
        w.bounds[1] = Some([0, 1, 0, 1]);
        w
    }

    /// Up and down the column, left and right the row, each held to the
    /// world's bounds; Square and Circle the world, the square back to 0, 0,
    /// held to 0..5; Enter goes, Triangle closes.
    #[test]
    fn the_keys_move_the_square_and_the_world() {
        let w = worlds();
        let mut g = Goto::new();
        g.focus(0);
        for _ in 0..9 {
            g.key(&w, 138);
        }
        assert_eq!((g.x, g.y), (3, 0), "the column at its bound");
        for _ in 0..9 {
            g.key(&w, 136);
        }
        assert_eq!((g.x, g.y), (3, -2), "the row at its bound");
        g.key(&w, 18);
        assert_eq!(g.key(&w, 13), Some(GotoUp::Go { world: 0, col: 3, row: -1 }));
        g.key(&w, 135);
        assert_eq!((g.world, g.x, g.y), (1, 0, 0), "the next world, at 0, 0");
        for _ in 0..9 {
            g.key(&w, 135);
        }
        assert_eq!(g.world, 5, "five at most");
        for _ in 0..9 {
            g.key(&w, 133);
        }
        assert_eq!(g.world, 0, "0 at least");
        assert_eq!(g.key(&w, 132), Some(GotoUp::Close));
        // the control: a key it has no case for moves nothing
        assert_eq!(g.key(&w, 140), None);
        assert_eq!((g.world, g.x, g.y), (0, 0, 0));
    }

    /// The zone's name: 641 out of the bounds, else the world's grid.
    #[test]
    fn the_zone_name_is_641_outside_the_bounds() {
        let w = worlds();
        let mut g = Goto::new();
        g.focus(1);
        g.x = 2;
        assert_eq!(g.zone_name(&w), 641);
        g.x = 0;
        assert_eq!(g.zone_name(&w), crate::zones::zone(1, 0, 0).unwrap_or(641));
        g.focus(-1);
        assert_eq!(g.zone_name(&w), 641, "no world, no name");
    }

    /// Through `pointer::testing`'s null raster: the world's line goes back
    /// or on by its half, the box goes; nothing outside it, and nothing
    /// before a draw has noted where.
    #[test]
    fn a_pointer_changes_the_world_or_goes() {
        use crate::pointer::testing::{NullRaster, vui};
        let (vui, mut r) = (vui(), NullRaster);
        let w = worlds();
        let mut g = Goto::new();
        g.focus(0);
        assert!(g.point([150.0, 110.0]).is_empty(), "not drawn: no hit notes");
        g.draw(&vui, &mut r, &w);
        assert_eq!(g.point([150.0, 110.0]), vec![key::SQUARE]);
        assert_eq!(g.point([380.0, 110.0]), vec![key::CIRCLE]);
        assert_eq!(g.point([200.0, 150.0]), vec![key::CROSS]);
        assert!(g.point([50.0, 50.0]).is_empty());
        // a world the table lacks draws the box alone: it only goes
        g.world = 7;
        g.draw(&vui, &mut r, &w);
        assert_eq!(g.point([150.0, 110.0]), vec![key::CROSS]);
    }
}
