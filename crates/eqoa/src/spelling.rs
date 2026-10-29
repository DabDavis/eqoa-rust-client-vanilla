//! The game's words with a misspelling put right. The disc's WORDS.TXT (and
//! a server's copy of it) spells the gnomes' city "Klick'Anon", in its name,
//! the zones' names the compass and the start menu's Location show, and the
//! race page's text; players know it as Klik'Anon (F3 20261025-172939; the
//! user: "you can spell it Klik though"). Only what is shown changes: a
//! word's row and everything the game reads by row stay the disc's.

use vi::strtab::StringTable;

/// Each fix: as the words spell it, as it is shown.
const FIXES: &[(&str, &str)] = &[("Klick'Anon", "Klik'Anon")];

/// WORDS.TXT read as `StringTable::parse` reads it, with `FIXES` made and
/// the live client's added rows put in where it lacks them (`livewords`).
pub fn words(bytes: &[u8]) -> Option<StringTable> {
    let mut t = StringTable::parse(bytes)?;
    for (from, to) in FIXES {
        t.replace(from, to);
    }
    crate::livewords::fill(&mut t);
    Some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn klik_anon_is_spelled_as_players_know_it() {
        let b = "\u{feff}Gnome\r\nKlick'Anon\r\nThe Gnomes of Klick'Anon are a carefree race.\r\nKlick".as_bytes();
        let t = words(b).unwrap();
        assert_eq!(t.get(2, 0), "Klik'Anon");
        assert_eq!(t.get(3, 0), "The Gnomes of Klik'Anon are a carefree race.");
        // a word that only starts the same is left alone, and so is the rest
        assert_eq!(t.get(4, 0), "Klick");
        assert_eq!(t.get(1, 0), "Gnome");
        // the plain parse keeps the disc's spelling: the fix is this module's
        assert_eq!(StringTable::parse(b).unwrap().get(2, 0), "Klick'Anon");
    }

    /// The disc's own WORDS.TXT: both of its two Klick'Anons are put right
    /// and no row moves. Skips without the disc.
    #[test]
    fn the_discs_words_lose_every_klick() {
        let Ok(mut iso) = iso::Iso::open(crate::disc_path(None)) else { return };
        let Ok(b) = iso.read("WORDS.TXT") else { return };
        let (disc, fixed) = (StringTable::parse(&b).unwrap(), words(&b).unwrap());
        assert_eq!(disc.cols, fixed.cols);
        // the live client's rows come after the disc's (`livewords::fill`)
        assert_eq!((disc.rows, fixed.rows), (1815, 1889));
        assert_eq!(fixed.get(1840, 0), crate::livewords::text(1840).unwrap());
        let mut changed = 0;
        for r in 1..=disc.rows as i32 {
            for c in 0..disc.cols as i32 {
                let (a, f) = (disc.get(r, c), fixed.get(r, c));
                assert!(!f.contains("Klick'Anon"), "row {r} col {c}: {f}");
                if a != f {
                    assert_eq!(a.replace("Klick'Anon", "Klik'Anon"), f);
                    changed += 1;
                }
            }
        }
        assert_eq!(changed, 2, "WORDS.TXT spells it Klick'Anon twice");
    }
}
