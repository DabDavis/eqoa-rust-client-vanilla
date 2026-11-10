//! `EQOA_BODIES_TRACE=1` (in a page `?bodiestrace`): how a server's races'
//! bodies reach character select and the create pages, a line each through
//! `crate::say`, for measuring the wait and the hitch (`res::Res::want_bodies`):
//! the character list's arrival, each ask of the disc, each read and what was
//! made of it, each figure built (its model, the body it was built of, how
//! long the frame's thread took), and every frame of the pregame that took
//! long (its time on the frame's thread, and since the frame before), with
//! the worst of each second. A line's first number is the milliseconds since
//! the trace's first line, its second the pregame's frame. Off, nothing is
//! measured or said.

use std::cell::RefCell;
use web_time::Instant;

/// A frame whose own work took this long, or that came this long after the
/// one before it, is said (ms).
const SLOW_WORK: f64 = 8.0;
const SLOW_GAP: f64 = 50.0;

struct Trace {
    start: Instant,
    frame: u64,
    /// this frame's start, and the one before's
    began: Option<Instant>,
    /// the second being summed: its start, its frames, its worst work and gap
    second: Instant,
    frames: u32,
    worst: (f64, f64),
    /// frames are summed until this long after the last line (a quiet screen
    /// says nothing)
    until: Instant,
}

thread_local! {
    static TRACE: RefCell<Option<Trace>> = const { RefCell::new(None) };
    static ON: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    /// the character list last said (`characters`)
    static LIST: RefCell<Vec<(String, u32)>> = const { RefCell::new(Vec::new()) };
}

/// Whether the trace is on.
pub fn on() -> bool {
    ON.with(|c| match c.get() {
        Some(on) => on,
        None => {
            #[cfg(target_arch = "wasm32")]
            let on = crate::web::page_flag("bodiestrace");
            #[cfg(not(target_arch = "wasm32"))]
            let on = std::env::var_os("EQOA_BODIES_TRACE").is_some();
            c.set(Some(on));
            on
        }
    })
}

/// A page's heap now, for a line (natively nothing).
fn heap() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let bytes = wasm_bindgen::memory().dyn_into::<js_sys::WebAssembly::Memory>().map_or(0.0, |m| m.buffer().dyn_into::<js_sys::ArrayBuffer>().map_or(0.0, |b| b.byte_length() as f64));
        format!(", heap {:.0} MB", bytes / 1048576.0)
    }
    #[cfg(not(target_arch = "wasm32"))]
    String::new()
}

fn with<R>(f: impl FnOnce(&mut Trace) -> R) -> R {
    TRACE.with(|t| {
        let mut t = t.borrow_mut();
        let now = Instant::now();
        f(t.get_or_insert_with(|| Trace { start: now, frame: 0, began: None, second: now, frames: 0, worst: (0.0, 0.0), until: now }))
    })
}

/// A line of the trace (`what` is made only when it is on).
pub fn note(what: impl FnOnce() -> String) {
    if !on() {
        return;
    }
    let (ms, frame) = with(|t| {
        t.until = Instant::now() + std::time::Duration::from_secs(4);
        (t.start.elapsed().as_secs_f64() * 1e3, t.frame)
    });
    crate::say(&format!("[bodies {ms:9.1} ms f{frame}] {}{}", what(), heap()));
}

/// A stopwatch for a line: the milliseconds since it was started.
pub fn watch() -> impl Fn() -> f64 {
    let t = Instant::now();
    move || t.elapsed().as_secs_f64() * 1e3
}

/// The pregame's frame begins: the time since the one before.
pub fn frame_begin() {
    if !on() {
        return;
    }
    with(|t| {
        let now = Instant::now();
        t.frame += 1;
        if let Some(b) = t.began {
            let gap = (now - b).as_secs_f64() * 1e3;
            t.worst.1 = t.worst.1.max(gap);
            if gap >= SLOW_GAP && now < t.until {
                crate::say(&format!("[bodies {:9.1} ms f{}] frame came {gap:.1} ms after the one before", t.start.elapsed().as_secs_f64() * 1e3, t.frame));
            }
        }
        t.began = Some(now);
    });
}

/// The pregame's frame has done its work (what it draws is the window's to
/// put up after): the time it took, said when long, and each second's worst.
pub fn frame_end() {
    if !on() {
        return;
    }
    with(|t| {
        let now = Instant::now();
        let Some(b) = t.began else { return };
        let work = (now - b).as_secs_f64() * 1e3;
        t.worst.0 = t.worst.0.max(work);
        t.frames += 1;
        let at = t.start.elapsed().as_secs_f64() * 1e3;
        if work >= SLOW_WORK && now < t.until {
            crate::say(&format!("[bodies {at:9.1} ms f{}] frame's own work took {work:.1} ms", t.frame));
        }
        if (now - t.second).as_secs_f64() >= 1.0 {
            if t.second < t.until {
                crate::say(&format!("[bodies {at:9.1} ms f{}] the last second: {} frames, the longest work {:.1} ms, the longest gap {:.1} ms{}", t.frame, t.frames, t.worst.0, t.worst.1, heap()));
            }
            t.second = now;
            t.frames = 0;
            t.worst = (0.0, 0.0);
        }
    });
}

/// The character list as character select has it now (each name and model):
/// said when it changes, which is when the server's list arrives.
pub fn characters(list: &[(&str, u32)]) {
    if !on() {
        return;
    }
    let changed = LIST.with(|l| {
        let mut l = l.borrow_mut();
        let same = l.len() == list.len() && l.iter().zip(list).all(|(a, b)| a.0 == b.0 && a.1 == b.1);
        if !same {
            *l = list.iter().map(|&(n, m)| (n.to_string(), m)).collect();
        }
        !same
    });
    if changed {
        note(|| format!("the character list: {}", list.iter().map(|(n, m)| format!("{n} {m:08X}")).collect::<Vec<_>>().join(", ")));
    }
}
