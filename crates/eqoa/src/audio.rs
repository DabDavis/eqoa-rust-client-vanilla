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
}

/// How far `catch_up` mixes past what was last handed to the device (100
/// ms): while a device stalls (its callback later than this) the IOP's clock
/// waits there, since what is mixed ahead is heard only once the device has
/// taken it, and a sound played meanwhile would come that much late.
const CATCH_UP_MAX: u64 = 4800;

impl Shared {
    fn new(disc: Box<dyn Disc>) -> Self {
        Shared { drv: Driver::new(disc), status: [0; 128], pcm: Vec::new(), handed: None, called: None, wall: None }
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
        let due = handed + ((at.elapsed().as_secs_f64() * 48000.0) as u64).min(CATCH_UP_MAX);
        if due > self.drv.now {
            self.pump(due - self.drv.now);
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
            g.pcm.clear();
            g.handed = Some((Instant::now(), g.drv.now));
        }
    });
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

pub struct Audio {
    shared: Arc<Mutex<Shared>>,
    _stream: Option<cpal::Stream>,
    pub describe: String,
    /// natively, the device output watched (`Watch`); none offline, in a
    /// page, or with `EQOA_AUDIO=0`
    watch: Option<Watch>,
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
            #[cfg(target_arch = "wasm32")]
            page: None,
        }
    }

    /// The driver, playing on the default output device, watched (`Watch`);
    /// with none, on the wall clock while one is looked for, and with
    /// `EQOA_AUDIO=0` on the wall clock for good.
    pub fn open(disc: Box<dyn Disc>) -> Audio {
        let shared = Arc::new(Mutex::new(Shared::new(disc)));
        let now = Instant::now();
        if std::env::var("EQOA_AUDIO").is_ok_and(|v| v == "0") {
            shared.lock().unwrap().wall_from(now);
            wall_clock(shared.clone());
            return Self::with_shared(shared, None, "no audio output (EQOA_AUDIO=0): the driver runs on the wall clock".into());
        }
        let lost = Arc::new(AtomicBool::new(false));
        let (stream, describe, mut watch) = match Self::stream(shared.clone(), lost.clone()) {
            Ok((stream, describe)) => (Some(stream), describe, Watch::new(now, true)),
            Err(e) => {
                shared.lock().unwrap().wall_from(now);
                wall_clock(shared.clone());
                let mut w = Watch::new(now, false);
                w.walling = true;
                (None, format!("no audio output ({e}): the driver runs on the wall clock, and an output is looked for now and then"), w)
            }
        };
        watch.lost = lost;
        let mut a = Self::with_shared(shared, stream, describe);
        a.watch = Some(watch);
        a
    }

    /// Natively, the output watched once a frame (`Watch`): a stream gone
    /// dropped and the driver put on the wall clock, an output looked for
    /// when one is due, and the log told when it stops, fails or plays.
    fn watch_output(&mut self) {
        let Some(w) = self.watch.as_mut() else { return };
        let now = Instant::now();
        let called = self.shared.lock().unwrap().called;
        match w.step(now, called) {
            Step::Keep => {}
            Step::Plays(again) => {
                crate::say(&format!("{}: {}", self.describe, if again { "the sound has its output again" } else { "an output found" }));
            }
            Step::Stopped(why) => {
                self._stream = None;
                self.shared.lock().unwrap().wall_from(now);
                if !std::mem::replace(&mut w.walling, true) {
                    wall_clock(self.shared.clone());
                }
                crate::say(&format!("audio: the output stopped ({why}): the game's sound goes on unheard while an output is looked for"));
            }
            Step::Silent => {
                self._stream = None;
                if let Some(line) = w.failed(now, "an output opened, but asked for no sound") {
                    crate::say(&line);
                }
            }
            Step::Look => match Self::stream(self.shared.clone(), w.lost.clone()) {
                Ok((stream, describe)) => {
                    self._stream = Some(stream);
                    self.describe = describe;
                    w.open(now);
                }
                Err(e) => {
                    if let Some(line) = w.failed(now, &e) {
                        crate::say(&line);
                    }
                }
            },
        }
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

    /// A stream on the default output device: SNDDRV mixed on its callback.
    /// `lost` is set when the device says it is gone.
    fn stream(shared: Arc<Mutex<Shared>>, lost: Arc<AtomicBool>) -> Result<(cpal::Stream, String), String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no output device")?;
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
        let mut fill = move |out: &mut dyn FnMut(usize, f32), frames: usize| {
            let mut g = s.lock().unwrap();
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
        };
        let stream = match default.sample_format() {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &config,
                move |d: &mut [f32], _| {
                    let n = d.len() / channels;
                    fill(&mut |k, v| d[k] = v, n)
                },
                err(),
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream(
                &config,
                move |d: &mut [i16], _| {
                    let n = d.len() / channels;
                    fill(&mut |k, v| d[k] = (v * 32767.0) as i16, n)
                },
                err(),
                None,
            ),
            f => return Err(format!("sample format {f:?}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok((stream, format!("audio on {name} at {rate} Hz, {channels} channels")))
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
        self.shared.lock().unwrap().status
    }

    pub fn link(&self) -> AudioLink {
        AudioLink(self.shared.clone())
    }
}

/// The EE's side of SIF, to the driver on the audio thread.
pub struct AudioLink(Arc<Mutex<Shared>>);

impl Link for AudioLink {
    fn call(&mut self, fno: u32, packet: &[u32; 128]) -> [u32; 128] {
        let mut g = self.0.lock().unwrap();
        g.catch_up();
        // the device refuses to flush while busy (`busy`): a call here
        // over a running one would be a packet lost
        if !g.drv.busy() {
            g.drv.begin_call(fno, packet);
            g.take_answer();
        }
        g.status
    }

    fn dma(&mut self, addr: u32, data: &[u8]) {
        // a call still waiting may yet read the IOP buffer: let it finish
        #[cfg(not(target_arch = "wasm32"))]
        {
            let start = Instant::now();
            loop {
                let mut g = self.0.lock().unwrap();
                g.catch_up();
                if !g.drv.busy() || start.elapsed() > Duration::from_millis(100) {
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
            let mut g = self.0.lock().unwrap();
            for _ in 0..100 {
                if !g.drv.busy() {
                    break;
                }
                g.pump(480);
            }
            g.drv.io.dma(addr, data);
        }
    }

    fn busy(&mut self) -> bool {
        let mut g = self.0.lock().unwrap();
        g.catch_up();
        g.drv.busy()
    }

    fn bgm_ready(&mut self, path: &str) -> bool {
        self.0.lock().unwrap().drv.disc_ready(path)
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
