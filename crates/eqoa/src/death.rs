//! The player's death, as `VIClient` shows it: `ResetCamera` (on 0x005E)
//! keeps where the camera is looking from, then `DeathCamera` each frame
//! swings it round the body and up, its look easing from the old direction
//! to the body over the first 4 seconds, when the screen starts its 4-second
//! fade to black (`SetFade(1, 4)`, drawn by `DrawFade`). A respawn (0x00B2,
//! or a new dump) ends it and the fade comes back in over a second.

use vi::fpu::{Ee, F};
use vi::vect::Vect3;

/// `m_camPos`, `m_camDir`, `m_camDist`, `m_camYaw`, `m_pC1D0`.
#[derive(Clone, Copy, Debug)]
pub struct DeathCam {
    pos: [f32; 3],
    dir: [f32; 3],
    dist: f32,
    yaw: f32,
    t: f32,
}

impl DeathCam {
    /// `ResetCamera`: from the camera now, and the body's level distance.
    pub fn new(cam_pos: [f32; 3], cam_dir: [f32; 3], body: [f32; 3]) -> Self {
        let d = [body[0] - cam_pos[0], 0.0, body[2] - cam_pos[2]];
        DeathCam { pos: cam_pos, dir: cam_dir, dist: (d[0] * d[0] + d[2] * d[2]).sqrt(), yaw: -cam_dir[2].atan2(cam_dir[0]), t: 0.0 }
    }

    /// `m_pC1D0`: the seconds the camera has run.
    pub fn time(&self) -> f32 {
        self.t
    }

    /// `DeathCamera`, a frame of `dt` seconds (over a tenth counts as a
    /// thirtieth): the eye and direction, and whether the fade starts now.
    pub fn step(&mut self, body: [f32; 3], dt: f32) -> (Vect3<Ee>, Vect3<Ee>, bool) {
        let dt = if dt > 0.1 { 1.0 / 30.0 } else { dt };
        let r = self.dist + self.t * 0.15;
        let eye = [body[0] - self.yaw.cos() * r, self.pos[1], body[2] + self.yaw.sin() * r];
        self.pos[1] += (self.t * 0.25).min(1.0) * 0.019;
        self.yaw += (self.t * 0.25).min(1.0) * 0.014;
        let norm = |v: [f32; 3]| {
            let m = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if m > 0.0 { [v[0] / m, v[1] / m, v[2] / m] } else { v }
        };
        let mut dir = norm([body[0] - eye[0], body[1] - eye[1], body[2] - eye[2]]);
        if self.t < 4.0 {
            let k = (self.t * 0.25 * std::f32::consts::PI + std::f32::consts::PI).cos() * 0.5 + 0.5;
            dir[1] = self.dir[1] + (dir[1] - self.dir[1]) * k;
            let (x2, z2) = (dir[0] * dir[0], dir[2] * dir[2]);
            let s = (1.0 - dir[1] * dir[1]) / (x2 + z2);
            let (sx, sz) = ((x2 * s).sqrt(), (z2 * s).sqrt());
            dir[0] = if dir[0] < 0.0 { -sx } else { sx };
            dir[2] = if dir[2] < 0.0 { -sz } else { sz };
        }
        let before = self.t;
        self.t += dt;
        let v = |a: [f32; 3]| Vect3::new(F::new(a[0]), F::new(a[1]), F::new(a[2]));
        (v(eye), v(dir), before <= 4.0 && self.t > 4.0)
    }
}

/// `SetFade` / `DrawFade`: the screen's fade to black.
#[derive(Clone, Copy, Debug, Default)]
pub struct Fade {
    pub cur: f32,
    to: f32,
    rate: f32,
}

impl Fade {
    /// `SetFade(to, time)`: at once for no time, else at a rate.
    pub fn set(&mut self, to: f32, time: f32) {
        if time == 0.0 {
            self.to = to;
            self.cur = to;
            self.rate = 0.0;
        } else {
            self.to = to;
            self.rate = (to - self.cur) / time;
        }
    }

    /// `DrawFade`'s step: toward `to` at the rate (a long frame a
    /// thirtieth). The alpha to draw black at.
    pub fn step(&mut self, dt: f32) -> f32 {
        let dt = if dt > 0.1 { 1.0 / 30.0 } else { dt };
        self.cur += self.rate * dt;
        self.cur = if self.rate > 0.0 { self.cur.min(self.to) } else { self.to.max(self.cur) };
        self.cur
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The camera starts where it was (looking the old way) and swings
    /// round and up; the fade starts once, as 4 s pass.
    #[test]
    fn the_death_camera_swings_up_and_fades_at_four_seconds() {
        let body = [10.0, 0.0, 0.0];
        let mut c = DeathCam::new([0.0, 2.0, 0.0], [1.0, 0.0, 0.0], body);
        let (e0, d0, f0) = c.step(body, 1.0 / 30.0);
        assert!((e0.x.0 - 0.0).abs() < 1e-4 && (e0.y.0 - 2.0).abs() < 1e-6, "{e0:?}");
        assert!((d0.y.0 - 0.0).abs() < 1e-6, "at first the old direction's pitch: {d0:?}");
        assert!(!f0);
        let mut fades = 0;
        let mut last = e0;
        for _ in 0..240 {
            let (e, _, f) = c.step(body, 1.0 / 30.0);
            fades += f as usize;
            last = e;
        }
        assert_eq!(fades, 1, "the fade starts once");
        assert!(last.y.0 > 2.5, "it rose: {last:?}");
        let mut fade = Fade::default();
        fade.set(1.0, 4.0);
        let a: Vec<f32> = (0..5).map(|_| fade.step(1.0)).collect();
        assert_eq!(a[0], 1.0 / 30.0 * 0.25, "a second-long frame counts as a thirtieth");
        fade.set(0.0, 0.0);
        assert_eq!(fade.step(0.016), 0.0);
    }
}
