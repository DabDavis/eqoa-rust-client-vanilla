//! `VIWndSplash` (SETUP `viwndsplash.cpp`): the title. The painting
//! (`splash00.tga` and `splash10.tga`, the screen's halves), the splash
//! effects over it (`splashfx`), and the menu along the stone bar: Play,
//! Network Setup, Options, the chosen one yellow between the pulsing arrows
//! (`|q`..`|w`), the others white, laid out on the thirds of a window that
//! is never drawn (440 wide about (304, 360)).
//!
//! The menu takes keys once it has drawn 30 frames (`m_frames`/`m_ready`);
//! on a PS2 the memory card's check (MCERR) comes in between, and its end
//! sets the two sound classes' volumes from the options. Left and right move
//! (sound 11), X or Enter picks: Play (15), Network Setup (13), Options (15).
//! What a pick does is the setup's (`mod.rs`): on a PS2, Play checks the card
//! and its network configurations and opens Connect, and Network Setup fades
//! the title out to start Sony's network utility.

use crate::pointer::{Hits, MOVED, key, rows_rect};
use crate::ui::Layout;
use vi::ui::{Vui, Window, wide};

/// A pick off the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Play,
    NetworkSetup,
    Options,
}

/// What the menu's three entries say (WORDS.TXT): Play, Network Setup, Options.
const WORDS: [i32; 3] = [790, 1635, 422];

#[derive(Clone, Debug, Default)]
pub struct Splash {
    /// `m_window`: the menu's geometry
    window: Window,
    /// `m_sel`: 0 Play, 1 Network Setup, 2 Options
    pub sel: i32,
    /// `m_frames`: the start-up count
    pub frames: i32,
    /// `m_ready`: the menu takes keys
    pub ready: bool,
    hits: Hits,
}

impl Splash {
    /// `Init`: the first entry, the count from the start.
    pub fn init(&mut self) {
        self.sel = 0;
        self.frames = 0;
        self.ready = false;
    }

    /// `OnMessage` 295, the focus coming from outside its own windows: the
    /// menu's window (440 wide, a row, about (304, 360)) open at once.
    pub fn focus(&mut self, vui: &Vui, r: &dyn Layout) {
        self.window.init(vui, r, [304, 360], 440, 1, 0);
        self.window.open(true);
    }

    /// `OnDrawSelf`'s start-up count: true on the frame the menu becomes
    /// ready (when the game sets the two sound classes' volumes).
    fn count(&mut self) -> bool {
        if self.frames < 30 {
            self.ready = false;
            self.frames += 1;
        }
        if self.frames == 30 {
            // MCERR's check of the card is left out: as though it passed
            // (its 0x1106 counting 31 to 32)
            self.frames = 32;
        }
        if self.frames == 32 {
            self.frames = 34;
            self.ready = true;
            return true;
        }
        false
    }

    /// `OnDrawSelf`: the painting, the effects, the menu. `focused`: the
    /// title has the focus (else its arrows' pulse stays at its start).
    /// True on the frame the menu becomes ready.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Layout, fx: &mut super::splashfx::SplashFx, now: i64, focused: bool) -> bool {
        if !focused {
            self.window.reset_tick_pulse(now);
        }
        let became_ready = self.count();
        for (name, at) in [("splash00.tga", [0, 0]), ("splash10.tga", [320, 0])] {
            let id = vi::hash::hash_resource_id(name);
            vui.blit_id(r, id, at, 0, 1.0, 0);
            // a window wider than 4:3: the picture carried on past the frame
            crate::pregame::wide::half(r, id, at, 0);
        }
        fx.render(r, now, true);
        let w = self.window.size[0] / 3;
        let x = [0, w, w * 2, w * 3];
        let (white, yellow) = ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 0.0, 1.0]);
        // each entry's box: its third and 10 or 20 beyond
        let boxes = [(x[0] - 10, x[1] - x[0] + 20), (x[1] - 20, x[2] - x[1] + 20), (x[2] - 10, x[3] - x[2] + 20)];
        self.hits.clear();
        for (k, &(bx, bw)) in boxes.iter().enumerate() {
            let on = self.sel == k as i32;
            let word = vui.words.get(WORDS[k], vui.language);
            let s = if on { format!("|q{word}|w") } else { word.to_string() };
            self.window.text_box(vui, r, &wide(&s), bx, 0, bw, 1, if on { yellow } else { white }, 9);
            self.hits.push(Self::third(&self.window, vui, r, k as i32), k as i32);
        }
        became_ready
    }

    /// Where entry `k` takes a pointer: its third of the window's width
    /// (the boxes the entries are centred in overlap).
    fn third(w: &Window, vui: &Vui, r: &dyn Layout, k: i32) -> [i32; 4] {
        let third = w.size[0] / 3;
        let mut rr = rows_rect(w, vui, r, k * third, 0, third, 1);
        rr[0] = w.top_left()[0] + k * third;
        rr
    }

    /// `OnKeyDown`: the pick, if one was made.
    pub fn key(&mut self, k: i32, now: i64, cues: &mut Vec<i32>) -> Option<Pick> {
        match k {
            17 | key::LEFT => {
                if self.ready && self.sel > 0 {
                    self.sel -= 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                }
            }
            18 | key::RIGHT => {
                if self.ready && self.sel < 2 {
                    self.sel += 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                }
            }
            13 | key::CROSS if self.ready => {
                return Some(match self.sel {
                    0 => {
                        cues.push(15);
                        Pick::Play
                    }
                    1 => {
                        cues.push(13);
                        Pick::NetworkSetup
                    }
                    _ => {
                        cues.push(15);
                        Pick::Options
                    }
                });
            }
            _ => {}
        }
        None
    }

    /// A pointer at frame point `p`: the entry under it chosen and picked
    /// (its arrows' pulse starts over, as a key's move starts it).
    pub fn point(&mut self, p: [f32; 2], now: i64) -> Vec<i32> {
        let Some(k) = self.hits.at(p) else { return Vec::new() };
        if !self.ready {
            return Vec::new();
        }
        let mut keys = Vec::new();
        if k != self.sel {
            self.sel = k;
            self.window.reset_tick_pulse(now);
            keys.push(MOVED);
        }
        keys.push(key::CROSS);
        keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, mid, vui};

    fn drawn(s: &mut Splash, frames: usize) -> usize {
        let (v, mut r) = (vui(), NullRaster);
        let mut fx = super::super::splashfx::SplashFx::new(3);
        let mut ready_at = 0;
        for k in 0..frames {
            if s.draw(&v, &mut r, &mut fx, k as i64 * 33, true) {
                ready_at = k + 1;
            }
        }
        ready_at
    }

    /// The menu is deaf for its first 30 frames, then moves and picks.
    #[test]
    fn the_menu_waits_thirty_frames_then_moves_and_picks() {
        let (v, r) = (vui(), NullRaster);
        let mut s = Splash::default();
        s.init();
        s.focus(&v, &r);
        let mut cues = Vec::new();
        assert_eq!(drawn(&mut s, 29), 0);
        assert_eq!(s.key(18, 0, &mut cues), None);
        assert_eq!((s.sel, cues.len()), (0, 0), "deaf before it is ready");
        assert_eq!(drawn(&mut s, 1), 1, "ready on the thirtieth frame");
        s.key(18, 0, &mut cues);
        s.key(key::RIGHT, 0, &mut cues);
        s.key(18, 0, &mut cues);
        assert_eq!(s.sel, 2, "stops at Options");
        assert_eq!(cues, vec![11, 11], "a move is 11, the end nothing");
        assert_eq!(s.key(13, 0, &mut cues), Some(Pick::Options));
        s.key(17, 0, &mut cues);
        assert_eq!(s.key(key::CROSS, 0, &mut cues), Some(Pick::NetworkSetup));
        s.key(17, 0, &mut cues);
        assert_eq!(s.key(13, 0, &mut cues), Some(Pick::Play));
        assert_eq!(cues, vec![11, 11, 15, 11, 13, 11, 15]);
        // the control: the keyboard's up is not a move here
        s.key(19, 0, &mut cues);
        assert_eq!(s.sel, 0);
    }

    /// A pointer picks the entry it is on, once the menu is ready.
    #[test]
    fn a_pointer_picks_an_entry() {
        let (v, r) = (vui(), NullRaster);
        let mut s = Splash::default();
        s.init();
        s.focus(&v, &r);
        drawn(&mut s, 1);
        let options = Splash::third(&s.window, &v, &r, 2);
        let play = Splash::third(&s.window, &v, &r, 0);
        assert!(play[2] <= options[0], "the thirds do not overlap");
        assert!(s.point(mid(options), 0).is_empty(), "not ready: nothing");
        drawn(&mut s, 30);
        assert_eq!(s.point(mid(options), 0), vec![MOVED, key::CROSS]);
        assert_eq!(s.sel, 2);
        // the control: without the hit notes nothing is under the pointer
        s.hits.clear();
        assert!(s.point(mid(options), 0).is_empty());
    }
}
