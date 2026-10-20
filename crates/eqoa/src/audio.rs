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
//! on the wall clock instead, so the game sees the same IOP either way.
//!
//! In a page there is no thread to give it: the driver runs on the game's,
//! mixing a little ahead of the page's audio clock once a frame (`feed`) for
//! an AudioWorklet that only plays what it is handed (web/sound.js).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use snd::device::Link;
use snd::{Disc, Driver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
}

impl Shared {
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

pub struct Audio {
    shared: Arc<Mutex<Shared>>,
    _stream: Option<cpal::Stream>,
    pub describe: String,
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
}

impl Audio {
    fn with_shared(shared: Arc<Mutex<Shared>>, stream: Option<cpal::Stream>, describe: String) -> Audio {
        Audio {
            shared,
            _stream: stream,
            describe,
            #[cfg(target_arch = "wasm32")]
            page: None,
        }
    }

    /// The driver, playing on the default output device.
    pub fn open(disc: Box<dyn Disc>) -> Audio {
        let shared = Arc::new(Mutex::new(Shared { drv: Driver::new(disc), status: [0; 128], pcm: Vec::new() }));
        let off = std::env::var("EQOA_AUDIO").is_ok_and(|v| v == "0");
        let stream = if off { Err("EQOA_AUDIO=0".to_string()) } else { Self::stream(shared.clone()) };
        match stream {
            Ok((stream, describe)) => Self::with_shared(shared, Some(stream), describe),
            Err(e) => {
                let s = shared.clone();
                std::thread::spawn(move || {
                    // the wall clock's samples, thrown away
                    let start = Instant::now();
                    loop {
                        std::thread::sleep(Duration::from_millis(10));
                        let mut g = s.lock().unwrap();
                        let due = (start.elapsed().as_secs_f64() * 48000.0) as u64;
                        let n = due.saturating_sub(g.drv.now);
                        g.pump(n);
                        g.pcm.clear();
                    }
                });
                Self::with_shared(shared, None, format!("no audio output ({e}): the driver runs on the wall clock"))
            }
        }
    }

    fn stream(shared: Arc<Mutex<Shared>>) -> Result<(cpal::Stream, String), String> {
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
        let err = |e| eprintln!("audio: {e}");
        let s = shared.clone();
        let mut fill = move |out: &mut dyn FnMut(usize, f32), frames: usize| {
            let mut g = s.lock().unwrap();
            // enough 48 kHz frames for this buffer, and one past it for the
            // interpolation
            let need = ((phase + frames as f64 * step).ceil() as usize + 2).saturating_sub(g.pcm.len() / 2);
            g.pump(need as u64);
            for k in 0..frames {
                let i = phase as usize;
                let t = (phase - i as f64) as f32;
                for c in 0..channels {
                    let ch = c.min(1);
                    let a = g.pcm[2 * i + ch] as f32;
                    let b = g.pcm.get(2 * (i + 1) + ch).copied().unwrap_or(0) as f32;
                    out(k * channels + c, (a + (b - a) * t) / 32768.0);
                }
                phase += step;
            }
            let used = phase as usize;
            let n = 2 * used.min(g.pcm.len() / 2);
            g.pcm.drain(..n);
            phase -= used as f64;
        };
        let stream = match default.sample_format() {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &config,
                move |d: &mut [f32], _| {
                    let n = d.len() / channels;
                    fill(&mut |k, v| d[k] = v, n)
                },
                err,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream(
                &config,
                move |d: &mut [i16], _| {
                    let n = d.len() / channels;
                    fill(&mut |k, v| d[k] = (v * 32767.0) as i16, n)
                },
                err,
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
        let shared = Arc::new(Mutex::new(Shared { drv: Driver::new(disc), status: [0; 128], pcm: Vec::new() }));
        Self::with_shared(shared, None, "offline".into())
    }

    /// In a page: the driver on the game's thread, mixed once a frame for the
    /// page's AudioWorklet (`feed`); `fetch` starts the disc reads the music
    /// asked for (`web::bgm`).
    #[cfg(target_arch = "wasm32")]
    pub fn web(disc: Box<dyn Disc>, fetch: Box<dyn FnMut()>) -> Audio {
        let shared = Arc::new(Mutex::new(Shared { drv: Driver::new(disc), status: [0; 128], pcm: Vec::new() }));
        let mut a = Self::with_shared(shared, None, "the page's AudioWorklet".into());
        a.page = Some(Page { fetch, base: None, lead: LEAD_START, last: None, moves: Default::default(), dry: 0, wall: web_time::Instant::now() });
        a
    }

    /// Once a frame (`Sound::frame`), and at each animation frame a capped
    /// page skips (`Sound::feed`). Natively nothing: the device's callback
    /// pulls. In a page: the music's disc reads started, then the driver
    /// mixed up to the page's audio clock (`web::audio_clock`, 48 kHz frames
    /// played) plus the lead (`lead_for`), and the frames handed to the
    /// AudioWorklet (`web::audio_push`). With no output running it keeps to
    /// the wall clock and its frames are dropped, as natively. A feed later
    /// than the lead (a stall, a hidden page) leaves the output dry for a
    /// moment: the driver is paused for it, so nothing is skipped.
    pub fn feed(&mut self) {
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
            let pcm = std::mem::take(&mut g.pcm);
            drop(g);
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
        self.0.lock().unwrap().drv.busy()
    }

    fn bgm_ready(&mut self, path: &str) -> bool {
        self.0.lock().unwrap().drv.disc_ready(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
