//! The client's windows open on one computer (multiboxing), and the
//! controller each reads (`input::Gamepads`, the Extended page's Controller
//! Focus). Each window takes a place, the first free one, for as long as it
//! runs, and says beside it which controller it reads, so that a window among
//! others reads a controller no other one reads.
//!
//! Natively a place is a file in the config directory, `windows/<n>.lock`
//! (from 1), locked while its window runs (`File::try_lock`: the system lets
//! go of a lock however its program ends, a crash included), and the
//! controller it reads is `windows/<n>.pad` beside it, believed only while
//! the place is held. In a page a place is a Web Lock, `eqoa-window-<n>`,
//! held while the page is open (main.js), and the controller is in
//! localStorage: the windows that see each other there are one browser's
//! pages of the site. With no config directory (the test commands) a window
//! is alone.
//!
//! The others are looked at twice a second, and with them the client's one
//! setting they share, Capture Controller (`settings::look_shared`). Natively a free place is taken
//! for a moment as it is looked at (a shared lock) and let go, so a window
//! opening just then waits a moment on a place it finds held before it
//! passes it by.
//!
//! A place is the program's, whichever of its threads asks first: the
//! world's files are read on a thread of their own, and the settings it reads
//! there (`settings::client`, the art) ask for this window's place. While the
//! place was each thread's, that thread took a second one (F3 20261024-204603
//! and -203920: "multibox: this is window 3" logged at the world's entry by
//! a window that was window 1 of 2) and read that place's options for the
//! world, and while it read, the other windows saw one more window, which
//! took a controller in their sums.

use crate::settings::Focus;
use std::sync::{Mutex, MutexGuard};

/// The places: a window past the sixteenth is alone.
pub const PLACES: usize = 16;

/// How often the other windows are looked at.
const LOOK_MS: u128 = 500;

/// Another window of the client, open now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Other {
    /// its place, from 0
    pub place: usize,
    /// the controller it reads (`input`'s name for it), when it reads one
    pub pad: Option<String>,
    /// its Controller Focus
    pub focus: Focus,
    /// when it came to the front (ms since 1970), while it has the focus;
    /// none behind it, or from a build that does not say
    pub front: Option<u64>,
}

/// This window among the others.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seat {
    /// its place, from 0
    pub place: usize,
    /// the other windows open
    pub others: Vec<Other>,
}

impl Seat {
    /// No other window of the client is open.
    pub fn alone(&self) -> bool {
        self.others.is_empty()
    }

    /// The other window in front, if one came to the front after this one
    /// did (`mine`: when this one came, none while it does not have the
    /// focus): of two windows that both say they have the focus, the one
    /// that came last is in front. A window is told when it has the focus
    /// and when it has lost it, but a system can leave the second out
    /// (Steam's game mode on a Steam Deck switching between two windows:
    /// "swapping between focuses will end up having both in focus"), and the
    /// window behind went on taking the controller with the one in front.
    /// The same time is told apart by the place (the later one is in front).
    pub fn front_other(&self, mine: Option<u64>) -> Option<&Other> {
        let key = |o: &Other| (o.front, o.place);
        self.others.iter().filter(|o| o.front.is_some() && mine.is_none_or(|m| key(o) > (Some(m), self.place))).max_by_key(|o| key(o))
    }
}

/// Now, as a window says when it came to the front (ms since 1970: the
/// windows' one clock).
pub fn now_ms() -> u64 {
    web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

struct Here {
    /// the place taken: unasked, then one or none for good
    taken: Option<Option<Place>>,
    /// the place's number once it was let go (`leave`): no longer held for
    /// the other windows, still this window's for what it writes as it ends
    left: Option<usize>,
    others: Vec<Other>,
    looked: Option<web_time::Instant>,
    /// what this window said it reads, its Controller Focus and since when
    /// it is in front, and whether saying it last failed
    said: Option<(Option<String>, Focus, Option<u64>)>,
    failed: bool,
}

/// The program's place among the client's windows: one for all its threads.
static HERE: Mutex<Here> = Mutex::new(Here::new());

/// `HERE`, held (a thread that panicked holding it left nothing half done
/// that matters: the place is a file or a Web Lock).
fn here() -> MutexGuard<'static, Here> {
    HERE.lock().unwrap_or_else(|e| e.into_inner())
}

impl Here {
    const fn new() -> Here {
        Here { taken: None, left: None, others: Vec::new(), looked: None, said: None, failed: false }
    }

    /// The place, taken the first time it is asked for (in a page, once
    /// main.js has one).
    fn place(&mut self) -> Option<&Place> {
        self.place_by(take_place)
    }

    /// The place, taken by `take` the first time it is asked for.
    fn place_by(&mut self, take: impl FnOnce() -> Option<Option<Place>>) -> Option<&Place> {
        if self.taken.is_none() {
            self.taken = take();
        }
        self.taken.as_ref()?.as_ref()
    }

    /// The place let go (`leave`), its number kept.
    #[cfg(not(target_arch = "wasm32"))]
    fn leave(&mut self) {
        if let Some(Some(p)) = &self.taken {
            self.left = self.left.or(Some(p.n));
        }
        self.taken = Some(None);
    }
}

/// This window's place, from 0 (0 alone, with nowhere to keep one): which
/// window's Controller Focus is its own (`settings::focus`), and whose
/// options it keeps (`settings`: window 1's files as they always were).
pub fn place() -> usize {
    #[cfg(test)]
    if let Some(n) = testing::PLACE.with(|p| p.get()) {
        return n;
    }
    let mut h = here();
    if let Some(n) = h.left {
        return n;
    }
    h.place().map_or(0, |p| p.n)
}

#[cfg(test)]
pub mod testing {
    thread_local! {
        pub(super) static PLACE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    }

    /// This thread's window as if it held place `n` (`None`: as it finds).
    pub fn set_place(n: Option<usize>) {
        PLACE.with(|p| p.set(n));
    }
}

/// This window's place let go (natively its lock closed) as its window
/// closes (`viewer`'s `exiting`), whatever the program does after: the
/// updater's build or the game started again takes the same place though
/// this program has not ended yet, and one that never ends (its teardown
/// stuck in a driver) holds no place and no controller from the windows
/// open after it (a window stopped while it held its place kept the next
/// window from its controller: the next took the next place and the spare
/// controller, none where there was none). Its number stays this
/// program's (`place`), for the files it writes as it ends.
#[cfg(not(target_arch = "wasm32"))]
pub fn leave() {
    here().leave();
}

/// This window and the others open now, as last looked at.
pub fn seat() -> Seat {
    let (seat, looked) = {
        let mut h = here();
        match h.place().map(|p| p.n) {
            None => (Seat::default(), false),
            Some(n) => {
                let now = web_time::Instant::now();
                let look = h.looked.is_none_or(|t| now.duration_since(t).as_millis() >= LOOK_MS);
                if look {
                    h.others = h.place().map(Place::others).unwrap_or_default();
                    h.looked = Some(now);
                }
                (Seat { place: n, others: h.others.clone() }, look)
            }
        }
    };
    // Capture Controller, one setting for every window: another window's
    // save of it reaches this one here, outside the borrow (the settings ask
    // this window's place, `settings::client`)
    if looked {
        crate::settings::look_shared();
    }
    seat
}

/// Says which controller this window reads (none: `None`), its Controller
/// Focus and since when it has had the focus (`front`, none behind), for
/// the others to see: when it changes, and again each frame until it is said
/// (the first failure is logged).
pub fn say(pad: Option<&str>, focus: Focus, front: Option<u64>) {
    let mut h = here();
    let now = (pad.map(str::to_string), focus, front);
    if h.said.as_ref() == Some(&now) {
        return;
    }
    let Some(p) = h.place() else { return };
    match p.say(pad, focus, front) {
        Ok(()) => {
            h.said = Some(now);
            h.failed = false;
        }
        Err(e) if !h.failed => {
            crate::say(&format!("multibox: {e}"));
            h.failed = true;
        }
        Err(_) => {}
    }
}

/// A `.pad` file's text, and back: the controller's name on the first line
/// (empty when it reads none), its Controller Focus as `client.opt` spells it
/// on the second, and on the third since when the window has had the focus
/// (ms since 1970; empty behind, and absent from an older build's).
fn pad_text(pad: Option<&str>, focus: Focus, front: Option<u64>) -> String {
    format!("{}\n{}\n{}\n", pad.unwrap_or(""), focus.key(), front.map_or(String::new(), |t| t.to_string()))
}

fn read_pad_text(place: usize, s: &str) -> Other {
    let mut lines = s.lines();
    let pad = lines.next().map(str::trim).filter(|p| !p.is_empty()).map(str::to_string);
    let focus = lines.next().and_then(Focus::from_key).unwrap_or_default();
    let front = lines.next().and_then(|t| t.trim().parse().ok());
    Other { place, pad, focus, front }
}

// ---- natively: lock files in the config directory -------------------------------

/// A place held: its lock file open and locked.
#[cfg(not(target_arch = "wasm32"))]
pub struct Place {
    dir: std::path::PathBuf,
    n: usize,
    _lock: std::fs::File,
}

#[cfg(not(target_arch = "wasm32"))]
impl Place {
    /// The first free place in `dir`, held until it is dropped.
    pub fn take(dir: &std::path::Path) -> std::io::Result<Option<Place>> {
        use std::fs::{OpenOptions, TryLockError};
        std::fs::create_dir_all(dir)?;
        for n in 0..PLACES {
            let f = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(Self::lock_path(dir, n))?;
            // held a moment by a window looking, or for good by one open
            for wait in [true, false] {
                match f.try_lock() {
                    Ok(()) => {
                        let place = Place { dir: dir.to_path_buf(), n, _lock: f };
                        // what the place's last window said is not this one's
                        place.say(None, Focus::Auto, None).map_err(std::io::Error::other)?;
                        return Ok(Some(place));
                    }
                    Err(TryLockError::WouldBlock) if wait => std::thread::sleep(std::time::Duration::from_millis(30)),
                    Err(TryLockError::WouldBlock) => {}
                    Err(TryLockError::Error(e)) => return Err(e),
                }
            }
        }
        Ok(None)
    }

    fn lock_path(dir: &std::path::Path, n: usize) -> std::path::PathBuf {
        dir.join(format!("{}.lock", n + 1))
    }

    fn pad_path(dir: &std::path::Path, n: usize) -> std::path::PathBuf {
        dir.join(format!("{}.pad", n + 1))
    }

    /// The other windows open: each place held, with what it says it reads.
    pub fn others(&self) -> Vec<Other> {
        use std::fs::TryLockError;
        (0..PLACES)
            .filter(|&n| n != self.n)
            .filter_map(|n| {
                let f = std::fs::File::open(Self::lock_path(&self.dir, n)).ok()?;
                match f.try_lock_shared() {
                    // free: let go as `f` closes
                    Ok(()) => None,
                    Err(TryLockError::WouldBlock) => {
                        let text = std::fs::read_to_string(Self::pad_path(&self.dir, n)).unwrap_or_default();
                        Some(read_pad_text(n, &text))
                    }
                    Err(TryLockError::Error(_)) => None,
                }
            })
            .collect()
    }

    /// Says what this window reads: written beside and moved over, so a
    /// window looking never reads half of it.
    pub fn say(&self, pad: Option<&str>, focus: Focus, front: Option<u64>) -> Result<(), String> {
        let path = Self::pad_path(&self.dir, self.n);
        let tmp = path.with_extension("pad.new");
        std::fs::write(&tmp, pad_text(pad, focus, front)).and_then(|()| std::fs::rename(&tmp, &path)).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// This window's place: the first free in the config directory's
/// `windows`; none (alone) with no config directory.
#[cfg(not(target_arch = "wasm32"))]
fn take_place() -> Option<Option<Place>> {
    let Some(dir) = crate::settings::config_dir() else { return Some(None) };
    let dir = dir.join("windows");
    Some(match Place::take(&dir) {
        Ok(Some(p)) => {
            crate::say(&format!("multibox: this is window {}", p.n + 1));
            Some(p)
        }
        Ok(None) => {
            crate::say(&format!("multibox: all {PLACES} windows' places are taken: this one reads as if alone"));
            None
        }
        Err(e) => {
            crate::say(&format!("multibox: no place in {}: {e}; this window reads as if alone", dir.display()));
            None
        }
    })
}

// ---- in a page: Web Locks (main.js) and localStorage ------------------------------

/// A place held: main.js's Web Lock.
#[cfg(target_arch = "wasm32")]
pub struct Place {
    n: usize,
}

#[cfg(target_arch = "wasm32")]
impl Place {
    fn key(n: usize) -> String {
        format!("eqoa.window.{}.pad", n + 1)
    }

    fn others(&self) -> Vec<Other> {
        let store = web_sys::window().and_then(|w| w.local_storage().ok().flatten());
        crate::web::window_places()
            .into_iter()
            .filter(|&n| n != self.n && n < PLACES)
            .map(|n| {
                let text = store.as_ref().and_then(|s| s.get_item(&Self::key(n)).ok().flatten()).unwrap_or_default();
                read_pad_text(n, &text)
            })
            .collect()
    }

    fn say(&self, pad: Option<&str>, focus: Focus, front: Option<u64>) -> Result<(), String> {
        let s = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).ok_or("no localStorage")?;
        s.set_item(&Self::key(self.n), &pad_text(pad, focus, front)).map_err(|e| format!("{e:?}"))
    }
}

/// This window's place once main.js has one (unknown until then: asked
/// again); none for good where the browser has no Web Locks or every place
/// is taken.
#[cfg(target_arch = "wasm32")]
fn take_place() -> Option<Option<Place>> {
    match crate::web::window_place()? {
        Some(n) if n < PLACES => {
            crate::say(&format!("multibox: this is window {}", n + 1));
            let place = Place { n };
            // what the place's last page said is not this one's
            let _ = place.say(None, Focus::Auto, None);
            Some(Some(place))
        }
        _ => Some(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of the test's own (removed when it is dropped).
    #[cfg(not(target_arch = "wasm32"))]
    struct Dir(std::path::PathBuf);

    #[cfg(not(target_arch = "wasm32"))]
    impl Dir {
        fn new(name: &str) -> Dir {
            let d = std::env::temp_dir().join(format!("eqoa-multibox-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            Dir(d)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Windows take the first free place in turn, a closed window's place is
    /// the next one's, and each sees the others open with what they read
    /// (written as it changes, and none once it reads none). The control: a
    /// place whose lock was dropped is not an open window, whatever its
    /// `.pad` file still says.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn windows_take_places_in_turn_and_see_each_other() {
        let d = Dir::new("places");
        let a = Place::take(&d.0).unwrap().unwrap();
        let b = Place::take(&d.0).unwrap().unwrap();
        let c = Place::take(&d.0).unwrap().unwrap();
        assert_eq!((a.n, b.n, c.n), (0, 1, 2), "first come, first place");
        assert_eq!(a.others().iter().map(|o| o.place).collect::<Vec<_>>(), vec![1, 2]);
        b.say(Some("/dev/input/event19"), Focus::Auto, None).unwrap();
        c.say(Some("xinput1"), Focus::Pad(2), Some(1_792_970_000_123)).unwrap();
        let seen = a.others();
        assert_eq!(seen[0], Other { place: 1, pad: Some("/dev/input/event19".into()), focus: Focus::Auto, front: None });
        assert_eq!(seen[1], Other { place: 2, pad: Some("xinput1".into()), focus: Focus::Pad(2), front: Some(1_792_970_000_123) });
        // the second window closes: its place is free, and the next taken
        drop(b);
        assert_eq!(a.others().iter().map(|o| o.place).collect::<Vec<_>>(), vec![2], "the control: a dropped lock is no window");
        let d2 = Place::take(&d.0).unwrap().unwrap();
        assert_eq!(d2.n, 1, "the place the closed window left");
        assert_eq!(a.others()[0], Other { place: 1, pad: None, focus: Focus::Auto, front: None }, "a new window says it reads nothing yet, whatever was there");
        c.say(None, Focus::Off, None).unwrap();
        assert_eq!(a.others()[1], Other { place: 2, pad: None, focus: Focus::Off, front: None }, "behind again: no time");
        // looking takes nothing: the places are as they were
        assert_eq!(d2.others().iter().map(|o| o.place).collect::<Vec<_>>(), vec![0, 2]);
    }

    /// One place for the program, whichever thread asks: two threads asking
    /// one `Here` (as they ask `HERE`) get the same place, and another window
    /// the next. The control: a `Here` each, as each thread had while the
    /// place was a thread's, takes a second place for the same program (the
    /// world's reading thread was one window more: F3 20261024-204603's
    /// "this is window 3").
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn a_program_has_one_place_whichever_thread_asks() {
        let d = Dir::new("threads");
        let take = || Some(Place::take(&d.0).unwrap());
        let shared = Mutex::new(Here::new());
        let a = shared.lock().unwrap().place_by(take).map(|p| p.n);
        let b = std::thread::scope(|s| s.spawn(|| shared.lock().unwrap().place_by(take).map(|p| p.n)).join().unwrap());
        assert_eq!((a, b), (Some(0), Some(0)), "one place, whichever thread");
        let other = Place::take(&d.0).unwrap().unwrap();
        assert_eq!(other.n, 1, "another window: the next place");
        assert_eq!(shared.lock().unwrap().place().map(|p| p.others().len()), Some(1), "and one other window seen");
        // the control: a Here of its own, as a thread's was
        let mut own = Here::new();
        assert_eq!(own.place_by(take).map(|p| p.n), Some(2), "a second place for the same program");
    }

    /// A window let go of (`leave`): its place is the next window's at once,
    /// and its number stays its own. The control: a window that has not let
    /// go still holds its place.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn a_window_that_closes_lets_go_of_its_place_at_once() {
        let d = Dir::new("leave");
        let first = Place::take(&d.0).unwrap().unwrap();
        let mut closing = Here::new();
        assert_eq!(closing.place_by(|| Some(Place::take(&d.0).unwrap())).map(|p| p.n), Some(1));
        let third = Place::take(&d.0).unwrap().unwrap();
        assert_eq!(third.n, 2, "the control: held, the next window passes it by");
        closing.leave();
        assert_eq!(closing.left, Some(1), "its number kept");
        assert!(closing.place().is_none(), "held no more");
        let next = Place::take(&d.0).unwrap().unwrap();
        assert_eq!(next.n, 1, "the next window takes it");
        assert_eq!(first.others().iter().map(|o| o.place).collect::<Vec<_>>(), vec![1, 2], "window 1 sees the new window 2 and window 3");
        closing.leave();
        assert_eq!(closing.left, Some(1), "let go twice: the first number");
    }

    /// Every place taken: none for the next window (it reads as if alone).
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn past_the_last_place_a_window_has_none() {
        let d = Dir::new("full");
        let held: Vec<Place> = (0..PLACES).map(|_| Place::take(&d.0).unwrap().unwrap()).collect();
        assert_eq!(held.last().unwrap().n, PLACES - 1);
        assert!(Place::take(&d.0).unwrap().is_none());
    }

    #[test]
    fn a_place_says_what_it_reads() {
        let s = Seat { place: 2, others: vec![Other { place: 0, ..Default::default() }] };
        assert!(!s.alone());
        assert!(Seat::default().alone());
        assert_eq!(read_pad_text(3, &pad_text(Some("pad1"), Focus::Pad(1), None)), Other { place: 3, pad: Some("pad1".into()), focus: Focus::Pad(1), front: None });
        assert_eq!(read_pad_text(3, &pad_text(None, Focus::Window, Some(42))), Other { place: 3, pad: None, focus: Focus::Window, front: Some(42) });
        assert_eq!(read_pad_text(3, ""), Other { place: 3, pad: None, focus: Focus::Auto, front: None });
        // an older build's file: two lines, no time (it never says it is in
        // front, as before)
        assert_eq!(read_pad_text(3, "xinput0\nwindow\n"), Other { place: 3, pad: Some("xinput0".into()), focus: Focus::Window, front: None });
    }

    /// Of the windows that say they have the focus, the one that came to the
    /// front last is in front: one that came before it is not, though it says
    /// it has the focus still (a focus lost without a word), and a window
    /// with the focus and nothing newer is. Ties go to the later place. The
    /// control: with no time said (behind, or an older build), no other
    /// window is in front.
    #[test]
    fn the_window_that_came_to_the_front_last_is_in_front() {
        let other = |place, front| Other { place, front, ..Default::default() };
        let s = Seat { place: 0, others: vec![other(1, Some(2_000)), other(2, Some(1_500))] };
        assert_eq!(s.front_other(Some(1_000)).map(|o| o.place), Some(1), "two came after this one: the last of them");
        assert_eq!(s.front_other(None).map(|o| o.place), Some(1), "behind: whichever came last");
        assert_eq!(s.front_other(Some(3_000)), None, "this one came last: in front");
        let tie = Seat { place: 1, others: vec![other(0, Some(2_000))] };
        assert_eq!(tie.front_other(Some(2_000)), None, "a tie: the later place (this one) is in front");
        let tie = Seat { place: 0, others: vec![other(1, Some(2_000))] };
        assert_eq!(tie.front_other(Some(2_000)).map(|o| o.place), Some(1), "a tie: the later place is in front");
        let quiet = Seat { place: 0, others: vec![other(1, None), other(2, None)] };
        assert_eq!(quiet.front_other(Some(1_000)), None, "the control: none says it has the focus");
        assert_eq!(quiet.front_other(None), None);
    }
}
