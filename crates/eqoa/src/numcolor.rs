//! Number Colors (Extended > Number Colors; the server owner's of 2026-11-09,
//! from a player's ask "to see/change colors of damage/procs/healing for
//! player/pets similar to the way SS does ... Really helpful in testing to see
//! if things are working correctly": "we can add the number colors to extended
//! menu"): the floating numbers (`damage`) of the player's own figures and its
//! pet's in a colour chosen for what made them, where the server says what that
//! was. The game colours a number by its sign and by who made it (white a hit
//! made, yellow one taken, green a heal), and a pet's hit reaches its owner as
//! the owner's own, so a proc, a spell and a swing look alike; go-eqoa-server
//! tags each figure the player or its pet made with one byte after 0x00DB's body
//! once the client answers its offer (`net::session::OP_NUMBER_TAGS_OFFER`,
//! `number_tag`): the client answers 1 while any row is not Default.
//!
//! Six rows: Your Damage (a swing, a spell or an ability, a damage over time's
//! tick, a damage shield), Your Procs, Your Heals (a heal and a heal over time's
//! tick), and the same three for the pet. Each is Default, the game's own colour,
//! or one of `PALETTE`; every row is Default until chosen, so nothing changes
//! for a player who never picks one. An untagged number (any other server's,
//! a figure on the player that another made, a heal from another) is always the
//! game's colour, and so is any number while a server's patch colours them
//! (Sandstorm's S2: `s2::Rules::damage_rgb`, its three bytes, read as before).

use net::session::NumberTag;

/// The rows, in the page's order: what each is called and its key in
/// `client.opt`.
pub const ROWS: usize = 6;
pub const LABELS: [&str; ROWS] = ["Your Damage", "Your Procs", "Your Heals", "Pet Damage", "Pet Procs", "Pet Heals"];
pub const KEYS: [&str; ROWS] = ["damage", "procs", "heals", "pet_damage", "pet_procs", "pet_heals"];

/// The Extended page's items for the rows (`options::EXTENDED_ITEMS`), in the
/// rows' order.
pub const ITEMS: [i32; ROWS] = [64, 65, 66, 67, 68, 69];

/// One colour a row may take: its name on the page, its key in `client.opt`,
/// and the number's RGB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Paint {
    pub name: &'static str,
    pub key: &'static str,
    pub rgb: [u8; 3],
}

/// The colours after Default, each one clearly unlike the others and the
/// game's own (white, the yellow of a hit taken, the green of a heal: the
/// game's are named White and Green here), bright enough to read over the
/// world in the damage font (ours).
pub const PALETTE: [Paint; 11] = [
    Paint { name: "White", key: "white", rgb: [255, 255, 255] },
    Paint { name: "Yellow", key: "yellow", rgb: [255, 236, 40] },
    Paint { name: "Orange", key: "orange", rgb: [255, 140, 0] },
    Paint { name: "Red", key: "red", rgb: [255, 48, 48] },
    Paint { name: "Pink", key: "pink", rgb: [255, 110, 199] },
    Paint { name: "Purple", key: "purple", rgb: [176, 96, 255] },
    Paint { name: "Blue", key: "blue", rgb: [64, 112, 255] },
    Paint { name: "Light Blue", key: "light_blue", rgb: [130, 190, 255] },
    Paint { name: "Cyan", key: "cyan", rgb: [0, 240, 240] },
    Paint { name: "Green", key: "green", rgb: [0, 255, 0] },
    Paint { name: "Gray", key: "gray", rgb: [160, 160, 160] },
];

/// A row's choice: 0 Default, else `PALETTE[choice - 1]`.
pub type Choice = u8;

/// The colour a choice paints, none for Default (or a choice past the
/// palette, which reads as Default).
pub fn paint(choice: Choice) -> Option<&'static Paint> {
    (choice as usize).checked_sub(1).and_then(|i| PALETTE.get(i))
}

/// A choice's name on the page.
pub fn name(choice: Choice) -> &'static str {
    paint(choice).map_or("Default", |p| p.name)
}

/// A choice moved a step, right (`d` > 0) or left, round Default and the
/// palette.
pub fn step(choice: Choice, d: i32) -> Choice {
    let n = PALETTE.len() as i32 + 1;
    let at = (choice as i32).clamp(0, n - 1);
    (at + if d < 0 { n - 1 } else { 1 }).rem_euclid(n) as Choice
}

/// The choice a `client.opt` value names, none for one not understood.
pub fn from_key(key: &str) -> Option<Choice> {
    if key == "default" {
        return Some(0);
    }
    PALETTE.iter().position(|p| p.key == key).map(|i| i as Choice + 1)
}

/// The row a tag colours: Damage for a swing, a spell, a tick and a shield
/// (kinds 1-4), Procs for a proc (5), Heals for a heal and its ticks (6, 7),
/// the pet's three for the pet's; none for a kind this client does not know.
pub fn row(tag: NumberTag) -> Option<usize> {
    let r = match tag.kind {
        1..=4 => 0,
        5 => 1,
        6 | 7 => 2,
        _ => return None,
    };
    Some(if tag.pet { r + 3 } else { r })
}

/// The colour a tagged number is drawn in with the rows `choices`: its row's,
/// none (the game's own) when it is Default, untagged, or of a kind not known.
pub fn rgb(tag: Option<NumberTag>, choices: &[Choice; ROWS]) -> Option<[u8; 3]> {
    paint(choices[row(tag?)?]).map(|p| p.rgb)
}

/// Whether the server should tag the numbers: any row not Default
/// (`Session::agree_number_tags`).
pub fn wanted(choices: &[Choice; ROWS]) -> bool {
    choices.iter().any(|&c| paint(c).is_some())
}

/// How a row's value is drawn on the page: in its colour (each channel a
/// float), with a shadow so a light one reads on the window; none for
/// Default, drawn as every other value.
pub fn swatch(choice: Choice) -> Option<[f32; 4]> {
    paint(choice).map(|p| [p.rgb[0] as f32 / 255.0, p.rgb[1] as f32 / 255.0, p.rgb[2] as f32 / 255.0, 1.0])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(kind: u8, pet: bool) -> Option<NumberTag> {
        Some(NumberTag { kind, pet })
    }

    /// Each kind to its row, the pet's three after the player's; a kind not
    /// known, and no tag, to none (the game's colour).
    #[test]
    fn kinds_go_to_their_rows() {
        let want = [(1, 0), (2, 0), (3, 0), (4, 0), (5, 1), (6, 2), (7, 2)];
        for (kind, r) in want {
            assert_eq!(row(NumberTag { kind, pet: false }), Some(r), "kind {kind}");
            assert_eq!(row(NumberTag { kind, pet: true }), Some(r + 3), "the pet's kind {kind}");
        }
        assert_eq!(row(NumberTag { kind: 8, pet: false }), None);
        assert_eq!(row(NumberTag { kind: 0, pet: true }), None);
        // the control: a table that put procs with damage would fail the above
        assert_ne!(row(NumberTag { kind: 5, pet: false }), row(NumberTag { kind: 1, pet: false }));
    }

    /// Default draws the game's colour (none), a choice its paint, each row
    /// its own; an untagged number is the game's whatever is chosen.
    #[test]
    fn a_rows_colour_is_its_choice() {
        let mut c = [0; ROWS];
        for k in 1..=7 {
            assert_eq!(rgb(tag(k, false), &c), None, "every row Default: the game's colours");
            assert_eq!(rgb(tag(k, true), &c), None);
        }
        assert!(!wanted(&c), "nothing asked of the server");
        c[1] = from_key("red").unwrap();
        c[5] = from_key("cyan").unwrap();
        assert!(wanted(&c));
        assert_eq!(rgb(tag(5, false), &c), Some([255, 48, 48]), "Your Procs");
        assert_eq!(rgb(tag(6, true), &c), Some([0, 240, 240]), "Pet Heals");
        assert_eq!(rgb(tag(7, true), &c), Some([0, 240, 240]), "the pet's heal over time: Pet Heals");
        assert_eq!(rgb(tag(1, false), &c), None, "Your Damage still Default");
        assert_eq!(rgb(tag(5, true), &c), None, "Pet Procs still Default");
        assert_eq!(rgb(None, &c), None, "untagged: the game's");
        assert_eq!(rgb(tag(9, false), &c), None, "a kind not known: the game's");
    }

    /// The round: Default, then the palette, then Default again, either way;
    /// every name and key its own; keys read back to their choice.
    #[test]
    fn the_round_and_the_keys() {
        let n = PALETTE.len() as Choice + 1;
        let mut c = 0;
        let mut seen = vec![name(c)];
        for _ in 1..n {
            c = step(c, 1);
            seen.push(name(c));
        }
        assert_eq!(seen[0], "Default");
        assert_eq!(seen[1..], PALETTE.iter().map(|p| p.name).collect::<Vec<_>>()[..]);
        assert_eq!(step(c, 1), 0, "round to Default");
        assert_eq!(step(0, -1), n - 1, "left of Default: the last");
        assert_eq!(step(200, 1), 0, "a choice past the palette steps on from the last");
        for (i, p) in PALETTE.iter().enumerate() {
            assert_eq!(from_key(p.key), Some(i as Choice + 1));
            assert_eq!(PALETTE.iter().filter(|q| q.name == p.name || q.key == p.key || q.rgb == p.rgb).count(), 1, "{} is its own", p.name);
        }
        assert_eq!(from_key("default"), Some(0));
        assert_eq!(from_key("mauve"), None);
        assert_eq!(name(200), "Default", "past the palette reads as Default");
        assert_eq!(swatch(0), None);
        assert_eq!(swatch(from_key("white").unwrap()), Some([1.0; 4]));
    }
}
