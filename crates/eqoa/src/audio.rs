//! The sound output: SNDDRV (`snd::Driver`) on the audio device's callback
//! thread, mixing as the device asks for samples, so its clock is the
//! device's, as the SPU2's was the console's.
//!
//! The game's `VISoundDevice` (`snd::device`) reaches it through `AudioLink`,
//! as the EE reached the IOP: a call runs at once when the driver is idle
//! (the port's synchronous RPC), and one that has to wait for a vblank (a BGM
//! stop) is left running on the audio thread; until it answers the link is
//! busy and the device keeps its next packet, as `sceSifCheckStatRpc` makes
//! it. A SIF DMA is a copy into the IOP's memory.
//!
//! Without an audio device (or with `EQOA_AUDIO=0`) a thread runs the driver
//! on the wall clock instead, so the game sees the same IOP either way. The
//! device is watched (`Watch`): one gone (unplugged, a sleep, the system's
//! audio restarted) puts the driver on the wall clock while another is
//! looked for, and the first that plays takes it back where it is.
//!
//! The callback mixes in the device's blocks, 25-30 ms apart here (PulseAudio,
//! measured; 50 now and then), while on the console the IOP and the SPU2 run
//! beside the EE all the time. So natively each call first brings the driver
//! up to the output's present (`Shared::catch_up`), as the IOP would be: the
//! status block the device reads then says what is playing now, not at the
//! last callback, and a sound it played a frame ago is seen playing
//! (`snd::device::process` stops one it has not seen three frames on).
//!
//! In a page there is no thread to give it: the driver runs on the game's,
//! mixing a little ahead of the page's audio clock once a frame (`feed`) for
//! an AudioWorklet that only plays what it is handed (web/sound.js).
//!
//! Sound in Background (the Extended page) off, a window not in front is not
//! heard: `set_heard(false)` takes the output's gain to nothing over about
//! 10 ms (`ramp`), and back the same way. The driver mixes on all the while,
//! so the game's sound keeps its clock and comes back where it is, with no
//! burst of what was held.
//!
//! Output Device (the Extended page's Sound, natively; F3 20261024-201433:
//! the game played into the system's default output, a television, while the
//! player listened elsewhere): the system's default, followed when it
//! changes, or a device by its name, the default meanwhile while it is not
//! connected and back on it when it is (`retarget`). cpal tells no program
//! that the default changed, so a thread reads the default's name every two
//! seconds and lists the devices while they are wanted (`lister`); a frame
//! only compares. With ALSA (Linux) the default is ALSA's own `default`,
//! which PulseAudio or PipeWire move to wherever the sound server sends it.

use crate::settings::DeviceName;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use snd::device::Link;
use snd::{Disc, Driver};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Whether this window is heard (`set_heard`).
static HEARD: AtomicBool = AtomicBool::new(true);

/// Whether this window is heard: Sound in Background off and the window not
/// in front is false. The output fades to nothing and back (`ramp`); the
/// driver mixes on regardless.
pub fn set_heard(on: bool) {
    if HEARD.swap(on, Ordering::Relaxed) != on {
        crate::say(if on { "sound: heard" } else { "sound: not heard (behind other windows, Sound in Background off)" });
    }
}

/// The game's thread's waits for the driver's lock (`Shared`), in µs: their
/// sum and the longest. The output's callback holds the lock while it mixes,
/// and the driver it runs reads the disc for the music (natively off a copy
/// read whole on a thread of its own, `bgm::read_on_a_thread`); a stall's
/// line says how long the game waited here in its time (`stall`, F3
/// 20261029-025532: a callback once mixing for 8,363 ms).
static GAME_WAITED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static GAME_WAIT_MOST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The driver's lock as the game's thread takes it (the sound device's
/// calls through `AudioLink`, the output's watch, a report's reads), its
/// wait counted.
fn game_lock(m: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    let t = web_time::Instant::now();
    let g = m.lock().unwrap();
    let us = t.elapsed().as_micros() as u64;
    GAME_WAITED.fetch_add(us, Ordering::Relaxed);
    GAME_WAIT_MOST.fetch_max(us, Ordering::Relaxed);
    g
}

/// How long the game's thread has waited for the driver's lock in all.
pub fn game_waits() -> Duration {
    Duration::from_micros(GAME_WAITED.load(Ordering::Relaxed))
}

/// The output's gain wanted now: 1 heard, 0 not.
fn heard_gain() -> f32 {
    if HEARD.load(Ordering::Relaxed) { 1.0 } else { 0.0 }
}

/// The gain a frame on, `step` nearer `target` (no further): muting and
/// coming back fade instead of clicking.
fn ramp(gain: &mut f32, target: f32, step: f32) -> f32 {
    if *gain < target {
        *gain = (*gain + step).min(target);
    } else if *gain > target {
        *gain = (*gain - step).max(target);
    }
    *gain
}

/// The steps of 10 ms of 48 kHz frames: the fade's.
const FADE_48K: f32 = 1.0 / 480.0;

/// Interleaved stereo 48 kHz frames through the gain, fading it toward
/// `target` a frame at a time (a page's output, `feed`).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn apply_gain(pcm: &mut [i16], gain: &mut f32, target: f32) {
    if *gain == 1.0 && target == 1.0 {
        return;
    }
    for f in pcm.chunks_mut(2) {
        let g = ramp(gain, target, FADE_48K);
        for v in f {
            *v = (*v as f32 * g) as i16;
        }
    }
}

/// CDSTREAM over the disc image: `\MUSIC\MUSIC0\TORGIRAN.BGM;1` by its
/// directory entry, sectors by number.
pub struct IsoDisc(pub iso::Iso);

impl Disc for IsoDisc {
    fn search(&mut self, name: &str) -> Option<(u32, u32)> {
        let path = name.trim_start_matches('\\').trim_end_matches(";1");
        self.0.find(path).map(|e| (e.lba, e.size))
    }
    fn read(&mut self, lsn: u32, sectors: u32, buf: &mut [u8]) -> bool {
        let n = (sectors as usize * 0x800).min(buf.len());
        self.0.read_sectors(lsn, &mut buf[..n]).is_ok()
    }
}

struct Shared {
    drv: Driver,
    status: [u32; 128],
    /// interleaved 48 kHz frames mixed and not yet played
    pcm: Vec<i16>,
    /// natively, the output's last callback: when it came, and the driver's
    /// clock at the end of what had been handed to the device then
    handed: Option<(Instant, u64)>,
    /// natively, when the device last called back (`Watch`: a stream that
    /// stops calling back is a stream gone)
    called: Option<Instant>,
    /// natively, no device takes the driver's frames: the wall clock's
    /// thread mixes them and drops them (`wall_clock`), from when this began
    /// and the driver's clock then; a device's first callback ends it
    wall: Option<(Instant, u64)>,
    /// natively, how the output has kept time (`Out`, the report's line)
    out: Out,
}

/// Natively, how the output has kept time since the start, for a report
/// (`Audio::report_line`; F3 20261025-225337, crunchy on a Mac with two
/// windows): what can be heard as crackling. A callback whose stream time
/// came later than the last one's end by more than half its buffer is a gap
/// (the device played something we never gave it); one that waited for the
/// lock and mixed for longer than its buffer lasts is late (the device will
/// have run dry). The driver held at the output's last block (`catch_up`'s
/// cap: no callback for `CATCH_UP_MAX`) stops its clock, and with it a call
/// that waits for a vblank; such a call is run on (`Shared::run_on`).
#[derive(Default)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct Out {
    callbacks: u64,
    /// the frames a callback asked for, fewest and most, and its rate
    frames: Option<(usize, usize)>,
    rate: u32,
    /// the last callback's stream time and frames
    last: Option<(cpal::StreamInstant, usize)>,
    gaps: u64,
    /// the last gap: when, and how much was missing
    last_gap: Option<(Instant, Duration)>,
    late: u64,
    last_late: Option<Instant>,
    /// the longest wait for the lock, and the longest mix
    wait_max: Duration,
    work_max: Duration,
    held: u64,
    last_held: Option<Instant>,
    ran_on: u64,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
impl Out {
    /// A callback for `frames` at `rate`, its stream time `at`, after waiting
    /// `wait` for the lock and mixing for `work`.
    fn callback(&mut self, at: cpal::StreamInstant, frames: usize, rate: u32, wait: Duration, work: Duration, now: Instant) {
        self.callbacks += 1;
        self.rate = rate;
        self.frames = Some(match self.frames {
            Some((lo, hi)) => (lo.min(frames), hi.max(frames)),
            None => (frames, frames),
        });
        let lasts = |n: usize| Duration::from_secs_f64(n as f64 / rate.max(1) as f64);
        if let Some((t, n)) = self.last
            && let Some(since) = at.duration_since(&t)
        {
            let ends = lasts(n);
            if since > ends + (ends / 2).max(Duration::from_millis(5)) {
                self.gaps += 1;
                self.last_gap = Some((now, since - ends));
            }
        }
        self.last = Some((at, frames));
        self.wait_max = self.wait_max.max(wait);
        self.work_max = self.work_max.max(work);
        if wait + work > lasts(frames) {
            self.late += 1;
            self.last_late = Some(now);
        }
    }

    /// The report's words for it.
    fn describe(&self) -> String {
        let ago = |t: Option<Instant>| t.map_or(String::new(), |t| format!(", the last {:.0} s ago", t.elapsed().as_secs_f64()));
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        let frames = match self.frames {
            Some((lo, hi)) if lo == hi => format!("{lo} frames"),
            Some((lo, hi)) => format!("{lo} to {hi} frames"),
            None => "no frames".into(),
        };
        let gap = self.last_gap.map_or(String::new(), |(t, d)| format!(", the last {:.0} s ago, {:.1} ms missing", t.elapsed().as_secs_f64(), ms(d)));
        format!(
            "{} callbacks of {frames} at {} Hz, {} gaps{gap}, {} late{} (waits for the mixer up to {:.1} ms, mixing up to {:.1} ms); the driver held at the output {} times{}, a waiting call run on {} times",
            self.callbacks,
            self.rate,
            self.gaps,
            self.late,
            ago(self.last_late),
            ms(self.wait_max),
            ms(self.work_max),
            self.held,
            ago(self.last_held),
            self.ran_on
        )
    }
}

/// How far `catch_up` mixes past what was last handed to the device (100
/// ms): while a device stalls (its callback later than this) the IOP's clock
/// waits there, since what is mixed ahead is heard only once the device has
/// taken it, and a sound played meanwhile would come that much late.
const CATCH_UP_MAX: u64 = 4800;

impl Shared {
    fn new(disc: Box<dyn Disc>) -> Self {
        Shared { drv: Driver::new(disc), status: [0; 128], pcm: Vec::new(), handed: None, called: None, wall: None, out: Out::default() }
    }

    /// The driver onto the wall clock from `now`: no device takes its frames.
    fn wall_from(&mut self, now: Instant) {
        if self.wall.is_none() {
            self.wall = Some((now, self.drv.now));
        }
    }

    /// The driver mixed up to the output's present: what the device was last
    /// handed and the time since, at 48 kHz (at most `CATCH_UP_MAX` past
    /// it). The frames wait in `pcm` for the callback, which then mixes only
    /// what it still lacks. Nothing before the first callback, nor offline.
    fn catch_up(&mut self) {
        let Some((at, handed)) = self.handed else { return };
        let since = (at.elapsed().as_secs_f64() * 48000.0) as u64;
        if since > CATCH_UP_MAX && self.out.last_held.is_none_or(|t| t < at) {
            // once a stall: the output's callback is that late
            self.out.held += 1;
            self.out.last_held = Some(Instant::now());
        }
        let due = handed + since.min(CATCH_UP_MAX);
        if due > self.drv.now {
            self.pump(due - self.drv.now);
        }
    }

    /// Whether the output's clock is held: its last callback is older than
    /// `catch_up` runs the driver past (`CATCH_UP_MAX`). Never offline, nor
    /// on the wall clock (its thread stands in for the callback).
    fn held(&self) -> bool {
        self.handed.is_some_and(|(at, _)| at.elapsed().as_secs_f64() * 48000.0 > CATCH_UP_MAX as f64)
    }

    /// The driver run on, its frames kept for the output, until the call it
    /// is running answers (a second of frames at most): for when the
    /// output's clock will not bring the vblank the call waits for
    /// (`AudioLink`). The output plays them once it calls back, a little
    /// late; nothing is skipped.
    fn run_on(&mut self) {
        self.out.ran_on += 1;
        for _ in 0..100 {
            if !self.drv.busy() {
                break;
            }
            self.pump(480);
        }
    }

    fn take_answer(&mut self) {
        if let Some(st) = self.drv.answer() {
            self.status = st;
        }
    }

    /// Mix `frames` more 48 kHz frames into `pcm`.
    fn pump(&mut self, frames: u64) {
        let until = self.drv.now + frames;
        while self.drv.now < until {
            self.drv.step(until, &mut self.pcm);
            self.take_answer();
        }
    }
}

/// Natively, while no device takes the driver's frames (`Shared::wall`):
/// the driver run on the wall clock every 10 ms and its frames dropped, so
/// the game's sound keeps its clock and comes back where it is. The thread
/// ends with the `Audio`.
fn wall_clock(s: Arc<Mutex<Shared>>) {
    std::thread::spawn(move || {
        let mut wav = WavOut::open();
        loop {
            std::thread::sleep(Duration::from_millis(10));
            if Arc::strong_count(&s) == 1 {
                return;
            }
            let mut g = s.lock().unwrap();
            let Some((at, from)) = g.wall else { continue };
            let due = from + (at.elapsed().as_secs_f64() * 48000.0) as u64;
            let n = due.saturating_sub(g.drv.now);
            g.pump(n);
            if let Some(w) = wav.as_mut() {
                w.write(&g.pcm);
            }
            g.pcm.clear();
            g.handed = Some((Instant::now(), g.drv.now));
        }
    });
}

/// `EQOA_WAV=<path>`: what the driver mixes on the wall clock (a silent run,
/// `EQOA_AUDIO=0`) written to a WAV as it comes, 48 kHz stereo, its sizes
/// kept right after every write so a run killed leaves a file that plays: for
/// a test to measure a scripted run's sound (F3 20261026-075325: the clipped
/// samples of a busy fight's, before and after a change).
struct WavOut {
    f: std::fs::File,
    bytes: u32,
}

impl WavOut {
    fn open() -> Option<WavOut> {
        use std::io::Write;
        let path = std::env::var_os("EQOA_WAV")?;
        let mut f = std::fs::File::create(&path).map_err(|e| crate::say(&format!("EQOA_WAV: {e}"))).ok()?;
        let mut h = Vec::with_capacity(44);
        h.extend(b"RIFF");
        h.extend(36u32.to_le_bytes());
        h.extend(b"WAVEfmt ");
        h.extend(16u32.to_le_bytes());
        h.extend(1u16.to_le_bytes());
        h.extend(2u16.to_le_bytes());
        h.extend(48000u32.to_le_bytes());
        h.extend((48000u32 * 4).to_le_bytes());
        h.extend(4u16.to_le_bytes());
        h.extend(16u16.to_le_bytes());
        h.extend(b"data");
        h.extend(0u32.to_le_bytes());
        f.write_all(&h).ok()?;
        Some(WavOut { f, bytes: 0 })
    }

    fn write(&mut self, pcm: &[i16]) {
        use std::io::{Seek, SeekFrom, Write};
        if pcm.is_empty() {
            return;
        }
        let data: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
        let _ = self.f.write_all(&data);
        self.bytes = self.bytes.saturating_add(data.len() as u32);
        for (at, v) in [(4u64, 36 + self.bytes), (40, self.bytes)] {
            let _ = self.f.seek(SeekFrom::Start(at)).and_then(|_| self.f.write_all(&v.to_le_bytes()));
        }
        let _ = self.f.seek(SeekFrom::End(0));
    }
}

/// How long a stream may go without calling back before it counts as gone
/// (a device's blocks come 25-50 ms apart).
const STALL: Duration = Duration::from_secs(2);
/// The waits between looks for an output while there is none: the first,
/// doubled after each failure up to the last.
const RETRY_FIRST: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(30);

/// Natively, the output watched (`Audio::feed` once a frame): a device gone
/// takes its stream with it, and the game never asked for another, so the
/// sound was lost until a restart (F3 crash 20261024-001640's log, a PC put
/// to sleep: "The requested device is no longer available"; headphones
/// unplugged on Windows say the same). A stream counts as gone when its
/// device says so (`StreamError::DeviceNotAvailable`: WASAPI, Core Audio) or
/// when it has not called back in `STALL` (ALSA reports a lost device as a
/// backend error, or its thread just stops). Then the driver goes onto the
/// wall clock and an output is looked for, at once and then after
/// `RETRY_FIRST`, waits doubling to `RETRY_MAX`; a stream that opens but
/// never calls back counts as not found. A start with no output looks the
/// same way. The log says when it stopped, the first failure, and when one
/// plays again: never once a look.
struct Watch {
    /// set by the stream's error callback: its device is gone
    lost: Arc<AtomicBool>,
    /// when the stream playing was opened, none playing: None
    opened: Option<Instant>,
    /// when to look next, and how long to wait after that look fails
    retry: (Instant, Duration),
    /// a stream has played since the start (so one found is one back)
    ever: bool,
    /// the stream opened has called back (said once)
    heard: bool,
    /// the failures said this time, each once
    said: Vec<String>,
    /// the wall clock's thread made
    walling: bool,
}

/// What the watch asks of the output this frame (`Watch::step`).
#[derive(Debug, PartialEq)]
enum Step {
    Keep,
    /// the stream playing is gone (why): dropped, the driver onto the wall
    /// clock, an output looked for
    Stopped(&'static str),
    /// a stream opened has not called back: dropped, a look that failed
    Silent,
    /// look for an output now
    Look,
    /// a stream opened has called back: it plays (true: one played before)
    Plays(bool),
}

impl Watch {
    /// The output at the start: its stream opened at `now`, or none (the
    /// first look after `RETRY_FIRST`).
    fn new(now: Instant, playing: bool) -> Watch {
        let retry = if playing { (now, RETRY_FIRST) } else { (now + RETRY_FIRST, RETRY_FIRST * 2) };
        Watch { lost: Arc::new(AtomicBool::new(false)), opened: playing.then_some(now), retry, ever: playing, heard: playing, said: Vec::new(), walling: false }
    }

    /// This frame's step at `now`, the device's last callback `called`.
    fn step(&mut self, now: Instant, called: Option<Instant>) -> Step {
        let Some(opened) = self.opened else {
            return if now >= self.retry.0 { Step::Look } else { Step::Keep };
        };
        // a callback of this stream's
        let back = called.filter(|c| *c >= opened);
        let gone = self.lost.swap(false, Ordering::Relaxed);
        let quiet = now.saturating_duration_since(back.unwrap_or(opened)) >= STALL;
        if gone || quiet {
            self.opened = None;
            if !self.heard {
                return Step::Silent;
            }
            self.heard = false;
            self.said.clear();
            self.retry = (now, RETRY_FIRST);
            return Step::Stopped(if gone { "its device is gone" } else { "it stopped asking for sound" });
        }
        if back.is_some() && !self.heard {
            self.heard = true;
            self.said.clear();
            return Step::Plays(std::mem::replace(&mut self.ever, true));
        }
        Step::Keep
    }

    /// A stream opened at `now`, playing once it calls back.
    fn open(&mut self, now: Instant) {
        self.opened = Some(now);
        self.heard = false;
    }

    /// A look that found nothing (`why`), at `now`: the next one after the
    /// wait, which doubles; the line to say when this failure is new.
    fn failed(&mut self, now: Instant, why: &str) -> Option<String> {
        let wait = self.retry.1;
        self.retry = (now + wait, (wait * 2).min(RETRY_MAX));
        if self.said.iter().any(|s| s == why) {
            return None;
        }
        self.said.push(why.to_string());
        Some(format!("audio: no output yet ({why}): looked for again now and then"))
    }
}

/// How the stream playing came to its device (`Audio::stream`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum How {
    /// the system's default, Output Device's System Default
    Default,
    /// the device Output Device names
    Chosen,
    /// the device Output Device names is not connected: the system's default
    /// meanwhile
    Missing,
}

/// The stream playing: its device's name as the system gives it, how it came
/// to be that one, and the choice it was opened for.
#[derive(Clone, Debug, PartialEq)]
struct On {
    name: String,
    how: How,
    choice: Option<DeviceName>,
}

/// What Output Device asks of the stream playing (`retarget`).
#[derive(Debug, PartialEq)]
enum Retarget {
    Stay,
    /// opened again, for the reason said
    Open(&'static str),
}

/// What the stream playing (`on`) should do now, for the choice `want` (none:
/// the system's default), the system's default `default` and the devices
/// `listed` lately, as the lister last read them: opened again when the
/// choice changed; on the default, when the default is another device now;
/// while the device chosen is not connected, when it is listed again (or the
/// default changed meanwhile). On the device chosen it stays, whatever the
/// default does; a device gone takes its stream (`Watch`), and the look after
/// it finds the default meanwhile.
fn retarget(want: Option<DeviceName>, on: &On, default: Option<&str>, listed: Option<&[String]>) -> Retarget {
    if want != on.choice {
        return Retarget::Open(if want.is_some() { "the device chosen" } else { "the system default chosen" });
    }
    let moved = default.is_some_and(|d| d != on.name);
    let back = want.is_some_and(|w| listed.is_some_and(|l| l.iter().any(|n| w.same(n))));
    match on.how {
        How::Default if moved => Retarget::Open("the system default changed"),
        How::Missing if back => Retarget::Open("the device chosen is connected again"),
        How::Missing if moved => Retarget::Open("the system default changed"),
        _ => Retarget::Stay,
    }
}

/// The device Output Device names among the system's `names` (by its name,
/// as `DeviceName` keeps it), if it is there.
fn find_chosen(want: DeviceName, names: &[String]) -> Option<usize> {
    names.iter().position(|n| want.same(n))
}

/// The system's outputs as the lister last read them: its default's name,
/// and every output device's name with when they were listed.
#[derive(Clone, Debug, Default)]
pub struct Outputs {
    pub default: Option<String>,
    pub listed: Option<(Instant, Vec<String>)>,
}

static OUTPUTS: Mutex<Outputs> = Mutex::new(Outputs { default: None, listed: None });
/// Until when the lister lists the devices again and again (`want_listing`).
static LIST_UNTIL: Mutex<Option<Instant>> = Mutex::new(None);
/// The device the stream playing is on, for the Options window (`playing`).
static PLAYING: Mutex<Option<String>> = Mutex::new(None);

/// How often the lister reads the system's default, lists the devices while
/// they are wanted, and how long one ask wants them.
const DEFAULT_EVERY: Duration = Duration::from_secs(2);
const LIST_EVERY: Duration = Duration::from_secs(4);
const LIST_FOR: Duration = Duration::from_secs(5);

/// The system's outputs as the lister last read them (none in a page, offline
/// or with `EQOA_AUDIO=0`, where there is no lister).
pub fn outputs() -> Outputs {
    OUTPUTS.lock().unwrap().clone()
}

/// The devices listed again within a second or so, and every few seconds
/// for the next `LIST_FOR` (the Options window's Sound page while it shows,
/// the device chosen while it is not connected).
pub fn want_listing() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        *LIST_UNTIL.lock().unwrap() = Some(Instant::now() + LIST_FOR);
    }
}

/// The device the game's sound plays on now, natively; none while there is
/// no output.
pub fn playing() -> Option<String> {
    PLAYING.lock().unwrap().clone()
}

/// The stream playing's device, for the Options window (`playing`).
fn note_playing(on: Option<&On>) {
    *PLAYING.lock().unwrap() = on.map(|o| o.name.clone());
}

/// Natively, the system's outputs read off the frames' thread (cpal says
/// nothing when the default changes, and a listing can take a moment): the
/// default's name every `DEFAULT_EVERY`, the devices once at the start and
/// then every `LIST_EVERY` while they are wanted (`want_listing`), into
/// `OUTPUTS`; a listing that differs from the last is said in the log. The
/// thread ends with the `Audio` (`alive`).
fn lister(alive: Arc<()>) {
    let run = move || {
        let host = cpal::default_host();
        let mut read: Option<Instant> = None;
        let mut first = true;
        loop {
            if Arc::strong_count(&alive) == 1 {
                return;
            }
            let now = Instant::now();
            if read.is_none_or(|t| now.saturating_duration_since(t) >= DEFAULT_EVERY) {
                let d = host.default_output_device().and_then(|d| d.name().ok());
                OUTPUTS.lock().unwrap().default = d;
                read = Some(now);
            }
            let wanted = first || LIST_UNTIL.lock().unwrap().is_some_and(|u| now < u);
            let stale = OUTPUTS.lock().unwrap().listed.as_ref().is_none_or(|(t, _)| now.saturating_duration_since(*t) >= LIST_EVERY);
            if wanted && (first || stale) {
                let names: Vec<String> = host.output_devices().map(|ds| ds.filter_map(|d| d.name().ok()).collect()).unwrap_or_default();
                let mut o = OUTPUTS.lock().unwrap();
                if o.listed.as_ref().is_none_or(|(_, was)| *was != names) {
                    crate::say(&format!("audio: the system's outputs: {}", if names.is_empty() { "none".to_string() } else { names.join(", ") }));
                }
                o.listed = Some((Instant::now(), names));
                first = false;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    };
    if let Err(e) = std::thread::Builder::new().name("audio outputs".into()).spawn(run) {
        crate::say(&format!("audio: the outputs are not watched ({e})"));
    }
}

pub struct Audio {
    shared: Arc<Mutex<Shared>>,
    _stream: Option<cpal::Stream>,
    pub describe: String,
    /// natively, the device output watched (`Watch`); none offline, in a
    /// page, or with `EQOA_AUDIO=0`
    watch: Option<Watch>,
    /// natively, the stream playing's device (Output Device, `retarget`)
    on: Option<On>,
    /// a stream opened again for Output Device: the reason, said when it
    /// plays
    switched: Option<&'static str>,
    /// the lister's thread runs while this is held twice (`lister`)
    _alive: Option<Arc<()>>,
    #[cfg(target_arch = "wasm32")]
    page: Option<Page>,
}

/// How far the page's output is mixed ahead of its clock (48 kHz frames): the
/// time a sound the game starts waits before it is heard. Each feed mixes up
/// to the clock plus the lead, and the next feed must come before that is
/// played out; so the lead is sized by how far the clock moved between two
/// feeds over the last `LEAD_WINDOW` seconds (`lead_for`). The clock moves in
/// steps, the blocks the browser's output takes from the AudioWorklet at
/// once (512 frames in Chromium, 1,408-1,536 in Firefox on Linux, measured),
/// so a move counts them whole, and half a move more is kept for the block
/// taken while a feed's frames are still on their way to the worklet. The
/// two largest moves in the window are left out (a stall runs dry once
/// rather than holding every sound late for the window), and so is a move
/// past `LEAD_MAX` (a world loading, a hidden page). It was 100 ms, 50 ms more
/// at each dry and 25 ms less each ten seconds without one: a few slow frames
/// held every sound 150-200 ms late (F3 20261020-203549).
#[cfg(any(target_arch = "wasm32", test))]
pub const LEAD_START: u64 = 4800;
#[cfg(any(target_arch = "wasm32", test))]
pub const LEAD_MIN: u64 = 1440;
#[cfg(any(target_arch = "wasm32", test))]
pub const LEAD_MAX: u64 = 14400;
#[cfg(any(target_arch = "wasm32", test))]
pub const LEAD_MARGIN: u64 = 240;
#[cfg(any(target_arch = "wasm32", test))]
pub const LEAD_WINDOW: f64 = 10.0;

/// The lead for the clock's moves between feeds `moves` (48 kHz frames): the
/// third largest, half of it again and `LEAD_MARGIN`, within `LEAD_MIN` and
/// `LEAD_MAX`; with none yet, `LEAD_START`.
#[cfg(any(target_arch = "wasm32", test))]
pub fn lead_for(moves: impl Iterator<Item = u64>) -> u64 {
    let mut top = [None::<u64>; 3];
    for m in moves {
        let mut v = Some(m);
        for t in top.iter_mut() {
            if v > *t {
                std::mem::swap(t, &mut v);
            }
        }
    }
    match top[2].or(top[1]).or(top[0]) {
        Some(m) => (m + m / 2 + LEAD_MARGIN).clamp(LEAD_MIN, LEAD_MAX),
        None => LEAD_START,
    }
}

/// The driver in a page (`Audio::web`).
#[cfg(target_arch = "wasm32")]
struct Page {
    /// starts the disc reads the music asked for (`web::bgm`)
    fetch: Box<dyn FnMut()>,
    /// the driver's clock and the page's (48 kHz frames) when they were last
    /// lined up, and whether the page's was its audio output's
    base: Option<(u64, f64, bool)>,
    lead: u64,
    /// the output's clock at the last feed, and its moves between feeds over
    /// the last `LEAD_WINDOW` seconds (wall clock s, 48 kHz frames)
    last: Option<f64>,
    moves: std::collections::VecDeque<(f64, u64)>,
    dry: u32,
    wall: web_time::Instant,
    /// the output's gain (`set_heard`)
    gain: f32,
}

impl Audio {
    fn with_shared(shared: Arc<Mutex<Shared>>, stream: Option<cpal::Stream>, describe: String) -> Audio {
        Audio {
            shared,
            _stream: stream,
            describe,
            watch: None,
            on: None,
            switched: None,
            _alive: None,
            #[cfg(target_arch = "wasm32")]
            page: None,
        }
    }

    /// The driver, playing on the output Output Device names (the default
    /// output device unless one is chosen), watched (`Watch`); with none, on
    /// the wall clock while one is looked for, and with `EQOA_AUDIO=0` on the
    /// wall clock for good.
    pub fn open(disc: Box<dyn Disc>) -> Audio {
        let shared = Arc::new(Mutex::new(Shared::new(disc)));
        let now = Instant::now();
        if std::env::var("EQOA_AUDIO").is_ok_and(|v| v == "0") {
            shared.lock().unwrap().wall_from(now);
            wall_clock(shared.clone());
            return Self::with_shared(shared, None, "no audio output (EQOA_AUDIO=0): the driver runs on the wall clock".into());
        }
        let alive = Arc::new(());
        lister(alive.clone());
        let lost = Arc::new(AtomicBool::new(false));
        let want = crate::settings::client().output;
        let (stream, describe, on, mut watch) = match Self::stream(shared.clone(), lost.clone(), want) {
            Ok((stream, on, describe)) => (Some(stream), describe, Some(on), Watch::new(now, true)),
            Err(e) => {
                shared.lock().unwrap().wall_from(now);
                wall_clock(shared.clone());
                let mut w = Watch::new(now, false);
                w.walling = true;
                (None, format!("no audio output ({e}): the driver runs on the wall clock, and an output is looked for now and then"), None, w)
            }
        };
        watch.lost = lost;
        let mut a = Self::with_shared(shared, stream, describe);
        a.watch = Some(watch);
        note_playing(on.as_ref());
        a.on = on;
        a._alive = Some(alive);
        a
    }


    /// Natively, the output watched once a frame (`Watch`): a stream gone
    /// dropped and the driver put on the wall clock, an output looked for
    /// when one is due, and the log told when it stops, fails or plays; and
    /// while one plays, opened again where Output Device and the system's
    /// default say (`retarget`).
    fn watch_output(&mut self) {
        let Some(w) = self.watch.as_mut() else { return };
        let now = Instant::now();
        let called = game_lock(&self.shared).called;
        let want = crate::settings::client().output;
        match w.step(now, called) {
            Step::Keep => {
                let Some(on) = self.on.as_ref() else { return };
                // the device chosen is not connected: is it again?
                if on.how == How::Missing {
                    want_listing();
                }
                let o = outputs();
                if let Retarget::Open(why) = retarget(want, on, o.default.as_deref(), o.listed.as_ref().map(|l| l.1.as_slice())) {
                    self.reopen(now, want, why);
                }
            }
            Step::Plays(again) => {
                let why = match self.switched.take() {
                    Some(why) => why,
                    None if again => "the sound has its output again",
                    None => "an output found",
                };
                crate::say(&format!("{}: {why}", self.describe));
            }
            Step::Stopped(why) => {
                self._stream = None;
                self.on = None;
                note_playing(None);
                game_lock(&self.shared).wall_from(now);
                if !std::mem::replace(&mut w.walling, true) {
                    wall_clock(self.shared.clone());
                }
                crate::say(&format!("audio: the output stopped ({why}): the game's sound goes on unheard while an output is looked for"));
            }
            Step::Silent => {
                self._stream = None;
                self.on = None;
                note_playing(None);
                if let Some(line) = w.failed(now, "an output opened, but asked for no sound") {
                    crate::say(&line);
                }
            }
            Step::Look => match Self::stream(self.shared.clone(), w.lost.clone(), want) {
                Ok((stream, on, describe)) => {
                    self._stream = Some(stream);
                    self.describe = describe;
                    w.open(now);
                    note_playing(Some(&on));
                    self.on = Some(on);
                }
                Err(e) => {
                    if let Some(line) = w.failed(now, &e) {
                        crate::say(&line);
                    }
                }
            },
        }
    }

    /// Output Device's move (`retarget`, `why`): the stream playing dropped
    /// first (a device that takes one stream at a time frees for the next)
    /// and one opened for `want`, the driver on the wall clock between them
    /// as for a device gone, so the game's sound keeps its clock; the new
    /// one said as it plays. One that will not open is a look that failed:
    /// looked for again as after a device gone.
    fn reopen(&mut self, now: Instant, want: Option<DeviceName>, why: &'static str) {
        let Some(w) = self.watch.as_mut() else { return };
        self._stream = None;
        game_lock(&self.shared).wall_from(now);
        if !std::mem::replace(&mut w.walling, true) {
            wall_clock(self.shared.clone());
        }
        w.lost.store(false, Ordering::Relaxed);
        crate::say(&format!("audio: {why}: opened again"));
        match Self::stream(self.shared.clone(), w.lost.clone(), want) {
            Ok((stream, on, describe)) => {
                self._stream = Some(stream);
                self.describe = describe;
                w.open(now);
                note_playing(Some(&on));
                self.on = Some(on);
                self.switched = Some(why);
            }
            Err(e) => {
                w.opened = None;
                w.heard = false;
                w.retry = (now, RETRY_FIRST);
                self.on = None;
                note_playing(None);
                if let Some(line) = w.failed(now, &e) {
                    crate::say(&line);
                }
            }
        }
    }

    /// For a report: where the sound plays and how it came to, the system's
    /// default and outputs as last read, and what Output Device names.
    pub fn report_line(&self) -> String {
        let o = outputs();
        let listed = match &o.listed {
            Some((at, names)) => format!("{} (listed {:.0} s ago)", if names.is_empty() { "none".to_string() } else { names.join(", ") }, at.elapsed().as_secs_f64()),
            None => "not listed".to_string(),
        };
        format!(
            "{}; the system's default {}; outputs {listed}; Output Device {}; output: {}; the game's waits for the driver up to {:.1} ms ({:.1} s in all)",
            self.describe,
            o.default.as_deref().unwrap_or("not read"),
            crate::settings::client().output.map_or("the system default".to_string(), |d| format!("{:?}", d.as_str())),
            game_lock(&self.shared).out.describe(),
            GAME_WAIT_MOST.load(Ordering::Relaxed) as f64 / 1000.0,
            game_waits().as_secs_f64()
        )
    }

    /// The sound device's uploads against the SPU2's RAM (`check_ram`), under
    /// the driver's lock.
    pub fn check_ram<M: vi::Model>(&self, dev: &snd::device::SoundDevice<M>) -> Option<snd::device::RamCheck> {
        let g = game_lock(&self.shared);
        dev.check_ram(g.drv.io.spu.ram(), g.drv.busy())
    }

    /// `EQOA_AUDIO_LOST_TEST=<ms>[,silent]`, natively: that long after the
    /// output opened, its device taken as gone (as WASAPI and Core Audio say
    /// it), or with `silent` the stream paused (as an ALSA device lost stops
    /// calling back).
    fn lost_test(&mut self) {
        static DONE: AtomicBool = AtomicBool::new(false);
        let (Some(w), Some(stream)) = (self.watch.as_ref(), self._stream.as_ref()) else { return };
        let Ok(spec) = std::env::var("EQOA_AUDIO_LOST_TEST") else { return };
        let (at, silent) = match spec.split_once(',') {
            Some((at, how)) => (at, how == "silent"),
            None => (spec.as_str(), false),
        };
        let Some(opened) = w.opened else { return };
        let Ok(at) = at.parse::<u64>() else { return };
        if opened.elapsed() < Duration::from_millis(at) || DONE.swap(true, Ordering::SeqCst) {
            return;
        }
        crate::say(&format!("audio: EQOA_AUDIO_LOST_TEST: the output {}", if silent { "paused" } else { "taken as gone" }));
        if silent {
            let _ = stream.pause();
        } else {
            w.lost.store(true, Ordering::Relaxed);
        }
    }

    /// A stream on the output device `want` names (Output Device: the
    /// system's default for none, or while the one named is not connected),
    /// SNDDRV mixed on its callback: the stream, its device (`On`) and the
    /// log's line. `lost` is set when the device says it is gone.
    fn stream(shared: Arc<Mutex<Shared>>, lost: Arc<AtomicBool>, want: Option<DeviceName>) -> Result<(cpal::Stream, On, String), String> {
        let host = cpal::default_host();
        let default_device = || host.default_output_device().ok_or("no output device");
        let (device, how) = match want {
            None => (default_device()?, How::Default),
            Some(w) => {
                let mut devices: Vec<cpal::Device> = host.output_devices().map(|ds| ds.collect()).unwrap_or_default();
                let names: Vec<String> = devices.iter().map(|d| d.name().unwrap_or_default()).collect();
                match find_chosen(w, &names) {
                    Some(k) => (devices.swap_remove(k), How::Chosen),
                    None => (default_device()?, How::Missing),
                }
            }
        };
        let name = device.name().unwrap_or_default();
        let default = device.default_output_config().map_err(|e| e.to_string())?;
        // 48 kHz stereo if the device takes it; otherwise its own rate,
        // resampled
        let takes_48k = device
            .supported_output_configs()
            .map_err(|e| e.to_string())?
            .any(|c| c.channels() == 2 && c.min_sample_rate().0 <= 48000 && 48000 <= c.max_sample_rate().0);
        let rate = if takes_48k { 48000 } else { default.sample_rate().0 };
        let channels = default.channels().max(1) as usize;
        let config = cpal::StreamConfig { channels: channels as u16, sample_rate: cpal::SampleRate(rate), buffer_size: cpal::BufferSize::Default };
        let step = 48000.0 / rate as f64;
        let mut phase = 0.0f64;
        // the output's gain (`set_heard`), faded over 10 ms of its frames
        let mut gain = 1.0f32;
        let fade = 1.0 / (rate as f32 * 0.01);
        // the device gone (WASAPI, Core Audio): the watch looks for another
        let err = move || {
            let lost = lost.clone();
            move |e: cpal::StreamError| {
                if matches!(e, cpal::StreamError::DeviceNotAvailable) {
                    lost.store(true, Ordering::Relaxed);
                }
                eprintln!("audio: {e}");
            }
        };
        let s = shared.clone();
        let mut fill = move |out: &mut dyn FnMut(usize, f32), frames: usize, at: cpal::StreamInstant| {
            let asked = Instant::now();
            let mut g = s.lock().unwrap();
            let locked = Instant::now();
            // the device takes the driver's frames from the wall clock
            // (`wall_clock`): what that mixed was dropped
            if g.wall.take().is_some() {
                g.pcm.clear();
            }
            g.called = Some(Instant::now());
            // enough 48 kHz frames for this buffer, and one past it for the
            // interpolation
            let need = ((phase + frames as f64 * step).ceil() as usize + 2).saturating_sub(g.pcm.len() / 2);
            g.pump(need as u64);
            let target = heard_gain();
            for k in 0..frames {
                let i = phase as usize;
                let t = (phase - i as f64) as f32;
                let gk = ramp(&mut gain, target, fade);
                for c in 0..channels {
                    let ch = c.min(1);
                    let a = g.pcm[2 * i + ch] as f32;
                    let b = g.pcm.get(2 * (i + 1) + ch).copied().unwrap_or(0) as f32;
                    out(k * channels + c, (a + (b - a) * t) / 32768.0 * gk);
                }
                phase += step;
            }
            let used = phase as usize;
            let n = 2 * used.min(g.pcm.len() / 2);
            g.pcm.drain(..n);
            phase -= used as f64;
            g.handed = Some((Instant::now(), g.drv.now - (g.pcm.len() / 2) as u64));
            let done = Instant::now();
            g.out.callback(at, frames, rate, locked - asked, done - locked, done);
        };
        let stream = match default.sample_format() {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &config,
                move |d: &mut [f32], info: &cpal::OutputCallbackInfo| {
                    let n = d.len() / channels;
                    fill(&mut |k, v| d[k] = v, n, info.timestamp().callback)
                },
                err(),
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream(
                &config,
                move |d: &mut [i16], info: &cpal::OutputCallbackInfo| {
                    let n = d.len() / channels;
                    fill(&mut |k, v| d[k] = (v * 32767.0) as i16, n, info.timestamp().callback)
                },
                err(),
                None,
            ),
            f => return Err(format!("sample format {f:?}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        let picked = match (how, want) {
            (How::Missing, Some(w)) => format!("{:?} is not connected: the system default meanwhile", w.as_str()),
            (How::Chosen, _) => "chosen".to_string(),
            _ => "the system default".to_string(),
        };
        let describe = format!("audio on {name} at {rate} Hz, {channels} channels ({picked})");
        Ok((stream, On { name, how, choice: want }, describe))
    }

    /// The driver with no output and no thread: `render` advances it.
    pub fn offline(disc: Box<dyn Disc>) -> Audio {
        let shared = Arc::new(Mutex::new(Shared::new(disc)));
        Self::with_shared(shared, None, "offline".into())
    }

    /// In a page: the driver on the game's thread, mixed once a frame for the
    /// page's AudioWorklet (`feed`); `fetch` starts the disc reads the music
    /// asked for (`web::bgm`).
    #[cfg(target_arch = "wasm32")]
    pub fn web(disc: Box<dyn Disc>, fetch: Box<dyn FnMut()>) -> Audio {
        let shared = Arc::new(Mutex::new(Shared::new(disc)));
        let mut a = Self::with_shared(shared, None, "the page's AudioWorklet".into());
        a.page = Some(Page { fetch, base: None, lead: LEAD_START, last: None, moves: Default::default(), dry: 0, wall: web_time::Instant::now(), gain: 1.0 });
        a
    }

    /// Once a frame (`Sound::frame`), and at each animation frame a capped
    /// page skips (`Sound::feed`). Natively the output watched
    /// (`watch_output`): the device's callback pulls. In a page: the music's
    /// disc reads started, then the driver
    /// mixed up to the page's audio clock (`web::audio_clock`, 48 kHz frames
    /// played) plus the lead (`lead_for`), and the frames handed to the
    /// AudioWorklet (`web::audio_push`). With no output running it keeps to
    /// the wall clock and its frames are dropped, as natively. A feed later
    /// than the lead (a stall, a hidden page) leaves the output dry for a
    /// moment: the driver is paused for it, so nothing is skipped.
    pub fn feed(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.lost_test();
            self.watch_output();
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(p) = self.page.as_mut() {
            (p.fetch)();
            let wall = p.wall.elapsed().as_secs_f64();
            let (now, live) = match crate::web::audio_clock() {
                Some(c) => (c, true),
                None => (wall * 48000.0, false),
            };
            // the clock's move since the last feed, into the lead's window
            match p.last.filter(|_| live) {
                Some(last) if now - last <= LEAD_MAX as f64 => p.moves.push_back((wall, (now - last).max(0.0) as u64)),
                _ => {}
            }
            p.last = live.then_some(now);
            while p.moves.front().is_some_and(|m| m.0 < wall - LEAD_WINDOW) {
                p.moves.pop_front();
            }
            p.lead = lead_for(p.moves.iter().map(|m| m.1));
            let mut g = self.shared.lock().unwrap();
            let drv = g.drv.now;
            let mut played = match p.base {
                Some((d, c, l)) if l == live => d + (now - c).max(0.0) as u64,
                _ => {
                    p.base = Some((drv, now, live));
                    drv
                }
            };
            if played > drv {
                p.base = Some((drv, now, live));
                played = drv;
                if live {
                    p.dry += 1;
                }
            }
            let target = played + if live { p.lead } else { 0 };
            let ahead = g.drv.now;
            if target > ahead {
                g.pump(target - ahead);
            }
            let mut pcm = std::mem::take(&mut g.pcm);
            drop(g);
            apply_gain(&mut pcm, &mut p.gain, heard_gain());
            if live && !pcm.is_empty() {
                crate::web::audio_push(&pcm);
            }
            PAGE_SOUND[0].store((p.lead / 48) as u32, std::sync::atomic::Ordering::Relaxed);
            PAGE_SOUND[1].store(p.dry, std::sync::atomic::Ordering::Relaxed);
        }
    }

}

/// In a page, for its status line (`web::rate`): the lead the sound is
/// mixed with (ms) and how often the output has run dry.
#[cfg(target_arch = "wasm32")]
pub static PAGE_SOUND: [std::sync::atomic::AtomicU32; 2] = [std::sync::atomic::AtomicU32::new(0), std::sync::atomic::AtomicU32::new(0)];

impl Audio {

    /// Offline: the next `frames` 48 kHz frames, interleaved.
    pub fn render(&self, frames: u64) -> Vec<i16> {
        let mut g = self.shared.lock().unwrap();
        g.pump(frames);
        std::mem::take(&mut g.pcm)
    }

    /// What SNDDRV says is playing: the status block's voices and BGM flag.
    pub fn status(&self) -> [u32; 128] {
        game_lock(&self.shared).status
    }

    pub fn link(&self) -> AudioLink {
        AudioLink(self.shared.clone())
    }
}

/// How long a DMA waits on the output's clock for a call still waiting
/// (`AudioLink::dma`): past a vblank's 16.7 ms, the clock is held.
#[cfg(not(target_arch = "wasm32"))]
const DMA_WAIT: Duration = Duration::from_millis(25);

/// The EE's side of SIF, to the driver on the audio thread.
pub struct AudioLink(Arc<Mutex<Shared>>);

impl Link for AudioLink {
    fn call(&mut self, fno: u32, packet: &[u32; 128]) -> [u32; 128] {
        let mut g = game_lock(&self.0);
        g.catch_up();
        #[cfg(not(target_arch = "wasm32"))]
        if g.drv.busy() && g.held() {
            g.run_on();
        }
        // the device refuses to flush while busy (`busy`): a call here
        // over a running one would be a packet lost
        if !g.drv.busy() {
            g.drv.begin_call(fno, packet);
            g.take_answer();
        }
        g.status
    }

    fn dma(&mut self, addr: u32, data: &[u8]) {
        // A call still waiting may yet copy out of the IOP buffer: let it
        // finish. It waits for a vblank, which the output's clock brings
        // within `DMA_WAIT`; past that the clock is held (its callback later
        // than `CATCH_UP_MAX`), and the call is run on to its vblank, the
        // frames kept for the output, as a page's is. It once wrote over the
        // buffer after 100 ms instead, and the waiting copy took the new
        // bytes (F3 20261025-225337).
        #[cfg(not(target_arch = "wasm32"))]
        {
            let start = Instant::now();
            loop {
                let mut g = game_lock(&self.0);
                g.catch_up();
                if g.drv.busy() && (g.held() || start.elapsed() > DMA_WAIT) {
                    g.run_on();
                }
                if !g.drv.busy() || start.elapsed() > DMA_WAIT {
                    g.drv.io.dma(addr, data);
                    return;
                }
                drop(g);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        // in a page the driver runs on this thread: on to the vblank the call
        // waits for (a second at most), the frames mixed kept for the output
        #[cfg(target_arch = "wasm32")]
        {
            let mut g = game_lock(&self.0);
            for _ in 0..100 {
                if !g.drv.busy() {
                    break;
                }
                g.pump(480);
            }
            g.drv.io.dma(addr, data);
        }
    }

    /// A call still running (`sceSifCheckStatRpc`): one waiting for a vblank
    /// the output's clock brings within one. With the clock held (no callback
    /// for `CATCH_UP_MAX`) it is run on instead: it would wait for as long as
    /// the output did, the device keeping its packets meanwhile and a
    /// `VISoundDevice::Flush` spinning on the game's thread, where the watch
    /// that would put the driver on the wall clock runs.
    fn busy(&mut self) -> bool {
        let mut g = game_lock(&self.0);
        g.catch_up();
        #[cfg(not(target_arch = "wasm32"))]
        if g.drv.busy() && g.held() {
            g.run_on();
        }
        g.drv.busy()
    }

    fn bgm_ready(&mut self, path: &str) -> bool {
        game_lock(&self.0).drv.disc_ready(path)
    }
}

/// The output's callback in a test, on an offline `Audio`.
#[cfg(test)]
pub mod testing {
    use super::*;

    /// A callback taking `frames` 48 kHz frames, as the device's does (the
    /// driver mixing what it lacks, two kept for the interpolation), what it
    /// took appended to `out`; with `anchor`, the catch-up's anchor set as
    /// the device's callback sets it (without, the driver keeps the
    /// callbacks' clock alone, as before the catch-up).
    pub fn callback(a: &Audio, frames: usize, anchor: bool, out: &mut Vec<i16>) {
        let mut g = a.shared.lock().unwrap();
        let need = (frames + 2).saturating_sub(g.pcm.len() / 2);
        g.pump(need as u64);
        let n = 2 * frames.min(g.pcm.len() / 2);
        out.extend(g.pcm.drain(..n));
        if anchor {
            g.handed = Some((Instant::now(), g.drv.now - (g.pcm.len() / 2) as u64));
        }
    }

    /// `ms` since the last callback, as the catch-up reads the time: its
    /// anchor moved that far back from now.
    pub fn since_callback(a: &Audio, ms: f64) {
        let mut g = a.shared.lock().unwrap();
        if let Some((_, h)) = g.handed {
            g.handed = Some((Instant::now() - Duration::from_secs_f64(ms / 1000.0), h));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sound in Background: a window not heard fades to nothing over 10 ms
    /// of frames and stays silent, then fades back the same way; no frame
    /// moves the gain more than a step (no click). The controls: heard all
    /// along, the samples are untouched; and the frames past the fade are
    /// all silent, not merely quieter.
    #[test]
    fn a_window_not_heard_fades_out_and_back() {
        let tone = |n: usize| vec![12000i16; 2 * n];
        let mut gain = 1.0f32;
        let mut pcm = tone(960);
        apply_gain(&mut pcm, &mut gain, 1.0);
        assert_eq!(pcm, tone(960), "heard: untouched");
        apply_gain(&mut pcm, &mut gain, 0.0);
        assert_eq!(gain, 0.0);
        assert!(pcm[..2 * 479].iter().all(|&v| v > 0), "fading over 10 ms, not cut");
        assert!(pcm[2 * 480..].iter().all(|&v| v == 0), "then silent");
        let steps = |p: &[i16]| p.chunks(2).zip(p.chunks(2).skip(1)).map(|(a, b)| (a[0] as i32 - b[0] as i32).abs()).max().unwrap_or(0);
        assert!(steps(&pcm) <= (12000.0 * FADE_48K) as i32 + 1, "no click: {}", steps(&pcm));
        let mut back = tone(960);
        apply_gain(&mut back, &mut gain, 1.0);
        assert_eq!(gain, 1.0);
        assert!(back[0] < 100 && back[2 * 480..].iter().all(|&v| v == 12000), "back over 10 ms");
        assert!(steps(&back) <= (12000.0 * FADE_48K) as i32 + 1);
        // the flag: what `set_heard` says is the gain wanted
        set_heard(false);
        assert_eq!(heard_gain(), 0.0);
        set_heard(true);
        assert_eq!(heard_gain(), 1.0);
    }

    /// Each call first mixes up to the output's present: what was last
    /// handed to the device and the time since (at most `CATCH_UP_MAX` past
    /// it, a stalled device), and nothing with no callback yet (offline).
    #[test]
    fn a_call_finds_the_driver_at_the_outputs_present() {
        let a = Audio::offline(Box::new(snd::NoDisc));
        let now = |a: &Audio| a.shared.lock().unwrap().drv.now;
        let mut link = a.link();
        link.busy();
        assert_eq!(now(&a), 0, "offline: the driver waits for `render`");
        let mut out = Vec::new();
        testing::callback(&a, 1200, true, &mut out);
        assert_eq!((out.len(), now(&a)), (2400, 1202), "a block taken, two kept");
        testing::since_callback(&a, 20.0);
        link.busy();
        let n = now(&a);
        assert!((2160..2170).contains(&n), "20 ms on: 960 frames past what was handed ({n})");
        // control: a device stalled a second does not run the driver a second ahead
        testing::since_callback(&a, 1000.0);
        link.busy();
        assert_eq!(now(&a), 1200 + CATCH_UP_MAX);
    }

    /// The lead is the third largest move between feeds, half of it again and
    /// a margin, within its bounds; with no move yet, the start's. The
    /// controls: two stalls do not raise it, a third does; the old rule's
    /// floor would hold a steady 60 fps page at 100 ms.
    #[test]
    fn the_lead_follows_the_clocks_moves() {
        assert_eq!(lead_for(std::iter::empty()), LEAD_START);
        // Chromium at 60 fps: steps of 512, two now and then
        assert_eq!(lead_for([512, 0, 1024, 512, 1024, 512, 1024].into_iter()), 1024 + 512 + LEAD_MARGIN);
        // Firefox: 1,440-frame steps, two in a slow frame
        assert_eq!(lead_for([1440, 0, 2880, 1440, 2880, 2880].into_iter()), 2880 + 1440 + LEAD_MARGIN);
        // two stalls of 200 ms run dry; a third raises the lead
        let steady = [512, 1024, 512, 1024, 1024];
        assert_eq!(lead_for(steady.into_iter().chain([9600, 9600])), lead_for(steady.into_iter()));
        assert_eq!(lead_for(steady.into_iter().chain([9600, 9600, 9600])), LEAD_MAX);
        // one or two moves: the smaller
        assert_eq!(lead_for([2000, 1000].into_iter()), 1500 + LEAD_MARGIN);
        assert_eq!(lead_for([0, 0, 0].into_iter()), LEAD_MIN);
        assert!(lead_for(steady.into_iter()) < 4800, "below the old 100 ms floor");
    }

    fn ms(t: Instant, n: u64) -> Instant {
        t + Duration::from_millis(n)
    }

    /// A device that says it is gone takes its stream: dropped at once, an
    /// output looked for in the same frame, and the one found said to be
    /// back at its first callback, not before. The control: a stream that
    /// calls back every 30 ms stays, for a minute of frames.
    #[test]
    fn a_device_gone_is_left_for_another() {
        let t = Instant::now();
        let mut w = Watch::new(t, true);
        for n in 0..2000 {
            assert_eq!(w.step(ms(t, 30 * n + 10), Some(ms(t, 30 * n))), Step::Keep, "playing at {n}");
        }
        let now = ms(t, 60_100);
        w.lost.store(true, Ordering::Relaxed);
        assert_eq!(w.step(now, Some(ms(t, 60_090))), Step::Stopped("its device is gone"));
        assert_eq!(w.step(now, Some(ms(t, 60_090))), Step::Look, "looked for at once");
        w.open(now);
        assert_eq!(w.step(ms(now, 20), Some(ms(t, 60_090))), Step::Keep, "the old stream's callback is not this one's");
        assert_eq!(w.step(ms(now, 40), Some(ms(now, 30))), Step::Plays(true), "back");
        assert_eq!(w.step(ms(now, 70), Some(ms(now, 60))), Step::Keep, "said once");
    }

    /// A stream that stops calling back (ALSA's lost device) is gone after
    /// `STALL`, not a frame sooner (the control); its device may not say so.
    #[test]
    fn a_stream_that_stops_calling_back_is_gone() {
        let t = Instant::now();
        let mut w = Watch::new(t, true);
        let last = Some(ms(t, 500));
        assert_eq!(w.step(ms(t, 2499), last), Step::Keep);
        assert_eq!(w.step(ms(t, 2500), last), Step::Stopped("it stopped asking for sound"));
        // nor is a fresh stream's silence before its first callback held
        // against it sooner
        let now = ms(t, 2500);
        w.step(now, last);
        w.open(now);
        assert_eq!(w.step(ms(now, 1999), last), Step::Keep);
        assert_eq!(w.step(ms(now, 2000), last), Step::Silent, "it never played: a look that failed, not a stop");
    }

    /// No output at the start: looked for after a second, then after waits
    /// doubling to half a minute, each failure said once; a stream that opens
    /// and never plays fails the same way; the one that plays is "found".
    #[test]
    fn looks_for_an_output_back_off() {
        let t = Instant::now();
        let mut w = Watch::new(t, false);
        assert_eq!(w.step(ms(t, 999), None), Step::Keep);
        let mut now = ms(t, 1000);
        let mut waits = Vec::new();
        for n in 0..8 {
            assert_eq!(w.step(now, None), Step::Look);
            let said = w.failed(now, "no output device");
            assert_eq!(said.is_some(), n == 0, "said once ({n})");
            let next = w.retry.0;
            waits.push((next - now).as_secs());
            assert_eq!(w.step(next - Duration::from_millis(1), None), Step::Keep, "not before its time");
            now = next;
        }
        // the start's own failure was the first: a second, then these
        assert_eq!(waits, [2, 4, 8, 16, 30, 30, 30, 30]);
        // a new reason is said too
        assert!(w.failed(now, "the device refused 48 kHz").is_some());
        // a stream that opens but never calls back
        let now = w.retry.0;
        assert_eq!(w.step(now, None), Step::Look);
        w.open(now);
        assert_eq!(w.step(ms(now, 2000), None), Step::Silent);
        assert!(w.failed(ms(now, 2000), "an output opened, but asked for no sound").is_some());
        // then one that plays: found, never having played
        let now = w.retry.0;
        assert_eq!(w.step(now, None), Step::Look);
        w.open(now);
        assert_eq!(w.step(ms(now, 50), Some(ms(now, 40))), Step::Plays(false));
        // and its loss looks again at once, the waits from the start
        w.lost.store(true, Ordering::Relaxed);
        assert_eq!(w.step(ms(now, 90), Some(ms(now, 80))), Step::Stopped("its device is gone"));
        assert_eq!(w.retry, (ms(now, 90), RETRY_FIRST));
    }

    fn on(name: &str, how: How, choice: Option<&str>) -> On {
        On { name: name.into(), how, choice: choice.map(DeviceName::new) }
    }

    /// Output Device's System Default follows the system's default: the
    /// stream opened again when the default is another device, not while it
    /// is the same one or not read yet. A device chosen stays whatever the
    /// default does (the control: a rule that follows the default for every
    /// stream fails here, and one that never follows fails the first half).
    #[test]
    fn the_system_default_is_followed_and_a_device_chosen_is_kept() {
        let tv = on("LG TV SSCR2 (NVIDIA High Definition Audio)", How::Default, None);
        assert_eq!(retarget(None, &tv, Some("LG TV SSCR2 (NVIDIA High Definition Audio)"), None), Retarget::Stay);
        assert_eq!(retarget(None, &tv, None, None), Retarget::Stay, "the default not read yet");
        assert_eq!(retarget(None, &tv, Some("Speakers (Realtek(R) Audio)"), None), Retarget::Open("the system default changed"));
        let chosen = on("Speakers (Realtek(R) Audio)", How::Chosen, Some("Speakers (Realtek(R) Audio)"));
        let want = Some(DeviceName::new("Speakers (Realtek(R) Audio)"));
        assert_eq!(retarget(want, &chosen, Some("LG TV SSCR2 (NVIDIA High Definition Audio)"), None), Retarget::Stay, "on the device chosen whatever the default");
        assert_eq!(retarget(want, &chosen, Some("Speakers (Realtek(R) Audio)"), Some(&[])), Retarget::Stay, "nor does a listing without it move it: its loss is the watch's");
    }

    /// A device chosen but not connected: the default meanwhile (followed
    /// as the default is), and the device chosen again once it is listed.
    /// The control: the same listing without it keeps the default.
    #[test]
    fn a_device_chosen_comes_back_when_it_is_connected() {
        let h = "Headphones (2- Arctis 7 Game)";
        let missing = on("LG TV SSCR2 (NVIDIA High Definition Audio)", How::Missing, Some(h));
        let want = Some(DeviceName::new(h));
        let without = ["LG TV SSCR2 (NVIDIA High Definition Audio)".to_string()];
        let with = ["LG TV SSCR2 (NVIDIA High Definition Audio)".to_string(), h.to_string()];
        assert_eq!(retarget(want, &missing, Some(&without[0]), Some(&without)), Retarget::Stay);
        assert_eq!(retarget(want, &missing, Some(&without[0]), None), Retarget::Stay, "not listed yet");
        assert_eq!(retarget(want, &missing, Some(&without[0]), Some(&with)), Retarget::Open("the device chosen is connected again"));
        assert_eq!(retarget(want, &missing, Some("Speakers (Realtek(R) Audio)"), Some(&without)), Retarget::Open("the system default changed"));
    }

    /// The choice moved in the options opens the stream again at once,
    /// either way; the same choice does not (the control).
    #[test]
    fn a_new_choice_opens_again() {
        let tv = on("LG TV", How::Default, None);
        let h = Some(DeviceName::new("Headphones"));
        assert_eq!(retarget(h, &tv, Some("LG TV"), None), Retarget::Open("the device chosen"));
        let chosen = on("Headphones", How::Chosen, Some("Headphones"));
        assert_eq!(retarget(None, &chosen, Some("LG TV"), None), Retarget::Open("the system default chosen"));
        assert_eq!(retarget(h, &chosen, Some("LG TV"), None), Retarget::Stay);
    }

    /// The device chosen is found among the system's by its whole name, a
    /// long one by the cut Output Device keeps; another device's name, or
    /// part of it, is not it (the controls).
    #[test]
    fn the_device_chosen_is_found_by_its_name() {
        let names = ["Speakers (Realtek(R) Audio)".to_string(), "LG TV SSCR2 (NVIDIA High Definition Audio)".to_string()];
        assert_eq!(find_chosen(DeviceName::new("LG TV SSCR2 (NVIDIA High Definition Audio)"), &names), Some(1));
        assert_eq!(find_chosen(DeviceName::new("Speakers (Realtek(R) Audio)"), &names), Some(0));
        assert_eq!(find_chosen(DeviceName::new("LG TV SSCR2"), &names), None, "part of a name");
        assert_eq!(find_chosen(DeviceName::new("Headphones"), &names), None);
        let long = format!("Lautsprecher ({})", "x".repeat(300));
        assert_eq!(find_chosen(DeviceName::new(&long), &[long.clone()]), Some(0), "by its cut");
    }

    /// A packet: its commands as (id, args), as the device ships one.
    fn packet(cmds: &[(u32, &[u32])]) -> [u32; 128] {
        let mut p = [0u32; 128];
        p[0] = cmds.len() as u32;
        let mut k = 1;
        for (id, args) in cmds {
            p[k] = *id;
            p[k + 1] = args.len() as u32;
            p[k + 2..k + 2 + args.len()].copy_from_slice(args);
            k += 2 + args.len();
        }
        p
    }

    /// A DMA into the IOP's buffer while a call still waits for its vblank
    /// with a copy out of that buffer queued behind it (F3 20261025-225337):
    /// with the output's clock held (here: no output at all) the call is run
    /// on first, so the copy takes the bytes that were there, not the new
    /// ones. It once wrote over the buffer after 100 ms and the copy took the
    /// new bytes. The control: with the call not waiting, the DMA lands at
    /// once and runs nothing on.
    #[test]
    fn a_dma_never_writes_under_a_waiting_copy() {
        for waiting in [false, true] {
            let a = Audio::offline(Box::new(snd::NoDisc));
            let mut link = a.link();
            link.call(0x80f0, &packet(&[(0, &[])]));
            let buf = a.shared.lock().unwrap().drv.io.buf_addr;
            let (old, new) = (vec![0x11u8; 64], vec![0x99u8; 64]);
            link.dma(buf, &old);
            // a BGM stop, which waits for the BGM thread's vblank, then the
            // copy of the buffer to SPU RAM 0x10000
            let first: &[(u32, &[u32])] = if waiting { &[(14, &[0])] } else { &[] };
            let cmds: Vec<(u32, &[u32])> = first.iter().copied().chain([(1, &[64u32, 0x10000][..])]).collect();
            link.call(0x80f0, &packet(&cmds));
            assert_eq!(a.shared.lock().unwrap().drv.busy(), waiting, "the call waits ({waiting})");
            link.dma(buf, &new);
            a.render(1600);
            let g = a.shared.lock().unwrap();
            assert!(!g.drv.busy(), "the call answered ({waiting})");
            assert_eq!(&g.drv.io.spu.ram()[0x10000..0x10040], &old[..], "the copy took what it was queued for ({waiting})");
            assert_eq!(&g.drv.io.buf[..64], &new[..], "and the DMA landed after it ({waiting})");
            assert_eq!(g.out.ran_on, waiting as u64, "run on only for a waiting call");
        }
    }

    /// A call waiting for its vblank while the output's clock is held (its
    /// last callback 200 ms ago, past `CATCH_UP_MAX`): `busy` runs it on, so
    /// the device's packets and a `Flush` go on. The control: with the clock
    /// live (5 ms since the callback) nothing is run on.
    #[test]
    fn a_held_clock_does_not_keep_a_call_waiting() {
        for held in [false, true] {
            let a = Audio::offline(Box::new(snd::NoDisc));
            let mut link = a.link();
            link.call(0x80f0, &packet(&[(0, &[])]));
            let mut out = Vec::new();
            testing::callback(&a, 512, true, &mut out);
            testing::since_callback(&a, if held { 200.0 } else { 5.0 });
            link.busy();
            link.call(0x80f0, &packet(&[(14, &[0])]));
            let busy = link.busy();
            let g = a.shared.lock().unwrap();
            assert_eq!(g.out.ran_on, held as u64, "run on only with the clock held ({held})");
            if held {
                assert!(!busy, "the stop answered");
            }
        }
    }

    /// The output's record: a callback that starts later than the last
    /// one's end by more than half a buffer is a gap; one whose wait and mix
    /// outlast its buffer is late. The controls: callbacks back to back, and
    /// one a little late (scheduling jitter), are neither.
    #[test]
    fn the_output_counts_gaps_and_late_callbacks() {
        let mut o = Out::default();
        let t0 = cpal::StreamInstant::new(1000, 0);
        let now = Instant::now();
        let ms = Duration::from_millis;
        let fast = (ms(0), ms(1));
        // 512 frames at 48 kHz last 10.67 ms
        let mut at = t0;
        for _ in 0..100 {
            o.callback(at, 512, 48000, fast.0, fast.1, now);
            at = at.add(Duration::from_micros(10_667)).unwrap();
        }
        assert_eq!((o.gaps, o.late), (0, 0), "back to back");
        at = at.add(ms(3)).unwrap();
        o.callback(at, 512, 48000, fast.0, fast.1, now);
        assert_eq!(o.gaps, 0, "a jitter of 3 ms is no gap");
        // a skipped cycle: the next callback a whole buffer later
        at = at.add(Duration::from_micros(10_667 * 2)).unwrap();
        o.callback(at, 512, 48000, fast.0, fast.1, now);
        assert_eq!(o.gaps, 1, "a buffer missing");
        let (_, missing) = o.last_gap.unwrap();
        assert!((10.0..11.0).contains(&(missing.as_secs_f64() * 1000.0)), "{missing:?}");
        // a callback that waited 6 ms for the lock and mixed for 6
        at = at.add(Duration::from_micros(10_667)).unwrap();
        o.callback(at, 512, 48000, ms(6), ms(6), now);
        assert_eq!((o.gaps, o.late), (1, 1), "late, not a gap yet");
        assert_eq!((o.wait_max, o.work_max), (ms(6), ms(6)));
        assert!(o.describe().contains("1 gaps"), "{}", o.describe());
    }

    /// While no device takes the frames the driver keeps to the wall clock,
    /// from where it was; the control: with the wall clock off it waits.
    #[test]
    fn with_no_output_the_driver_keeps_the_wall_clock() {
        let a = Audio::offline(Box::new(snd::NoDisc));
        let now = |a: &Audio| a.shared.lock().unwrap().drv.now;
        a.render(480);
        wall_clock(a.shared.clone());
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(now(&a), 480, "off: the driver waits");
        a.shared.lock().unwrap().wall_from(Instant::now());
        std::thread::sleep(Duration::from_millis(200));
        let n = now(&a);
        assert!((480 + 4800..480 + 14400).contains(&n), "about 200 ms of 48 kHz frames on the wall clock ({n})");
        assert!(a.shared.lock().unwrap().pcm.is_empty(), "dropped, not kept");
    }
}
