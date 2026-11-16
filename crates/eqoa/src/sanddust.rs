//! A server's sandstorm's blowing sand (the regional weather, `weather_zones`;
//! board entry #1550), drawn as fine dust and grit: not the game's (a PS2 has
//! no sandstorm), so none of it is in `vi`. The owner, 2026-11-16: "the sand
//! for the sandstorm particles look like disks flying at me, lets make it look
//! more natural".
//!
//! Three kinds, all in the storm's fog colour and all world-space streaks (the
//! renderer's strips), never sprites facing the camera:
//! - grains: thousands of fine, faint streaks, each stretched along the wind
//!   by its own speed (as a short exposure would smear it) and turned about
//!   that line to face the eye, so a grain blowing straight at the camera is a
//!   short speck, not a disc; varied width, length and alpha, soft at the
//!   sides and ends (snow.tga's soft dot stretched), the tail fainter than
//!   the head. A share of them skim low over the ground.
//! - wisps: a few wide, very faint sheets of dust sliding sideways with the
//!   wind near the ground, for the storm's body.
//! - near the camera every grain and wisp fades out (`NEAR`..`NEAR_FULL`),
//!   so none balloons across the view.
//!
//! The grains live in a 24-unit box whose centre is 10 ahead of the camera
//! (it holds the camera and the player in every camera), the wisps in a
//! 48-unit one; both follow the camera, wrapping as the game's weather box
//! does. Their random numbers are their own (xorshift).

use render::billboard::Strips;

/// The grains' box and how far ahead of the camera its centre is.
pub const RANGE: f32 = 24.0;
pub const AHEAD: f32 = 10.0;
/// The wisps' box, its centre as far ahead.
pub const WISP_RANGE: f32 = 48.0;
/// The most grains and wisps, at a heavy storm.
pub const GRAINS: usize = 4000;
pub const WISPS: usize = 28;
/// The share of the grains that skim the ground.
const LOW: f32 = 0.3;
/// Within `NEAR` of the eye nothing is drawn; by `NEAR_FULL` all of it.
pub const NEAR: f32 = 1.5;
pub const NEAR_FULL: f32 = 4.0;
/// A streak's length: the distance it moves in this long, and at least this.
const EXPOSURE: f32 = 0.12;
const MIN_LEN: f32 = 0.06;

#[derive(Clone, Copy, Debug)]
struct Grain {
    /// place in the box, relative to its centre
    p: [f32; 3],
    /// half its width, its alpha, its speed against the wind's
    half: f32,
    alpha: f32,
    speed: f32,
    /// a grain skimming the ground (kept within 0.6 of it)
    low: bool,
    /// its height over the ground, for a low one
    lift: f32,
}

#[derive(Clone, Copy, Debug)]
struct Wisp {
    p: [f32; 3],
    /// half its length along the wind and half its height
    half: [f32; 2],
    alpha: f32,
    speed: f32,
}

/// The blowing sand round the camera.
#[derive(Clone, Debug)]
pub struct Dust {
    grains: Vec<Grain>,
    wisps: Vec<Wisp>,
    /// the boxes' centre (the grains'; the wisps' is the same point)
    centre: [f32; 3],
    /// how many of each drawn now
    pub grain_count: usize,
    pub wisp_count: usize,
    /// the sand's weight now (0 to 1), and the wind (units a second)
    pub sand: f32,
    pub wind: [f32; 2],
    /// the ground's height under the camera, if known (`Weather`'s probe)
    pub ground: Option<f32>,
    rng: u32,
    last: Option<i64>,
    time: f32,
}

impl Default for Dust {
    fn default() -> Self {
        Dust { grains: Vec::new(), wisps: Vec::new(), centre: [0.0; 3], grain_count: 0, wisp_count: 0, sand: 0.0, wind: [0.0; 2], ground: None, rng: 0x2545_F491, last: None, time: 0.0 }
    }
}

fn len3(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn wrap(v: &mut f32, range: f32) {
    let half = range * 0.5;
    *v = (*v + half).rem_euclid(range) - half;
}

/// How much of a thing at `d` from the eye is drawn: none within `NEAR`,
/// all past `NEAR_FULL`, smooth between.
pub fn near_fade(d: f32) -> f32 {
    let t = ((d - NEAR) / (NEAR_FULL - NEAR)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Dust {
    fn unit(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn fill(&mut self) {
        let h = RANGE * 0.5;
        self.grains = (0..GRAINS)
            .map(|_| {
                let p = [self.unit() * RANGE - h, self.unit() * RANGE - h, self.unit() * RANGE - h];
                // most grains fine and faint, a few coarser
                let coarse = self.unit();
                let half = 0.008 + 0.022 * coarse * coarse;
                let alpha = 0.16 + 0.26 * self.unit();
                let speed = 0.7 + 0.6 * self.unit();
                let low = self.unit() < LOW;
                let lift = 0.6 * self.unit() * self.unit();
                Grain { p, half, alpha, speed, low, lift }
            })
            .collect();
        let wh = WISP_RANGE * 0.5;
        self.wisps = (0..WISPS)
            .map(|_| Wisp {
                p: [self.unit() * WISP_RANGE - wh, 0.0, self.unit() * WISP_RANGE - wh],
                half: [2.0 + 4.0 * self.unit(), 0.6 + 1.2 * self.unit()],
                alpha: 0.04 + 0.06 * self.unit(),
                speed: 0.6 + 0.3 * self.unit(),
            })
            .collect();
    }

    /// One frame at `time` (ms): the camera at `eye` looking along `dir` (any
    /// length), the sand's weight (0 to 1) and the wind (units a second).
    pub fn update(&mut self, eye: [f32; 3], dir: [f32; 3], sand: f32, wind: [f32; 2], time: i64) {
        let dt = self.last.map_or(0.0, |t| ((time - t).max(0) as f32 / 1000.0).min(0.25));
        self.last = Some(time);
        self.time += dt;
        self.sand = sand.clamp(0.0, 1.0);
        self.wind = wind;
        self.grain_count = (self.sand * GRAINS as f32) as usize;
        self.wisp_count = (self.sand * WISPS as f32).ceil() as usize;
        if self.grain_count == 0 {
            return;
        }
        let n = len3(dir);
        let dir = if n > 1e-6 { dir.map(|v| v / n) } else { [0.0, 0.0, 1.0] };
        let c = [eye[0] + dir[0] * AHEAD, eye[1] + dir[1] * AHEAD, eye[2] + dir[2] * AHEAD];
        let d = [c[0] - self.centre[0], c[1] - self.centre[1], c[2] - self.centre[2]];
        let jump = self.grains.is_empty() || d.iter().any(|v| v.abs() > RANGE);
        self.centre = c;
        if jump {
            self.fill();
            return;
        }
        let t = self.time;
        for (i, g) in self.grains.iter_mut().enumerate() {
            // gusting a little, each grain its own
            let flutter = ((i as f32) * 0.37 + t * 1.4).sin();
            g.p[0] += wind[0] * g.speed * dt - d[0];
            g.p[1] += flutter * 0.25 * dt - d[1];
            g.p[2] += wind[1] * g.speed * dt - d[2];
            for v in g.p.iter_mut() {
                wrap(v, RANGE);
            }
        }
        for w in self.wisps.iter_mut() {
            w.p[0] += wind[0] * w.speed * dt - d[0];
            w.p[2] += wind[1] * w.speed * dt - d[2];
            wrap(&mut w.p[0], WISP_RANGE);
            wrap(&mut w.p[2], WISP_RANGE);
        }
    }

    /// The streak of a thing at `at` moving along `vel` (its length `len`,
    /// half width `half`), turned about that line to face `eye`: four
    /// corners, tail then head, each side.
    fn streak(eye: [f32; 3], at: [f32; 3], vel: [f32; 3], len: f32, half: f32) -> Option<[[f32; 3]; 4]> {
        let s = len3(vel);
        let a = if s > 1e-4 { vel.map(|v| v / s) } else { [1.0, 0.0, 0.0] };
        let to_eye = [eye[0] - at[0], eye[1] - at[1], eye[2] - at[2]];
        let mut side = cross(a, to_eye);
        let sl = len3(side);
        if sl < 1e-5 {
            // straight at the eye: a speck its own width across
            side = cross(a, [0.0, 1.0, 0.0]);
            let k = len3(side);
            if k < 1e-5 {
                return None;
            }
            side = side.map(|v| v / k);
        } else {
            side = side.map(|v| v / sl);
        }
        let h = len * 0.5;
        let tail = [at[0] - a[0] * h, at[1] - a[1] * h, at[2] - a[2] * h];
        let head = [at[0] + a[0] * h, at[1] + a[1] * h, at[2] + a[2] * h];
        let o = side.map(|v| v * half);
        Some([
            [tail[0] - o[0], tail[1] - o[1], tail[2] - o[2]],
            [tail[0] + o[0], tail[1] + o[1], tail[2] + o[2]],
            [head[0] - o[0], head[1] - o[1], head[2] - o[2]],
            [head[0] + o[0], head[1] + o[1], head[2] + o[2]],
        ])
    }

    /// This frame's sand as strips seen from `eye`, in `color` (the storm's
    /// fog colour), on the soft texture `texture` (a DictID): the grains,
    /// then the wisps, alpha-blended, depth-tested and not writing depth.
    pub fn strips(&self, eye: [f32; 3], color: [f32; 3], texture: i32) -> Vec<Strips> {
        let mut out = Vec::new();
        if self.grain_count == 0 {
            return out;
        }
        // the grains a touch lighter than the fog, so they read against it
        let grain_rgb = color.map(|c| (c * 1.18 + 0.04).min(1.0));
        let mut g = Strips { texture, blend: 3, ztest: true, zwrite: false, color: [0.0; 4], strips: Vec::new(), colors: Vec::new() };
        let strength = 0.45 + 0.55 * self.sand;
        let ground = self.ground;
        for gr in self.grains.iter().take(self.grain_count) {
            let mut at = [gr.p[0] + self.centre[0], gr.p[1] + self.centre[1], gr.p[2] + self.centre[2]];
            if gr.low {
                let Some(y) = ground else { continue };
                at[1] = y + 0.05 + gr.lift;
            }
            let d = len3([at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]]);
            let fade = near_fade(d);
            if fade <= 0.0 {
                continue;
            }
            let vel = [self.wind[0] * gr.speed, 0.0, self.wind[1] * gr.speed];
            let len = (len3(vel) * EXPOSURE).max(MIN_LEN);
            let Some(q) = Self::streak(eye, at, vel, len, gr.half) else { continue };
            let a = gr.alpha * strength * fade * if gr.low { 1.3 } else { 1.0 };
            let (tail, head) = ([grain_rgb[0], grain_rgb[1], grain_rgb[2], a * 0.35], [grain_rgb[0], grain_rgb[1], grain_rgb[2], a]);
            g.strips.push(vec![([0.0, 0.0], q[0]), ([0.0, 1.0], q[1]), ([1.0, 0.0], q[2]), ([1.0, 1.0], q[3])]);
            g.colors.push(vec![tail, tail, head, head]);
        }
        if !g.strips.is_empty() {
            out.push(g);
        }
        let mut w = Strips { texture, blend: 3, ztest: true, zwrite: false, color: [0.0; 4], strips: Vec::new(), colors: Vec::new() };
        let base = ground.unwrap_or(eye[1] - 2.0);
        for wi in self.wisps.iter().take(self.wisp_count) {
            let at = [wi.p[0] + self.centre[0], base + wi.half[1] * 0.8, wi.p[2] + self.centre[2]];
            let d = len3([at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]]);
            // a wide sheet: faded in further off, so it never fills the view
            let fade = near_fade(d * 0.5);
            if fade <= 0.0 {
                continue;
            }
            let vel = [self.wind[0], 0.0, self.wind[1]];
            let Some(q) = Self::streak(eye, at, vel, wi.half[0] * 2.0, wi.half[1]) else { continue };
            let a = wi.alpha * self.sand * fade;
            let c = [color[0], color[1], color[2], a];
            w.strips.push(vec![([0.0, 0.0], q[0]), ([0.0, 1.0], q[1]), ([1.0, 0.0], q[2]), ([1.0, 1.0], q[3])]);
            w.colors.push(vec![c; 4]);
        }
        if !w.strips.is_empty() {
            out.push(w);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dust(sand: f32, wind: [f32; 2]) -> Dust {
        let mut d = Dust { ground: Some(0.0), ..Default::default() };
        d.update([0.0, 2.0, -12.0], [0.0, -1.0, 12.0], sand, wind, 0);
        d.update([0.0, 2.0, -12.0], [0.0, -1.0, 12.0], sand, wind, 100);
        d
    }

    /// Many grains at a heavy storm, fewer at a moderate one, none clear; a
    /// few wisps for its body.
    #[test]
    fn grains_and_wisps_by_the_sands_weight() {
        let heavy = dust(1.0, [6.0, 0.0]);
        assert_eq!((heavy.grain_count, heavy.wisp_count), (GRAINS, WISPS));
        let moderate = dust(0.5, [6.0, 0.0]);
        assert_eq!(moderate.grain_count, GRAINS / 2);
        let clear = dust(0.0, [6.0, 0.0]);
        assert!(clear.strips([0.0, 2.0, -12.0], [0.6, 0.5, 0.3], 1).is_empty(), "clear: nothing");
    }

    /// Each grain is a thin streak along the wind, longer than it is wide
    /// (none a round sprite), its tail fainter than its head; every grain's
    /// alpha faint; none within `NEAR` of the eye.
    #[test]
    fn grains_are_thin_faint_streaks_along_the_wind() {
        let eye = [0.0, 2.0, -12.0];
        let d = dust(1.0, [8.0, 0.0]);
        let s = d.strips(eye, [0.65, 0.49, 0.29], 1);
        let grains = &s[0];
        assert!(grains.strips.len() > GRAINS / 2, "{}", grains.strips.len());
        for (q, c) in grains.strips.iter().zip(&grains.colors) {
            let along = [q[2].1[0] - q[0].1[0], q[2].1[1] - q[0].1[1], q[2].1[2] - q[0].1[2]];
            let across = [q[1].1[0] - q[0].1[0], q[1].1[1] - q[0].1[1], q[1].1[2] - q[0].1[2]];
            assert!(len3(along) > 3.0 * len3(across), "a streak, not a disc: {along:?} {across:?}");
            assert!(along[0].abs() > 0.99 * len3(along), "along the wind (x): {along:?}");
            assert!(c[0][3] < c[2][3], "the tail fainter");
            assert!(c[2][3] < 0.6, "faint: {}", c[2][3]);
            let centre = [(q[0].1[0] + q[3].1[0]) * 0.5, (q[0].1[1] + q[3].1[1]) * 0.5, (q[0].1[2] + q[3].1[2]) * 0.5];
            assert!(len3([centre[0] - eye[0], centre[1] - eye[1], centre[2] - eye[2]]) > NEAR, "none at the eye");
        }
        // the control: the wisps are wide sheets, wider than any grain
        let wisps = &s[1];
        assert!(!wisps.strips.is_empty());
        let w = &wisps.strips[0];
        assert!(len3([w[1].1[0] - w[0].1[0], w[1].1[1] - w[0].1[1], w[1].1[2] - w[0].1[2]]) > 1.0);
    }

    /// A grain blowing straight at the eye is a speck its own width, not a
    /// disc: its streak's both axes are small.
    #[test]
    fn a_grain_at_the_eye_is_a_speck() {
        let eye = [0.0, 0.0, 0.0];
        let q = Dust::streak(eye, [0.0, 0.0, 10.0], [0.0, 0.0, -8.0], 0.4, 0.01).unwrap();
        let across = len3([q[1][0] - q[0][0], q[1][1] - q[0][1], q[1][2] - q[0][2]]);
        assert!((across - 0.02).abs() < 1e-5);
        // seen from the eye its length lies along the line of sight: its
        // corners' directions from the eye differ by less than its width
        let dir = |p: [f32; 3]| p.map(|v| v / len3(p));
        let (t, h) = (dir(q[0]), dir(q[2]));
        assert!(len3([t[0] - h[0], t[1] - h[1], t[2] - h[2]]) < 0.01);
    }

    /// The low grains skim within 0.65 of the ground; with no ground known
    /// they are left out.
    #[test]
    fn low_grains_skim_the_ground() {
        let eye = [0.0, 2.0, -12.0];
        let mut d = dust(1.0, [6.0, 0.0]);
        let all = d.strips(eye, [0.6, 0.5, 0.3], 1)[0].strips.len();
        let low = d.grains.iter().take(d.grain_count).filter(|g| g.low).count();
        assert!(low > GRAINS / 5, "{low}");
        d.ground = None;
        let without = d.strips(eye, [0.6, 0.5, 0.3], 1)[0].strips.len();
        assert!(without < all, "{without} < {all}");
        assert!(d.grains.iter().filter(|g| g.low).all(|g| g.lift <= 0.6));
    }

    /// The near fade: none at the eye, all past `NEAR_FULL`.
    #[test]
    fn the_near_fade() {
        assert_eq!(near_fade(0.5), 0.0);
        assert_eq!(near_fade(NEAR_FULL + 1.0), 1.0);
        assert!(near_fade((NEAR + NEAR_FULL) * 0.5) > 0.4 && near_fade((NEAR + NEAR_FULL) * 0.5) < 0.6);
    }

    /// The grains move with the wind from frame to frame.
    #[test]
    fn the_grains_move_with_the_wind() {
        let mut d = dust(1.0, [6.0, 0.0]);
        let before: Vec<[f32; 3]> = d.grains.iter().map(|g| g.p).collect();
        d.update([0.0, 2.0, -12.0], [0.0, -1.0, 12.0], 1.0, [6.0, 0.0], 200);
        let moved = before.iter().zip(&d.grains).filter(|(a, g)| g.p[0] - a[0] > 0.3 && g.p[0] - a[0] < 1.0).count();
        assert!(moved > GRAINS * 9 / 10, "{moved}");
    }
}
