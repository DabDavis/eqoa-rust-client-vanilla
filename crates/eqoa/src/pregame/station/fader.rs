//! `VIFader` (vifader.cpp, SETUP's and STATION's alike): a black quad over
//! the whole screen at an alpha that moves toward a target at a rate per
//! second, on the clock.

use crate::ui::Layout;
use render::ui::Anchor;

#[derive(Clone, Copy, Debug, Default)]
pub struct Fader {
    pub alpha: f32,
    target: f32,
    rate: f32,
    time: i64,
}

impl Fader {
    /// `Init(alpha)` at `now`.
    pub fn new(alpha: f32, now: i64) -> Fader {
        Fader { alpha, target: 0.0, rate: 0.0, time: now }
    }

    /// `Begin(target, seconds)`: at once when `seconds` is 0.
    pub fn begin(&mut self, target: f32, seconds: f32) {
        self.target = target;
        if seconds == 0.0 {
            self.alpha = target;
            self.rate = 0.0;
        } else {
            self.rate = (target - self.alpha) / seconds;
        }
    }

    /// `Update`: the alpha stepped to `now`, then the quad, from x 0 to one
    /// pixel past the right and y -1 to the bottom (across a wider window,
    /// its whole width, as the port's `VI_WIDE_EDGE` spreads it).
    pub fn update(&mut self, r: &mut dyn Layout, now: i64) {
        let dt = (now - self.time) as f32 * 0.001;
        self.time = now;
        self.alpha += self.rate * dt;
        if self.rate > 0.0 {
            self.alpha = self.alpha.min(self.target);
        } else {
            self.alpha = if self.alpha < self.target { self.target } else { self.alpha };
        }
        if self.alpha > 0.0 {
            r.anchor(Anchor::Wide);
            let side = r.side();
            // the strip (0, H), (0, -1), (W+1, H), (W+1, -1), as a fan
            r.fill_blended([0.0, 0.0, 0.0, self.alpha], [[-side, -1], [641 + side, -1], [641 + side, 448], [-side, 448]]);
            r.anchor(Anchor::Middle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_fades_on_the_clock_and_stops_at_the_target() {
        let mut f = Fader::new(1.0, 0);
        f.begin(0.0, 0.5);
        let mut r = crate::pointer::testing::NullRaster;
        f.update(&mut r, 250);
        assert!((f.alpha - 0.5).abs() < 1e-6);
        f.update(&mut r, 1000);
        assert_eq!(f.alpha, 0.0, "not past its target");
        f.begin(1.0, 0.5);
        f.update(&mut r, 1100);
        assert!((f.alpha - 0.2).abs() < 1e-6);
        f.update(&mut r, 5000);
        assert_eq!(f.alpha, 1.0);
        // the control: no time, no fade
        let mut g = Fader::new(1.0, 0);
        g.begin(0.0, 0.5);
        g.update(&mut r, 0);
        assert_eq!(g.alpha, 1.0);
    }
}
