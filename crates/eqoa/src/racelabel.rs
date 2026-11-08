//! A race a server has and the game does not (the user, 2026-11-07: "my race says
//! ogre, but lets make it Hill Giant, Titan as MC"). go-eqoa-server keeps such races
//! in a table of its own, and on the wire each passes as a race of the game's
//! (`net::session::OP_RACE_LABELS`): its camera, its items, the `$Race` tables are
//! that race's on every client, and nothing here changes them. What this client
//! does change is the word: the server tells it, by character name, who reads
//! differently and what their race's words are, and every place that shows a named
//! character's race asks here first ([`long`], [`short`]). A name not on the list
//! shows the game's word, and a server that sends no list (Sandstorm's, eqoa.live's,
//! a PS2's view of any) is the game as it was. A second race needs nothing new: the
//! words come from the list.
//!
//! One list a program, as one `VIClient` a PS2 (`live::features`); a new world entry
//! starts with none (`Play::new`). A thread's own, so tests running at once never see
//! each other's.
//!
//! What it does not reach: character select, which shows no race at all (level and
//! class, `pregame::client::select`), and the list of races an item's text prints
//! (`infotext`), which says who may wear it, by the race on the wire.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A race's two words: the race line's, and the short columns' (who list, guild
/// roster, buddy list, group search).
#[derive(Debug, PartialEq)]
pub struct Label {
    pub long: String,
    pub short: String,
}

/// The list: each character's race's words, by the character's name as the
/// server spells it everywhere.
pub type Labels = HashMap<String, Rc<Label>>;

thread_local! {
    static LABELS: RefCell<Labels> = RefCell::default();
}

/// 0x7E1D's body: a u8 count of races, then each race's name and short name, a u16
/// count of its characters and each one's name (a string is a u8 byte count and
/// UTF-8). `None` for one cut short.
pub fn read(data: &[u8]) -> Option<Labels> {
    let mut at = 0;
    let mut bytes = |n: usize| {
        let b = data.get(at..at + n)?;
        at += n;
        Some(b)
    };
    let mut labels = Labels::new();
    let races = bytes(1)?[0];
    for _ in 0..races {
        let n = bytes(1)?[0] as usize;
        let long = String::from_utf8_lossy(bytes(n)?).into_owned();
        let n = bytes(1)?[0] as usize;
        let short = String::from_utf8_lossy(bytes(n)?).into_owned();
        let label = Rc::new(Label { long, short });
        let names = u16::from_le_bytes(bytes(2)?.try_into().ok()?);
        for _ in 0..names {
            let n = bytes(1)?[0] as usize;
            labels.insert(String::from_utf8_lossy(bytes(n)?).into_owned(), label.clone());
        }
    }
    Some(labels)
}

/// The list the server sent: the whole of it, the last one standing. One cut
/// short leaves the one before.
pub fn take(data: &[u8]) {
    match read(data) {
        Some(l) => set(l),
        None => eprintln!("race labels: a list of {} bytes cut short: the last kept", data.len()),
    }
}

/// `labels` the list.
pub fn set(labels: Labels) {
    LABELS.with_borrow_mut(|l| *l = labels);
}

/// No list: a new world entry.
pub fn clear() {
    set(Labels::new());
}

/// Whether the list names anyone: the server has races of its own.
pub fn any() -> bool {
    LABELS.with_borrow(|l| !l.is_empty())
}

/// The race line's word for the character of this name, if it reads differently.
pub fn long(name: &str) -> Option<String> {
    LABELS.with_borrow(|l| l.get(name).map(|x| x.long.clone()))
}

/// The short columns' word for the character of this name, if it reads differently.
pub fn short(name: &str) -> Option<String> {
    LABELS.with_borrow(|l| l.get(name).map(|x| x.short.clone()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The list's bytes, as go-eqoa-server writes it: `(race, short, names)` a race.
    pub(crate) fn list(races: &[(&str, &str, &[&str])]) -> Vec<u8> {
        let mut b = vec![races.len() as u8];
        let s = |b: &mut Vec<u8>, t: &str| {
            b.push(t.len() as u8);
            b.extend(t.as_bytes());
        };
        for (long, short, names) in races {
            s(&mut b, long);
            s(&mut b, short);
            b.extend((names.len() as u16).to_le_bytes());
            for n in *names {
                s(&mut b, n);
            }
        }
        b
    }

    /// A raster that keeps the strings a window draws.
    #[derive(Default)]
    pub(crate) struct Drawn(pub Vec<String>);

    impl crate::ui::Layout for Drawn {}

    impl vi::ui::Raster for Drawn {
        fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], s: &[u16]) {
            self.0.push(String::from_utf16_lossy(s));
        }
        fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], s: &[u16], _: u32) {
            self.0.push(String::from_utf16_lossy(s));
        }
        fn string_width(&self, s: &[u16]) -> i32 {
            s.len() as i32 * 8
        }
        fn point_size(&self) -> i32 {
            16
        }
        fn viewport(&self) -> [i32; 4] {
            [0, 0, 640, 448]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    /// A `Vui` whose WORDS.TXT line N says wN, so what is drawn says which.
    pub(crate) fn numbered_vui() -> vi::ui::Vui {
        let mut v = crate::pointer::testing::vui();
        v.words = crate::groupsearch::tests::numbered();
        v
    }

    /// `(race, short, names)`s the list on this test's thread.
    pub(crate) fn given(races: &[(&str, &str, &[&str])]) {
        take(&list(races));
    }

    /// Each name its race's words, a second race beside the first needing nothing
    /// of its own; a name not on the list has none. A later list replaces the whole,
    /// an empty one empties it, and one cut short at any byte leaves the last.
    #[test]
    fn the_list_names_who_reads_differently() {
        clear();
        assert_eq!((long("Lagerbottoms"), short("Lagerbottoms")), (None, None), "the control: no list");
        let whole = list(&[("Hill Giant", "HGT", &["Lagerbottoms", "Bigfellow"]), ("Stone Giant", "SGT", &["Boulder"])]);
        take(&whole);
        assert_eq!(long("Lagerbottoms").as_deref(), Some("Hill Giant"));
        assert_eq!(short("Bigfellow").as_deref(), Some("HGT"));
        assert_eq!((long("Boulder").as_deref(), short("Boulder").as_deref()), (Some("Stone Giant"), Some("SGT")));
        assert_eq!(long("Drizzt"), None, "a name not on the list");
        assert_eq!(long("lagerbottoms"), None, "as the server spells it");
        for cut in 0..whole.len() {
            assert_eq!(read(&whole[..cut]), None, "cut at {cut}");
            take(&whole[..cut]);
            assert_eq!(short("Boulder").as_deref(), Some("SGT"), "the last list kept");
        }
        given(&[("Hill Giant", "HGT", &["Lagerbottoms"])]);
        assert_eq!((long("Bigfellow"), long("Boulder")), (None, None), "the whole list replaced");
        take(&[0]);
        assert_eq!(long("Lagerbottoms"), None, "an empty list");
        given(&[("Hill Giant", "HGT", &["Lagerbottoms"])]);
        clear();
        assert_eq!(long("Lagerbottoms"), None, "a new world entry");
    }
}
