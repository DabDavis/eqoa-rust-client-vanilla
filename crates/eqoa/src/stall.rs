//! Stalls: a frame drawn a second or more after the one before it. Each is
//! said in one line of the log (and so in a problem report, and in another
//! window's report too: `report::other_windows` carries its `stall:` lines),
//! with where the time went: in the frame, by its longest phases
//! (`viewer::Perf`'s, from stream and pads to gpu wait), or outside it, as the
//! client's own handling of the window's other events (the longest named) or
//! with no call from the event loop at all; how long the game's thread waited
//! meanwhile for the sound's driver (`audio::game_waits`: the output's
//! callback holds it while it mixes), the disc's slowest read in that time
//! (`disc`, one still under way said so), and the game server: how long
//! before the next frame its last datagram had come, and how many were
//! waiting for that frame to read (`net::Session::pump`): a stall of the
//! client's own leaves the server sending, a network gone quiet is older than
//! the stall. A line comes at most once in `SAY_EVERY`, the stalls between
//! counted into the next; a report's state carries the run's count and its
//! longest (`Watch::describe`).
//!
//! F3 20261029-025532 (Windows, two windows on Sandstorm, two DualSenses on
//! Bluetooth): "Game keeps freezing on both windows at the same time.
//! Sometimes briefly for like 5 second then sometimes for like a whole
//! minute". The report's own frames were on time; what showed the freezes was
//! the other window's 1006 s in the back at 54 frames a second (59-60 in its
//! other stretches: some 100 s with no frame), its last frame 2911 ms old as
//! it came to the front, the sound's callback once mixing for 8,363 ms with
//! the driver's lock held, and 94% of the guaranteed messages received as
//! duplicates (the server sending again what went unacknowledged); nothing
//! said where a frame's time had gone.

use std::time::Duration;
use web_time::Instant;

/// A frame this long or more after the one before it is a stall.
pub const STALL: Duration = Duration::from_secs(1);

/// A stall's line comes at most this often, and once `SAID_SOON` have come,
/// at most every `SAY_LATER` (a window held to a frame a second for good
/// would fill the report's last lines); the stalls between are counted into
/// the next.
const SAY_EVERY: Duration = Duration::from_secs(10);
const SAY_LATER: Duration = Duration::from_secs(60);
const SAID_SOON: u32 = 20;

/// A phase, a wait or a read this long is named in a stall's line.
const NAMED: Duration = Duration::from_millis(50);

/// What has been heard from the game server: the datagrams read so far,
/// and when the last was read.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Heard {
    pub count: u64,
    pub last: Option<Instant>,
}

/// A frame as the watch keeps it.
#[derive(Clone, Debug)]
struct Frame {
    start: Instant,
    end: Instant,
    /// its phases, by name, in seconds
    phases: Vec<(&'static str, f64)>,
    /// drawn under the loading screen (a world entered, its zones read at
    /// once): a stall there is a load's, said so and kept apart
    loading: bool,
}

/// A stall found as a frame begins, said once the frame has read what the
/// server sent meanwhile.
#[derive(Clone, Debug)]
struct Stall {
    /// from the frame before's start to this one's
    gap: Duration,
    /// the frame before: how long it took, its phases, and whether it was
    /// under the loading screen
    frame: Duration,
    phases: Vec<(&'static str, f64)>,
    loading: bool,
    /// from its end to this one's start, and the client's own handling in
    /// that: its time, its calls, the longest one
    outside: Duration,
    own: Duration,
    calls: u32,
    longest: Option<(Duration, &'static str)>,
    /// the game's thread's waits for the sound's driver in the gap
    waited: Duration,
    /// the disc's slowest read in the gap: what, how long, still under way
    disc: Option<(disc::Read, Duration, bool)>,
    /// what had been heard as this frame began, and how long before the
    /// last datagram was read
    heard: Option<Heard>,
    since_heard: Option<Duration>,
    /// when it ended: UTC, and on the run's clock (the `[memory]` lines')
    utc: String,
    run: String,
}

fn ms(d: Duration) -> u128 {
    d.as_millis()
}

impl Stall {
    /// The line's words after "stall: " (and the state's longest), with the
    /// datagrams that were waiting for this frame, when it read them.
    fn words(&self, waiting: Option<u64>) -> String {
        let mut s = format!("{} ms from one frame to the next, to {} UTC ({} into the run){}: ", ms(self.gap), self.utc, self.run, if self.loading { ", under the loading screen" } else { "" });
        // the frame's phases that took a part worth naming, longest first
        let mut phases: Vec<&(&'static str, f64)> = self.phases.iter().filter(|p| p.1 >= NAMED.as_secs_f64()).collect();
        phases.sort_by(|a, b| b.1.total_cmp(&a.1));
        let named: Vec<String> = phases.iter().take(3).map(|(n, t)| format!("{n} {:.0} ms", t * 1000.0)).collect();
        s += &format!("the frame itself took {} ms", ms(self.frame));
        if !named.is_empty() {
            s += &format!(" ({})", named.join(", "));
        }
        s += &format!(", then {} ms to the next", ms(self.outside));
        if self.own.is_zero() && self.calls == 0 {
            s += ", no call from the event loop";
        } else {
            let calls = match self.calls {
                0 => String::new(),
                1 => " in 1 call".to_string(),
                n => format!(" in {n} calls"),
            };
            let longest = self.longest.map_or(String::new(), |(d, what)| format!(" (the longest {what}, {} ms)", ms(d)));
            s += &format!(": {} ms the client's own{calls}{longest}, {} ms no call from the event loop", ms(self.own), ms(self.outside.saturating_sub(self.own)));
        }
        if self.waited >= NAMED {
            s += &format!("; the game's thread waited {} ms for the sound's driver", ms(self.waited));
        }
        if let Some((what, d, going)) = self.disc.filter(|d| d.1 >= NAMED) {
            s += &format!("; the disc's slowest read meanwhile {} ms ({}{})", ms(d), what.name(), if going { ", still under way" } else { "" });
        }
        match (self.since_heard, waiting) {
            (Some(since), Some(n)) => s += &format!("; the game server's last datagram had come {} ms before the stall's end, and {n} were waiting", ms(since)),
            (Some(since), None) => s += &format!("; the game server's last datagram had come {} ms before the stall's end", ms(since)),
            (None, _) if self.heard.is_some() => s += "; nothing heard from the game server yet",
            _ => {}
        }
        s
    }
}

/// The stalls of a window's frames in the world: the frames as they begin
/// and end (`begin`, `end`), and the window's calls from the event loop
/// (`enter`, `leave`), whose time between two frames is the client's own.
#[derive(Debug, Default)]
pub struct Watch {
    /// the last frame drawn
    last: Option<Frame>,
    /// the client's own handling since the last frame ended: its time, the
    /// calls between (the frames' own calls' parts before and after their
    /// frames count in its time, not as calls) and the longest one
    own: Duration,
    calls: u32,
    longest: Option<(Duration, &'static str)>,
    /// the call from the event loop under way: when it began, and whether
    /// a frame ended in it (when)
    call: Option<Instant>,
    ended_in_call: Option<Instant>,
    /// the game's thread's waits for the sound's driver, as of the last
    /// frame's start
    waits: Duration,
    /// the stall this frame found, said once it has read the datagrams
    pending: Option<Stall>,
    /// when the last line was said, the lines said, and the stalls since
    /// not said
    said: Option<Instant>,
    lines: u32,
    unsaid: u32,
    /// this run's stalls, and the longest (its length and its words), and
    /// those under the loading screen (counted apart)
    count: u32,
    worst: Option<(Duration, String)>,
    loading: u32,
}

impl Watch {
    /// A call from the event loop begins (a window event, the loop's turn).
    pub fn enter(&mut self, now: Instant) {
        self.call = Some(now);
    }

    /// The call ends: its time is the client's own handling between two
    /// frames (`what` names it), or, for the call that drew a frame, the part
    /// after the frame ended.
    pub fn leave(&mut self, what: &'static str, now: Instant) {
        let Some(t0) = self.call.take() else { return };
        match self.ended_in_call.take() {
            Some(end) => self.own_add(now.saturating_duration_since(end), "the frame's own call after it", false),
            None => self.own_add(now.saturating_duration_since(t0), what, true),
        }
    }

    fn own_add(&mut self, d: Duration, what: &'static str, whole: bool) {
        self.own += d;
        self.calls += whole as u32;
        if self.longest.is_none_or(|l| d > l.0) {
            self.longest = Some((d, what));
        }
    }

    /// Frames stop for a reason of their own (the pregame, a window hidden or
    /// minimised, nothing to draw on): the next frame is no gap from the last.
    /// (A frame begun and never ended is none either: `begin` takes the last
    /// and only `end` gives it back.)
    pub fn away(&mut self) {
        self.last = None;
        self.pending = None;
    }

    /// A frame in the world begins at `now`; `waits` the game's thread's
    /// waits for the sound's driver so far, `heard` the game server's
    /// datagrams so far. A stall since the last frame is kept for
    /// `after_pump` to say.
    pub fn begin(&mut self, now: Instant, waits: Duration, heard: Option<Heard>) {
        let waited = waits.saturating_sub(self.waits);
        self.waits = waits;
        // the call drawing this frame, before it began: the client's own
        if let Some(t0) = self.call {
            self.own_add(now.saturating_duration_since(t0), "the frame's own call before it", false);
        }
        self.pending = None;
        if let Some(f) = self.last.take() {
            let gap = now.saturating_duration_since(f.start);
            if gap >= STALL {
                self.pending = Some(Stall {
                    gap,
                    frame: f.end.saturating_duration_since(f.start),
                    phases: f.phases,
                    loading: f.loading,
                    outside: now.saturating_duration_since(f.end),
                    own: self.own,
                    calls: self.calls,
                    longest: self.longest,
                    waited,
                    disc: disc::during(f.start, now),
                    heard,
                    since_heard: heard.and_then(|h| h.last).map(|t| now.saturating_duration_since(t)),
                    utc: utc_now(),
                    run: crate::mem::clock_at(now),
                });
            }
        }
        self.own = Duration::ZERO;
        self.calls = 0;
        self.longest = None;
    }

    /// The frame begun at `start` ends at `end`, its phases as measured,
    /// under the loading screen or not.
    pub fn end(&mut self, start: Instant, end: Instant, phases: Vec<(&'static str, f64)>, loading: bool) {
        self.last = Some(Frame { start, end, phases, loading });
        self.own = Duration::ZERO;
        self.calls = 0;
        self.longest = None;
        if self.call.is_some() {
            self.ended_in_call = Some(end);
        }
    }

    /// After the frame has read the datagrams that came (`heard`, none
    /// without a game server): the stall it found, if any, as its line, when
    /// one is due (`SAY_EVERY`); counted for the run either way.
    pub fn after_pump(&mut self, heard: Option<Heard>, now: Instant) -> Option<String> {
        let s = self.pending.take()?;
        let waiting = heard.zip(s.heard).map(|(h, before)| h.count.saturating_sub(before.count));
        let words = s.words(waiting);
        if s.loading {
            self.loading += 1;
        } else {
            self.count += 1;
            if self.worst.as_ref().is_none_or(|w| s.gap > w.0) {
                self.worst = Some((s.gap, words.clone()));
            }
        }
        let every = if self.lines >= SAID_SOON { SAY_LATER } else { SAY_EVERY };
        if self.said.is_some_and(|t| now.saturating_duration_since(t) < every) {
            self.unsaid += 1;
            return None;
        }
        self.said = Some(now);
        self.lines += 1;
        let more = std::mem::take(&mut self.unsaid);
        Some(match more {
            0 => format!("stall: {words}"),
            n => format!("stall: {words} (and {n} more since the last line said, each 1 s or more)"),
        })
    }

    /// For a report's state: the run's stalls, and the longest; those under
    /// the loading screen counted apart.
    pub fn describe(&self) -> String {
        let loading = match self.loading {
            0 => String::new(),
            n => format!(" ({n} more under the loading screen)"),
        };
        match &self.worst {
            None => format!("stalls (a frame 1 s or more after the one before): none this run{loading}"),
            Some((_, words)) => format!("stalls (a frame 1 s or more after the one before): {} this run{loading}, the longest {words}", self.count),
        }
    }
}

/// The time of day in UTC, as a line says when it was ("02:41:07"): the
/// windows' one clock, which another window's lines are read against.
fn utc_now() -> String {
    let s = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    format!("{:02}:{:02}:{:02}", s / 3600 % 24, s / 60 % 60, s % 60)
}

/// A window event as a stall's line names the call that handled it.
pub fn event_name(e: &winit::event::WindowEvent) -> &'static str {
    use winit::event::WindowEvent as W;
    match e {
        W::RedrawRequested => "a redraw that drew nothing",
        W::KeyboardInput { .. } => "a key",
        W::CursorMoved { .. } => "a move of the pointer",
        W::MouseInput { .. } => "a mouse button",
        W::MouseWheel { .. } => "the wheel",
        W::Focused(_) => "a change of focus",
        W::Resized(_) => "a resize",
        W::Moved(_) => "a move of the window",
        W::Touch(_) => "a touch",
        W::ModifiersChanged(_) => "the modifier keys",
        W::Ime(_) => "text typed",
        W::CursorEntered { .. } | W::CursorLeft { .. } => "the pointer crossing the window's edge",
        W::Occluded(_) => "the window covered or shown",
        W::ScaleFactorChanged { .. } => "a change of scale",
        W::CloseRequested => "the window's close",
        _ => "another of the window's events",
    }
}

/// The disc's reads, timed: the music's files (natively read whole on a
/// thread of their own, `bgm::read_on_a_thread`), the zones the walk streams
/// in and the worlds a change opens (`stream`, on the game's thread). For a
/// stall's line (the slowest read in its time, `during`) and a report's
/// state (each kind's count and slowest, `describe`).
pub mod disc {
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::time::Duration;
    use web_time::Instant;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Read {
        Music,
        Zone,
        World,
    }

    impl Read {
        /// As a stall's line names one.
        pub fn name(self) -> &'static str {
            match self {
                Read::Music => "music",
                Read::Zone => "a zone",
                Read::World => "a world",
            }
        }

        /// As the state names their kind.
        fn kind(self) -> &'static str {
            match self {
                Read::Music => "music",
                Read::Zone => "zones",
                Read::World => "worlds",
            }
        }
    }

    const KINDS: [Read; 3] = [Read::Music, Read::Zone, Read::World];

    /// The last reads kept, for `during`.
    const RECENT: usize = 32;

    /// The reads so far.
    #[derive(Debug)]
    pub(crate) struct Reads {
        /// each kind's count, and its slowest (how long, and when it ended on
        /// the run's clock)
        kinds: [(u64, Option<(Duration, String)>); 3],
        /// the last reads: what, when each ended, and how long it took
        recent: VecDeque<(Read, Instant, Duration)>,
        /// reads under way: their number, what, and when each began
        going: Vec<(u64, Read, Instant)>,
        next: u64,
    }

    const NONE_YET: (u64, Option<(Duration, String)>) = (0, None);

    impl Reads {
        pub(crate) const fn new() -> Reads {
            Reads { kinds: [NONE_YET; 3], recent: VecDeque::new(), going: Vec::new(), next: 0 }
        }

        pub(crate) fn began(&mut self, what: Read, at: Instant) -> u64 {
            self.next += 1;
            self.going.push((self.next, what, at));
            self.next
        }

        pub(crate) fn ended(&mut self, id: u64, what: Read, start: Instant, end: Instant, clock: String) {
            self.going.retain(|g| g.0 != id);
            let d = end.saturating_duration_since(start);
            let k = &mut self.kinds[what as usize];
            k.0 += 1;
            if k.1.as_ref().is_none_or(|s| d > s.0) {
                k.1 = Some((d, clock));
            }
            self.recent.push_back((what, end, d));
            while self.recent.len() > RECENT {
                self.recent.pop_front();
            }
        }

        /// The slowest read in the time from `from` to `now`: one that ended
        /// in it (however long before it began), or one still under way.
        pub(crate) fn during(&self, from: Instant, now: Instant) -> Option<(Read, Duration, bool)> {
            let ended = self.recent.iter().filter(|r| r.1 >= from).map(|&(what, _, d)| (what, d, false));
            let going = self.going.iter().map(|&(_, what, at)| (what, now.saturating_duration_since(at), true));
            ended.chain(going).max_by_key(|r| r.1)
        }

        pub(crate) fn describe(&self, now: Instant) -> String {
            let mut parts = Vec::new();
            for what in KINDS {
                let (n, slowest) = &self.kinds[what as usize];
                if *n > 0
                    && let Some((d, at)) = slowest
                {
                    parts.push(format!("{} {n} (the slowest {} ms, at {at})", what.kind(), d.as_millis()));
                }
            }
            for &(_, what, at) in &self.going {
                parts.push(format!("{} under way {} ms", what.name(), now.saturating_duration_since(at).as_millis()));
            }
            if parts.is_empty() { "disc reads: none timed yet".to_string() } else { format!("disc reads: {}", parts.join(", ")) }
        }
    }

    static READS: Mutex<Reads> = Mutex::new(Reads::new());

    fn lock() -> std::sync::MutexGuard<'static, Reads> {
        READS.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `f`, a read of the disc of kind `what`, timed.
    pub fn timed<T>(what: Read, f: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let id = lock().began(what, start);
        let out = f();
        let end = Instant::now();
        let clock = crate::mem::clock_at(end);
        lock().ended(id, what, start, end, clock);
        out
    }

    /// The slowest read from `from` to `now` (`Reads::during`).
    pub fn during(from: Instant, now: Instant) -> Option<(Read, Duration, bool)> {
        lock().during(from, now)
    }

    /// For a report's state: each kind's reads and the slowest, and any
    /// under way.
    pub fn describe() -> String {
        lock().describe(Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::disc::{Read, Reads};
    use super::*;

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    /// Frames at 60 a second from `from` for `n` frames, each 10 ms long
    /// (its play phase), each drawn in a call of its own.
    fn frames(w: &mut Watch, t0: Instant, from: u64, n: u64) -> u64 {
        let mut t = from;
        for _ in 0..n {
            w.enter(at(t0, t));
            w.begin(at(t0, t), Duration::ZERO, None);
            assert_eq!(w.after_pump(None, at(t0, t + 5)), None);
            w.end(at(t0, t), at(t0, t + 10), vec![("play", 0.010)], false);
            w.leave("a redraw", at(t0, t + 11));
            t += 16;
        }
        t
    }

    /// A frame whose sound took 8.3 s is said with its phase, the game's
    /// thread's wait for the sound's driver, the disc's read and the
    /// server's datagrams: the last 8.4 s before, 143 waiting. The controls:
    /// frames 16 ms apart say nothing, and nor does one 999 ms on.
    #[test]
    fn a_stall_in_the_frame_names_its_phase() {
        let t0 = Instant::now();
        let mut w = Watch::default();
        let t = frames(&mut w, t0, 0, 30);
        // the frame that stalls, in its sound, waiting on the driver
        w.enter(at(t0, t));
        w.begin(at(t0, t), Duration::ZERO, Some(Heard { count: 1000, last: Some(at(t0, t - 10)) }));
        assert_eq!(w.after_pump(Some(Heard { count: 1001, last: Some(at(t0, t + 2)) }), at(t0, t + 3)), None);
        w.end(at(t0, t), at(t0, t + 8395), vec![("play", 0.030), ("sound", 8.355), ("hud", 0.010)], false);
        w.leave("a redraw", at(t0, t + 8396));
        // the next frame: the gap found, said once the datagrams are read
        let next = t + 8412;
        w.enter(at(t0, next));
        w.begin(at(t0, next), Duration::from_millis(8350), Some(Heard { count: 1001, last: Some(at(t0, t + 10)) }));
        let line = w.after_pump(Some(Heard { count: 1144, last: Some(at(t0, next + 1)) }), at(t0, next + 2)).expect("the stall said");
        assert!(line.starts_with("stall: 8412 ms from one frame to the next, to "), "{line}");
        assert!(line.contains("the frame itself took 8395 ms (sound 8355 ms), then 17 ms to the next: 1 ms the client's own (the longest the frame's own call after it, 1 ms), 16 ms no call from the event loop"), "{line}");
        assert!(line.contains("; the game's thread waited 8350 ms for the sound's driver"), "{line}");
        assert!(line.ends_with("; the game server's last datagram had come 8402 ms before the stall's end, and 143 were waiting"), "{line}");
        assert!(!line.contains("play 30"), "a phase under 50 ms is not named: {line}");
        w.end(at(t0, next), at(t0, next + 10), vec![], false);
        w.leave("a redraw", at(t0, next + 11));
        assert!(w.describe().starts_with("stalls (a frame 1 s or more after the one before): 1 this run, the longest 8412 ms from one frame"), "{}", w.describe());
        // the controls: a frame 999 ms after the last is none
        let t = frames(&mut w, t0, next + 16, 5);
        w.enter(at(t0, t + 983));
        w.begin(at(t0, t + 983), Duration::from_millis(8350), None);
        assert_eq!(w.after_pump(None, at(t0, t + 990)), None, "999 ms is no stall");
        let mut quiet = Watch::default();
        frames(&mut quiet, t0, 0, 200);
        assert_eq!(quiet.describe(), "stalls (a frame 1 s or more after the one before): none this run");
    }

    /// A stall between two frames says how much of it was the client's own
    /// handling (its calls, the longest named) and how much no call from the
    /// event loop came at all. The control: a call of the client's own that
    /// takes the time is said as the client's, not the loop's.
    #[test]
    fn a_stall_outside_the_frame_says_whose_it_was() {
        let t0 = Instant::now();
        let mut w = Watch::default();
        let t = frames(&mut w, t0, 0, 10);
        // a key (2 ms) and three moves of the pointer, then nothing for 5 s
        w.enter(at(t0, t));
        w.leave("a key", at(t0, t + 2));
        for k in 0..3 {
            w.enter(at(t0, t + 10 + k));
            w.leave("a move of the pointer", at(t0, t + 10 + k));
        }
        let next = t + 5000;
        w.enter(at(t0, next));
        w.begin(at(t0, next), Duration::ZERO, None);
        let line = w.after_pump(None, at(t0, next + 1)).expect("said");
        assert!(line.contains("5016 ms from one frame to the next"), "{line}");
        assert!(line.contains("the frame itself took 10 ms, then 5006 ms to the next: 3 ms the client's own in 4 calls (the longest a key, 2 ms), 5003 ms no call from the event loop"), "{line}");
        assert!(!line.contains("server"), "no game server, nothing said of one: {line}");
        w.end(at(t0, next), at(t0, next + 10), vec![], false);
        w.leave("a redraw", at(t0, next + 11));
        // the control: the loop's turn taking 4 s is the client's own
        let t = frames(&mut w, t0, next + 16, 1);
        w.enter(at(t0, t));
        w.leave("the loop's turn", at(t0, t + 4000));
        let next = t + 4010;
        w.enter(at(t0, next));
        w.begin(at(t0, next), Duration::ZERO, None);
        // said 10 s after the last at the earliest: kept for the state
        assert_eq!(w.after_pump(None, at(t0, next + 1)), None);
        assert!(w.describe().contains(": 2 this run, the longest 5016 ms"), "{}", w.describe());
        let mut v = Watch::default();
        let t = frames(&mut v, t0, 0, 1);
        v.enter(at(t0, t));
        v.leave("the loop's turn", at(t0, t + 4000));
        v.enter(at(t0, t + 4010));
        v.begin(at(t0, t + 4010), Duration::ZERO, None);
        let line = v.after_pump(None, at(t0, t + 4011)).unwrap();
        assert!(line.contains("then 4016 ms to the next: 4001 ms the client's own in 1 call (the longest the loop's turn, 4000 ms), 15 ms no call from the event loop"), "{line}");
    }

    /// One line in ten seconds at most, the stalls between counted into the
    /// next; the state keeps the run's count and the longest.
    #[test]
    fn stalls_are_said_at_most_every_ten_seconds() {
        let t0 = Instant::now();
        let mut w = Watch::default();
        let mut said = Vec::new();
        let mut t = frames(&mut w, t0, 0, 2);
        // stalls of 1.5, 3 and 1.2 s, 3 s apart, then one 10 s on
        for (gap, pause) in [(1500, 1500), (3000, 1500), (1200, 10000), (1100, 0)] {
            let next = t + gap;
            w.enter(at(t0, next));
            w.begin(at(t0, next), Duration::ZERO, None);
            if let Some(l) = w.after_pump(None, at(t0, next + 1)) {
                said.push(l);
            }
            w.end(at(t0, next), at(t0, next + 10), vec![], false);
            w.leave("a redraw", at(t0, next + 11));
            t = frames(&mut w, t0, next + 16, (pause / 16) as u64);
        }
        assert_eq!(said.len(), 2, "{said:?}");
        assert!(said[0].starts_with("stall: 1516 ms"), "{said:?}");
        assert!(said[1].ends_with("(and 2 more since the last line said, each 1 s or more)"), "{said:?}");
        assert!(w.describe().contains(": 4 this run, the longest 3016 ms"), "{}", w.describe());
    }

    /// A window that stalls for good (each frame 11 s after the last) says
    /// twenty lines, one a stall, then one a minute at most, the rest
    /// counted into them. The control: the first twenty, 11 s apart, are all
    /// said.
    #[test]
    fn after_twenty_lines_one_a_minute() {
        let t0 = Instant::now();
        let mut w = Watch::default();
        let mut said = Vec::new();
        for k in 0..30u64 {
            let t = k * 11_000;
            w.enter(at(t0, t));
            w.begin(at(t0, t), Duration::ZERO, None);
            if let Some(l) = w.after_pump(None, at(t0, t + 1)) {
                said.push((k, l));
            }
            w.end(at(t0, t), at(t0, t + 10), vec![], false);
            w.leave("a redraw", at(t0, t + 11));
        }
        let ks: Vec<u64> = said.iter().map(|s| s.0).collect();
        assert_eq!(ks, (1..=20).chain([26]).collect::<Vec<u64>>(), "the control: every one of the first twenty");
        assert!(said[20].1.ends_with("(and 5 more since the last line said, each 1 s or more)"), "{:?}", said[20]);
        assert!(w.describe().contains(": 29 this run"), "{}", w.describe());
    }

    /// A stall under the loading screen (a world's zones read at once as it
    /// is entered) is said so and counted apart: the run's longest is the
    /// longest of the others, though shorter. The control: the line of one in
    /// play says nothing of a loading screen.
    #[test]
    fn a_stall_under_the_loading_screen_is_kept_apart() {
        let t0 = Instant::now();
        let mut w = Watch::default();
        let t = frames(&mut w, t0, 0, 2);
        w.enter(at(t0, t));
        w.begin(at(t0, t), Duration::ZERO, None);
        assert_eq!(w.after_pump(None, at(t0, t + 1)), None);
        w.end(at(t0, t), at(t0, t + 6000), vec![("zones", 5.9)], true);
        w.leave("a redraw", at(t0, t + 6001));
        let next = t + 6016;
        w.enter(at(t0, next));
        w.begin(at(t0, next), Duration::ZERO, None);
        let line = w.after_pump(None, at(t0, next + 1)).expect("said");
        assert!(line.contains(" into the run), under the loading screen: the frame itself took 6000 ms (zones 5900 ms)"), "{line}");
        w.end(at(t0, next), at(t0, next + 10), vec![], false);
        w.leave("a redraw", at(t0, next + 11));
        assert_eq!(w.describe(), "stalls (a frame 1 s or more after the one before): none this run (1 more under the loading screen)");
        // one of 2 s in play, more than 10 s on (said)
        let t = frames(&mut w, t0, next + 16, 700);
        let next = t + 2000;
        w.enter(at(t0, next));
        w.begin(at(t0, next), Duration::ZERO, None);
        let line = w.after_pump(None, at(t0, next + 1)).expect("said");
        assert!(line.starts_with("stall: 2016 ms") && !line.contains("loading"), "the control: {line}");
        assert!(w.describe().contains(": 1 this run (1 more under the loading screen), the longest 2016 ms"), "{}", w.describe());
    }

    /// Frames that stop for a reason of their own (a window minimised, the
    /// pregame) or a frame that never ended make no stall of the time; the
    /// control: the same gap with neither is one.
    #[test]
    fn a_window_away_or_a_frame_unended_is_no_stall() {
        let t0 = Instant::now();
        let mut w = Watch::default();
        let t = frames(&mut w, t0, 0, 3);
        w.away();
        w.begin(at(t0, t + 20_000), Duration::ZERO, None);
        assert_eq!(w.after_pump(None, at(t0, t + 20_001)), None, "away");
        // begun and never ended: the next is no gap
        w.begin(at(t0, t + 30_000), Duration::ZERO, None);
        assert_eq!(w.after_pump(None, at(t0, t + 30_001)), None, "a frame never ended");
        let mut c = Watch::default();
        let t = frames(&mut c, t0, 0, 3);
        c.begin(at(t0, t + 20_000), Duration::ZERO, None);
        assert!(c.after_pump(None, at(t0, t + 20_001)).is_some(), "the control");
    }

    /// The disc's reads: the slowest of those that ended in a stretch, or one
    /// still under way, and each kind's count and slowest for the state. The
    /// controls: a read that ended before the stretch is not its, and a read
    /// done is under way no more.
    #[test]
    fn the_discs_slowest_read_in_a_stretch() {
        let t0 = Instant::now();
        let mut r = Reads::new();
        assert_eq!(r.describe(t0), "disc reads: none timed yet");
        let id = r.began(Read::World, at(t0, 0));
        r.ended(id, Read::World, at(t0, 0), at(t0, 900), "0:00:01".into());
        let id = r.began(Read::Zone, at(t0, 1000));
        r.ended(id, Read::Zone, at(t0, 1000), at(t0, 1040), "0:00:02".into());
        assert_eq!(r.during(at(t0, 1020), at(t0, 1050)), Some((Read::Zone, Duration::from_millis(40), false)), "the zone's, begun before it: the world's ended before it");
        assert_eq!(r.during(at(t0, 1041), at(t0, 1050)), None, "the control: nothing ended in it");
        let music = r.began(Read::Music, at(t0, 1100));
        assert_eq!(r.during(at(t0, 950), at(t0, 9400)), Some((Read::Music, Duration::from_millis(8300), true)), "under way, the slowest");
        r.ended(music, Read::Music, at(t0, 1100), at(t0, 9441), "0:00:10".into());
        assert_eq!(r.during(at(t0, 950), at(t0, 9500)), Some((Read::Music, Duration::from_millis(8341), false)));
        assert_eq!(r.during(at(t0, 9442), at(t0, 9500)), None, "the control: all ended before it");
        assert_eq!(r.describe(at(t0, 9500)), "disc reads: music 1 (the slowest 8341 ms, at 0:00:10), zones 1 (the slowest 40 ms, at 0:00:02), worlds 1 (the slowest 900 ms, at 0:00:01)");
        let going = r.began(Read::Zone, at(t0, 9600));
        assert!(r.describe(at(t0, 9700)).ends_with(", a zone under way 100 ms"), "{}", r.describe(at(t0, 9700)));
        r.ended(going, Read::Zone, at(t0, 9600), at(t0, 9610), "0:00:10".into());
        assert!(!r.describe(at(t0, 9700)).contains("under way"), "the control: done");
    }
}
