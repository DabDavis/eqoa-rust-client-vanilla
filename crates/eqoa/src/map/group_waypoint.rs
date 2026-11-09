//! The group's waypoint (F3 20261026-212034, a Sandstorm player on a
//! controller: "should allow a Long-press square on map to set a group
//! waypoint"; the user's choice: over the group's chat). The client's own: the
//! game has no such thing, and no server is asked for anything new.
//!
//! On the map a long press puts it (□ held `HOLD` seconds, so Q held on a
//! keyboard; G at once; a click with Shift held; a finger held still on the
//! map), at the spot `Map` picks (its header says which). The spot goes to the
//! group as a plain line of group chat through the game's own channel (`/g`:
//! 0x000E 2, 0x000B, 0x000E back), at most one every `EVERY_MS`: `line`'s
//! "Waypoint in Tunaria at x 24611, z 15502 near Freeport", short plain words a
//! PS2 player in the group reads as they are (the map's readout's x and z, the
//! world's and the area's names the game's own, out of WORDS.TXT). Every
//! server relays it as "<Name> tells the group: <line>" on 0x069E
//! (`net::session::Event::GroupChat`): go-eqoa-server's `groupChat`, and
//! Sandstorm's (its players' chat logs in the F3 reports carry the same words;
//! the C# server it descends from sends them on 0x069E).
//!
//! Every vanilla client in the group that hears such a line puts the group's
//! waypoint on its map, its minimap and its compass's rim (`COLOR`, orange: no
//! group member's colour, nor the personal waypoint's magenta), its sender's
//! name under it on the map. One at a time: a newer one replaces it. It goes
//! when the player reaches it, when its sender is no longer in the group or the
//! player leaves the group, and with a click on it (or □ where the personal
//! one would go, `Map::square`). The sender's own line puts the sender's too
//! (at once, and again as the server echoes it). Kept in memory only, never
//! sent anywhere else.
//!
//! Heard strictly (`parse`): only the group channel's lines (0x069E: a say, a
//! shout or a tell comes as `Event::Chat` and is never read here), only a line
//! that begins with a member's name of the group as it stands (`Play`'s
//! check), and only these exact words; a world it cannot name is nothing.

/// The group waypoint's colour on the map, the minimap and the compass:
/// orange, which neither a member's (green, blue, cyan, yellow), the
/// personal waypoint's magenta nor the player's gold dart is.
pub const COLOR: [f32; 4] = [1.0, 0.5, 0.05, 1.0];
/// How long □ (Q on a keyboard) or a finger on the map is held for a long
/// press, seconds (by the clock, `Map::hold`).
pub const HOLD: f32 = 0.6;
/// The least time between two of the player's group waypoints, ms.
pub const EVERY_MS: u32 = 5000;
/// What the server puts between a group line's sender and its words.
pub const PHRASE: &str = " tells the group: ";
/// The line's first words.
const OPENING: &str = "Waypoint in ";
/// The longest world name, area name and character name read, in characters
/// (the game's longest world, "Lavastorm Mountains", is 19; a name 24 bytes
/// on the wire).
const WORLD_MOST: usize = 40;
const AREA_MOST: usize = 40;
const NAME_MOST: usize = 24;

/// The line said to the group for a spot `at` (x, z) of world `world`, and
/// the area it is in when the map names one. Whole numbers, as the map's
/// readout shows them (`Map::compose`'s "x 24611, z 15502").
pub fn line(world: &str, at: [f32; 2], area: Option<&str>) -> String {
    let [x, z] = whole(at);
    let mut s = format!("{OPENING}{world} at x {x}, z {z}");
    if let Some(a) = area.map(str::trim).filter(|a| words(a, AREA_MOST)) {
        s.push_str(" near ");
        s.push_str(a);
    }
    s
}

/// A spot's whole units (half away from zero; never "-0").
fn whole(at: [f32; 2]) -> [i64; 2] {
    [at[0].round() as i64, at[1].round() as i64]
}

/// A spot as the line carries it: to the whole unit.
pub fn rounded(at: [f32; 2]) -> [f32; 2] {
    whole(at).map(|v| v as f32)
}

/// A group line heard (`parse`): who said it, the world's name and the spot.
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
    pub from: String,
    pub world: String,
    pub at: [f32; 2],
}

/// A group chat line as the server relays it: "<Name> tells the group:
/// Waypoint in <World> at x <n>, z <n>", then " near <Area>" or nothing; any
/// other words, a number with a fraction, a sign but a leading minus or more
/// than six digits, or a name with a colon (a say's "Name: ...") is none.
pub fn parse(text: &str) -> Option<Heard> {
    let (from, said) = text.split_once(PHRASE)?;
    if !words(from, NAME_MOST) || from.contains(':') {
        return None;
    }
    let rest = said.strip_prefix(OPENING)?;
    let (world, rest) = rest.split_once(" at x ")?;
    let (x, rest) = rest.split_once(", z ")?;
    let (z, area) = match rest.split_once(" near ") {
        Some((z, a)) => (z, Some(a)),
        None => (rest, None),
    };
    if !words(world, WORLD_MOST) || area.is_some_and(|a| !words(a, AREA_MOST)) {
        return None;
    }
    Some(Heard { from: from.to_string(), world: world.to_string(), at: [coord(x)?, coord(z)?] })
}

/// A coordinate's whole number: an optional minus and one to six digits.
fn coord(s: &str) -> Option<f32> {
    let digits = s.strip_prefix('-').unwrap_or(s);
    if !(1..=6).contains(&digits.len()) || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<i32>().ok().map(|v| v as f32)
}

/// Whether `s` is words of 1 to `most` characters, none a control, with no
/// space at either end.
fn words(s: &str, most: usize) -> bool {
    (1..=most).contains(&s.chars().count()) && s.trim() == s && !s.chars().any(char::is_control)
}

/// Two characters' names the same: as the server spells them, in any case,
/// a trailing NUL or space aside (a roster's names are fixed fields).
pub fn same_name(a: &str, b: &str) -> bool {
    let clean = |s: &str| s.trim_end_matches(['\0', ' ']).trim_start().to_string();
    let (a, b) = (clean(a), clean(b));
    !a.is_empty() && a.eq_ignore_ascii_case(&b)
}

/// The group's waypoint: its world (`gWorldInfo`'s index, as the map keeps
/// the personal one), its spot (x, z) and the member who put it.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupMark {
    pub world: i32,
    pub at: [f32; 2],
    pub from: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heard(text: &str) -> Option<([f32; 2], String, String)> {
        parse(text).map(|h| (h.at, h.world, h.from))
    }

    /// A line round-trips through the server's relaying: the world, the spot
    /// to the whole unit and the sender come back; with an area and without,
    /// a world of two words, a negative coordinate. The PS2's chat box wraps
    /// at 63 characters, but the line is read whole, before any wrap.
    #[test]
    fn a_line_round_trips() {
        for (world, at, area) in [("Tunaria", [24611.4, 15502.6], Some("Freeport")), ("Lavastorm Mountains", [4768.0, 4500.0], None), ("Plane of Sky", [-120.5, 3.0], Some("Island 3 (Grove)"))] {
            let said = line(world, at, area);
            assert!(said.len() < 100 && said.is_ascii(), "{said}");
            let relayed = format!("Dairine{PHRASE}{said}");
            assert_eq!(heard(&relayed), Some((rounded(at), world.to_string(), "Dairine".to_string())), "{relayed}");
        }
        assert_eq!(line("Tunaria", [24611.4, 15502.6], Some("Freeport")), "Waypoint in Tunaria at x 24611, z 15503 near Freeport");
        assert_eq!(line("Odus", [10.0, 20.0], Some("  ")), "Waypoint in Odus at x 10, z 20", "no area named: none said");
        assert_eq!(line("Odus", [-0.4, 0.2], None), "Waypoint in Odus at x 0, z 0", "never a minus zero");
        // the control: the words without the server's sender are no group line
        assert_eq!(heard(&line("Tunaria", [1.0, 2.0], None)), None);
    }

    /// Only these exact words: a say, a shout or a tell quoting them, a line
    /// with more before or after, a number with a fraction, a plus or seven
    /// digits, a lower-case opening, a name with a colon, an empty world. The
    /// control: the same line as the server relays it is read.
    #[test]
    fn anything_else_is_nothing() {
        let good = "Bob tells the group: Waypoint in Tunaria at x 1, z 2";
        assert!(heard(good).is_some(), "the control");
        for bad in [
            "Bob: Waypoint in Tunaria at x 1, z 2",
            "Bob shouts: Waypoint in Tunaria at x 1, z 2",
            "Bob tells you, 'Waypoint in Tunaria at x 1, z 2'",
            "Mallory: Bob tells the group: Waypoint in Tunaria at x 1, z 2",
            "Bob tells the group: hey Waypoint in Tunaria at x 1, z 2",
            "Bob tells the group: Waypoint in Tunaria at x 1, z 2!",
            "Bob tells the group: Waypoint in Tunaria at x 1, z 2 ",
            "Bob tells the group: Waypoint in Tunaria at x 1.5, z 2",
            "Bob tells the group: Waypoint in Tunaria at x +1, z 2",
            "Bob tells the group: Waypoint in Tunaria at x 1234567, z 2",
            "Bob tells the group: Waypoint in Tunaria at x 1,z 2",
            "Bob tells the group: waypoint in Tunaria at x 1, z 2",
            "Bob tells the group: Waypoint in  at x 1, z 2",
            "Bob tells the group: Waypoint in Tunaria at x 1, z 2 near ",
            " tells the group: Waypoint in Tunaria at x 1, z 2",
            "Bob tells the group: Waypoint in Tunaria at x -, z 2",
        ] {
            assert_eq!(heard(bad), None, "{bad:?}");
        }
        assert_eq!(heard("Bob tells the group: Waypoint in Tunaria at x -7, z 2 near Freeport"), Some(([-7.0, 2.0], "Tunaria".into(), "Bob".into())));
    }

    /// Names match as the server spells them in any case, a roster's
    /// trailing NULs aside; the control: another name, or none, does not.
    #[test]
    fn names_match_in_any_case() {
        assert!(same_name("Dairine", "dairine"));
        assert!(same_name("Dairine\0\0", "Dairine"));
        assert!(!same_name("Dairine", "Dairin"), "the control");
        assert!(!same_name("", ""));
    }
}
