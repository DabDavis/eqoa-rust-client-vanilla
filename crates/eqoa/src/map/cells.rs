//! The map's grid. Every world's zones are 2,000-unit squares on a grid
//! aligned to 2,000, or lie inside one such square (Lavastorm's first zone
//! is x 4357..6000; measured over all six worlds' proxies), so a zone is a
//! cell and its tile is the cell: 512 texels across (`TILE`, 3.9 world units
//! a texel), 128 in the world's overview (`OVERVIEW`). The area names are
//! the game's own: `ZoneName`'s grid by the cell (`UpdateLocation`'s row
//! and column, less one) within `gWorldInfo`'s bounds, words out of
//! WORDS.TXT.

/// A cell's side, world units (`VIWorld`'s grid cell, 0x8220).
pub const CELL: f32 = 2000.0;
/// A tile's texels across.
pub const TILE: u32 = 512;
/// A cell's texels across in the overview.
pub const OVERVIEW: u32 = 128;
/// The overview's mip levels: down to a texel a cell, so a cell's region is
/// whole at every level.
pub const OVERVIEW_LEVELS: u32 = 8;

/// The cells a world's zones take.
#[derive(Clone, Debug, PartialEq)]
pub struct Cells {
    /// the first column and row (cells counted from the world's origin)
    pub col0: i32,
    pub row0: i32,
    pub cols: i32,
    pub rows: i32,
    /// the zone in each cell, rows from the north
    pub zone_at: Vec<Option<usize>>,
    /// each zone's cell
    pub cell_of: Vec<Option<usize>>,
}

impl Cells {
    /// Out of each zone's bounds (x0, z0, x1, z1): a zone is the cell its
    /// middle is in. A second zone in a cell is left out of the map.
    pub fn new(bounds: &[[f32; 4]]) -> Self {
        let at: Vec<(i32, i32)> = bounds.iter().map(|b| (((b[0] + b[2]) / 2.0 / CELL).floor() as i32, ((b[1] + b[3]) / 2.0 / CELL).floor() as i32)).collect();
        let (col0, row0) = at.iter().fold((i32::MAX, i32::MAX), |a, c| (a.0.min(c.0), a.1.min(c.1)));
        let (col1, row1) = at.iter().fold((i32::MIN, i32::MIN), |a, c| (a.0.max(c.0), a.1.max(c.1)));
        if at.is_empty() {
            return Cells { col0: 0, row0: 0, cols: 0, rows: 0, zone_at: Vec::new(), cell_of: Vec::new() };
        }
        let (cols, rows) = (col1 - col0 + 1, row1 - row0 + 1);
        let mut zone_at = vec![None; (cols * rows) as usize];
        let mut cell_of = vec![None; bounds.len()];
        for (z, &(c, r)) in at.iter().enumerate() {
            let k = ((r - row0) * cols + (c - col0)) as usize;
            if zone_at[k].is_none() {
                zone_at[k] = Some(z);
                cell_of[z] = Some(k);
            }
        }
        Cells { col0, row0, cols, rows, zone_at, cell_of }
    }

    pub fn len(&self) -> usize {
        self.zone_at.len()
    }

    /// The cell's column and row from the world's origin.
    pub fn col_row(&self, cell: usize) -> (i32, i32) {
        (self.col0 + cell as i32 % self.cols.max(1), self.row0 + cell as i32 / self.cols.max(1))
    }

    /// The cell's world rectangle (x0, z0, x1, z1).
    pub fn rect(&self, cell: usize) -> [f32; 4] {
        let (c, r) = self.col_row(cell);
        [c as f32 * CELL, r as f32 * CELL, (c + 1) as f32 * CELL, (r + 1) as f32 * CELL]
    }

    /// The world rectangle every cell together takes.
    pub fn extent(&self) -> [f32; 4] {
        [self.col0 as f32 * CELL, self.row0 as f32 * CELL, (self.col0 + self.cols) as f32 * CELL, (self.row0 + self.rows) as f32 * CELL]
    }

    /// The cell at world (x, z), if it is one of the world's.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn at(&self, x: f32, z: f32) -> Option<usize> {
        let (c, r) = ((x / CELL).floor() as i32 - self.col0, (z / CELL).floor() as i32 - self.row0);
        ((0..self.cols).contains(&c) && (0..self.rows).contains(&r)).then(|| (r * self.cols + c) as usize)
    }

    /// The overview's size in texels.
    pub fn overview_size(&self) -> (u32, u32) {
        (self.cols.max(0) as u32 * OVERVIEW, self.rows.max(0) as u32 * OVERVIEW)
    }

    /// Where the cell's region begins in the overview.
    pub fn overview_at(&self, cell: usize) -> [u32; 2] {
        let (c, r) = self.col_row(cell);
        [(c - self.col0) as u32 * OVERVIEW, (r - self.row0) as u32 * OVERVIEW]
    }
}

/// The game's name for the cell at column `col`, row `row` (`ZoneName(world,
/// row - 1, col - 1)` as `UpdateLocation` asks it, within `gWorldInfo`'s
/// `bounds`, columns then rows): a WORDS.TXT word, none for 641.
pub fn name(world: i32, bounds: Option<[i32; 4]>, col: i32, row: i32) -> Option<u16> {
    let b = bounds?;
    let (r, c) = (row - 1, col - 1);
    if c < b[0] || b[1] < c || r < b[2] || b[3] < r {
        return None;
    }
    crate::zones::zone(world, r, c).filter(|&w| w != 641)
}

/// `UpdateLocation`'s ninth of a cell: which third across and down world
/// (x, z) is in, measured from the grid's `origin` (x, z) as
/// `CalcWorldGridOffset` measures it (every world's is half a cell in, so
/// from the cell's middle): its word, 1422..1430 (the eight directions and
/// the middle).
pub fn ninth(x: f32, z: f32, origin: [f32; 2]) -> i32 {
    const DIRS: [[i32; 3]; 3] = [[1426, 1422, 1427], [1425, 1430, 1424], [1428, 1423, 1429]];
    let (col, row) = ((x / CELL) as i32, (z / CELL) as i32);
    let off = [x - col as f32 * CELL - origin[0], z - row as f32 * CELL - origin[1]];
    let third = |v: f32| if v < -333.34 { 0 } else if v <= 333.34 { 1 } else { 2 };
    DIRS[third(off[1])][third(off[0])]
}

/// What the map's cache keeps a world's pictures under: the file's name and
/// a hash (FNV-1a, 64 bits) of the map's own version, the world's records
/// read before its zones (its tree, grid and every zone's proxy) and where
/// each zone lies in the file, so another disc's world, or another build of
/// the map, never answers for it.
pub fn cache_key(name: &str, world_records: &[u8], ranges: &[(u64, usize)]) -> String {
    const VERSION: &[u8] = b"eqoa-rs map 2: 512 a cell, terrain and placements, pinned light";
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    let mut eat = |b: &[u8]| {
        for &x in b {
            h = (h ^ x as u64).wrapping_mul(0x0000_0100_0000_01B3);
        }
    };
    eat(VERSION);
    eat(name.as_bytes());
    eat(world_records);
    for &(off, len) in ranges {
        eat(&off.to_le_bytes());
        eat(&(len as u64).to_le_bytes());
    }
    let stem: String = name.split('.').next().unwrap_or(name).chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
    format!("{stem}-{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tunaria's layout: a zone a 2,000 square from x 2,000 and z 0; and
    /// Lavastorm's first zone, short of its cell's west side, is still that
    /// cell's. The control: a zone placed by its far corner instead of its
    /// middle lands a cell off.
    #[test]
    fn a_zone_is_the_cell_its_middle_is_in() {
        let tunaria: Vec<[f32; 4]> = (0..17).flat_map(|r| (1..14).map(move |c| [c as f32 * CELL, r as f32 * CELL, (c + 1) as f32 * CELL, (r + 1) as f32 * CELL])).collect();
        let t = Cells::new(&tunaria);
        assert_eq!((t.col0, t.row0, t.cols, t.rows), (1, 0, 13, 17));
        assert_eq!(t.extent(), [2000.0, 0.0, 28000.0, 34000.0]);
        assert_eq!(t.overview_size(), (1664, 2176));
        assert!(t.cell_of.iter().all(Option::is_some));
        // the zone at Freeport's place
        let k = t.at(25300.0, 15200.0).unwrap();
        assert_eq!(t.rect(k), [24000.0, 14000.0, 26000.0, 16000.0]);
        assert_eq!(t.overview_at(k), [11 * OVERVIEW, 7 * OVERVIEW]);
        assert_eq!(t.at(1999.0, 100.0), None, "west of the world");
        let lava = Cells::new(&[[4357.0, 4000.0, 6000.0, 6000.0], [6000.0, 4000.0, 8000.0, 6000.0]]);
        assert_eq!((lava.col0, lava.cols, lava.rows), (2, 2, 1));
        assert_eq!(lava.zone_at, vec![Some(0), Some(1)]);
        assert_eq!(lava.rect(0), [4000.0, 4000.0, 6000.0, 6000.0]);
        let far = (6000.0 / CELL).floor() as i32;
        assert_ne!(far, lava.col0, "the control: by its far corner, a cell east");
    }

    /// A cell's name is ZoneName one row and one column back, inside
    /// gWorldInfo's bounds; 641 is none. The controls: the cell itself
    /// (not one back), and a column past the bounds.
    #[test]
    fn a_cells_name_is_the_games() {
        // Tunaria's bounds as CLIENT has them (columns 0..12, rows 0..15)
        let b = Some([0, 12, 0, 15]);
        // Freeport, x 25,300 z 15,200: column 12, row 7 -> ZONES0[6][11]
        assert_eq!(name(0, b, 12, 7), crate::zones::zone(0, 6, 11));
        assert_eq!(name(0, b, 12, 7), Some(358));
        assert_ne!(name(0, b, 12, 7), crate::zones::zone(0, 7, 12), "the control: not one back");
        assert_eq!(name(0, Some([0, 10, 0, 15]), 12, 7), None, "past the bounds");
        assert_eq!(name(0, None, 12, 7), None, "no CLIENT, no names");
        // the first row of Tunaria is 641 (none) save its last columns
        assert_eq!(name(0, b, 1, 1), None);
        assert_eq!(name(0, b, 10, 1), Some(1152));
    }

    /// The ninth: the middle third of a cell is its middle (1430), and the
    /// corners are the diagonals. The control: measured from the corner,
    /// not the middle, the middle of a cell is its north-west.
    #[test]
    fn the_ninth_is_measured_from_the_cells_middle() {
        let o = [1000.0, 1000.0];
        assert_eq!(ninth(25000.0, 15000.0, o), 1430);
        assert_eq!(ninth(24100.0, 14100.0, o), 1426);
        assert_eq!(ninth(25900.0, 15900.0, o), 1429);
        assert_eq!(ninth(25000.0, 14100.0, o), 1422);
        assert_ne!(ninth(25000.0, 15000.0, [0.0, 0.0]), 1430, "the control: from the cell's corner");
    }

    /// The key changes with any of the world's records, its zones' places
    /// and its name, and is the same for the same world.
    #[test]
    fn the_cache_key_is_the_worlds() {
        let recs = vec![1u8, 2, 3, 4];
        let ranges = vec![(100u64, 50usize), (150, 70)];
        let k = cache_key("TUNARIA.ESF", &recs, &ranges);
        assert!(k.starts_with("tunaria-") && k.len() == "tunaria-".len() + 16, "{k}");
        assert_eq!(k, cache_key("TUNARIA.ESF", &recs, &ranges));
        assert_ne!(k, cache_key("TUNARIA.ESF", &[1, 2, 3, 5], &ranges));
        assert_ne!(k, cache_key("TUNARIA.ESF", &recs, &[(100, 50), (150, 71)]));
        assert_ne!(k, cache_key("ODUS.ESF", &recs, &ranges));
    }
}
