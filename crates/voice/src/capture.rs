//! The microphone: the default input device through cpal, downmixed to mono and resampled to
//! 48 kHz into a short ring the encoder takes 20 ms frames from. The stream lives on a thread
//! of its own (a cpal stream is not `Send` everywhere), which keeps it until `stop`. No
//! microphone (or one not allowed) is told once, through `denied`; the engine then receives
//! only, its level 0.

use crate::jitter::{FRAME, RATE};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Captured samples, 48 kHz mono, waiting for the encoder.
#[derive(Default)]
pub(crate) struct Mic {
    ring: Mutex<VecDeque<f32>>,
}

impl Mic {
    /// Samples in; past a second, the oldest go.
    pub fn push(&self, samples: &[f32]) {
        let mut ring = self.ring.lock().unwrap();
        ring.extend(samples);
        let over = ring.len().saturating_sub(RATE as usize);
        ring.drain(..over);
    }

    /// One 20 ms frame into `out`, if one is in. A backlog past 100 ms (the device's clock
    /// ahead of the encoder's) is cut to 40 ms, so the delay stays short.
    pub fn take_frame(&self, out: &mut [f32]) -> bool {
        let mut ring = self.ring.lock().unwrap();
        if ring.len() < FRAME {
            return false;
        }
        if ring.len() > 5 * FRAME {
            let cut = ring.len() - 2 * FRAME;
            ring.drain(..cut);
        }
        for (o, s) in out.iter_mut().zip(ring.drain(..FRAME)) {
            *o = s;
        }
        true
    }
}

/// Linear resampling of one channel to 48 kHz, across callbacks.
pub(crate) struct Resampler {
    step: f64,
    pos: f64,
    prev: f32,
}

impl Resampler {
    pub fn new(from: u32) -> Resampler {
        Resampler { step: from as f64 / RATE as f64, pos: 0.0, prev: 0.0 }
    }

    pub fn run(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if input.is_empty() {
            return;
        }
        // positions count from `prev` (index 0), the input following it
        let len = input.len() + 1;
        let at = |i: usize| if i == 0 { self.prev } else { input[i - 1] };
        while self.pos + 1.0 < len as f64 {
            let i = self.pos as usize;
            let t = (self.pos - i as f64) as f32;
            out.push(at(i) + (at(i + 1) - at(i)) * t);
            self.pos += self.step;
        }
        self.pos -= (len - 1) as f64;
        self.prev = input[input.len() - 1];
    }
}

/// The capture thread, kept until `stop` (or drop), which only tells it to end: the thread
/// closes the stream itself within 50 ms, so the game never waits on a device.
pub(crate) struct Capture {
    stop: Arc<AtomicBool>,
}

impl Capture {
    /// The default input into `mic`; `denied` hears why, when it cannot be opened.
    pub fn start(mic: Arc<Mic>, denied: impl FnOnce(String) + Send + 'static) -> Capture {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let _ = std::thread::Builder::new()
            .name("voice-mic".into())
            .spawn(move || {
                let stream = match open(mic) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("voice: no microphone ({e}): receiving only");
                        denied(e);
                        return;
                    }
                };
                while !flag.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                drop(stream);
            });
        Capture { stop }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop();
    }
}

fn open(mic: Arc<Mic>) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("none on this system")?;
    let config = device.default_input_config().map_err(|e| e.to_string())?;
    let channels = config.channels().max(1) as usize;
    let rate = config.sample_rate().0;
    let stream_config: cpal::StreamConfig = config.clone().into();
    eprintln!("voice: microphone {} at {rate} Hz, {channels} channels", device.name().unwrap_or_default());
    let mut resampler = Resampler::new(rate);
    let mut mono = Vec::new();
    let mut out = Vec::new();
    let mut feed = move |samples: &mut dyn Iterator<Item = f32>| {
        mono.clear();
        let mut frame_sum = 0.0;
        for (i, s) in samples.enumerate() {
            frame_sum += s;
            if i % channels == channels - 1 {
                mono.push(frame_sum / channels as f32);
                frame_sum = 0.0;
            }
        }
        out.clear();
        resampler.run(&mono, &mut out);
        mic.push(&out);
    };
    let err = |e: cpal::StreamError| eprintln!("voice: microphone: {e}");
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |d: &[f32], _: &cpal::InputCallbackInfo| feed(&mut d.iter().copied()),
            err,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &stream_config,
            move |d: &[i16], _: &cpal::InputCallbackInfo| feed(&mut d.iter().map(|&s| s as f32 / 32768.0)),
            err,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            &stream_config,
            move |d: &[u16], _: &cpal::InputCallbackInfo| feed(&mut d.iter().map(|&s| (s as f32 - 32768.0) / 32768.0)),
            err,
            None,
        ),
        f => return Err(format!("sample format {f:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 44.1 kHz in, 48 kHz out: the count is the rate's (within one sample a block), and a
    /// tone keeps its frequency (zero crossings); the control, 48 kHz in, is passed through
    /// sample for sample (a sample late: the resampler's one of memory).
    #[test]
    fn it_resamples_to_48k() {
        let mut r = Resampler::new(44_100);
        let mut out = Vec::new();
        let tone: Vec<f32> = (0..44_100).map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 44_100.0).sin()).collect();
        for block in tone.chunks(441) {
            r.run(block, &mut out);
        }
        assert!((out.len() as i64 - 48_000).abs() <= 2, "{} samples", out.len());
        let crossings = out.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!((crossings as i64 - 1000).abs() <= 2, "{crossings} rising crossings for 1 kHz");
        let mut same = Resampler::new(48_000);
        let mut out = Vec::new();
        same.run(&[0.1, 0.2, 0.3], &mut out);
        assert_eq!(out, [0.0, 0.1, 0.2]);
    }

    /// The encoder takes whole 20 ms frames, and a backlog is cut short.
    #[test]
    fn the_mic_hands_out_frames_and_cuts_a_backlog() {
        let mic = Mic::default();
        let mut frame = vec![0.0; FRAME];
        mic.push(&vec![0.5; FRAME - 1]);
        assert!(!mic.take_frame(&mut frame), "not a whole frame yet");
        mic.push(&[0.5]);
        assert!(mic.take_frame(&mut frame) && frame[0] == 0.5);
        mic.push(&vec![0.25; 10 * FRAME]);
        assert!(mic.take_frame(&mut frame));
        assert_eq!(mic.ring.lock().unwrap().len(), FRAME, "cut to 40 ms, one taken");
    }
}
