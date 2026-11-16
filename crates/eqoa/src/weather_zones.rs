//! The regional weather a server sends (go-eqoa-server's 0x7E91,
//! `net::weather`; board entry #1550): each 2000x2000 zone's timelines kept as
//! they last came, sampled at the camera as `Grid::profile_blend` blends the
//! terrain's profiles (the four 500x500 squares' centres round it, bilinear),
//! and what the client draws of it on its own paths: the storms' fog
//! (`fog`, `viewer::lit`), the sand's drops (`vi::atmosphere::Sand`), the name
//! plates' fade (`Play::plates`' labels) and the wind's bed (`WindBed`, after
//! the sound driver). Rain and snow still come as the retail bytes (0x00C5/
//! 0x00C7) to the game's atmosphere; a sandstorm is byte 0 there and a
//! whiteout byte 6 (heavy snow), which this draws the rest of.
//!
//! Options' Weather scales every weight (`Sample::scaled`): at 0 nothing is
//! drawn and the server is told to send nothing (`Session::agree_weather`).
//! `/weatherpreview` (`Preview`) is the client's own, never sent: it stands in
//! for the sample until turned off or the world changes.

use net::weather::{self as wx, Timeline, Zone};
use std::collections::HashMap;

/// A zone's side, and a square's.
pub const ZONE: f32 = 2000.0;
pub const SQUARE: f32 = 500.0;

/// The weather at a place: each kind's weight (0 to 1), the wind (units a
/// second, x and z), and the heaviest square's seed and time (seconds since
/// its timeline began), which the wind's gusts take.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    pub rain: f32,
    pub snow: f32,
    pub sand: f32,
    pub whiteout: f32,
    pub wind: [f32; 2],
    pub seed: u32,
    pub time_s: f32,
}

impl Sample {
    /// How much weather there is: the weights' sum, at most 1.
    pub fn intensity(&self) -> f32 {
        (self.rain + self.snow + self.sand + self.whiteout).min(1.0)
    }

    /// Every weight through Options' Weather at `share` of `SHARE_FULL`.
    pub fn scaled(mut self, share: u8) -> Sample {
        let k = share.min(crate::settings::SHARE_FULL) as f32 / crate::settings::SHARE_FULL as f32;
        self.rain *= k;
        self.snow *= k;
        self.sand *= k;
        self.whiteout *= k;
        self
    }

    fn add(&mut self, kind: u8, w: f32) {
        match kind {
            wx::RAIN => self.rain += w,
            wx::SNOW => self.snow += w,
            wx::SAND => self.sand += w,
            wx::WHITEOUT => self.whiteout += w,
            _ => {}
        }
    }
}

/// `/weatherpreview`'s: a kind at a strength, the wind (4, 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preview {
    pub kind: u8,
    pub strength: f32,
}

impl Preview {
    /// `/weatherpreview rain|snow|sand|whiteout|clear [light|moderate|heavy]`:
    /// Some(Some(..)) a preview, Some(None) `off`, None not understood.
    pub fn parse(arg: &str) -> Option<Option<Preview>> {
        let mut w = arg.split_whitespace().map(|s| s.to_ascii_lowercase());
        let kind = match w.next()?.as_str() {
            "off" => return Some(None),
            "clear" => wx::CLEAR,
            "rain" => wx::RAIN,
            "snow" => wx::SNOW,
            "sand" | "sandstorm" => wx::SAND,
            "whiteout" => wx::WHITEOUT,
            _ => return None,
        };
        let strength = match w.next().as_deref() {
            None | Some("heavy") => 1.0,
            Some("moderate") | Some("medium") => 0.5,
            Some("light") => 0.15,
            Some(_) => return None,
        };
        Some(Some(Preview { kind, strength }))
    }

    /// The retail byte a rain or snow preview sets the game's atmosphere to
    /// (1-3 rain, 4-6 snow; a whiteout's snow is heavy); None for the rest.
    pub fn retail_byte(&self) -> Option<u8> {
        let step = if self.strength >= 0.75 {
            3
        } else if self.strength >= 0.325 {
            2
        } else {
            1
        };
        match self.kind {
            wx::RAIN => Some(step),
            wx::SNOW => Some(3 + step),
            _ => None,
        }
    }

    pub fn sample(&self, now_ms: i64) -> Sample {
        let mut s = Sample { wind: [4.0, 1.0], seed: 0, time_s: now_ms as f32 / 1000.0, ..Default::default() };
        s.add(self.kind, self.strength);
        s
    }
}

/// A timeline's weights at `t` ms after its message came: between two phases
/// a and b the weather eases (smoothstep) from a's kind at a's strength to
/// b's at b's; outside the first and last phase, nothing. Clear weighs
/// nothing.
pub fn timeline_weights(tl: &Timeline, t: i64) -> Sample {
    let mut s = Sample::default();
    let ph = &tl.phases;
    let (Some(first), Some(last)) = (ph.first(), ph.last()) else { return s };
    if t < first.at_ms as i64 || t > last.at_ms as i64 {
        return s;
    }
    for w in ph.windows(2) {
        let (a, b) = (w[0], w[1]);
        if t < a.at_ms as i64 || t > b.at_ms as i64 {
            continue;
        }
        let mut k = (t - a.at_ms as i64) as f32 / (b.at_ms as i64 - a.at_ms as i64) as f32;
        k = k * k * (3.0 - 2.0 * k);
        s.add(a.kind, a.strength as f32 / 255.0 * (1.0 - k));
        s.add(b.kind, b.strength as f32 / 255.0 * k);
        break;
    }
    s
}

/// The zones the server last sent, each with when it came (ms on the
/// caller's clock), for one world at a time, and the preview.
#[derive(Default)]
pub struct Zones {
    world: Option<i32>,
    zones: HashMap<(u8, u16, u16), (i64, Zone)>,
    pub preview: Option<Preview>,
}

impl Zones {
    /// The player's world is `world`: another clears what was kept and ends a
    /// preview (true when one ended).
    pub fn set_world(&mut self, world: i32) -> bool {
        if self.world == Some(world) {
            return false;
        }
        self.world = Some(world);
        self.zones.clear();
        self.preview.take().is_some()
    }

    /// A message's zones, come at `now_ms`: each replaces what was kept of
    /// it. A zone of another world than the player's is not kept.
    pub fn take(&mut self, zones: Vec<Zone>, now_ms: i64) {
        for z in zones {
            if self.world.is_some_and(|w| w != z.world as i32) {
                continue;
            }
            self.zones.insert((z.world, z.col, z.row), (now_ms, z));
        }
    }

    pub fn len(&self) -> usize {
        self.zones.len()
    }

    /// Square (sx, sz)'s weights at `now_ms` and its weight for the seed's
    /// pick, and its timeline (none: clear).
    fn square(&self, world: u8, sx: i32, sz: i32, now_ms: i64) -> Option<(Sample, &Timeline, i64)> {
        let (col, row) = (sx.div_euclid(4), sz.div_euclid(4));
        if !(0..=u16::MAX as i32).contains(&col) || !(0..=u16::MAX as i32).contains(&row) {
            return None;
        }
        let (at, z) = self.zones.get(&(world, col as u16, row as u16))?;
        let bit = wx::square_bit(sx, sz);
        let tl = z.timelines.iter().find(|t| t.squares & bit != 0)?;
        let t = now_ms - at;
        Some((timeline_weights(tl, t), tl, t))
    }

    /// The weather at (x, z) in `world` at `now_ms`: the four squares'
    /// centres round it (500*i + 250), bilinear, a square of a zone not kept
    /// clear; the wind likewise; the seed and time of the heaviest. The
    /// preview, when there is one, in its place. Not yet scaled by Options'
    /// Weather.
    pub fn sample(&self, world: i32, x: f32, z: f32, now_ms: i64) -> Sample {
        if let Some(p) = self.preview {
            return p.sample(now_ms);
        }
        let mut s = Sample::default();
        if !(0..=255).contains(&world) {
            return s;
        }
        let (u, v) = ((x - SQUARE * 0.5) / SQUARE, (z - SQUARE * 0.5) / SQUARE);
        let (i0, j0) = (u.floor(), v.floor());
        let (fx, fz) = (u - i0, v - j0);
        let mut best = 0.0f32;
        for (di, dj, w) in [(0, 0, (1.0 - fx) * (1.0 - fz)), (1, 0, fx * (1.0 - fz)), (0, 1, (1.0 - fx) * fz), (1, 1, fx * fz)] {
            if w <= 0.0 {
                continue;
            }
            let Some((q, tl, t)) = self.square(world as u8, i0 as i32 + di, j0 as i32 + dj, now_ms) else { continue };
            s.rain += q.rain * w;
            s.snow += q.snow * w;
            s.sand += q.sand * w;
            s.whiteout += q.whiteout * w;
            s.wind[0] += tl.wind[0] * w;
            s.wind[1] += tl.wind[1] * w;
            let heavy = q.intensity() * w;
            if heavy > best {
                best = heavy;
                s.seed = tl.seed;
                s.time_s = (t - tl.phases.first().map_or(0, |p| p.at_ms as i64)) as f32 / 1000.0;
            }
        }
        s
    }
}

/// The storms' fog: its distances, its colour, how much weather there is,
/// and how far the sky moves toward the colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fog {
    pub near: f32,
    pub far: f32,
    pub color: [f32; 3],
    pub intensity: f32,
    /// the sky's and the clear colour's move toward `color`: sand and
    /// whiteout, at most 1
    pub sky: f32,
}

/// Each kind's fog colour by day.
const COLORS: [(u8, [f32; 3]); 4] = [(wx::RAIN, [0.42, 0.47, 0.50]), (wx::SNOW, [0.72, 0.77, 0.81]), (wx::SAND, [0.65, 0.49, 0.29]), (wx::WHITEOUT, [0.87, 0.90, 0.92])];

/// The night's dimming of the storms' colour: 1 at midnight, 0 from 6:00 to
/// 18:00, straight between.
pub fn night(hour: f32) -> f32 {
    if hour < 6.0 {
        1.0 - hour / 6.0
    } else if hour > 18.0 {
        (hour - 18.0) / 6.0
    } else {
        0.0
    }
}

/// The fog a sample makes at `hour` (none when there is no weather): far =
/// min(800, 1 / (rain²/200 + snow²/100 + sand²/40 + whiteout²/20)), near
/// 0.08 of it, the kinds' colours by weight, dimmed by night.
pub fn fog(s: &Sample, hour: f32) -> Option<Fog> {
    let intensity = s.intensity();
    if intensity <= 0.0 {
        return None;
    }
    let k = s.rain * s.rain / 200.0 + s.snow * s.snow / 100.0 + s.sand * s.sand / 40.0 + s.whiteout * s.whiteout / 20.0;
    let far = (1.0 / k.max(1e-5)).min(800.0);
    let sum = s.rain + s.snow + s.sand + s.whiteout;
    let mut color = [0.0f32; 3];
    for (kind, c) in COLORS {
        let w = match kind {
            wx::RAIN => s.rain,
            wx::SNOW => s.snow,
            wx::SAND => s.sand,
            _ => s.whiteout,
        } / sum;
        for i in 0..3 {
            color[i] += c[i] * w;
        }
    }
    let dim = 1.0 - 0.78 * night(hour);
    Some(Fog { near: 0.08 * far, far, color: color.map(|c| c * dim), intensity, sky: (s.sand + s.whiteout).min(1.0) })
}

/// A name plate's alpha at `distance` in the storms' fog (near, far): fading
/// out across it, none past it.
pub fn plate_alpha(fog: Option<(f32, f32)>, distance: f32) -> f32 {
    match fog {
        Some((near, far)) if far > near => 1.0 - ((distance - near) / (far - near)).clamp(0.0, 1.0),
        _ => 1.0,
    }
}

/// The wind's gusts at `t` seconds of the zone's time.
pub fn gust(seed: u32, t: f32) -> f32 {
    0.75 + 0.15 * (0.7 * t + (seed & 255) as f32).sin() + 0.1 * (1.31 * t).sin()
}

/// The wind bed's loudness: the snow, sand and whiteout's weight in gusts,
/// by the effects' volume, half in the open and 0.07 under a roof or by a
/// wall upwind, at most 0.7. No disc wind sound exists.
pub fn wind_target(s: &Sample, fx_volume: f32, sheltered: bool) -> f32 {
    let open = if sheltered { 0.07 } else { 0.5 };
    ((s.snow + s.sand + s.whiteout) * gust(s.seed, s.time_s) * fx_volume * open).clamp(0.0, 0.7)
}

/// A synthesised wind: per channel, white noise (xorshift32) low-passed, added
/// to the driver's mix at a gain eased toward its target 0.0003 a frame.
#[derive(Clone, Debug)]
pub struct WindBed {
    rng: u32,
    low: [f32; 2],
    gain: f32,
    pub target: f32,
}

impl Default for WindBed {
    fn default() -> Self {
        WindBed { rng: 0x9E37_79B9, low: [0.0; 2], gain: 0.0, target: 0.0 }
    }
}

impl WindBed {
    fn noise(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x as f32 / u32::MAX as f32 * 2.0 - 1.0
    }

    /// The bed added to interleaved stereo frames.
    pub fn mix(&mut self, pcm: &mut [i16]) {
        if self.gain == 0.0 && self.target == 0.0 {
            return;
        }
        for f in pcm.chunks_mut(2) {
            if self.gain < self.target {
                self.gain = (self.gain + 0.0003).min(self.target);
            } else if self.gain > self.target {
                self.gain = (self.gain - 0.0003).max(self.target);
            }
            for (c, v) in f.iter_mut().enumerate() {
                let n = self.noise();
                self.low[c] = self.low[c] * 0.994 + n * 0.006;
                *v = (*v as f32 + self.low[c] * self.gain * 20000.0).clamp(-32768.0, 32767.0) as i16;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::weather::Phase;

    fn tl(squares: u16, kind: u8) -> Timeline {
        Timeline {
            squares,
            revision: 1,
            seed: 7,
            wind: [3.0, 0.0],
            phases: vec![
                Phase { at_ms: 0, kind: wx::CLEAR, strength: 0 },
                Phase { at_ms: 1000, kind, strength: 255 },
                Phase { at_ms: 9000, kind, strength: 255 },
                Phase { at_ms: 10_000, kind: wx::CLEAR, strength: 0 },
            ],
        }
    }

    /// Across a phase the weight eases (smoothstep): half way it is half; at
    /// the peak whole; before the first phase and after the last, none.
    #[test]
    fn a_timeline_eases_across_a_phase() {
        let t = tl(1, wx::SAND);
        assert_eq!(timeline_weights(&t, -1).sand, 0.0);
        assert_eq!(timeline_weights(&t, 0).sand, 0.0);
        assert!((timeline_weights(&t, 500).sand - 0.5).abs() < 1e-6);
        assert!(timeline_weights(&t, 250).sand < 0.25, "eased in: {}", timeline_weights(&t, 250).sand);
        assert_eq!(timeline_weights(&t, 5000).sand, 1.0);
        assert_eq!(timeline_weights(&t, 10_000).sand, 0.0);
        assert_eq!(timeline_weights(&t, 10_001), Sample::default(), "after the last, nothing");
        assert_eq!(timeline_weights(&t, 5000).rain, 0.0, "the control: no other kind");
    }

    /// The sample is the squares' centres' bilinear blend: on the edge
    /// between a sand square and a clear one, half; at the sand square's
    /// centre, whole; a zone not kept is clear. Its time counts from the
    /// message's receipt.
    #[test]
    fn the_sample_blends_across_a_square_edge() {
        let mut z = Zones::default();
        z.set_world(0);
        // zone (1, 0): x 2000..4000; its square sx 4 (bit 0) sand
        z.take(vec![Zone { world: 0, col: 1, row: 0, timelines: vec![tl(1, wx::SAND)] }], 100_000);
        let at = 100_000 + 5000;
        assert_eq!(z.sample(0, 2250.0, 250.0, at).sand, 1.0, "the square's centre");
        let edge = z.sample(0, 2500.0, 250.0, at);
        assert!((edge.sand - 0.5).abs() < 1e-6, "half on the edge: {edge:?}");
        assert!((edge.wind[0] - 1.5).abs() < 1e-6, "the wind likewise");
        assert_eq!(edge.seed, 7);
        assert!((edge.time_s - 5.0).abs() < 1e-6);
        let west = z.sample(0, 2000.0, 250.0, at);
        assert!((west.sand - 0.5).abs() < 1e-6, "half from a zone not kept (clear): {west:?}");
        assert_eq!(z.sample(0, 2250.0, 250.0, 100_000 + 20_000).sand, 0.0, "after its end");
        assert_eq!(z.sample(1, 2250.0, 250.0, at).sand, 0.0, "another world's");
        // another world clears it
        z.set_world(1);
        assert_eq!(z.len(), 0);
    }

    /// The fog: heavy whiteout 20 far, heavy sand 40, rain 200 at most 800;
    /// none when clear; near 0.08 of far; at night darker.
    #[test]
    fn the_storms_fog() {
        let f = |s: Sample| fog(&s, 12.0);
        assert_eq!(f(Sample { whiteout: 1.0, ..Default::default() }).unwrap().far, 20.0);
        assert_eq!(f(Sample { sand: 1.0, ..Default::default() }).unwrap().far, 40.0);
        let r = f(Sample { rain: 1.0, ..Default::default() }).unwrap();
        assert_eq!((r.far, r.near), (200.0, 16.0));
        assert_eq!(f(Sample { rain: 0.01, ..Default::default() }).unwrap().far, 800.0);
        assert_eq!(f(Sample::default()), None, "clear: no fog");
        let sand = f(Sample { sand: 1.0, ..Default::default() }).unwrap();
        assert_eq!((sand.color, sand.sky), ([0.65, 0.49, 0.29], 1.0));
        let night = fog(&Sample { sand: 1.0, ..Default::default() }, 0.0).unwrap();
        assert!((night.color[0] - 0.65 * 0.22).abs() < 1e-6);
    }

    /// Options' Weather at 0: nothing; at 50, half.
    #[test]
    fn the_slider_scales_every_weight() {
        let s = Sample { rain: 0.4, snow: 0.2, sand: 1.0, whiteout: 0.6, wind: [3.0, 1.0], ..Default::default() };
        let off = s.scaled(0);
        assert_eq!((off.intensity(), fog(&off, 12.0)), (0.0, None));
        assert_eq!(off.wind, s.wind);
        assert_eq!(s.scaled(50).sand, 0.5);
        assert_eq!(s.scaled(100), s);
    }

    /// A preview stands in for the sample until off; a world change ends it.
    #[test]
    fn the_preview_ends_on_a_world_change() {
        let mut z = Zones::default();
        z.set_world(0);
        z.preview = Preview::parse("sand").unwrap();
        assert_eq!(z.sample(0, 0.0, 0.0, 0).sand, 1.0);
        assert_eq!(z.preview.unwrap().retail_byte(), None);
        assert_eq!(Preview::parse("snow moderate").unwrap().unwrap().retail_byte(), Some(5));
        assert_eq!(Preview::parse("rain light").unwrap().unwrap().retail_byte(), Some(1));
        assert_eq!(Preview::parse("off"), Some(None));
        assert_eq!(Preview::parse("hail"), None);
        assert!(!z.set_world(0), "the same world: kept");
        assert!(z.preview.is_some());
        assert!(z.set_world(2), "another world ends it");
        assert_eq!(z.sample(2, 0.0, 0.0, 0), Sample::default());
    }

    /// A plate fades across the fog and is gone past it.
    #[test]
    fn plates_fade_in_the_fog() {
        assert_eq!(plate_alpha(None, 1000.0), 1.0);
        assert_eq!(plate_alpha(Some((2.0, 20.0)), 1.0), 1.0);
        assert_eq!(plate_alpha(Some((2.0, 20.0)), 11.0), 0.5);
        assert_eq!(plate_alpha(Some((2.0, 20.0)), 25.0), 0.0);
    }

    /// The wind bed is silent with no target, rises toward one, and keeps
    /// within the target's bound.
    #[test]
    fn the_wind_bed() {
        let mut b = WindBed::default();
        let mut pcm = vec![0i16; 9600];
        b.mix(&mut pcm);
        assert!(pcm.iter().all(|&v| v == 0), "no wind, nothing");
        b.target = 0.7;
        b.mix(&mut pcm);
        assert!(pcm.iter().any(|&v| v != 0));
        assert!(pcm.iter().all(|&v| (v as i32).abs() < 20000));
        assert_eq!(wind_target(&Sample { rain: 1.0, ..Default::default() }, 1.0, false), 0.0, "rain has no wind bed");
        assert!(wind_target(&Sample { sand: 1.0, ..Default::default() }, 1.0, false) > wind_target(&Sample { sand: 1.0, ..Default::default() }, 1.0, true));
    }
}
