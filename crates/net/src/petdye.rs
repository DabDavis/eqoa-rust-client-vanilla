//! go-eqoa-server's own, no retail message: a pet's dyes, two colours on two
//! areas of its body (the server owner, 2026-11-16: a player's drachnid pet
//! should "have 2 different color areas that can be changed like armor dye
//! (will be a template for pet color dye)"; the server's Pet Groomers put them
//! on). The server offers them once the player is in the world
//! ([`crate::session::OP_PET_DYES_OFFER`]); this client answers 1 and is told,
//! in the world, the dyed pets it sees ([`crate::session::OP_PET_DYES`]), by
//! the id it knows each by: each the body the dyes are for, the colour of each
//! area and area 2's materials. Each list is the whole of it: a pet left out
//! is drawn in its own colours. A PS2 ignores the offer and draws every pet in
//! its body's own colours.

use crate::stream::Reader;

/// The layout of [`crate::session::OP_PET_DYES`] this client reads.
pub const LAYOUT: u8 = 2;

/// Layout 2's rule byte: the areas are this client's to tell apart, area 1
/// the body's clothing and area 2 the rest of it (the server owner,
/// 2026-11-16: "the pets need their clothes as one dye and the rest another
/// like we two tone equipment"; the eqoa crate's `petdye::clothing`).
pub const BY_RULE: u8 = 0;
/// Layout 2's rule byte: area 2 is the mask's materials (a server's row for
/// the body), as layout 1 has it.
pub const BY_MASK: u8 = 1;

/// A pet's dyes: the body they are for (a model DictID: drawn only on it),
/// each area's colour 0xRRGGBBAA (red first; 0 that area's own colours) and
/// area 2's materials (bit n the body's palette material n; 0 area 1 is the
/// whole body).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PetDye {
    pub look: u32,
    pub area: [u32; 2],
    pub mask: u64,
    /// The areas are the rule's ([`BY_RULE`]): area 1 the clothing, area 2
    /// the rest; the mask unused.
    pub rule: bool,
}

impl PetDye {
    /// The area of a batch of the body by its palette material (-1 none: area
    /// 1): 0 area 1, 1 area 2.
    pub fn area_of(&self, material: i32) -> usize {
        usize::from((0..64).contains(&material) && self.mask & (1u64 << material) != 0)
    }
}

/// A list of dyed pets: a u8 count, then each a u32 id, u32 body, u32 area 1
/// and u32 area 2 colours and a u64 mask, little-endian; in layout 2 each
/// followed by a u8, [`BY_RULE`] or [`BY_MASK`] (another is read as the mask).
/// None when the message is short; an entry dyeing nothing is left out.
pub fn read(data: &[u8], layout: u8) -> Option<Vec<(u32, PetDye)>> {
    let mut r = Reader::new(data);
    let n = r.u8() as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let id = r.u32();
        let look = r.u32();
        let area = [r.u32(), r.u32()];
        let mask = u64::from(r.u32()) | u64::from(r.u32()) << 32;
        let rule = layout >= 2 && r.u8() == BY_RULE;
        if area != [0, 0] {
            out.push((id, PetDye { look, area, mask, rule }));
        }
    }
    (!r.error).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u32, look: u32, a: u32, b: u32, mask: u64) -> Vec<u8> {
        let mut v = id.to_le_bytes().to_vec();
        for x in [look, a, b] {
            v.extend_from_slice(&x.to_le_bytes());
        }
        v.extend_from_slice(&mask.to_le_bytes());
        v
    }

    /// A list reads to its entries in order, one dyeing nothing left out; a
    /// list cut short reads to nothing (the control), and so does none. The
    /// mask's high word is read past bit 31.
    #[test]
    fn a_list_reads_whole_or_not_at_all() {
        let mut m = vec![3u8];
        m.extend(entry(0x0101_0203, 0x1576_4DA8, 0xD100_00FF, 0x0080_00FF, 0b1010 | 1 << 40));
        m.extend(entry(7, 0x1576_4DA8, 0, 0, 1));
        m.extend(entry(9, 0x9CF7_2381, 0x8020_10FF, 0, 0));
        let got = read(&m, 1).unwrap();
        assert_eq!(
            got,
            vec![
                (0x0101_0203, PetDye { look: 0x1576_4DA8, area: [0xD100_00FF, 0x0080_00FF], mask: 0b1010 | 1 << 40, rule: false }),
                (9, PetDye { look: 0x9CF7_2381, area: [0x8020_10FF, 0], mask: 0, rule: false })
            ]
        );
        assert_eq!(read(&[0], 1).unwrap(), vec![], "an empty list: no pet dyed");
        assert!(read(&m[..m.len() - 1], 1).is_none(), "the control: one byte short");
        assert!(read(&[], 1).is_none());
    }

    /// Layout 2 reads a rule byte after each mask: 0 the rule, 1 the mask;
    /// layout 1 reads none (the control: the same bytes cut short).
    #[test]
    fn layout_two_reads_the_rule_byte() {
        let mut m = vec![2u8];
        m.extend(entry(5, 0x1576_4DA8, 0xD100_00FF, 0x0080_00FF, 0));
        m.push(BY_RULE);
        m.extend(entry(6, 1, 0x0000_FFFF, 0, 0b10));
        m.push(BY_MASK);
        let got = read(&m, 2).unwrap();
        assert_eq!((got[0].1.rule, got[1].1.rule, got[1].1.mask), (true, false, 0b10));
        assert!(read(&m[..m.len() - 1], 2).is_none());
    }

    /// A material in the mask is area 2; every other, -1 and one past 63
    /// included, area 1.
    #[test]
    fn a_material_is_area_two_by_the_mask() {
        let d = PetDye { look: 1, area: [1, 1], mask: 0b100 | 1 << 63, rule: false };
        assert_eq!([d.area_of(2), d.area_of(63), d.area_of(0), d.area_of(-1), d.area_of(64)], [1, 1, 0, 0, 0]);
    }
}
