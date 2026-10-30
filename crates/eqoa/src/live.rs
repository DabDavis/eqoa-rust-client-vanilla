//! SOE's live client (`c:/eqoa-live-1500`, the CLIENT the servers ran last,
//! read out of the Corsten and Lostologist save states,
//! `../claude-notes/live-1500-build-in-retail-savestates-20261015.md`): its
//! server feature word (0x07F5), which more than one of this client's
//! windows reads. The words its windows show past the disc's WORDS.TXT are
//! `livewords`'.
//!
//! The disc has no case for 0x07F5: a server that sends none (Sandstorm,
//! eqoa.live, any PS2's) is played as the disc plays it, and one that sends
//! it gets what the live client does with each of its bits.

/// `VIClient+0x813C` as `GetFeatures` (live 0x012C79E0) reads it: the u32
/// the server's 0x07F5 last carried (handler 0x012C9548; 0x1B in both retail
/// save states), or `None` while no server has sent one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Features(pub Option<u32>);

impl Features {
    /// The Start menu's Auction entry and the bank menu's Auction row (the
    /// disc's are the MemoryDump's bank flag, `m_p78CC`).
    pub const AUCTION: u32 = 0x01;
    /// Race 9 (Ogre) and class 14 (Alchemist) in the pickers and in an
    /// item's race and class lines.
    pub const RACES: u32 = 0x02;
    /// The auction's stacks: a lot's quantity and its prices per item.
    pub const STACKS: u32 = 0x08;
    /// The duel part's challenge and surrender (VIWndDuel; the duel
    /// window's to read).
    #[allow(dead_code)]
    pub const DUELS: u32 = 0x10;

    /// A server sent the word: the live client's windows, each part gated as
    /// it gates it.
    pub fn live(self) -> bool {
        self.0.is_some()
    }

    /// The word has `bit` (none has none).
    pub fn has(self, bit: u32) -> bool {
        self.0.is_some_and(|w| w & bit != 0)
    }

    /// The auction offered: the word's bit 0x01, or with no word the disc's
    /// rule, the MemoryDump's bank flag (`dump`).
    pub fn auction(self, dump: bool) -> bool {
        match self.0 {
            Some(w) => w & Self::AUCTION != 0,
            None => dump,
        }
    }

    /// Race 9 and class 14 shown: the disc shows every race and class; the
    /// live client only with bit 0x02.
    pub fn all_races(self) -> bool {
        self.0.is_none_or(|w| w & Self::RACES != 0)
    }

    /// The race at `race` is one this word lets a picker or a mask show.
    pub fn shows_race(self, race: i32) -> bool {
        race != 9 || self.all_races()
    }

    /// The class at `class` is one this word lets a picker or a mask show.
    pub fn shows_class(self, class: i32) -> bool {
        class != 14 || self.all_races()
    }

    /// 0x07F5's payload: one u32 (shorter is nothing).
    pub fn parse(data: &[u8]) -> Option<u32> {
        data.first_chunk::<4>().map(|b| u32::from_le_bytes(*b))
    }
}

#[cfg(not(test))]
static WORD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(test)]
thread_local! {
    /// A test's own word, so tests running at once never see each other's.
    static WORD: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Bit 32 marks a word as sent, the low 32 bits are the word.
fn encode(f: Features) -> u64 {
    f.0.map_or(0, |w| (1 << 32) | w as u64)
}

fn decode(v: u64) -> Features {
    Features((v >> 32 != 0).then_some(v as u32))
}

/// The session's feature word (one client a program, as one `VIClient` a
/// PS2).
pub fn features() -> Features {
    #[cfg(not(test))]
    let v = WORD.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    let v = WORD.with(|w| w.get());
    decode(v)
}

/// The word a server sent (`Msg07F5`), or none at a new session.
pub fn set_features(f: Features) {
    #[cfg(not(test))]
    WORD.store(encode(f), std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    WORD.with(|w| w.set(encode(f)));
}

/// `f` run with the word `f` as this thread's (a test's), put back after.
#[cfg(test)]
pub fn with_features<T>(word: Option<u32>, f: impl FnOnce() -> T) -> T {
    let old = features();
    set_features(Features(word));
    let out = f();
    set_features(old);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No word is the disc's rules (the auction by the dump's flag, every
    /// race and class); a word is the live client's, bit by bit.
    #[test]
    fn the_word_gates_as_the_live_client_does() {
        let none = Features(None);
        assert!(!none.live() && !none.has(Features::STACKS));
        assert!(none.auction(true) && !none.auction(false), "the disc: the dump's flag");
        assert!(none.all_races() && none.shows_race(9) && none.shows_class(14));
        let retail = Features(Some(0x1B));
        assert!(retail.live() && retail.has(Features::STACKS) && retail.has(Features::DUELS) && !retail.has(0x04));
        assert!(retail.auction(false), "bit 0x01, whatever the dump says");
        assert!(retail.shows_race(9) && retail.shows_class(14));
        // the control: a word without 0x01 and 0x02 (go-eqoa-server's duel bit alone)
        let duels = Features(Some(0x10));
        assert!(!duels.auction(true), "the dump's flag no longer counts");
        assert!(!duels.shows_race(9) && duels.shows_race(8) && !duels.shows_class(14) && duels.shows_class(13));
        assert_eq!(Features::parse(&[0x1B, 0, 0, 0, 9]), Some(0x1B));
        assert_eq!(Features::parse(&[0x1B, 0, 0]), None);
    }

    /// The word is the thread's own in a test, and a new session's none.
    #[test]
    fn the_word_is_kept_until_the_next_session() {
        assert_eq!(features(), Features(None));
        set_features(Features(Some(0)));
        assert!(features().live() && !features().has(Features::AUCTION), "a word of 0 is a word");
        with_features(Some(8), || assert!(features().has(Features::STACKS)));
        assert_eq!(features(), Features(Some(0)), "put back");
        set_features(Features(None));
        assert!(!features().live());
    }

    /// `livewords`' rows against the live client's own table, read out of
    /// the Corsten save state (`$EQOA_LIVE_STATE`, default
    /// `1.6-savestates/EEEE1FCC.001.p2s` beside this repository: PCSX2's old
    /// gzip state, EE RAM at offset 228): the `VIStringTable` at 0x01FAD924,
    /// each row's cell a pointer to its UTF-16. Every row 1840..1889 is
    /// there, word for word, its second column empty, and row 1890 (the
    /// table's last) is empty. Skipped without the state.
    #[test]
    fn the_words_are_the_live_clients() {
        use std::io::Read;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let path = std::env::var_os("EQOA_LIVE_STATE").map(std::path::PathBuf::from).unwrap_or_else(|| root.join("1.6-savestates/EEEE1FCC.001.p2s"));
        let Ok(file) = std::fs::File::open(&path) else {
            eprintln!("SKIP: no live save state at {}", path.display());
            return;
        };
        let mut state = Vec::new();
        flate2::read::GzDecoder::new(file).read_to_end(&mut state).unwrap();
        let ee = &state[228..228 + (32 << 20)];
        let w32 = |a: u32| u32::from_le_bytes(ee[a as usize..a as usize + 4].try_into().unwrap());
        let (rows, cols, strings) = (w32(0x01FA_D924), w32(0x01FA_D928), w32(0x01FA_D930));
        assert_eq!((rows, cols), (1890, 2));
        let word = |id: u32, col: u32| -> String {
            let p = w32(strings + 4 * ((id - 1) * cols + col));
            let mut s = Vec::new();
            let mut a = p as usize;
            while p != 0 && ee[a] | ee[a + 1] != 0 {
                s.push(u16::from_le_bytes([ee[a], ee[a + 1]]));
                a += 2;
            }
            String::from_utf16_lossy(&s)
        };
        for id in 1840..=1889 {
            assert_eq!(word(id, 0), crate::livewords::text(id as i32).unwrap(), "word {id}");
            assert_eq!(word(id, 1), "", "word {id}'s second column");
        }
        assert_eq!(word(1890, 0), "");
        // the control: a row the disc has keeps its own number (1673 Search)
        assert_eq!(word(1673, 0), "Search");
    }
}
