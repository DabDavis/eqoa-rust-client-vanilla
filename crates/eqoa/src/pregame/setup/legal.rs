//! `VIWndLegal` (SETUP `viwndlegal.cpp`): SOE's legal splash, `legal_l.tga`
//! and `legal_r.tga` side by side, up 7.5 seconds from when it got the focus:
//! fading in over the first second and out from 6 to 7, then the focus given
//! back (`SetFocus(0)`), which `VISetup::Process` takes as done. START, once
//! it has been up five seconds, skips to the end of the fade out.
//! (`VIWndPresents`, the same code over `presents_l/r.tga`, is linked and
//! never shown: `Process` goes from this screen to the licence.)

use crate::pointer::key;
use crate::ui::Layout;
use vi::fpu::{Ee, F};
use vi::ui::Vui;

#[derive(Clone, Copy, Debug, Default)]
pub struct Legal {
    /// `m_time`: when the focus arrived
    pub time: i64,
}

impl Legal {
    /// `OnMessage` 295, the focus arriving.
    pub fn focus(&mut self, now: i64) {
        self.time = now;
    }

    /// `OnDrawSelf`: true when it gives the focus back (done).
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Layout, now: i64) -> bool {
        let t = F::<Ee>::from_int((now - self.time) as i32) * F::new(0.001);
        let mut alpha = F::<Ee>::ONE;
        let mut done = false;
        if F::new(7.5) < t {
            done = true;
        } else if F::new(6.0) < t {
            alpha = F::ONE - (t - F::new(6.0));
        } else if t < F::ONE {
            alpha = t;
        }
        if t < F::new(7.0) {
            vui.blit_id(r, vi::hash::hash_resource_id("legal_l.tga"), [0, 0], 3, alpha.0, 0);
            vui.blit_id(r, vi::hash::hash_resource_id("legal_r.tga"), [320, 0], 3, alpha.0, 0);
        }
        done
    }

    /// `OnKeyDown`: START (144) after five seconds jumps to seven.
    pub fn key(&mut self, k: i32, now: i64) {
        if k == key::START && now - self.time > 5000 {
            self.time = now - 7000;
        }
    }

    /// A pointer: a click anywhere is START (the screen has nothing else).
    pub fn point(&self, _p: [f32; 2]) -> Vec<i32> {
        vec![key::START]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, vui};

    /// Up 7.5 s from the focus; START skips only after 5 s, to the fade's end.
    #[test]
    fn up_seven_and_a_half_seconds_and_start_skips_after_five() {
        let (v, mut r) = (vui(), NullRaster);
        let mut l = Legal::default();
        l.focus(1000);
        assert!(!l.draw(&v, &mut r, 1000 + 7400));
        assert!(l.draw(&v, &mut r, 1000 + 7600), "done past 7.5 s");
        l.focus(0);
        l.key(key::START, 4000);
        assert!(!l.draw(&v, &mut r, 4100), "START before 5 s does nothing");
        l.key(key::START, 5100);
        assert!(l.draw(&v, &mut r, 5100 + 600), "START after 5 s: 7 s, then done half a second later");
        // the control: Enter is not START on this screen
        let mut m = Legal::default();
        m.focus(0);
        m.key(13, 6000);
        assert!(!m.draw(&v, &mut r, 6100));
    }
}
