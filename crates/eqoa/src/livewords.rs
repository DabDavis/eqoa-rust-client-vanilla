//! SOE's final live client's WORDS: the rows `c:/eqoa-live-1500` added past
//! the Frontiers disc's WORDS.TXT, and the rows it changed in place, for the
//! features ported from that client (its weapon sets, mail, the one Master
//! Class, the destroy and sell boxes, petitions, duels, the group search,
//! auction stacks). One table for all of them: a feature that needs a live
//! word reads it here, and a word not yet here is added here.
//!
//! Every WORDS.TXT the client reads (the disc's, a server's, S2's) goes
//! through `spelling::words`, which `fill`s the added rows in where the
//! table lacks them, so `words.get(1840, ..)` reads the live word anywhere.
//! A row the table has stays its own; the rows changed in place keep the
//! disc's text (the disc's forms read them), and a live form takes the live
//! text with `live`.
//!
//! Read off the live client's `VIStringTable` (`VIClient+4`, 0x01FAD924 in
//! Corsten's retail state: 1,890 rows by 2 columns, the second empty; its
//! cells' pointers at 0x01107340, the text at 0x010F0920). `GetString` does
//! `row--`, so ids count from 1, as `vi::strtab` takes them. The disc's
//! table has 1,814 rows (1,815 with its last, empty one).
//!
//! The live client inserted 25 rows at 1811-1835 (Space, Backspace, the
//! network and account-creation errors, "Shift+Enter = End", ...), moving the
//! disc's 1811-1814 to 1836-1839. Vanilla's code is the disc's and reads
//! 1811-1814 in the disc's numbering, so those rows are not given here: a
//! live id from 1811 to 1839 is not the disc's same id, and a port that
//! needs one adds it under an id of its own with the live id said.

use vi::strtab::StringTable;

/// The first row past the disc's table that the live client added.
pub const FIRST_ADDED: i32 = 1840;

/// The rows the live client added, 1840 to 1889 in order.
const ADDED: [&str; 50] = [
    // 1840: the one Master Class rule (VIWndClassMasteryInspect::OnKeyDown)
    "You cannot possess more than one Master Class at a time.",
    // 1841, 1842: the destroy and sell boxes (VIWndItemMenu::Use, VIWndMerchant::Sell)
    "Destroy item \"%S\"?",
    "Sell item \"%S\"?",
    // 1843: the options' Screen Shake
    "Screen Shake",
    // 1844-1852: the group search
    "Sort",
    "Search for Group",
    "Search for Player",
    "Your group is full.",
    "You are not a group leader.",
    "%d players found",
    "%d groups found",
    "Register",
    "Unregister",
    // 1853-1858: auction stacks
    "Quantity",
    "Min. price per",
    "Max. price per",
    "High bid",
    "Buy all now for %d Tunar each",
    "You can only bid the buy-it-now price if you bid on all %d items in the stack.",
    // 1859-1869: duels
    "The duel invitation has expired.",
    "Your duel invitation has been declined.",
    "%s declined your duel invitation.",
    "You cannot duel yourself.",
    "Invalid target.",
    "Duel",
    "Surrender",
    "%s challenges you to a duel.",
    "Wager",
    "You",
    "Opponent",
    // 1870, 1871: mail (the undelivered names, the reader's Reply All)
    "Unable to deliver to the following recipients",
    "Reply All",
    // 1872: duels
    "Cancel duel?",
    // 1873-1875: auction stacks
    "Set Quantity",
    "Multiple",
    "CLEAR",
    // 1876-1882: the group search's help
    "You can register or unregister in chat with:",
    "/lfg on",
    "/lfg off",
    "You can set Min and Max Level to a default level range of characters you could hunt with and receive EXP by entering -1.",
    "To sort the results page, use |7 and |8 to switch between different columns of results. This will sort those columns as well.",
    "It is more polite to send a tell before sending a group invite.  Tell them about where you want to hunt, who is with you already, etc. and then ask if they want to group.",
    "Have fun!",
    // 1883: duels
    "%s is in a duel.",
    // 1884: a slash command where the mode takes none
    "You cannot enter slash commands in this mode.",
    // 1885, 1886: the resets (referenced by no live code: the live server's text)
    "Your Mastery points have been reset",
    "Your Training points have been reset",
    // 1887: GM Programming's Show Pad
    "Show Pad",
    // 1888: petitions, the -8 box (the live text's spelling)
    "You may be seeing this error message because this petition has been addressed.  If further assitance is needed, please submit a new petition.  Thank you.",
    // 1889
    "Bogus",
];

/// The rows the live client changed in place: (id, the disc's text, the
/// live client's). A live format pairs with the live text (841 under "%S"
/// reads "Ticket #"; the disc's "%S #" took "Ticket").
const CHANGED: [(i32, &str, &str); 8] = [
    (159, "UNUSED", "bare hands"),
    (160, "UNUSED", "NAME CONFIGURATION"),
    (201, "INFO", "Info"),
    (827, "REASON_UNKNOWN", "REASON UNKNOWN"),
    (841, "Ticket", "Ticket #"),
    (890, "Unknown recipient.", "Unknown recipient(s)"),
    (
        1390,
        "Your e-mail address contains invalid characters.  The only allowed characters are letters, numbers, '_', '@', and '.'",
        "Your e-mail address contains invalid characters.  The only allowed characters are letters, numbers, ' ', '@', and '.'",
    ),
    (1701, "Acquire now for %d Tunar", "Buy now for %d Tunar"),
];

/// The live client's text for a row it added (1840-1889) or changed in
/// place, else None.
pub fn text(id: i32) -> Option<&'static str> {
    if let Some(s) = usize::try_from(id - FIRST_ADDED).ok().and_then(|i| ADDED.get(i)) {
        return Some(s);
    }
    CHANGED.iter().find(|c| c.0 == id).map(|c| c.2)
}

/// The table's row is not there: past its end, or a cell it lacks (the
/// table's "(null)"), empty, or "UNUSED".
fn missing(words: &StringTable, id: i32, w: &str) -> bool {
    id < 1 || id as usize > words.rows || w == "(null)" || w.is_empty() || w.eq_ignore_ascii_case("unused")
}

/// Word `id`: the table's own where it has the row (a server's WORDS.TXT may
/// carry it, in its own words), else the live client's for a row it added or
/// filled (159 and 160 read "UNUSED" on the disc). A row the table has stays
/// the table's, so this is safe wherever the disc's words are read.
pub fn word(words: &StringTable, id: i32, language: i32) -> String {
    let w = words.get(id, language);
    match text(id) {
        Some(live) if missing(words, id, w) => live.to_string(),
        _ => w.to_string(),
    }
}

/// The added rows (1840-1889) put into `words` where it lacks them, in
/// each of its columns: past its end (the table grown to 1889), a cell
/// missing, empty or "UNUSED". The rows it has are its own.
pub fn fill(words: &mut StringTable) {
    for (i, live) in ADDED.iter().enumerate() {
        let id = FIRST_ADDED + i as i32;
        for col in 0..words.cols.max(1) as i32 {
            if missing(words, id, words.get(id, col)) {
                words.set(id, col, live);
            }
        }
    }
}

/// Word `id` for a form ported from the live client: as `word`, and the
/// live client's text for a row it changed in place where the table still
/// reads the disc's there (a table with its own text keeps it).
pub fn live(words: &StringTable, id: i32, language: i32) -> String {
    let w = words.get(id, language);
    if let Some(&(_, disc, live)) = CHANGED.iter().find(|c| c.0 == id)
        && (missing(words, id, w) || w == disc)
    {
        return live.to_string();
    }
    word(words, id, language)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The disc's table: 1,814 rows, "UNUSED" at 159 and 160, "Ticket" at
    /// 841, "Unknown recipient." at 890.
    fn disc() -> StringTable {
        let mut lines: Vec<String> = (1..=1814).map(|i| format!("w{i}")).collect();
        lines[158] = "UNUSED".into();
        lines[159] = "UNUSED".into();
        lines[840] = "Ticket".into();
        lines[889] = "Unknown recipient.".into();
        lines.push(String::new());
        StringTable::parse(lines.join("\r\n").as_bytes()).unwrap()
    }

    /// The rows past the disc's come from the live table, by its numbering:
    /// the first and the last it added, the mail and petition ones between.
    #[test]
    fn the_added_rows_are_the_live_tables() {
        assert_eq!(text(1840), Some("You cannot possess more than one Master Class at a time."));
        assert_eq!(text(1841), Some("Destroy item \"%S\"?"));
        assert_eq!(text(1842), Some("Sell item \"%S\"?"));
        assert_eq!(text(1870), Some("Unable to deliver to the following recipients"));
        assert_eq!(text(1871), Some("Reply All"));
        assert!(text(1888).unwrap().starts_with("You may be seeing this error message"));
        assert_eq!(text(1889), Some("Bogus"));
        assert_eq!(text(1890), None, "the live table's last row is empty");
        assert_eq!(text(1839), None, "1811-1839 are renumbered, not given");
        assert_eq!(text(1811), None);
        assert_eq!(text(0), None);
    }

    /// `word` fills only what the table lacks: a row past its end, "UNUSED";
    /// a row the table has stays its own, a changed row reads the disc's
    /// (the control: `live` reads the live text there).
    #[test]
    fn word_fills_what_the_table_lacks() {
        let t = disc();
        assert_eq!(word(&t, 1840, 0), text(1840).unwrap());
        assert_eq!(word(&t, 159, 0), "bare hands");
        assert_eq!(word(&t, 160, 0), "NAME CONFIGURATION");
        assert_eq!(word(&t, 841, 0), "Ticket", "a row the table has is its own");
        assert_eq!(word(&t, 890, 0), "Unknown recipient.");
        assert_eq!(word(&t, 1811, 0), "w1811", "the disc's numbering stays");
        assert_eq!(word(&t, 1815, 0), "", "the disc's last, empty row: no live word");
        assert_eq!(word(&t, 1900, 0), "(null)", "a row neither has");
        assert_eq!(live(&t, 841, 0), "Ticket #");
        assert_eq!(live(&t, 890, 0), "Unknown recipient(s)");
        assert_eq!(live(&t, 1871, 0), "Reply All");
        assert_eq!(live(&t, 882, 0), "w882");
    }

    /// `fill` puts the added rows past the disc's end (the rows between
    /// stay missing) and leaves every row the table has, the changed ones
    /// too; a table with its own rows there keeps them.
    #[test]
    fn fill_puts_the_added_rows_past_the_end() {
        let mut t = disc();
        fill(&mut t);
        assert_eq!(t.rows, 1889);
        assert_eq!(t.get(1840, 0), text(1840).unwrap());
        assert_eq!(t.get(1889, 0), "Bogus");
        assert_eq!(t.get(1838, 0), "(null)", "renumbered rows are not put in");
        assert_eq!((t.get(841, 0), t.get(159, 0), t.get(1814, 0)), ("Ticket", "UNUSED", "w1814"), "the disc's rows stay");
        let mut lines: Vec<String> = (1..=1889).map(|i| format!("own{i}")).collect();
        lines[1870] = String::new();
        let mut own = StringTable::parse(lines.join("\n").as_bytes()).unwrap();
        fill(&mut own);
        assert_eq!((own.get(1840, 0), own.get(1871, 0)), ("own1840", "Reply All"), "an empty row is filled");
        // two columns: each filled
        let mut two = StringTable::parse(b"a\tb\n").unwrap();
        fill(&mut two);
        assert_eq!((two.get(1840, 0), two.get(1840, 1)), (text(1840).unwrap(), text(1840).unwrap()));
    }

    /// A server's WORDS.TXT with the rows keeps its own text, in a live
    /// form too.
    #[test]
    fn a_tables_own_rows_are_kept() {
        let mut lines: Vec<String> = (1..=1889).map(|i| format!("own{i}")).collect();
        lines[840] = "Petition #".into();
        let t = StringTable::parse(lines.join("\n").as_bytes()).unwrap();
        assert_eq!(word(&t, 1840, 0), "own1840");
        assert_eq!(live(&t, 1840, 0), "own1840");
        assert_eq!(live(&t, 841, 0), "Petition #", "its own text, not the disc's");
    }
}
