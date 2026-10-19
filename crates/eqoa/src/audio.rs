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
}

impl Audio {
    /// The driver, playing on the default output device.
    pub fn open(disc: Box<dyn Disc>) -> Audio {
        let shared = Arc::new(Mutex::new(Shared { drv: Driver::new(disc), status: [0; 128], pcm: Vec::new() }));
        let off = std::env::var("EQOA_AUDIO").is_ok_and(|v| v == "0");
        let stream = if off { Err("EQOA_AUDIO=0".to_string()) } else { Self::stream(shared.clone()) };
        match stream {
            Ok((stream, describe)) => Audio { shared, _stream: Some(stream), describe },
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
                Audio { shared, _stream: None, describe: format!("no audio output ({e}): the driver runs on the wall clock") }
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
        Audio { shared, _stream: None, describe: "offline".into() }
    }

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

    fn busy(&mut self) -> bool {
        self.0.lock().unwrap().drv.busy()
    }
}
