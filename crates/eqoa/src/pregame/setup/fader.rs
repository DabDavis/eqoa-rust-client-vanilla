//! `VIFader` (SETUP `vifader.cpp`, linked into STATION byte for byte): the
//! screen's fade. A black quad over the whole screen, one pixel past each
//! edge, at `m_alpha`, which moves toward `m_target` at `m_rate` a second
//! on the clock (`VITimer::GetTime`, ms here). Its material is a solid fill
//! blended (mode 3), drawn after every window (`VISetup::Process` updates it
//! last).

use crate::ui::Layout;
use vi::fpu::{Ee, F};

#[derive(Clone, Copy, Debug)]
pub struct Fader {
    pub alpha: F<Ee>,
    pub target: F<Ee>,
    /// alpha per second
    pub rate: F<Ee>,
    /// `m_time`: the last update's clock
    pub time: i64,
}

impl Default for Fader {
    fn default() -> Self {
        Fader { alpha: F::ZERO, target: F::ZERO, rate: F::ZERO, time: 0 }
    }
}

impl Fader {
    /// `Init(raster, dict, alpha)`: at `alpha`, still, its clock now.
    pub fn init(&mut self, alpha: f32, now: i64) {
        self.target = F::ZERO;
        self.rate = F::ZERO;
        self.alpha = F::new(alpha);
        self.time = now;
    }

    /// `Begin(target, seconds)`: 0 seconds is at once.
    pub fn begin(&mut self, target: f32, seconds: f32) {
        let (target, seconds) = (F::<Ee>::new(target), F::<Ee>::new(seconds));
        if seconds == F::ZERO {
            self.target = target;
            self.alpha = target;
            self.rate = F::ZERO;
        } else {
            self.target = target;
            self.rate = (target - self.alpha) / seconds;
        }
    }

    /// `End`: at the target at once.
    pub fn end(&mut self) {
        self.alpha = self.target;
        self.rate = F::ZERO;
    }

    /// `Alpha()`.
    pub fn alpha(&self) -> f32 {
        self.alpha.0
    }

    /// `Update`: the alpha moved by the time since the last update (`(now -
    /// m_time) * 0.001f`), stopped at the target, then the quad when it
    /// shows at all.
    pub fn update(&mut self, r: &mut dyn Layout, now: i64) {
        let dt = F::<Ee>::from_int((now - self.time) as i32) * F::new(0.001);
        self.time = now;
        self.alpha = self.alpha + self.rate * dt;
        if F::ZERO < self.rate {
            self.alpha = if self.alpha < self.target { self.alpha } else { self.target };
        } else {
            self.alpha = if self.alpha < self.target { self.target } else { self.alpha };
        }
        if F::ZERO < self.alpha {
            // the strip (0, H) (0, -1) (W+1, H) (W+1, -1), in fan order; a
            // quad the frame's whole width runs a wider window's too
            // (`VI_WIDE_EDGE`)
            let (w, h) = (640.0, 448.0);
            r.solid(3, [0.0, 0.0, 0.0, self.alpha.0], [[0.0, -1.0], [w + 1.0, -1.0], [w + 1.0, h], [0.0, h]]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A raster that keeps the solid quads' alphas.
    #[derive(Default)]
    struct Solids(Vec<f32>);
    impl Layout for Solids {
        fn solid(&mut self, _mode: i32, c: [f32; 4], _corners: [[f32; 2]; 4]) {
            self.0.push(c[3]);
        }
    }
    impl vi::ui::Raster for Solids {
        fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16]) {}
        fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16], _: u32) {}
        fn string_width(&self, s: &[u16]) -> i32 {
            s.len() as i32 * 8
        }
        fn point_size(&self) -> i32 {
            16
        }
        fn viewport(&self) -> [i32; 4] {
            [0, 0, 639, 447]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    /// `Init(1)` then `Begin(0, 0.5)`: black at once, clear half a second
    /// later, no quad once clear; a fade out stops at 1 and stays drawn.
    #[test]
    fn a_fade_runs_on_the_clock_and_stops_at_its_target() {
        let mut f = Fader::default();
        let mut r = Solids::default();
        f.init(1.0, 0);
        f.begin(0.0, 0.5);
        f.update(&mut r, 0);
        assert_eq!(r.0, vec![1.0], "black at first");
        f.update(&mut r, 250);
        assert!((r.0[1] - 0.5).abs() < 1e-6, "half way at 250 ms: {}", r.0[1]);
        f.update(&mut r, 600);
        assert_eq!(f.alpha(), 0.0, "stopped at the target, not past it");
        assert_eq!(r.0.len(), 2, "nothing drawn once clear");
        f.begin(1.0, 0.5);
        f.update(&mut r, 1000);
        f.update(&mut r, 5000);
        assert_eq!(f.alpha(), 1.0);
        assert_eq!(*r.0.last().unwrap(), 1.0, "a full fade stays drawn");
        // the control: `Begin(t, 0)` is at once, whatever the clock
        f.begin(0.25, 0.0);
        assert_eq!(f.alpha(), 0.25);
    }
}
