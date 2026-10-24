//! Each game window's place on the desktop, kept between runs (F3
//! 20261024-063145, Liav: "Window minimized/maximized/resolution/current
//! monitor persists between sessions, ideally saved on per-client basis";
//! the user chose it per window). The client's addition, natively only: the
//! window's size, where it stood, on which monitor, and whether it was
//! maximized or full screen (F11), in the config directory's `window.opt`
//! (window n's `window-<n>.opt`, as `settings` names each window's own
//! options), written as it changes and put back as the window opens.
//!
//! A place no monitor shows any more is dropped and the system places the
//! window, on the monitor it chooses; a size larger than the monitor is
//! made to fit. A minimized window keeps the place it had before (it opens
//! again as it was, not minimized: a game that starts minimized looks as if
//! it never started), and a maximized or full-screen one keeps its normal
//! size and place beneath, for when it is restored. Wayland lets no program
//! know or choose where its window is: there only the size and the states
//! are kept. `EQOA_OFFSCREEN` runs keep and take nothing.

use std::time::{Duration, Instant};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Fullscreen, Window, WindowAttributes};

/// What is kept of a window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Place {
    /// the outer top left, in physical pixels on the whole desktop
    pub pos: Option<[i32; 2]>,
    /// the inner (drawn) size in physical pixels
    pub size: Option<[u32; 2]>,
    pub maximized: bool,
    pub fullscreen: bool,
    /// the monitor it was on, by the name the system gives it
    pub monitor: Option<String>,
}

impl Place {
    /// `key=value` lines; anything else is passed over.
    pub fn parse(text: &str) -> Place {
        let mut p = Place::default();
        let (mut x, mut y, mut w, mut h) = (None, None, None, None);
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "x" => x = v.parse::<i32>().ok(),
                "y" => y = v.parse::<i32>().ok(),
                "width" => w = v.parse::<u32>().ok().filter(|&n| n > 0),
                "height" => h = v.parse::<u32>().ok().filter(|&n| n > 0),
                "maximized" => p.maximized = v == "1",
                "fullscreen" => p.fullscreen = v == "1",
                "monitor" if !v.is_empty() => p.monitor = Some(v.to_string()),
                _ => {}
            }
        }
        p.pos = x.zip(y).map(|(x, y)| [x, y]);
        p.size = w.zip(h).map(|(w, h)| [w, h]);
        p
    }

    pub fn to_text(&self) -> String {
        let mut s = String::new();
        if let Some([x, y]) = self.pos {
            s.push_str(&format!("x={x}\ny={y}\n"));
        }
        if let Some([w, h]) = self.size {
            s.push_str(&format!("width={w}\nheight={h}\n"));
        }
        s.push_str(&format!("maximized={}\nfullscreen={}\n", self.maximized as u8, self.fullscreen as u8));
        if let Some(m) = &self.monitor {
            s.push_str(&format!("monitor={}\n", m.replace(['\n', '\r'], " ")));
        }
        s
    }
}

/// A monitor present now: its name and where it is on the desktop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    pub name: Option<String>,
    pub pos: [i32; 2],
    pub size: [u32; 2],
}

/// A kept place as this desktop can take it (`fit`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fitted {
    pub pos: Option<[i32; 2]>,
    pub size: Option<[u32; 2]>,
    /// the screen it goes on (index into the list `fit` was given)
    pub screen: Option<usize>,
    pub maximized: bool,
    pub fullscreen: bool,
}

/// How much of a window's top edge must show on some monitor for its place
/// to be kept: enough to take it by its title bar.
const SHOWN: [i64; 2] = [96, 24];
/// The least size put back.
const LEAST: [u32; 2] = [320, 240];

/// `p` on `screens`: its monitor by name (else the one its top edge is on);
/// its place only if that much of its top edge shows on some monitor; its
/// size no larger than its monitor (else the largest).
pub fn fit(p: &Place, screens: &[Screen]) -> Fitted {
    let top_on = |pos: [i32; 2], w: u32, s: &Screen| {
        let (l, r) = (pos[0] as i64, pos[0] as i64 + w.max(1) as i64);
        let (t, b) = (pos[1] as i64, pos[1] as i64 + SHOWN[1]);
        let (sl, st) = (s.pos[0] as i64, s.pos[1] as i64);
        let (sr, sb) = (sl + s.size[0] as i64, st + s.size[1] as i64);
        r.min(sr) - l.max(sl) >= SHOWN[0] && b.min(sb) - t.max(st) >= SHOWN[1]
    };
    let width = p.size.map_or(LEAST[0], |s| s[0]);
    let named = p.monitor.as_ref().and_then(|n| {
        let mut it = screens.iter().enumerate().filter(|(_, s)| s.name.as_ref() == Some(n));
        let first = it.next();
        // a name two monitors share says nothing
        if it.next().is_some() { None } else { first.map(|(i, _)| i) }
    });
    let pos = p.pos.filter(|&pos| screens.iter().any(|s| top_on(pos, width, s)));
    let screen = named.or_else(|| pos.and_then(|pos| screens.iter().position(|s| top_on(pos, width, s))));
    let room = screen.map(|i| screens[i].size).or_else(|| screens.iter().map(|s| s.size).max_by_key(|s| s[0] as u64 * s[1] as u64));
    let size = p.size.map(|[w, h]| {
        let [rw, rh] = room.unwrap_or([u32::MAX; 2]);
        [w.min(rw).max(LEAST[0].min(rw)), h.min(rh).max(LEAST[1].min(rh))]
    });
    Fitted { pos, size, screen, maximized: p.maximized, fullscreen: p.fullscreen }
}

/// A window as it is now, for `Keeper::note`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub minimized: bool,
    pub maximized: bool,
    pub fullscreen: bool,
    pub pos: Option<[i32; 2]>,
    pub size: [u32; 2],
    pub monitor: Option<String>,
}

impl State {
    fn of(w: &Window) -> State {
        State {
            minimized: w.is_minimized() == Some(true),
            maximized: w.is_maximized(),
            fullscreen: w.fullscreen().is_some(),
            pos: w.outer_position().ok().map(|p| [p.x, p.y]),
            size: [w.inner_size().width, w.inner_size().height],
            monitor: w.current_monitor().and_then(|m| m.name()),
        }
    }
}

/// Windows moves a minimized window to (-32000, -32000).
const MINIMIZED_AT: i32 = -30000;

/// The place kept, as the window changes, and when it was last written.
pub struct Keeper {
    /// `EQOA_OFFSCREEN`: no window of the player's to keep
    off: bool,
    place: Place,
    dirty: bool,
    written: Option<Instant>,
}

impl Default for Keeper {
    fn default() -> Keeper {
        let off = cfg!(target_arch = "wasm32") || std::env::var_os("EQOA_OFFSCREEN").is_some();
        let place = if off { Place::default() } else { read().map(|t| Place::parse(&t)).unwrap_or_default() };
        Keeper { off, place, dirty: false, written: None }
    }
}

/// Written at most this often while the window moves.
const EVERY: Duration = Duration::from_millis(500);

impl Keeper {
    /// The window's attributes with its kept place, for the monitors there
    /// are now.
    pub fn attributes(&self, el: &ActiveEventLoop, mut a: WindowAttributes) -> WindowAttributes {
        if self.off || self.place == Place::default() {
            return a;
        }
        let monitors: Vec<_> = el.available_monitors().collect();
        let screens: Vec<Screen> = monitors.iter().map(|m| Screen { name: m.name(), pos: [m.position().x, m.position().y], size: [m.size().width, m.size().height] }).collect();
        let f = fit(&self.place, &screens);
        let on = |i: usize| screens[i].name.clone().unwrap_or_else(|| format!("monitor {}", i + 1));
        eprintln!(
            "window: as kept ({}{}{}{}), on {} of {} monitor(s)",
            f.pos.map_or("placed by the system".to_string(), |[x, y]| format!("at {x},{y}")),
            f.size.map_or(String::new(), |[w, h]| format!(", {w}x{h}")),
            if f.maximized { ", maximized" } else { "" },
            if f.fullscreen { ", full screen" } else { "" },
            f.screen.map_or("the system's choice".to_string(), on),
            screens.len()
        );
        if let Some([x, y]) = f.pos {
            a = a.with_position(PhysicalPosition::new(x, y));
        }
        if let Some([w, h]) = f.size {
            a = a.with_inner_size(PhysicalSize::new(w, h));
        }
        if f.maximized {
            a = a.with_maximized(true);
        }
        if f.fullscreen {
            a = a.with_fullscreen(Some(Fullscreen::Borderless(f.screen.map(|i| monitors[i].clone()))));
        }
        a
    }

    /// What the window is now, from an event that can change it.
    pub fn event(&mut self, w: &Window, e: &WindowEvent) {
        match e {
            WindowEvent::Moved(_) | WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => self.note(&State::of(w)),
            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                self.note(&State::of(w));
                self.flush(true);
            }
            _ => {}
        }
    }

    /// The window as `s` has it: its states, and its normal size and place
    /// while it is neither minimized, maximized nor full screen.
    pub fn note(&mut self, s: &State) {
        if self.off || s.minimized || s.pos.is_some_and(|p| p[0] <= MINIMIZED_AT && p[1] <= MINIMIZED_AT) {
            return;
        }
        let mut p = self.place.clone();
        p.maximized = s.maximized;
        p.fullscreen = s.fullscreen;
        if s.monitor.is_some() {
            p.monitor = s.monitor.clone();
        }
        if !s.maximized && !s.fullscreen {
            if s.size[0] > 0 && s.size[1] > 0 {
                p.size = Some(s.size);
            }
            // Wayland answers no place: the one kept stays
            if s.pos.is_some() {
                p.pos = s.pos;
            }
        }
        if p != self.place {
            self.place = p;
            self.dirty = true;
        }
    }

    /// The place written when it changed: at once with `now`, else at most
    /// every `EVERY` (`about_to_wait` asks each pass).
    pub fn flush(&mut self, now: bool) {
        if !self.dirty || (!now && self.written.is_some_and(|t| t.elapsed() < EVERY)) {
            return;
        }
        self.dirty = false;
        self.written = Some(Instant::now());
        if let Err(e) = write(&self.place.to_text()) {
            eprintln!("window: its place could not be kept: {e}");
        }
    }
}

/// `window.opt`, or window n's `window-<n>.opt` (`multibox::place`).
fn file() -> String {
    match crate::multibox::place() {
        0 => "window.opt".to_string(),
        n => format!("window-{}.opt", n + 1),
    }
}

#[cfg(all(not(test), not(target_arch = "wasm32")))]
fn read() -> Option<String> {
    std::fs::read_to_string(crate::settings::config_dir()?.join(file())).ok()
}

/// Written whole beside the file, then moved over it.
#[cfg(all(not(test), not(target_arch = "wasm32")))]
fn write(text: &str) -> Result<(), String> {
    let Some(d) = crate::settings::config_dir() else { return Ok(()) };
    let name = file();
    let go = || -> std::io::Result<()> {
        std::fs::create_dir_all(&d)?;
        let tmp = d.join(format!("{name}.new"));
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, d.join(&name))
    };
    go().map_err(|e| format!("{}: {e}", d.join(&name).display()))
}

/// The tests' and a page's: nothing kept.
#[cfg(any(test, target_arch = "wasm32"))]
fn read() -> Option<String> {
    let _ = file;
    None
}

#[cfg(any(test, target_arch = "wasm32"))]
fn write(_: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(name: &str, pos: [i32; 2], size: [u32; 2]) -> Screen {
        Screen { name: Some(name.to_string()), pos, size }
    }

    fn keeper() -> Keeper {
        Keeper { off: false, place: Place::default(), dirty: false, written: None }
    }

    /// The file's lines and back; a line not its own is passed over. The
    /// control: a size of 0 is none.
    #[test]
    fn a_place_reads_back_as_written() {
        let p = Place { pos: Some([-1920, 40]), size: Some([1280, 720]), maximized: true, fullscreen: false, monitor: Some("DP-2".into()) };
        assert_eq!(Place::parse(&p.to_text()), p);
        assert_eq!(Place::parse("junk\nx=5\ny=6\nwidth=0\nheight=10\n"), Place { pos: Some([5, 6]), ..Place::default() });
    }

    /// Two monitors side by side: a place on the left one is kept on it,
    /// found by its name; a monitor gone with the place off every screen
    /// drops the place (the system places the window) and fits the size
    /// to what there is.
    #[test]
    fn a_place_is_kept_only_where_a_monitor_shows_it() {
        let two = [screen("DP-1", [0, 0], [2560, 1440]), screen("DP-2", [-1920, 0], [1920, 1080])];
        let p = Place { pos: Some([-1800, 100]), size: Some([1600, 900]), monitor: Some("DP-2".into()), ..Place::default() };
        assert_eq!(fit(&p, &two), Fitted { pos: Some([-1800, 100]), size: Some([1600, 900]), screen: Some(1), ..Fitted::default() });
        let one = [screen("DP-1", [0, 0], [1280, 720])];
        assert_eq!(fit(&p, &one), Fitted { pos: None, size: Some([1280, 720]), screen: None, ..Fitted::default() });
        // the control: the same place on a desktop that has the monitor
        // still is not moved
        assert_eq!(fit(&p, &two).pos, p.pos);
    }

    /// Only the top edge has to show: a window hanging below its monitor
    /// keeps its place; one whose title bar is above every monitor does not.
    #[test]
    fn the_top_edge_decides() {
        let s = [screen("A", [0, 0], [1920, 1080])];
        let low = Place { pos: Some([100, 1000]), size: Some([800, 600]), ..Place::default() };
        assert_eq!(fit(&low, &s).pos, Some([100, 1000]));
        let high = Place { pos: Some([100, -200]), ..low.clone() };
        assert_eq!(fit(&high, &s).pos, None);
        let left = Place { pos: Some([-750, 10]), ..low.clone() };
        assert_eq!(fit(&left, &s).pos, None, "50 px of its top showing is too little");
    }

    /// Full screen goes back on its monitor by name; a name two monitors
    /// share picks neither, and the one under the place is taken instead.
    #[test]
    fn full_screen_goes_back_to_its_monitor() {
        let s = [screen("Same", [0, 0], [1920, 1080]), screen("Same", [1920, 0], [1920, 1080])];
        let p = Place { pos: Some([2000, 50]), size: Some([800, 600]), fullscreen: true, monitor: Some("Same".into()), ..Place::default() };
        let f = fit(&p, &s);
        assert_eq!((f.screen, f.fullscreen), (Some(1), true));
        let named = [screen("L", [0, 0], [1920, 1080]), screen("R", [1920, 0], [1920, 1080])];
        assert_eq!(fit(&Place { monitor: Some("L".into()), ..p.clone() }, &named).screen, Some(0), "by name over its place");
    }

    /// Minimized (Windows moves the window to -32000): nothing changes; a
    /// maximized window keeps its normal size and place beneath. The
    /// control: a normal window's move is kept.
    #[test]
    fn minimized_and_maximized_keep_the_normal_place() {
        let mut k = keeper();
        let normal = State { pos: Some([200, 100]), size: [1280, 720], monitor: Some("A".into()), ..State::default() };
        k.note(&normal);
        assert!(k.dirty);
        assert_eq!((k.place.pos, k.place.size), (Some([200, 100]), Some([1280, 720])));
        k.dirty = false;
        k.note(&State { minimized: true, pos: Some([-32000, -32000]), size: [0, 0], ..normal.clone() });
        k.note(&State { pos: Some([-32000, -32000]), size: [160, 28], ..normal.clone() });
        assert!(!k.dirty, "a minimized window changes nothing");
        k.note(&State { maximized: true, pos: Some([0, 0]), size: [2560, 1400], ..normal.clone() });
        assert!(k.place.maximized && k.dirty);
        assert_eq!((k.place.pos, k.place.size), (Some([200, 100]), Some([1280, 720])), "the normal place beneath");
        // Wayland: no place known, the size still kept
        k.note(&State { pos: None, size: [1000, 700], monitor: None, ..State::default() });
        assert_eq!((k.place.pos, k.place.size, k.place.maximized), (Some([200, 100]), Some([1000, 700]), false));
    }

    /// Off (`EQOA_OFFSCREEN`): nothing noted, nothing put back.
    #[test]
    fn an_offscreen_run_keeps_nothing() {
        let mut k = Keeper { off: true, ..keeper() };
        k.note(&State { pos: Some([1, 2]), size: [3, 4], ..State::default() });
        assert!(!k.dirty && k.place == Place::default());
    }
}
