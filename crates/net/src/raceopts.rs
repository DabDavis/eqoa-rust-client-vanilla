//! go-eqoa-server's own, no retail message: a custom race's own character options
//! and the looks its characters chose ([`crate::session::OP_RACE_OPTIONS`]). A
//! race the server has and the game does not wears a creature's body, which has
//! none of the text slots a player race's face, hair and armour go on; what such a
//! body has instead is listed here, race by race: other bodies of the same
//! creature (a row of [`Kind::Body`]: the character's model, which every client
//! draws), a part a colour is laid on ([`Kind::Colour`]) and a part a texture is
//! laid on ([`Kind::Texture`]: a face, hair). The create pages offer the rows
//! (`pregame::client::create`), a creation sends the steps chosen after 0x002B's
//! own fields ([`choices_block`]), and the server tells every client that asked
//! what each character chose ([`read_looks`]): drawn only on that character's own
//! body (`eqoa::raceopts`). A server that never offers them is the game's alone.

use crate::stream::Reader;

/// The one version this client reads, of both messages.
pub const VERSION: u8 = 1;

/// What a row's steps change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// a body (CHAR.ESF CSprite DictID): the character's model
    Body,
    /// a colour laid on a part: a recolour (`render::recolor`)
    Colour,
    /// a texture laid on a part (its DictID)
    Texture,
}

impl Kind {
    fn of(b: u8) -> Option<Kind> {
        match b {
            1 => Some(Kind::Body),
            2 => Some(Kind::Colour),
            3 => Some(Kind::Texture),
            _ => None,
        }
    }
}

/// A part of one body: the triangles of its palette entries whose texture
/// coordinates' centre (the first layer's, after its transform) lies in one of
/// the rects, each `[u0, v0, u1, v1]` in 255ths of the texture; no rects, all of
/// the entries' triangles.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Part {
    pub entries: Vec<u8>,
    pub rects: Vec<[u8; 4]>,
}

/// One step of a row: its name, its value (a body's DictID, a recolour, a
/// texture's DictID; a colour of 0 is the part's own) and, for a body, its scale
/// against the race's own body (1000ths; 0 none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub name: String,
    pub value: u32,
    pub size: u16,
}

/// One row of a race's options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// its name on the wire and in what a creation sends
    pub key: String,
    /// the create page's word for it
    pub label: String,
    pub kind: Kind,
    /// the parts it changes, each of one body
    pub parts: Vec<(u32, Part)>,
    pub male: Vec<Step>,
    /// a female's steps; empty, the male's
    pub female: Vec<Step>,
}

impl Row {
    /// Its steps for `sex` (1 female).
    pub fn steps(&self, sex: i32) -> &[Step] {
        if sex == 1 && !self.female.is_empty() { &self.female } else { &self.male }
    }

    /// Its parts of `body`.
    pub fn parts_of(&self, body: u32) -> impl Iterator<Item = &Part> {
        self.parts.iter().filter(move |(b, _)| *b == body).map(|(_, p)| p)
    }
}

/// Every race's options, as the server last sent them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub races: Vec<(i32, Vec<Row>)>,
}

impl Options {
    /// The rows of race `id`; none for a race with none.
    pub fn rows(&self, id: i32) -> &[Row] {
        self.races.iter().find(|(r, _)| *r == id).map_or(&[], |(_, rows)| rows)
    }

    /// Whether `model` is a body any race's body row offers.
    pub fn offers_body(&self, model: u32) -> bool {
        model != 0 && self.races.iter().flat_map(|(_, rows)| rows).filter(|r| r.kind == Kind::Body).any(|r| r.male.iter().chain(&r.female).any(|s| s.value == model))
    }

    /// The race's own body of the race whose body row offers `model`: the
    /// row's first male step (a body row opens on its race's own body), when
    /// `model` is another body. None for a body no row offers, and for the
    /// race's own.
    pub fn race_body_of(&self, model: u32) -> Option<u32> {
        let row = self.races.iter().flat_map(|(_, rows)| rows).filter(|r| r.kind == Kind::Body).find(|r| r.male.iter().chain(&r.female).any(|s| s.value == model))?;
        row.male.first().map(|s| s.value).filter(|&b| b != model && b != 0)
    }

    /// The scale a body a body row offers is drawn at against its race's own
    /// (`Step::size`); 1 for any other.
    pub fn body_size(&self, race: i32, model: u32) -> f32 {
        self.rows(race).iter().filter(|r| r.kind == Kind::Body).flat_map(|r| r.male.iter().chain(&r.female)).find(|s| s.value == model && s.size > 0).map_or(1.0, |s| s.size as f32 / 1000.0)
    }
}

fn text(r: &mut Reader) -> String {
    let n = r.u8() as usize;
    String::from_utf8_lossy(r.bytes(n)).into_owned()
}

fn part(r: &mut Reader) -> Part {
    let n = r.u8() as usize;
    let entries = r.bytes(n).to_vec();
    let n = r.u8() as usize;
    let rects = (0..n).map(|_| [r.u8(), r.u8(), r.u8(), r.u8()]).collect();
    Part { entries, rects }
}

fn steps(r: &mut Reader) -> Vec<Step> {
    let n = r.u8() as usize;
    (0..n)
        .map(|_| {
            let name = text(r);
            Step { name, value: r.u32(), size: r.u16() }
        })
        .collect()
}

/// [`crate::session::OP_RACE_OPTIONS_DATA`]'s body (after the opcode): u8
/// version, u8 races, each a u8 id and u8 rows; each row its key and label (a u8
/// byte count and UTF-8), u8 kind, u8 parts (each u32 body, u8 entries and each
/// entry, u8 rects and each rect's four u8), u8 male steps (each its name, u32
/// value, u16 size), u8 female steps. None when it is short, of another version
/// or of a kind this client does not know.
pub fn read_options(data: &[u8]) -> Option<Options> {
    let mut r = Reader::new(data);
    if r.u8() != VERSION {
        return None;
    }
    let n = r.u8() as usize;
    let mut races = Vec::with_capacity(n);
    for _ in 0..n {
        let id = r.u8() as i32;
        let rows_n = r.u8() as usize;
        let mut rows = Vec::with_capacity(rows_n);
        for _ in 0..rows_n {
            let key = text(&mut r);
            let label = text(&mut r);
            let kind = Kind::of(r.u8());
            let parts_n = r.u8() as usize;
            let parts = (0..parts_n)
                .map(|_| {
                    let body = r.u32();
                    (body, part(&mut r))
                })
                .collect();
            let male = steps(&mut r);
            let female = steps(&mut r);
            rows.push(Row { key, label, kind: kind?, parts, male, female });
        }
        races.push((id, rows));
    }
    (!r.error && r.pos == data.len()).then_some(Options { races })
}

/// What one character chose, as the server tells it: drawn only on `body`, its
/// own (never under an illusion or another body).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Look {
    pub body: u32,
    /// each part's kind and value
    pub parts: Vec<(Kind, u32, Part)>,
}

/// [`crate::session::OP_RACE_LOOKS`]'s body: u8 version, u16 characters, each
/// its name (a u8 byte count and UTF-8), u32 body, u8 parts, each u8 kind, u32
/// value, u8 entries and each entry, u8 rects and each rect's four u8. None when
/// it is short or of another version.
pub fn read_looks(data: &[u8]) -> Option<Vec<(String, Look)>> {
    let mut r = Reader::new(data);
    if r.u8() != VERSION {
        return None;
    }
    let n = r.u16() as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let name = text(&mut r);
        let body = r.u32();
        let parts_n = r.u8() as usize;
        let mut parts = Vec::with_capacity(parts_n);
        for _ in 0..parts_n {
            let kind = Kind::of(r.u8())?;
            let value = r.u32();
            parts.push((kind, value, part(&mut r)));
        }
        out.push((name, Look { body, parts }));
    }
    (!r.error && r.pos == data.len()).then_some(out)
}

/// The steps a creation chose, as 0x002B carries them after its own fields
/// (go-eqoa-server's `raceOptionsBlock`): 'R', u8 version, u8 count, each its
/// row's key (a u8 byte count and UTF-8) and a u8 step. None for no choice at
/// all, so a creation of a race with no options is 0x002B's own bytes.
pub fn choices_block(choices: &[(String, u8)]) -> Option<Vec<u8>> {
    if choices.is_empty() {
        return None;
    }
    let mut b = vec![b'R', VERSION, choices.len().min(255) as u8];
    for (key, step) in choices.iter().take(255) {
        let k = &key.as_bytes()[..key.len().min(255)];
        b.push(k.len() as u8);
        b.extend_from_slice(k);
        b.push(*step);
    }
    Some(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// go-eqoa-server's bytes for a race of two rows: a body row (two male
    /// steps, one female) and a colour row of one part with a rect.
    fn servers() -> Vec<u8> {
        let mut b = vec![1, 1, 28, 2];
        let s = |b: &mut Vec<u8>, t: &str| {
            b.push(t.len() as u8);
            b.extend(t.as_bytes());
        };
        s(&mut b, "body");
        s(&mut b, "Fungus");
        b.extend([1, 0, 2]);
        s(&mut b, "Blue");
        b.extend(0x4A4D_88FBu32.to_le_bytes());
        b.extend(1000u16.to_le_bytes());
        s(&mut b, "Pale");
        b.extend(0xE8B0_ADC8u32.to_le_bytes());
        b.extend(910u16.to_le_bytes());
        b.push(1);
        s(&mut b, "Red");
        b.extend(0x5874_ACFCu32.to_le_bytes());
        b.extend(1000u16.to_le_bytes());
        s(&mut b, "cap");
        s(&mut b, "Cap");
        b.extend([2, 1]);
        b.extend(0x4A4D_88FBu32.to_le_bytes());
        b.extend([1, 0, 1, 115, 115, 255, 255]);
        b.push(2);
        s(&mut b, "Natural");
        b.extend(0u32.to_le_bytes());
        b.extend(0u16.to_le_bytes());
        s(&mut b, "Crimson");
        b.extend(0x01FA_B3B6u32.to_le_bytes());
        b.extend(0u16.to_le_bytes());
        b.push(0);
        b
    }

    #[test]
    fn the_servers_options_read() {
        let o = read_options(&servers()).expect("read");
        let rows = o.rows(28);
        assert_eq!(rows.len(), 2);
        let body = &rows[0];
        assert_eq!((body.key.as_str(), body.label.as_str(), body.kind), ("body", "Fungus", Kind::Body));
        assert_eq!(body.steps(0).iter().map(|s| s.value).collect::<Vec<_>>(), vec![0x4A4D_88FB, 0xE8B0_ADC8]);
        assert_eq!(body.steps(1).iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["Red"], "a female's own");
        let cap = &rows[1];
        assert_eq!(cap.kind, Kind::Colour);
        assert_eq!(cap.parts_of(0x4A4D_88FB).collect::<Vec<_>>(), vec![&Part { entries: vec![0], rects: vec![[115, 115, 255, 255]] }]);
        assert_eq!(cap.parts_of(0x1234).count(), 0, "no part of another body");
        assert_eq!(cap.steps(1).len(), 2, "no female steps: the male's");
        assert!(o.offers_body(0xE8B0_ADC8) && o.offers_body(0x5874_ACFC) && !o.offers_body(0) && !o.offers_body(0x1234));
        assert_eq!(o.body_size(28, 0xE8B0_ADC8), 0.91);
        assert_eq!(o.body_size(28, 0x1234), 1.0, "a body no row offers: 1");
        assert!(o.rows(11).is_empty(), "a race the server listed nothing for");
        // the race's own body of another body a row offers: the row's first male step
        assert_eq!(o.race_body_of(0xE8B0_ADC8), Some(0x4A4D_88FB));
        assert_eq!(o.race_body_of(0x5874_ACFC), Some(0x4A4D_88FB), "a female's step");
        assert_eq!(o.race_body_of(0x4A4D_88FB), None, "the race's own");
        assert_eq!(o.race_body_of(0x1234), None, "a body no row offers");
    }

    /// Every cut of either message is refused, never half read; another
    /// version, a kind past the three and bytes past the end too (the control:
    /// the whole one reads).
    #[test]
    fn a_short_or_other_message_is_none() {
        let b = servers();
        assert!(read_options(&b).is_some(), "the control");
        for n in 0..b.len() {
            assert_eq!(read_options(&b[..n]), None, "cut at {n}");
        }
        let mut v = b.clone();
        v[0] = 2;
        assert_eq!(read_options(&v), None);
        let mut k = b.clone();
        k[4 + 5 + 7] = 9; // the body row's kind
        assert_eq!(read_options(&k), None);
        let mut long = b.clone();
        long.push(0);
        assert_eq!(read_options(&long), None);
        assert_eq!(read_options(&[1, 0]), Some(Options::default()));
    }

    fn looks() -> Vec<u8> {
        let mut b = vec![1, 2, 0];
        for name in ["Spore", "[GM] Spore"] {
            b.push(name.len() as u8);
            b.extend(name.as_bytes());
            b.extend(0x4A4D_88FBu32.to_le_bytes());
            b.push(2);
            b.push(2);
            b.extend(0x01FA_B3B6u32.to_le_bytes());
            b.extend([1, 0, 1, 115, 115, 255, 255]);
            b.push(3);
            b.extend(0x98F8_6489u32.to_le_bytes());
            b.extend([2, 1, 2, 0]);
        }
        b
    }

    #[test]
    fn the_servers_looks_read() {
        let l = read_looks(&looks()).expect("read");
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].0.as_str(), l[1].0.as_str()), ("Spore", "[GM] Spore"));
        let look = &l[0].1;
        assert_eq!(look.body, 0x4A4D_88FB);
        assert_eq!(look.parts[0], (Kind::Colour, 0x01FA_B3B6, Part { entries: vec![0], rects: vec![[115, 115, 255, 255]] }));
        assert_eq!(look.parts[1], (Kind::Texture, 0x98F8_6489, Part { entries: vec![1, 2], rects: vec![] }));
        let b = looks();
        for n in 0..b.len() {
            assert_eq!(read_looks(&b[..n]), None, "cut at {n}");
        }
        assert_eq!(read_looks(&[1, 0, 0]), Some(vec![]));
        assert_eq!(read_looks(&[2, 0, 0]), None, "another version");
    }

    /// The choices as go-eqoa-server reads them; none at all, no bytes (the
    /// control: 0x002B is its own).
    #[test]
    fn choices_go_after_0x002b() {
        let b = choices_block(&[("body".into(), 2), ("cap".into(), 1)]).expect("some");
        assert_eq!(b, [b"R".as_slice(), &[1, 2, 4], b"body", &[2, 3], b"cap", &[1]].concat());
        assert_eq!(choices_block(&[]), None);
    }
}
