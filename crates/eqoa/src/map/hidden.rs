//! Where the map and the minimap show nothing (Heirloom petition 3233, Cucumber's, the owner's yes of
//! 2026-11-26: "The minimap should be disabled on Night 4, because it shows where the false floors are"). The
//! Plane of Sky's Night Chain 4 is a floor of squares some of which are not there: the map's picture of it,
//! drawn from straight above, shows those squares as holes, which is the puzzle given away.
//!
//! So, for each area here: the map and the minimap cover it with a plain square wherever they show it (the isle
//! stays on the map, its floor does not), and while the player stands in it the minimap shows no picture at all,
//! only `WORDS`. Off it, both are as ever. The area is the disc's own (the isle's terrain, the same on every
//! server), so it is a table here and not a server's message.

/// One area: `gWorldInfo`'s world and (x0, z0, x1, z1), world units.
pub struct Area {
    pub world: i32,
    pub rect: [f32; 4],
    pub name: &'static str,
}

/// The areas. Night Chain 4: its outline in `skyisles_gen.rs` (x 4108..4344, z 7564..7776, its floor at
/// 353) with 4 units round it, so no edge of a hole shows past the cover.
pub const AREAS: &[Area] = &[Area { world: 4, rect: [4104.0, 7560.0, 4348.0, 7780.0], name: "Night Chain 4" }];

/// What the minimap says in place of its picture in an area.
pub const WORDS: &str = "The sky hides the way";

/// The cover's colour: about the isle's floor, flat.
pub const COVER: [f32; 4] = [0.13, 0.13, 0.15, 1.0];

/// Whether (x, z) of world `world` is in an area: the minimap shows nothing there.
pub fn at(world: i32, x: f32, z: f32) -> bool {
    AREAS.iter().any(|a| a.world == world && x >= a.rect[0] && x <= a.rect[2] && z >= a.rect[1] && z <= a.rect[3])
}

/// The areas of world `world` that meet `shown` (x0, z0, x1, z1): what is covered where the map shows `shown`.
pub fn covers(world: i32, shown: [f32; 4]) -> impl Iterator<Item = [f32; 4]> {
    AREAS.iter().filter(move |a| a.world == world && a.rect[0] < shown[2] && a.rect[2] > shown[0] && a.rect[1] < shown[3] && a.rect[3] > shown[1]).map(|a| a.rect)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On Night Chain 4, where its dark barons stand (4129..4293, 7579..7758) and at its four corner pillars,
    /// the minimap shows nothing; the isles round it (Night 3 above it to the north, Night 5's King Ruthra
    /// under it to the south), the same spot in another world and a step past the cover are as ever.
    #[test]
    fn night_four_hides_the_minimap_and_its_neighbours_do_not() {
        for (x, z) in [(4129.0, 7579.0), (4293.0, 7732.0), (4155.0, 7758.0), (4118.0, 7767.0), (4337.0, 7573.0), (4337.0, 7768.0), (4120.0, 7573.0)] {
            assert!(at(4, x, z), "({x}, {z}) is on Night 4");
            assert_eq!(crate::skyisles::isle(x, 353.0, z), Some("Night Chain 4"), "and skyisles names it so");
        }
        for (x, z, what) in [(4250.0, 6578.0, "the Night Maiden, Night 3"), (4499.0, 7234.0, "King Ruthra, Night 5"), (4305.0, 7283.0, "Night 5's temple"), (4100.0, 7670.0, "west of the cover"), (4230.0, 7785.0, "south of the cover")] {
            assert!(!at(4, x, z), "{what} keeps its minimap");
        }
        assert!(!at(0, 4230.0, 7670.0), "the control: the same spot in Tunaria");
    }

    /// The cover holds the isle's whole outline as skyisles has it, so no hole's edge shows past it, and
    /// nothing else's: no other isle's outline point falls inside it.
    #[test]
    fn the_cover_holds_night_four_and_no_other_isle() {
        let r = AREAS[0].rect;
        let outline = [(4112.0, 7564.0), (4108.0, 7776.0), (4344.0, 7776.0), (4344.0, 7564.0)];
        for (x, z) in outline {
            assert!(x > r[0] && x < r[2] && z > r[1] && z < r[3], "({x}, {z}) under the cover");
        }
        // a grid over the cover: every point skyisles names is Night 4's
        let mut n = 0;
        let mut z = r[1];
        while z <= r[3] {
            let mut x = r[0];
            while x <= r[2] {
                if let Some(i) = crate::skyisles::isle(x, 353.0, z) {
                    assert_eq!(i, "Night Chain 4", "({x}, {z})");
                    n += 1;
                }
                x += 8.0;
            }
            z += 8.0;
        }
        assert!(n > 700, "the grid found the isle ({n} points)");
    }

    /// The map covers the area where it shows it: a view over the isle, one with an edge across it, one
    /// round the whole world; none away from it, nor in another world (the control).
    #[test]
    fn the_map_covers_the_area_where_it_shows_it() {
        let r = AREAS[0].rect;
        assert_eq!(covers(4, [4200.0, 7600.0, 4260.0, 7700.0]).collect::<Vec<_>>(), [r], "inside it");
        assert_eq!(covers(4, [4300.0, 7000.0, 5000.0, 7600.0]).count(), 1, "an edge across it");
        assert_eq!(covers(4, [0.0, 0.0, 10000.0, 10000.0]).count(), 1, "the whole world");
        assert_eq!(covers(4, [4400.0, 7000.0, 4600.0, 7300.0]).count(), 0, "Ruthra's, away from it");
        assert_eq!(covers(0, [0.0, 0.0, 30000.0, 30000.0]).count(), 0, "the control: Tunaria");
    }
}
