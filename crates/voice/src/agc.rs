//! The microphone's automatic gain, natively: what a browser does for the page's microphone
//! (`getUserMedia`'s `autoGainControl`, `web/voice.js`). A microphone hands over its own level,
//! and a laptop's built-in array speaks some 25 dB under a headset's: heard through the
//! forwarding server it was "suuuuuper low volume. needs to be approx 10-15x louder" at the
//! listener's 200% (F3 20261026-115036). So each 20 ms frame, before it is coded:
//!
//! - the noise floor is followed (a quieter frame takes it at once; it rises 1 dB a second), and a
//!   frame 9 dB over it, and over -65 dBFS, is speech;
//! - speech's level is followed (rising quickly, falling slowly), and the gain moves toward
//!   what puts it at `TARGET_DB`: never under 0 dB (a loud microphone is left as it is), never
//!   over `MAX_GAIN_DB`, nor so far that the noise floor comes out over `NOISE_OUT_DB`; it rises
//!   only on speech (silence never winds it up) at `UP_DB_PER_S`, and falls at `DOWN_DB_PER_S`;
//! - a frame whose loudest sample would pass `PEAK` takes, from its first sample, the gain that
//!   keeps it there (a sudden loud start covers the step); any other change of gain is spread
//!   over the frame; and every sample goes through a soft knee from `KNEE`, so none reaches
//!   full scale.
//!
//! The numbers are ours, in the range a browser's own uses (WebRTC's AGC2 targets speech near
//! -18 dBFS with up to 30 dB of gain); nothing here is the game's.

/// Speech's RMS aimed at, dBFS.
pub(crate) const TARGET_DB: f32 = -18.0;
/// The most gain, dB.
pub(crate) const MAX_GAIN_DB: f32 = 30.0;
/// The gain a start takes before any speech is heard, dB.
const START_GAIN_DB: f32 = 10.0;
/// The noise floor is never raised past this, dBFS.
pub(crate) const NOISE_OUT_DB: f32 = -50.0;
/// How fast the gain rises (on speech only) and falls, dB a second.
const UP_DB_PER_S: f32 = 12.0;
const DOWN_DB_PER_S: f32 = 30.0;
/// A frame this far over the noise floor, and over `SPEECH_MIN_DB`, is speech.
const SPEECH_OVER_NOISE_DB: f32 = 9.0;
const SPEECH_MIN_DB: f32 = -65.0;
/// The noise floor's rise, dB a second (a quieter frame takes it at once).
const NOISE_RISE_DB_PER_S: f32 = 1.0;
/// The loudest sample a frame's gain lets through, and where the soft knee starts.
const PEAK: f32 = 0.9;
const KNEE: f32 = 0.75;
/// A frame's length, seconds (20 ms at 48 kHz).
const FRAME_S: f32 = 0.02;

fn dbfs(rms: f32) -> f32 {
    if rms > 1e-10 { 20.0 * rms.log10() } else { -200.0 }
}

fn gain_of(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// The gain's state, kept from frame to frame for as long as the microphone is open.
pub(crate) struct Agc {
    /// the gain the last frame ended on, dB
    gain_db: f32,
    /// the noise floor, dBFS (none before the first frame)
    noise_db: Option<f32>,
    /// speech's level, dBFS (none before the first speech)
    speech_db: Option<f32>,
}

impl Default for Agc {
    fn default() -> Self {
        Agc { gain_db: START_GAIN_DB, noise_db: None, speech_db: None }
    }
}

impl Agc {
    /// The gain now, dB.
    #[cfg(test)]
    pub fn gain_db(&self) -> f32 {
        self.gain_db
    }

    /// One frame (any length; the rates are a 20 ms frame's), its gain applied in place.
    pub fn process(&mut self, frame: &mut [f32]) {
        if frame.is_empty() {
            return;
        }
        let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
        let peak = frame.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let level = dbfs(rms);
        let noise = match self.noise_db {
            Some(n) if level >= n => n + NOISE_RISE_DB_PER_S * FRAME_S,
            _ => level,
        };
        self.noise_db = Some(noise);
        let speech = level > SPEECH_MIN_DB && level > noise + SPEECH_OVER_NOISE_DB;
        if speech {
            self.speech_db = Some(match self.speech_db {
                None => level,
                Some(s) if level > s => s + (level - s) * 0.3,
                Some(s) => s + (level - s) * 0.03,
            });
        }
        // where the gain is going: speech at the target, the noise kept under its ceiling
        let mut want = self.speech_db.map_or(self.gain_db, |s| (TARGET_DB - s).clamp(0.0, MAX_GAIN_DB));
        want = want.min((NOISE_OUT_DB - noise).max(0.0));
        let from = self.gain_db;
        let mut to = if want > from {
            if speech { (from + UP_DB_PER_S * FRAME_S).min(want) } else { from }
        } else {
            (from - DOWN_DB_PER_S * FRAME_S).max(want)
        };
        // the frame's loudest sample held under the peak, from its first sample
        let limit = if peak > 0.0 { dbfs(PEAK / peak).max(0.0) } else { MAX_GAIN_DB };
        let limited = to > limit;
        to = to.min(limit);
        self.gain_db = to;
        let (g0, g1) = (if limited { gain_of(to) } else { gain_of(from) }, gain_of(to));
        let n = frame.len() as f32;
        for (k, s) in frame.iter_mut().enumerate() {
            let g = g0 + (g1 - g0) * (k + 1) as f32 / n;
            *s = soft_knee(*s * g);
        }
    }
}

/// A sample past `KNEE` bent toward full scale (a frame's gain keeps every sample within a few
/// percent of `PEAK`, where the bend leaves it under 0.93).
fn soft_knee(x: f32) -> f32 {
    let a = x.abs();
    if a <= KNEE {
        return x;
    }
    let over = (a - KNEE) / (1.0 - KNEE);
    (KNEE + (1.0 - KNEE) * over.tanh()).copysign(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jitter::{FRAME, RATE};

    /// A speaker, as a frame source: syllables of a voiced tone (200 ms of a 220 Hz sine with its
    /// harmonics, then 100 ms off) at `speech` dBFS RMS while voiced, over white noise at `noise`
    /// dBFS RMS (a fixed generator, so every run is the same).
    struct Speaker {
        n: u64,
        seed: u32,
        speech: f32,
        noise: f32,
    }

    impl Speaker {
        fn new(speech: f32, noise: f32) -> Speaker {
            Speaker { n: 0, seed: 12345, speech, noise }
        }

        fn voiced(&self) -> bool {
            self.n % (RATE as u64 * 3 / 10) < RATE as u64 / 5
        }

        /// The next frame, and whether it is voiced all through.
        fn frame(&mut self) -> (Vec<f32>, bool) {
            let voiced = self.voiced();
            let mut out = Vec::with_capacity(FRAME);
            // the sine's sum has an RMS of sqrt(0.5 + 0.125 + 0.03125); the noise's uniform
            // [-1, 1] is 1/sqrt(3)
            let tone = gain_of(self.speech) / (0.5f32 + 0.125 + 0.03125).sqrt();
            let hiss = gain_of(self.noise) * 3f32.sqrt();
            for _ in 0..FRAME {
                let t = self.n as f32 / RATE as f32;
                let mut s = 0.0;
                if self.voiced() {
                    let w = std::f32::consts::TAU * 220.0 * t;
                    s = tone * (w.sin() + 0.5 * (2.0 * w).sin() + 0.25 * (3.0 * w).sin());
                }
                self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                s += hiss * ((self.seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0);
                out.push(s);
                self.n += 1;
            }
            (out, voiced && self.voiced())
        }
    }

    fn rms_db(s: &[f32]) -> f32 {
        dbfs((s.iter().map(|x| x * x).sum::<f32>() / s.len().max(1) as f32).sqrt())
    }

    /// `seconds` of `speaker` through an Agc (or none, the control): the voiced frames' level
    /// over the last 3 s in and out, and the loudest sample out.
    fn run(speaker: &mut Speaker, agc: Option<&mut Agc>, seconds: u32) -> (f32, f32, f32) {
        let frames = seconds as usize * 50;
        let (mut inn, mut out, mut peak) = (Vec::new(), Vec::new(), 0.0f32);
        let mut agc = agc;
        for f in 0..frames {
            let (frame, voiced) = speaker.frame();
            let mut done = frame.clone();
            if let Some(a) = agc.as_deref_mut() {
                a.process(&mut done);
            }
            peak = peak.max(done.iter().fold(0.0f32, |m, s| m.max(s.abs())));
            if voiced && f >= frames - 150 {
                inn.extend(frame);
                out.extend(done);
            }
        }
        (rms_db(&inn), rms_db(&out), peak)
    }

    /// A laptop's array: speech at -46 dBFS over a -82 dBFS floor comes out within 1.5 dB of the
    /// target, and never past full scale; the control, the same voice with no gain, stays 28 dB
    /// under it. Over a -70 dBFS floor the gain stops where the floor reaches the noise ceiling
    /// (20 dB).
    #[test]
    fn a_quiet_microphone_comes_up_to_the_target() {
        let (inn, out, peak) = run(&mut Speaker::new(-46.0, -82.0), Some(&mut Agc::default()), 10);
        assert!((out - TARGET_DB).abs() < 1.5, "out {out:.1} dBFS");
        assert!(peak < 1.0, "peak {peak}");
        let (_, raw, _) = run(&mut Speaker::new(-46.0, -82.0), None, 10);
        assert!((raw - inn).abs() < 0.01 && raw < TARGET_DB - 25.0, "the control: {raw:.1} dBFS");
        let mut noisy = Agc::default();
        run(&mut Speaker::new(-46.0, -70.0), Some(&mut noisy), 10);
        assert!((noisy.gain_db() - (NOISE_OUT_DB + 70.0)).abs() < 1.0, "gain {:.1} dB over a -70 floor", noisy.gain_db());
    }

    /// The loudest sample of `seconds` of `speaker` at a fixed gain, dB: a control's measure.
    fn peak_at(mut speaker: Speaker, db: f32, seconds: u32) -> f32 {
        (0..seconds as usize * 50).map(|_| speaker.frame().0.iter().fold(0.0f32, |m, s| m.max(s.abs()))).fold(0.0, f32::max) * gain_of(db)
    }

    /// A headset's speech, louder than the target already, keeps its level (no gain under 0 dB),
    /// and its first words, met at the start's gain, are held under full scale.
    #[test]
    fn a_loud_microphone_is_left_as_it_is_and_never_clips() {
        let mut agc = Agc::default();
        let (inn, out, peak) = run(&mut Speaker::new(-12.0, -70.0), Some(&mut agc), 6);
        assert!((out - inn).abs() < 1.0, "in {inn:.1}, out {out:.1}");
        assert!(agc.gain_db() < 0.5, "gain {:.1} dB", agc.gain_db());
        assert!(peak < 1.0, "peak {peak}");
        // the control: the start's gain on this voice, held, passes full scale
        assert!(peak_at(Speaker::new(-12.0, -70.0), START_GAIN_DB, 1) > 1.0);
    }

    /// No speech, a -58 dBFS hiss (a fan, a noisy room): the gain comes down until the hiss is
    /// under the noise ceiling, and never rises; the control, the start's gain on the same hiss,
    /// leaves it over the ceiling. And a voice over a quiet floor is told from it and gets gain.
    #[test]
    fn noise_is_held_under_its_ceiling() {
        let mut agc = Agc::default();
        let mut hiss = Speaker::new(-200.0, -58.0);
        let (mut out, mut most) = (Vec::new(), 0.0f32);
        for _ in 0..500 {
            let (mut f, _) = hiss.frame();
            agc.process(&mut f);
            most = most.max(agc.gain_db());
            out.extend(f);
        }
        let level = rms_db(&out[out.len() - 50 * FRAME..]);
        assert!(level <= NOISE_OUT_DB + 0.5, "hiss out {level:.1} dBFS");
        assert!(most <= START_GAIN_DB, "gain up to {most:.1} dB");
        assert!(rms_db(&out[..FRAME]) > NOISE_OUT_DB + 0.5, "the control: the start's gain on it");
        let mut voiced = Agc::default();
        run(&mut Speaker::new(-50.0, -80.0), Some(&mut voiced), 10);
        assert!(voiced.gain_db() > 25.0, "a voice: {:.1} dB", voiced.gain_db());
    }

    /// Quiet speech, the gain near its most, then a shout 34 dB louder: no sample passes full
    /// scale on the way, and half a second on the shout comes through at its own level (it is
    /// over the target: no gain).
    #[test]
    fn a_shout_after_quiet_speech_is_held() {
        let mut agc = Agc::default();
        let mut quiet = Speaker::new(-46.0, -82.0);
        run(&mut quiet, Some(&mut agc), 8);
        assert!(agc.gain_db() > 25.0, "gain {:.1} dB", agc.gain_db());
        let mut shout = Speaker::new(-12.0, -80.0);
        shout.n = quiet.n;
        let (mut peak, mut late) = (0.0f32, Vec::new());
        for f in 0..100 {
            let (mut frame, voiced) = shout.frame();
            agc.process(&mut frame);
            peak = peak.max(frame.iter().fold(0.0f32, |m, s| m.max(s.abs())));
            if voiced && f >= 25 {
                late.extend(frame);
            }
        }
        assert!(peak < 1.0, "peak {peak}");
        let out = rms_db(&late);
        assert!((out + 12.0).abs() < 1.0, "out {out:.1} dBFS");
        // the control: the shout at the quiet voice's gain passes full scale
        assert!(peak_at(Speaker::new(-12.0, -80.0), 25.0, 1) > 1.0);
    }
}
