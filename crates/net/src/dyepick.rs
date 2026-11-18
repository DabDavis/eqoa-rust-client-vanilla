//! go-eqoa-server's own, no retail message: the dyers' colour picker (the
//! server owner, 2026-11-17: "lets upgrade the dyer NPCs to show a RGB color
//! picker, add to client if needed. then we have any rgb combo"). The server
//! offers it once the player is in the world ([`OP_DYE_PICKER`] kind
//! [`OFFER`]); this client answers its version ([`OP_DYE_PICKER_ANSWER`] kind
//! [`OFFER`]) and from then on its Equipment Dyers, Pet Groomers and Weapon
//! Enchanters ask "Pick any colour..." beside their palettes. Chosen, the
//! server opens the picker ([`OPEN`], read by [`read_open`]): what it dyes
//! (the character's dye slots, its pet's areas or a held weapon's aura), the
//! colour now, the dyer's palette and what to preview on; this client answers
//! the colour ([`picked`]) or that it was cancelled ([`cancelled`]). A PS2
//! ignores the offer and keeps the palettes. All little-endian.

use crate::stream::Reader;

/// Server to client: the offer, a picker opened, a picker closed.
pub const OP_DYE_PICKER: u16 = 0x7E95;
/// Client to server: the offer's answer, a colour picked, a picker cancelled.
pub const OP_DYE_PICKER_ANSWER: u16 = 0x7E96;

/// The version of the picker's messages this client speaks: 2 adds
/// [`no_dye`] (the server owner, 2026-11-18).
pub const VERSION: u8 = 2;

/// The messages' kinds (each one's first byte).
pub const OFFER: u8 = 0;
pub const OPEN: u8 = 1;
pub const CLOSE: u8 = 2;
pub const PICKED: u8 = 1;
pub const CANCELLED: u8 = 2;
pub const NO_DYE: u8 = 3;

/// What a picker dyes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Target {
    /// the character's dye slots (go-eqoa-server's chardyes: 0-5 chest ..
    /// helm, 6 primary, 7 secondary, 8 shield, 9 robe, 10 cape, 11-16 the
    /// armour's trims)
    #[default]
    Character,
    /// its pet's areas (0 area 1, 1 area 2)
    Pet,
    /// a held weapon's aura: the place in the live channel's auras
    /// (`crate::entity::MAIN_HAND_AURA` ... `OFF_HAND_AURA2`)
    Aura,
}

/// A picker the server opened.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Open {
    /// handed back with the answer
    pub token: u32,
    pub target: Target,
    /// whether the alpha is the player's to choose (else FF)
    pub alpha: bool,
    /// the colour now, 0xRRGGBBAA (0 none)
    pub current: u32,
    pub title: String,
    pub slots: Vec<u8>,
    /// the pet to preview on ([`Target::Pet`]): the id this client knows it
    /// by (0 not in view) and its dyes as the server's list has them
    pub pet: Option<(u32, crate::petdye::PetDye)>,
    /// the aura to preview on ([`Target::Aura`]): its kind, source and nodes
    pub aura: Option<(u8, u32, u8)>,
    /// the dyer's palette, 0xRRGGBBAA
    pub palette: Vec<u32>,
}

/// What the server said of the picker (`OP_DYE_PICKER`'s kinds after the
/// offer, which the session answers itself).
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Open(Open),
    Close(u32),
}

/// [`OP_DYE_PICKER`]'s body (after the opcode) of kind [`OPEN`] or [`CLOSE`];
/// None for another kind or one cut short.
pub fn read(data: &[u8]) -> Option<Message> {
    match *data.first()? {
        OPEN => read_open(&data[1..]).map(Message::Open),
        CLOSE => {
            let mut r = Reader::new(&data[1..]);
            let t = r.u32();
            (!r.error).then_some(Message::Close(t))
        }
        _ => None,
    }
}

/// An open's body after its kind byte: u32 token, u8 target, u8 flags (bit 0
/// the alpha), u32 colour, u8 title length and its UTF-8, u8 n and n slots,
/// for a pet u32 id, u32 body, two u32 colours, u64 mask, u8 rule, for an
/// aura u8 kind, u32 source, u8 nodes, then u8 n and n u32 palette colours.
pub fn read_open(data: &[u8]) -> Option<Open> {
    let mut r = Reader::new(data);
    let token = r.u32();
    let target = match r.u8() {
        0 => Target::Character,
        1 => Target::Pet,
        2 => Target::Aura,
        _ => return None,
    };
    let alpha = r.u8() & 1 != 0;
    let current = r.u32();
    let n = r.u8() as usize;
    let title = String::from_utf8_lossy(r.bytes(n)).into_owned();
    let n = r.u8() as usize;
    let slots = r.bytes(n).to_vec();
    let mut o = Open { token, target, alpha, current, title, slots, ..Default::default() };
    match target {
        Target::Pet => {
            let id = r.u32();
            let look = r.u32();
            let area = [r.u32(), r.u32()];
            let mask = r.u64();
            let rule = r.u8() == crate::petdye::BY_RULE;
            o.pet = Some((id, crate::petdye::PetDye { look, area, mask, rule }));
        }
        Target::Aura => o.aura = Some((r.u8(), r.u32(), r.u8())),
        Target::Character => {}
    }
    let n = r.u8() as usize;
    o.palette = (0..n).map(|_| r.u32()).collect();
    (!r.error).then_some(o)
}

/// The answer to the offer: this client's version.
pub fn offer_answer() -> Vec<u8> {
    let mut w = crate::stream::Writer::op(OP_DYE_PICKER_ANSWER);
    w.u8(OFFER).u8(VERSION);
    w.data
}

/// The colour picked in the picker of `token`.
pub fn picked(token: u32, rgba: u32) -> Vec<u8> {
    let mut w = crate::stream::Writer::op(OP_DYE_PICKER_ANSWER);
    w.u8(PICKED).u32(token).u32(rgba);
    w.data
}

/// No dye (version 2): the dye taken off what the picker of `token` dyes.
pub fn no_dye(token: u32) -> Vec<u8> {
    let mut w = crate::stream::Writer::op(OP_DYE_PICKER_ANSWER);
    w.u8(NO_DYE).u32(token);
    w.data
}

/// The picker of `token` cancelled.
pub fn cancelled(token: u32) -> Vec<u8> {
    let mut w = crate::stream::Writer::op(OP_DYE_PICKER_ANSWER);
    w.u8(CANCELLED).u32(token);
    w.data
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// go-eqoa-server's `encodeDyePickerOpen` (its `dyepicker_test.go`'s
    /// reader) as bytes, after the opcode.
    pub fn open_bytes(o: &Open) -> Vec<u8> {
        let mut v = vec![OPEN];
        v.extend(o.token.to_le_bytes());
        v.push(match o.target {
            Target::Character => 0,
            Target::Pet => 1,
            Target::Aura => 2,
        });
        v.push(o.alpha as u8);
        v.extend(o.current.to_le_bytes());
        v.push(o.title.len() as u8);
        v.extend(o.title.as_bytes());
        v.push(o.slots.len() as u8);
        v.extend(&o.slots);
        if let Some((id, d)) = o.pet {
            for x in [id, d.look, d.area[0], d.area[1]] {
                v.extend(x.to_le_bytes());
            }
            v.extend(d.mask.to_le_bytes());
            v.push(if d.rule { crate::petdye::BY_RULE } else { crate::petdye::BY_MASK });
        }
        if let Some((k, s, n)) = o.aura {
            v.push(k);
            v.extend(s.to_le_bytes());
            v.push(n);
        }
        v.push(o.palette.len() as u8);
        for c in &o.palette {
            v.extend(c.to_le_bytes());
        }
        v
    }

    /// Each target's open reads back as it was written; a close reads its
    /// token. The controls: a message cut short, a target past 2 and the
    /// offer's kind read as nothing.
    #[test]
    fn opens_and_closes_read_back() {
        let ch = Open { token: 0x7E95_0003, target: Target::Character, current: 0xD100_00FF, title: "Dye chest".into(), slots: vec![0, 9], palette: vec![0xFFFF_FFFF, 0x1010_10FF], ..Default::default() };
        let pet = Open {
            token: 7,
            target: Target::Pet,
            alpha: true,
            title: "Dye your fire elemental's its whole body".into(),
            slots: vec![1],
            pet: Some((0x0300_0001, crate::petdye::PetDye { look: 0x8A41_6241, area: [0, 0x3FA7_D680], mask: 0b110, rule: true })),
            ..Default::default()
        };
        let aura = Open { token: 8, target: Target::Aura, title: "Colour of Virtuous".into(), slots: vec![11], aura: Some((7, 0x88EC_092D, 0)), ..Default::default() };
        for o in [ch, pet, aura] {
            let b = open_bytes(&o);
            assert_eq!(read(&b), Some(Message::Open(o.clone())), "{o:?}");
            assert_eq!(read(&b[..b.len() - 1]), None, "the control: cut short");
        }
        assert_eq!(read(&[CLOSE, 3, 0, 0x95, 0x7E]), Some(Message::Close(0x7E95_0003)));
        assert_eq!(read(&[CLOSE, 3, 0]), None);
        let mut bad = open_bytes(&Open::default());
        bad[5] = 3;
        assert_eq!(read(&bad), None, "a target this client does not know");
        assert_eq!(read(&[OFFER, 1]), None, "the offer is the session's");
    }

    /// The answers: the offer's version, a colour and a cancel, little-endian.
    #[test]
    fn the_answers_bytes() {
        assert_eq!(offer_answer(), vec![0x96, 0x7E, 0, VERSION]);
        assert_eq!(picked(0x7E95_0003, 0x3FA7_D6FF), vec![0x96, 0x7E, 1, 3, 0, 0x95, 0x7E, 0xFF, 0xD6, 0xA7, 0x3F]);
        assert_eq!(cancelled(9), vec![0x96, 0x7E, 2, 9, 0, 0, 0]);
    }
}
