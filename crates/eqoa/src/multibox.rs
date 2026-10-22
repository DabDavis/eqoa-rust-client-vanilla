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
//! The others are looked at twice a second. Natively a free place is taken
//! for a moment as it is looked at (a shared lock) and let go, so a window
//! opening just then waits a moment on a place it finds held before it
//! passes it by.

use crate::settings::Focus;
use std::cell::RefCell;

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
}

#[derive(Default)]
struct Here {
    /// the place taken: unasked, then one or none for good
    taken: Option<Option<Place>>,
    others: Vec<Other>,
    looked: Option<web_time::Instant>,
    /// what this window said it reads, and whether saying it last failed
    said: Option<(Option<String>, Focus)>,
    failed: bool,
}

thread_local! {
    static HERE: RefCell<Here> = RefCell::new(Here::default());
}

impl Here {
    /// The place, taken the first time it is asked for (in a page, once
    /// main.js has one).
    fn place(&mut self) -> Option<&Place> {
        if self.taken.is_none() {
            self.taken = take_place();
        }
        self.taken.as_ref()?.as_ref()
    }
}

/// This window's place, from 0 (0 alone, with nowhere to keep one): which
/// window's Controller Focus is its own (`settings::focus`).
pub fn place() -> usize {
    HERE.with(|h| h.borrow_mut().place().map_or(0, |p| p.n))
}

/// This window and the others open now, as last looked at.
pub fn seat() -> Seat {
    HERE.with(|h| {
        let mut h = h.borrow_mut();
        let Some(n) = h.place().map(|p| p.n) else { return Seat::default() };
        let now = web_time::Instant::now();
        if h.looked.is_none_or(|t| now.duration_since(t).as_millis() >= LOOK_MS) {
            h.others = h.place().map(Place::others).unwrap_or_default();
            h.looked = Some(now);
        }
        Seat { place: n, others: h.others.clone() }
    })
}

/// Says which controller this window reads (none: `None`) and its
/// Controller Focus, for the others to see: when it changes, and again each
/// frame until it is said (the first failure is logged).
pub fn say(pad: Option<&str>, focus: Focus) {
    HERE.with(|h| {
        let mut h = h.borrow_mut();
        let now = (pad.map(str::to_string), focus);
        if h.said.as_ref() == Some(&now) {
            return;
        }
        let Some(p) = h.place() else { return };
        match p.say(pad, focus) {
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
    })
}

/// A `.pad` file's text, and back: the controller's name on the first line
/// (empty when it reads none), its Controller Focus as `client.opt` spells it
/// on the second.
fn pad_text(pad: Option<&str>, focus: Focus) -> String {
    format!("{}\n{}\n", pad.unwrap_or(""), focus.key())
}

fn read_pad_text(place: usize, s: &str) -> Other {
    let mut lines = s.lines();
    let pad = lines.next().map(str::trim).filter(|p| !p.is_empty()).map(str::to_string);
    let focus = lines.next().and_then(Focus::from_key).unwrap_or_default();
    Other { place, pad, focus }
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
                        place.say(None, Focus::Auto).map_err(std::io::Error::other)?;
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
    pub fn say(&self, pad: Option<&str>, focus: Focus) -> Result<(), String> {
        let path = Self::pad_path(&self.dir, self.n);
        let tmp = path.with_extension("pad.new");
        std::fs::write(&tmp, pad_text(pad, focus)).and_then(|()| std::fs::rename(&tmp, &path)).map_err(|e| format!("{}: {e}", path.display()))
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

    fn say(&self, pad: Option<&str>, focus: Focus) -> Result<(), String> {
        let s = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).ok_or("no localStorage")?;
        s.set_item(&Self::key(self.n), &pad_text(pad, focus)).map_err(|e| format!("{e:?}"))
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
            let _ = place.say(None, Focus::Auto);
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
        b.say(Some("/dev/input/event19"), Focus::Auto).unwrap();
        c.say(Some("xinput1"), Focus::Pad(2)).unwrap();
        let seen = a.others();
        assert_eq!(seen[0], Other { place: 1, pad: Some("/dev/input/event19".into()), focus: Focus::Auto });
        assert_eq!(seen[1], Other { place: 2, pad: Some("xinput1".into()), focus: Focus::Pad(2) });
        // the second window closes: its place is free, and the next taken
        drop(b);
        assert_eq!(a.others().iter().map(|o| o.place).collect::<Vec<_>>(), vec![2], "the control: a dropped lock is no window");
        let d2 = Place::take(&d.0).unwrap().unwrap();
        assert_eq!(d2.n, 1, "the place the closed window left");
        assert_eq!(a.others()[0], Other { place: 1, pad: None, focus: Focus::Auto }, "a new window says it reads nothing yet, whatever was there");
        c.say(None, Focus::Off).unwrap();
        assert_eq!(a.others()[1], Other { place: 2, pad: None, focus: Focus::Off });
        // looking takes nothing: the places are as they were
        assert_eq!(d2.others().iter().map(|o| o.place).collect::<Vec<_>>(), vec![0, 2]);
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
        assert_eq!(read_pad_text(3, &pad_text(Some("pad1"), Focus::Pad(1))), Other { place: 3, pad: Some("pad1".into()), focus: Focus::Pad(1) });
        assert_eq!(read_pad_text(3, &pad_text(None, Focus::Window)), Other { place: 3, pad: None, focus: Focus::Window });
        assert_eq!(read_pad_text(3, ""), Other { place: 3, pad: None, focus: Focus::Auto });
    }
}
