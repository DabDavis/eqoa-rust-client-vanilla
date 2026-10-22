//! `VISplashFx` (SETUP `visplashfx.cpp`): the title's animated overlays,
//! drawn over the painting by the title window. The original game's: a bird
//! that flies past now and then, one perched and flapping, and a flock
//! crossing the sky along one of three paths. Frontiers' (expansion flag 2,
//! which this disc sets) replaces all three with the island layer: twinkles
//! on the water, a sail and its reflection, the camp fire and the ship
//! (splashAnims1/2), three stars, the floating island bobbing, a glow in
//! three layers, and a comet now and then.
//!
//! Everything counts in frames of SETUP's loop (30 a second: `VISetup::Process`
//! holds each frame 18 ms past the field), except the flock's clock and its
//! path, which go by `VITimer`. The game's file statics are this struct's
//! fields; `rand()` is newlib's, a stream of the window's own here.

use crate::ui::Layout;
use vi::csprite::Rand;
use vi::fpu::{Ee, F, vimin};

/// A source rectangle in an atlas, in texels: x, y, w, h.
type Frame = [i32; 4];

/// The bird's four flap sequences: frame 1..3 of `BIRD_FRAMES`, 0 not drawn.
const BIRD_ANIM: [[u8; 8]; 4] = [[0, 1, 2, 3, 2, 3, 2, 1], [0, 1, 2, 3, 3, 3, 2, 1], [3, 3, 3, 3, 0, 0, 2, 0], [0, 1, 2, 3, 2, 3, 2, 1]];
/// A flock's paths, entry to exit, in screen pixels.
const PATHS: [[f32; 4]; 3] = [[296.0, 207.0, -200.0, 119.0], [296.0, 207.0, -200.0, 119.0], [369.0, 167.0, -200.0, 131.0]];
/// The flock's wing beat: seven frames, large and small.
const FLOCK_FRAMES: [[Frame; 7]; 2] = [
    [[0, 0, 17, 27], [19, 0, 17, 27], [38, 0, 17, 27], [57, 0, 17, 27], [76, 0, 17, 27], [95, 0, 17, 27], [0, 29, 17, 27]],
    [[19, 29, 8, 13], [29, 29, 8, 13], [39, 29, 8, 13], [49, 29, 8, 13], [59, 29, 8, 13], [69, 29, 8, 13], [79, 29, 8, 13]],
];
/// The perched bird's three 27x25 frames.
const FLAP: [[i32; 2]; 3] = [[19, 44], [48, 44], [77, 44]];
/// The flying bird's three frames.
const BIRD_FRAMES: [Frame; 3] = [[0, 73, 39, 54], [40, 73, 44, 53], [84, 73, 43, 54]];
/// Island: the stars' eight frames, and where the three stars are.
const STAR_FRAMES: [Frame; 8] = [[0, 100, 14, 28], [15, 100, 14, 28], [30, 100, 14, 28], [45, 100, 14, 28], [60, 100, 14, 28], [75, 100, 14, 28], [90, 100, 14, 28], [105, 100, 14, 28]];
const STARS: [[i32; 2]; 3] = [[401, 272], [436, 297], [521, 293]];
/// Island: a twinkle's three phases.
const TWINKLE_FRAMES: [Frame; 3] = [[88, 29, 40, 28], [88, 0, 40, 28], [88, 58, 40, 28]];
/// Island: the glow's three layers.
const GLOW_FRAMES: [Frame; 3] = [[0, 57, 60, 56], [0, 0, 60, 56], [61, 0, 60, 56]];
/// Island: the comet's frames, and where it may appear (the last three are
/// never chosen: `% 4`).
const COMET_FRAMES: [Frame; 12] = [
    [0, 0, 22, 18],
    [0, 19, 22, 18],
    [0, 38, 22, 18],
    [0, 57, 22, 18],
    [0, 76, 22, 18],
    [0, 95, 22, 18],
    [23, 0, 22, 18],
    [23, 19, 22, 18],
    [23, 38, 22, 18],
    [23, 57, 22, 18],
    [23, 76, 22, 18],
    [23, 95, 22, 18],
];
const COMETS: [[i32; 2]; 7] = [[182, 248], [45, 267], [77, 264], [204, 237], [0, 0], [0, 0], [0, 0]];

/// The surfaces, by their names' DictIDs (`VIHashResourceID`).
fn id(name: &str) -> u32 {
    vi::hash::hash_resource_id(name)
}

#[derive(Clone, Debug)]
pub struct SplashFx {
    /// `VIGetExpansionFlags()`: bit 2 is Frontiers' island layer
    pub expansion: u32,
    pub rand: Rand,
    bird_sub: i32,
    flap_sub: i32,
    /// the flock's wing beat, 1/256 frames, mod 12
    flock_clock: i32,
    /// island: the stars' frame, mod 8
    pub star_frame: i32,
    /// island: toggles every frame, for the half-rate steps
    pub odd_frame: i32,
    /// island: frames, for the bob
    pub wave: i32,
    /// island: the glow's cycle, 0..119 then 30..119
    pub glow: i32,
    flock: [[F<Ee>; 2]; 4],
    time: i64,
    flock_size: i32,
    flock_path: i32,
    flock_small: [i32; 4],
    /// frames to the next flock; -1 while one flies
    flock_wait: i32,
    flock_t: F<Ee>,
    flock_alpha: F<Ee>,
    flap_frame: i32,
    flap_dir: i32,
    /// frames to the next flight; -1 while flying
    bird_wait: i32,
    bird_col: i32,
    bird_row: i32,
    /// island: the sail's position, counting down; which way it crosses
    pub sail: i32,
    pub sail_dir: i32,
    /// island: each twinkle's frames left, and where
    pub twinkle: [i32; 8],
    twinkles: [[i32; 2]; 8],
    /// island: the comet's frames left, and which of `COMETS`
    pub comet: i32,
    comet_at: i32,
}

impl SplashFx {
    /// The file statics as the module loads (zero), on the disc's expansion
    /// flags; `VISplashFxInit` then `Reset`s it.
    pub fn new(expansion: u32) -> Self {
        SplashFx {
            expansion,
            rand: Rand(1),
            bird_sub: 0,
            flap_sub: 0,
            flock_clock: 0,
            star_frame: 0,
            odd_frame: 0,
            wave: 0,
            glow: 0,
            flock: [[F::ZERO; 2]; 4],
            time: 0,
            flock_size: 0,
            flock_path: 0,
            flock_small: [0; 4],
            flock_wait: 0,
            flock_t: F::ZERO,
            flock_alpha: F::ZERO,
            flap_frame: 0,
            flap_dir: 0,
            bird_wait: 0,
            bird_col: 0,
            bird_row: 0,
            sail: 0,
            sail_dir: 0,
            twinkle: [0; 8],
            twinkles: [[0; 2]; 8],
            comet: 0,
            comet_at: 0,
        }
    }

    fn rand(&mut self) -> i32 {
        self.rand.rand()
    }

    /// `VISplashFxReset` (`VISplashFxInit` ends with it): only these start
    /// over; the frame counters (the glow, the bob, the stars) run on from
    /// wherever they were.
    pub fn reset(&mut self, now: i64) {
        self.time = now;
        self.flock_wait = 0;
        self.bird_wait = (self.rand() & 0xFF) + 150;
        self.flap_frame = 0;
        self.flap_dir = 1;
        self.sail = 316;
        self.sail_dir = 0;
        self.comet = 0;
        self.comet_at = 0;
        self.twinkle = [0; 8];
    }

    /// `VISplashFxRender(draw)`: one frame. The animation steps whether or
    /// not it draws (every caller draws).
    pub fn render(&mut self, r: &mut dyn Layout, now: i64, draw: bool) {
        if self.expansion & 2 != 0 {
            self.render_island(r, draw);
            return;
        }
        let bird = id("Bird_Animation.tga");
        // the flying bird
        if self.bird_wait == -1 {
            self.bird_sub += 140;
            if self.bird_sub > 256 {
                self.bird_sub -= 256;
                self.bird_col += 1;
            }
            if self.bird_col < 8 {
                let f = BIRD_ANIM[self.bird_row as usize][self.bird_col as usize];
                if f != 0 {
                    let [x, y, w, h] = BIRD_FRAMES[f as usize - 1];
                    if draw {
                        r.blit(bird, 0, [45, 210], [x, y], [w, h], 3, 1.0);
                    }
                }
            } else {
                self.bird_wait = (self.rand() & 0xFF) + 150;
            }
        } else if self.bird_wait == 0 {
            self.bird_wait = -1;
            self.bird_col = 0;
            self.bird_row = self.rand() & 3;
            self.bird_sub = 0;
        } else {
            self.bird_wait -= 1;
        }
        // the perched bird
        self.flap_sub += 66;
        if self.flap_sub > 256 {
            self.flap_sub -= 256;
            self.flap_frame = (self.flap_frame + self.flap_dir).clamp(0, 2);
            self.flap_dir = if self.rand() & 1 != 0 { 1 } else { -1 };
        }
        if draw {
            r.blit(bird, 0, [562, 185], FLAP[self.flap_frame as usize], [27, 25], 3, 1.0);
        }
        // the flock
        let dt = (now - self.time) as i32;
        self.time = now;
        self.flock_clock += dt * 4;
        if self.flock_clock >= 3072 {
            self.flock_clock -= 3072;
        }
        if self.flock_wait == -1 {
            self.flock_alpha = vimin(self.flock_alpha + F::new(0.04), F::new(0.7));
            let [fx, fy, tx, ty] = PATHS[self.flock_path as usize].map(F::<Ee>::new);
            let t = self.flock_t;
            let fdt = F::<Ee>::from_int(dt);
            let px = fx + (tx - fx) * t;
            let py = fy + (ty - fy) * t;
            for i in 0..self.flock_size as usize {
                let f = ((self.flock_clock >> 8) + i as i32 * 5) % 12;
                let f = if f < 7 { f } else { 12 - f };
                if draw {
                    let [sx, sy, sw, sh] = FLOCK_FRAMES[self.flock_small[i] as usize][f as usize];
                    // the `+ 0.0f` is in the image: an add.s of zero
                    let at = [(px + self.flock[i][0]).0 as i32, (py + self.flock[i][1] + F::ZERO).0 as i32];
                    r.blit(bird, 4, at, [sx, sy], [sw, sh], 3, self.flock_alpha.0);
                }
                match i {
                    1 => self.flock[1][0] = self.flock[1][0] + F::new(0.11),
                    2 => {
                        self.flock[2][0] = self.flock[2][0] + F::new(0.06);
                        self.flock[2][1] = self.flock[2][1] + F::new(0.12);
                    }
                    3 => {
                        self.flock[3][0] = self.flock[3][0] + F::new(0.09);
                        self.flock[3][1] = self.flock[3][1] + F::new(0.045);
                    }
                    _ => {}
                }
            }
            // in double, by libcall: 0.0001 is a double and 0.73f is widened
            self.flock_t = F::new((self.flock_t.0 as f64 + fdt.0 as f64 * 0.0001 * 0.73f32 as f64) as f32);
            if F::ONE <= self.flock_t {
                self.flock_wait = (self.rand() & 0x7F) + 100;
            }
        } else if self.flock_wait == 0 {
            self.flock_wait = -1;
            self.flock_size = (self.rand() & 3) + 1;
            self.flock_path = self.rand() % 3;
            for i in 0..self.flock_size as usize {
                self.flock_small[i] = self.rand() & 1;
                self.flock[i][0] = F::<Ee>::from_int((self.rand() & 63) - 31) * F::new(1.2);
                self.flock[i][1] = F::<Ee>::from_int((self.rand() & 63) - 31) * F::new(0.5);
            }
            self.flock_t = F::ZERO;
            self.flock_alpha = F::ZERO;
        } else {
            self.flock_wait -= 1;
        }
    }

    /// `VISplashFxRenderIsland` (0x0056BEC0), Frontiers' layer.
    fn render_island(&mut self, r: &mut dyn Layout, draw: bool) {
        let anims = ["splashAnims1.tga", "splashAnims2.tga", "splashAnims3.tga", "splashAnims4.tga"].map(id);
        if anims.iter().any(|&a| !r.has(a)) {
            return;
        }
        let f = F::<Ee>::from_int;
        self.odd_frame ^= 1;
        // the twinkles: each fades through three frames over 60
        for i in 0..8 {
            if self.twinkle[i] != 0 {
                self.twinkle[i] -= 1;
                let t = self.twinkle[i];
                let fifteen = F::<Ee>::new(15.0);
                let (mut a0, mut a1, mut a2) = (F::<Ee>::ZERO, F::<Ee>::ZERO, F::<Ee>::ZERO);
                if t < 30 {
                    a0 = if t < 15 { f(t) / fifteen } else { F::ONE - f(t - 15) / fifteen };
                }
                if (15..45).contains(&t) {
                    a1 = if t < 30 { f(t - 15) / fifteen } else { F::ONE - f(t - 30) / fifteen };
                }
                if (30..60).contains(&t) {
                    a2 = if t < 45 { f(t - 30) / fifteen } else { F::ONE - f(t - 45) / fifteen };
                }
                if draw {
                    for (k, a) in [a0, a1, a2].into_iter().enumerate() {
                        if F::ZERO < a {
                            let [x, y, w, h] = TWINKLE_FRAMES[k];
                            r.blit(anims[0], 4, self.twinkles[i], [x, y], [w, h], 3, a.0);
                        }
                    }
                }
            }
        }
        if (self.rand() / 17) % 30 < 3
            && let Some(i) = (0..8).find(|&i| self.twinkle[i] == 0)
        {
            self.twinkle[i] = 60;
            self.twinkles[i][0] = (self.rand() / 17) % 51 + 184;
            self.twinkles[i][1] = (self.rand() / 17) % 43 + 265;
        }
        // the sail, and its reflection: hidden until it counts below 216,
        // then a pixel every other frame
        self.sail -= 1;
        if self.sail < 216 {
            if self.odd_frame != 0 {
                self.sail += 1;
            }
            let x = if self.sail_dir != 0 { 309 - self.sail } else { self.sail + 93 };
            if draw {
                // the reflection, flipped, then the sail 20 above it
                let y = 289;
                r.blit(anims[1], if self.sail_dir != 0 { 7 } else { 6 }, [x, y], [0, 95], [54, 33], 3, 0.2);
                r.blit(anims[1], if self.sail_dir != 0 { 5 } else { 4 }, [x, y - 20], [0, 95], [54, 33], 3, 1.0);
            }
            if self.sail == 0 {
                self.sail_dir ^= 1;
                self.sail = (self.rand() / 17) % 600 + 516;
            }
        }
        // the camp fire and the ship: drawn whatever `draw` says, as shipped
        r.blit(anims[0], 0, [73, 234], [0, 0], [77, 99], 3, 1.0);
        r.blit(anims[1], 0, [293, 233], [56, 41], [72, 87], 3, 1.0);
        // the three stars
        for (i, at) in STARS.iter().enumerate() {
            let k = ((self.star_frame + i as i32 * 3) & 7) as usize;
            if draw {
                let [x, y, w, h] = STAR_FRAMES[k];
                r.blit(anims[0], 4, *at, [x, y], [w, h], 3, 1.0);
            }
        }
        if self.odd_frame != 0 {
            self.star_frame = (self.star_frame + 1) & 7;
        }
        // the island, bobbing: a fan through its own material (blended)
        let (x, y) = self.island_at();
        if draw {
            let (x, y) = (x.0, y.0);
            let corners = [[x, y], [x + 64.0, y], [x + 64.0, y + 64.0], [x, y + 64.0]];
            r.textured(id("island.tga"), 3, [1.0; 4], corners, [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        }
        self.wave += 1;
        // the glow: three layers come up one after another, then all go out
        let glow = self.glow_alphas();
        if draw {
            for (i, a) in glow.iter().enumerate() {
                if F::ZERO < *a {
                    let [x, y, w, h] = GLOW_FRAMES[i % 3];
                    r.blit(anims[2], 4, [171, 100], [x, y], [w, h], 3, a.0);
                }
            }
        }
        self.glow += 1;
        if self.glow == 120 {
            self.glow = 30;
        }
        // the comet
        if self.comet != 0 {
            let k = ((22 - self.comet) >> 1) as usize;
            if self.odd_frame != 0 {
                self.comet -= 1;
            }
            if draw {
                let [x, y, w, h] = COMET_FRAMES[k];
                r.blit(anims[3], 4, COMETS[self.comet_at as usize], [x, y], [w, h], 3, 0.7);
            }
        } else if (self.rand() / 17) % 100 < 2 {
            self.comet = 22;
            self.comet_at = (self.rand() / 17) % 4;
        }
    }

    /// The island's corner this frame: a 4000-frame swing of 17 across and
    /// a 340-frame one of 8 up and down, about (451, 110).
    pub fn island_at(&self) -> (F<Ee>, F<Ee>) {
        let pi2 = F::<Ee>::new(2.0) * F::new(3.141_592_65);
        let x = (F::<Ee>::from_int(self.wave % 4000) / F::new(4000.0) * pi2).sinf() * F::new(17.0) + F::new(451.0);
        let y = (F::<Ee>::from_int(self.wave % 340) / F::new(340.0) * pi2).sinf() * F::new(8.0) + F::new(110.0);
        (x, y)
    }

    /// The glow's four layers' alphas this frame.
    pub fn glow_alphas(&self) -> [F<Ee>; 4] {
        let g = self.glow;
        let thirty = F::<Ee>::new(30.0);
        let f = F::<Ee>::from_int;
        let mut a = [F::<Ee>::ZERO; 4];
        if g < 30 {
            a[0] = f(g) / thirty;
        }
        if (30..60).contains(&g) {
            a[0] = F::ONE;
            a[1] = f(g - 30) / thirty;
        }
        if (60..90).contains(&g) {
            a[0] = F::ONE;
            a[1] = F::ONE;
            a[2] = f(g - 60) / thirty;
        }
        if (90..120).contains(&g) {
            a[0] = F::ONE;
            a[1] = F::ONE;
            a[2] = F::ONE;
            a[3] = f(g - 90) / thirty;
        }
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::NullRaster;

    /// The island layer counts in SETUP's frames: the glow comes up over
    /// 120 and cycles 30..119; the stars step every other frame; the sail
    /// waits 100 frames, then moves a pixel every other one.
    #[test]
    fn the_island_layer_counts_in_frames() {
        let mut fx = SplashFx::new(3);
        fx.reset(0);
        let mut r = NullRaster;
        assert_eq!(fx.glow_alphas()[0], F::ZERO);
        for k in 1..=120 {
            fx.render(&mut r, k * 33, true);
        }
        assert_eq!(fx.glow, 30, "120 frames: round to 30");
        assert_eq!(fx.star_frame, 60 & 7, "a step every other frame");
        // the sail: 316 counts down to 216 hidden, then by half
        let mut s = SplashFx::new(3);
        s.reset(0);
        for _ in 0..100 {
            s.render(&mut r, 0, true);
        }
        assert_eq!(s.sail, 216, "hidden while it counts the first hundred");
        for _ in 0..20 {
            s.render(&mut r, 0, true);
        }
        assert_eq!(s.sail, 206, "then a pixel every other frame");
        // the control: the original game's layer is not the island's
        let mut o = SplashFx::new(1);
        o.reset(0);
        o.render(&mut r, 33, true);
        assert_eq!((o.glow, o.wave, o.sail), (0, 0, 316), "flag 2 off: the birds, not the island");
    }

    /// The glow's layers: one up at a time, each over 30 frames.
    #[test]
    fn the_glow_comes_up_one_layer_at_a_time() {
        let mut fx = SplashFx::new(3);
        let at = |fx: &mut SplashFx, g: i32| {
            fx.glow = g;
            fx.glow_alphas().map(|a| a.0)
        };
        assert_eq!(at(&mut fx, 15), [0.5, 0.0, 0.0, 0.0]);
        assert_eq!(at(&mut fx, 45), [1.0, 0.5, 0.0, 0.0]);
        assert_eq!(at(&mut fx, 75), [1.0, 1.0, 0.5, 0.0]);
        assert_eq!(at(&mut fx, 105), [1.0, 1.0, 1.0, 0.5]);
        // the island bobs about (451, 110): a quarter of its 340-frame swing is 8 down
        fx.wave = 85;
        let (_, y) = fx.island_at();
        assert!((y.0 - 118.0).abs() < 0.01, "{}", y.0);
        fx.wave = 0;
        assert_eq!(fx.island_at().0.0, 451.0, "the control: no swing at frame 0");
    }

    /// The splash's frames are the game's, not the display's: a second at
    /// 165 Hz steps the layer 30 times, as at 30 Hz (`frames.rs`).
    #[test]
    fn a_second_is_thirty_frames_at_any_display_rate() {
        for hz in [30.0f64, 60.0, 165.0] {
            let mut fx = SplashFx::new(3);
            fx.reset(0);
            let mut frames = crate::frames::Frames::default();
            let mut r = NullRaster;
            let n = (hz as usize) + 1;
            for k in 0..n {
                let now = k as f64 * 1000.0 / hz;
                if frames.due(now) {
                    fx.render(&mut r, now as i64, true);
                }
            }
            assert!((30..=31).contains(&fx.wave), "{hz} Hz: {} frames in a second", fx.wave);
        }
        // the control: stepped once a display frame, 165 Hz runs 5.5 times as fast
        let mut fx = SplashFx::new(3);
        let mut r = NullRaster;
        for k in 0..166 {
            fx.render(&mut r, k * 6, true);
        }
        assert!(fx.wave > 160);
    }
}
