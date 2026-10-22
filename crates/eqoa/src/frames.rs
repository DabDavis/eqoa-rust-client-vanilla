//! The game's frame, as the console keeps it. CLIENT's loop holds each frame
//! to at least 20 VITimer units (20.35 ms) and the raster then waits for a
//! field of NTSC video, so a frame is two fields: 30 a second
//! (`claude-notes/qol-patches-20260816.md`, "The 30 fps lock is a
//! BUSY-WAIT").
//!
//! What the game moves on its clock (`VIScene::GetTime`: the walk and the
//! camera in 32 ms ticks, animation, particles, the weather) steps correctly
//! at any rate, and the client runs it every frame it draws. What it counts
//! in frames does not: the windows (`VIKeyFlash`'s repeat after 16 frames, a
//! window opening by 0.15 a frame, `VIUITextScroll`'s two pixels a frame and
//! 30-frame pause, the quick menus' light, the "..." dots), `TrackTarget`'s
//! colour easing, `DeathCamera`'s swing and the camera shake's samples. Run
//! once for every frame of a 165 Hz display they went 5.5 times as fast
//! (F3, 2026-10-20: the target's name rocking, the menus' arrows repeating
//! almost at once). So the client draws at the display's rate and runs those
//! on this clock: the HUD and its windows once a frame of the game's, their
//! last quads drawn again on the frames between (`render::ui::Ui::keep`).

/// The game's frame, in ms. The console's is two fields at 59.94 Hz
/// (33.37 ms); a thirtieth keeps step with a page held to 30 frames a
/// second (`web::frame_due`, whose frames would otherwise come early
/// against it one in a hundred) and with 60 and 120 Hz displays. What is
/// counted in these frames is too short for the difference to show (a
/// repeat after 16 frames: 533 ms against 534).
pub const FRAME_MS: f64 = 1000.0 / 30.0;

/// A frame drawn up to this early still counts as on time: displays'
/// frames jitter by a millisecond or two, and a frame that came just early
/// would put the next of the game's two of the display's away.
const EARLY_MS: f64 = 4.0;

/// When the game's next frame is due.
#[derive(Clone, Copy, Debug, Default)]
pub struct Frames {
    due: Option<f64>,
}

impl Frames {
    /// Whether a frame drawn at `now` (ms, on any steady clock) is one of
    /// the game's: the first, then each that comes once the next is due.
    /// Due times step by `FRAME_MS` from the last, so a display at 165 Hz
    /// has one every fifth or sixth frame and 33.3 ms between them on
    /// average; a display slower than 30 Hz has one every frame (as the
    /// console's frames get longer when it cannot keep up); after a stall
    /// the next is a frame from now, not a burst to catch up.
    pub fn due(&mut self, now: f64) -> bool {
        self.due_every(now, FRAME_MS)
    }

    /// `due` on a clock of another frame, `frame_ms` long (STATION's, two
    /// to one of the game's: `pregame::host`'s sound device).
    pub fn due_every(&mut self, now: f64, frame_ms: f64) -> bool {
        match self.due {
            Some(at) if now + EARLY_MS < at => false,
            Some(at) if now - at <= frame_ms => {
                self.due = Some(at + frame_ms);
                true
            }
            _ => {
                self.due = Some(now + frame_ms);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::startmenu::KeyFlash;
    use vi::ui::TextScroll;

    /// Render frames at `hz` (with a deterministic jitter of up to `jitter`
    /// ms either way) over `ms`, and the game's frames among them.
    fn frames_at(hz: f64, jitter: f64, ms: f64) -> Vec<(f64, bool)> {
        let mut clock = Frames::default();
        let mut out = Vec::new();
        let mut seed = 0x2545_F491u32;
        let mut k = 0.0;
        loop {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let j = (seed % 1001) as f64 / 1000.0 * 2.0 * jitter - jitter;
            let t = (k * 1000.0 / hz + j).max(0.0);
            if t > ms {
                return out;
            }
            out.push((t, clock.due(t)));
            k += 1.0;
        }
    }

    /// Thirty of the game's frames a second whatever the display's rate,
    /// none closer than the display allows, and at a display under 30 Hz
    /// one every frame.
    #[test]
    fn thirty_a_second_at_any_rate() {
        for (hz, jitter) in [(30.0, 1.0), (60.0, 0.0), (60.0, 1.5), (75.0, 1.0), (120.0, 1.0), (144.0, 1.0), (165.0, 1.0), (240.0, 1.0)] {
            let f = frames_at(hz, jitter, 60_000.0);
            let n = f.iter().filter(|f| f.1).count();
            assert!((1795..=1805).contains(&n), "{hz} Hz: {n} of the game's frames in a minute");
            // never two in a row on a display at 60 Hz or more: no burst
            if hz >= 60.0 {
                assert!(f.windows(2).all(|w| !(w[0].1 && w[1].1)), "{hz} Hz: two of the game's frames in a row");
            }
        }
        // 60 Hz with no jitter: every other frame, all minute (no frame of
        // the game's ever three of the display's long)
        let f = frames_at(60.0, 0.0, 60_000.0);
        assert!(f.chunks(2).all(|c| c[0].1 && !c.get(1).is_some_and(|c| c.1)));
        // 20 Hz: every frame is one (the game cannot run ahead of its draws)
        assert!(frames_at(20.0, 0.0, 10_000.0).iter().all(|f| f.1));
        // control: a clock that answered every frame gives 165 a second
        let mut always = 0;
        for (t, _) in frames_at(165.0, 1.0, 1000.0) {
            always += (t >= 0.0) as usize;
        }
        assert!(always > 160);
    }

    /// After a stall the next is a frame from now: no burst.
    #[test]
    fn a_stall_does_not_catch_up() {
        let mut c = Frames::default();
        assert!(c.due(0.0));
        assert!(!c.due(10.0));
        assert!(c.due(500.0), "the first frame after the stall");
        assert!(!c.due(506.0) && !c.due(512.0) && !c.due(520.0), "not three more to catch up");
        assert!(c.due(534.0));
    }

    /// `KeyFlash` stepped on the game's frames: the repeat's times are the
    /// console's (fire, then 16 frames on, then 9, 8, ...) at a 165 Hz
    /// display and at 30 Hz alike; pulsed every display frame (the client
    /// before 2026-10-20) at 165 Hz the first repeat came after 97 ms.
    #[test]
    fn the_key_repeat_keeps_the_consoles_time_at_any_rate() {
        let fires = |hz: f64, gated: bool| -> Vec<f64> {
            let mut f = KeyFlash::default();
            f.press(139);
            let mut out = Vec::new();
            for (t, due) in frames_at(hz, 1.0, 1500.0) {
                if (due || !gated) && f.pulse(139) {
                    out.push(t);
                }
            }
            out
        };
        for hz in [30.0, 60.0, 144.0, 165.0] {
            let t = fires(hz, true);
            // the console's: 0, 533 (16 frames), 833 (+9), 1100 (+8), 1333 (+7)
            let want = [0.0, 533.3, 833.3, 1100.0, 1333.3];
            assert!(t.len() >= 5, "{hz} Hz: {t:?}");
            for (got, want) in t.iter().zip(want) {
                assert!((got - want).abs() <= 1000.0 / hz + EARLY_MS + 1.0, "{hz} Hz: {t:?}");
            }
        }
        // control: pulsed every frame at 165 Hz, the first repeat in 97 ms
        let t = fires(165.0, false);
        assert!(t[1] < 110.0, "{t:?}");
    }

    /// A tap shorter than one of the game's frames, pressed and let go
    /// between two of them, still moves once (`KeyFlash::release` keeps a
    /// key whose first pulse has not fired); one held across frames repeats
    /// as before, and one let go after its first pulse stops at once.
    #[test]
    fn a_tap_between_two_frames_is_not_lost() {
        let mut f = KeyFlash::default();
        f.press(138);
        f.release(138);
        assert!(f.pulse(138), "the tap fires on the next frame");
        assert!((0..40).all(|_| !f.pulse(138)), "once");
        // pressed again before the frame: still held
        f.press(138);
        f.release(138);
        f.press(138);
        let n = (0..40).filter(|_| f.pulse(138)).count();
        assert!(n > 3, "held: it repeats ({n})");
        // let go after the first pulse: nothing more
        let mut g = KeyFlash::default();
        g.press(139);
        assert!(g.pulse(139));
        g.release(139);
        assert!((0..40).all(|_| !g.pulse(139)));
    }

    /// The target's name, wider than its bar, stepped on the game's frames:
    /// 60 pixels a second (two a frame) and a second's pause at each end, at
    /// 165 Hz as at 30; drawn every display frame at 165 Hz it went 330 a
    /// second with a pause of 0.18 s, the rocking the F3 report saw.
    #[test]
    fn the_targets_name_scrolls_at_the_consoles_speed_at_any_rate() {
        use crate::pointer::testing::{NullRaster, vui};
        let name: Vec<u16> = "Assistant Recruiter of Freeport".encode_utf16().collect();
        // pixels a second while moving, and the pause at the far end, ms
        let measure = |hz: f64, gated: bool| -> (f64, f64) {
            let (v, mut r) = (vui(), NullRaster);
            let mut s = TextScroll::default();
            s.init(&v, &r, [100, 100], 160, 0, 1);
            s.window.open(true);
            let mut trace = Vec::new();
            for (t, due) in frames_at(hz, 1.0, 6000.0) {
                if due || !gated {
                    s.text_scroll(&v, &mut r, &name, [1.0; 4], 8);
                }
                trace.push((t, s.offset));
            }
            // the first run left: from the first move to the far end
            let start = trace.iter().find(|p| p.1 < 0).unwrap().0;
            let min = trace.iter().map(|p| p.1).min().unwrap();
            let end = trace.iter().find(|p| p.1 == min).unwrap().0;
            let back = trace.iter().skip_while(|p| p.1 != min).find(|p| p.1 != min).unwrap().0;
            (-(min as f64) / ((end - start) / 1000.0), back - end)
        };
        for hz in [30.0, 60.0, 144.0, 165.0] {
            let (speed, pause) = measure(hz, true);
            assert!((speed - 60.0).abs() < 4.0, "{hz} Hz: {speed:.1} px/s");
            assert!((pause - 1000.0).abs() < 40.0, "{hz} Hz: paused {pause:.0} ms");
        }
        // control: drawn every display frame at 165 Hz
        let (speed, pause) = measure(165.0, false);
        assert!(speed > 300.0 && pause < 200.0, "{speed:.1} px/s, paused {pause:.0} ms");
    }
}
