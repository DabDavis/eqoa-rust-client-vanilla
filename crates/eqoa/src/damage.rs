//! The floating damage numbers (`VIClientPart4FD28`, CLIENT 0x00BE8DF0 ..
//! 0x00BE9C9C, at `VIClient+0x4FD28`; the name is the decomp's, provisional).
//! 0x00DB reports a hit: `DamageFromSelf` for one the player made, over the
//! defender, and `DamageFromOther` for one on the player, over the player
//! (only from a walk camera that is not first person, with no manual camera
//! up); the game options' `m_x92` turns both off. A number rises and falls
//! over its target's slot 4 for a second, grows, fades in and out and shakes
//! for its first 0.6 s, white for a hit made, yellow for one taken, green for
//! a heal, in "damage_font.tga" (FX.CSF: a 4 x 4 grid of digits).
//!
//! The game projects through its own camera into its 640x448 screen; here
//! the point goes through the view that is drawn and into the HUD's frame
//! (`render::ui::frame_from_ndc`), cut only to the window, so a number over a
//! creature off the frame's sides is still drawn there.

// The decomp's own literal for the bounce, 3.1415, not pi.
#![allow(clippy::approx_constant)]

use glam::{Mat4, Vec3};
use render::ui::Quad;
use vi::csprite::Rand;

/// `VIDamageNumber`, 44 bytes on the console.
#[derive(Clone, Copy, Debug, Default)]
struct Number {
    amount: i32,
    /// From the actor's position to its slot 4, taken on the first frame.
    offset: [f32; 3],
    /// 0 a hit the player made, 1 one it took.
    z: i16,
    /// `m_slot[m_z]` when added: which bounce height.
    slot: i16,
    defender: u32,
    /// 0 when added; gone past 1 (1.1 a second).
    time: f32,
    /// the colour a server's patch sends with it (Sandstorm's S2: three
    /// bytes after 0x00DB's body), in place of the game's white, yellow or
    /// green
    rgb: Option<[u8; 3]>,
}

/// Where a number's entity is this frame (`GetActorPos`), and its slot 4
/// (`CalcSlotWorldTransform(4)`'s translation) when it is a character whose
/// sprite has that slot.
#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub pos: [f32; 3],
    pub slot4: Option<[f32; 3]>,
}

/// A number this frame: where it floats, and what `Draw` is handed; its
/// creature, and that creature's point it floats from (`Target::pos`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shown {
    pub at: Vec3,
    pub amount: i32,
    pub color: [f32; 4],
    pub time: f32,
    pub defender: u32,
    pub from: Vec3,
}

/// A number laid out on a frame of the game's: its creature, where it
/// floats from that creature's point (`rel`: slot 4's offset and the
/// bounce), where that was then (`at`, for a creature gone since), and its
/// digits' quads about the point.
struct Laid {
    defender: u32,
    rel: Vec3,
    at: Vec3,
    quads: Vec<Quad>,
}

pub struct Damage {
    slot: [i16; 2],
    numbers: [Number; 32],
    /// `m_active` and `m_free`, head first.
    active: Vec<usize>,
    free: Vec<usize>,
    rand: Rand,
    /// The scene particle system's frame delta (`VIParticleSystem::Render`:
    /// the clock's step capped at 0.1 s, the mean of the last eight), which
    /// `VIClient` hands `Process`.
    last: Option<i64>,
    history: [f32; 8],
    /// The numbers `frame` laid out: each one's creature and where it floats
    /// from it, and its digits' quads about that point (shaken, faded and
    /// grown as `Draw` drew them).
    laid: Vec<Laid>,
}

impl Default for Damage {
    /// `Init`: all 32 numbers on the free list, in order.
    fn default() -> Self {
        Damage { slot: [0; 2], numbers: [Number::default(); 32], active: Vec::new(), free: (0..32).collect(), rand: Rand(1), last: None, history: [0.0; 8], laid: Vec::new() }
    }
}

impl Damage {
    /// `AddDamage`: a free number, set up and put at the end of the active
    /// list; the side's bounce slot moves on (0, 1, 2). With none free,
    /// nothing.
    #[cfg(test)]
    pub fn add(&mut self, defender: u32, amount: i32, z: i16) {
        self.add_colored(defender, amount, z, None);
    }

    /// `AddDamage` with the colour a server's patch sent (`s2`: Sandstorm's
    /// damage colours), the game's own without one.
    pub fn add_colored(&mut self, defender: u32, amount: i32, z: i16, rgb: Option<[u8; 3]>) {
        if self.free.is_empty() {
            return;
        }
        let n = self.free.remove(0);
        self.numbers[n] = Number { amount, offset: [0.0; 3], z, slot: self.slot[z as usize], defender, time: 0.0, rgb };
        self.active.push(n);
        self.slot[z as usize] = (self.slot[z as usize] + 1) % 3;
    }

    /// How many numbers are up.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.active.len()
    }

    /// The particle system's delta for a frame at `now_ms`.
    fn delta(&mut self, now_ms: i64) -> f32 {
        let dt = (((now_ms - self.last.unwrap_or(now_ms)) as i32) as f32 * 0.001).min(0.1);
        self.last = Some(now_ms);
        let sum = (self.history[..7].iter().sum::<f32>() + dt) * 0.125;
        self.history.copy_within(0..7, 1);
        self.history[0] = dt;
        sum
    }

    /// `Process`'s list: each number in turn, dropped once its entity is
    /// gone or has no actor, or once it is past its second; its offset taken
    /// on its first frame; moved on and bounced. `find` is `FindEntity` over
    /// the 24 slots, answering for an entity with an actor.
    pub fn step(&mut self, now_ms: i64, find: impl Fn(u32) -> Option<Target>) -> Vec<Shown> {
        const BOUNCE: [f32; 3] = [0.8, 1.0, 1.25];
        let dt = self.delta(now_ms);
        let mut out = Vec::new();
        let mut k = 0;
        while k < self.active.len() {
            let i = self.active[k];
            let n = &mut self.numbers[i];
            let Some(e) = find(n.defender).filter(|_| n.time <= 1.0) else {
                self.active.remove(k);
                self.free.push(i);
                continue;
            };
            if n.time == 0.0 {
                n.offset = e.slot4.map_or([0.0; 3], |s| [s[0] - e.pos[0], s[1] - e.pos[1], s[2] - e.pos[2]]);
            }
            n.time += dt * 1.1;
            let mut at = Vec3::from(e.pos) + Vec3::from(n.offset);
            at.y += (n.time * 3.1415 * 0.7).sin() * BOUNCE[n.slot as usize];
            let color = if let Some([r, g, b]) = n.rgb {
                [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
            } else if n.amount > 0 {
                [0.0, 1.0, 0.0, 1.0]
            } else if n.z != 0 {
                [0.8, 0.8, 0.1, 1.0]
            } else {
                [1.0, 1.0, 1.0, 1.0]
            };
            out.push(Shown { at, amount: n.amount, color, time: n.time, defender: n.defender, from: Vec3::from(e.pos) });
            k += 1;
        }
        out
    }

    /// `Process`'s drawing: each number inside the camera's frustum (its
    /// near 0.1 and far 900) projected into the HUD's frame and drawn.
    #[cfg(test)]
    pub fn quads(&mut self, shown: &[Shown], view_proj: Mat4, size: (u32, u32), tex: usize) -> Vec<Quad> {
        let mut out = Vec::new();
        for s in shown {
            let h = view_proj * s.at.extend(1.0);
            if !in_frustum(h) {
                continue;
            }
            let at = render::ui::frame_from_ndc(size, [h.x / h.w, h.y / h.w]);
            self.draw(&mut out, tex, at, s.amount, s.color, s.time);
        }
        out
    }

    /// `Process` on a frame of the game's (`frames.rs`): the numbers moved
    /// on, and those in the frustum laid out as `Draw` draws them (its
    /// `rand()`s are the game's, a frame each). Where they go on the screen
    /// is `place`'s, every frame drawn, so a number keeps to its creature
    /// while the view and the creature move between the game's frames.
    pub fn frame(&mut self, tex: Option<usize>, now_ms: i64, view_proj: Mat4, find: impl Fn(u32) -> Option<Target>) {
        let shown = self.step(now_ms, find);
        self.laid.clear();
        let Some(tex) = tex else { return };
        for s in shown {
            if !in_frustum(view_proj * s.at.extend(1.0)) {
                continue;
            }
            let mut quads = Vec::new();
            self.draw(&mut quads, tex, [0.0, 0.0], s.amount, s.color, s.time);
            self.laid.push(Laid { defender: s.defender, rel: s.at - s.from, at: s.at, quads });
        }
    }

    /// The laid-out numbers through this frame's view, into the HUD's frame:
    /// the quads to draw under the HUD. Each floats from its creature where
    /// `find` says it is drawn this frame (the game moves both on its own
    /// frames; a display's rate moves the creature between them, and a
    /// number kept at the game's frame's point lagged its runner and jumped
    /// after it), else from where it was laid out.
    pub fn place(&self, view_proj: Mat4, size: (u32, u32), find: impl Fn(u32) -> Option<[f32; 3]>) -> Vec<Quad> {
        let mut out = Vec::new();
        for l in &self.laid {
            let at = find(l.defender).map_or(l.at, |p| Vec3::from(p) + l.rel);
            let quads = &l.quads;
            let h = view_proj * at.extend(1.0);
            if !in_frustum(h) {
                continue;
            }
            let p = render::ui::frame_from_ndc(size, [h.x / h.w, h.y / h.w]);
            for q in quads {
                let mut q = *q;
                for v in &mut q.pos {
                    v[0] += p[0];
                    v[1] += p[1];
                }
                q.clip = Some(render::ui::WHOLE);
                out.push(q);
            }
        }
        out
    }

    /// `Draw`: the digits of |amount| from `pos` less half their width and
    /// 13 up, each 12x24 scaled by 0.9 + 1.2 t and 10 apart, fading in over
    /// 0.2 s and out from 0.5, each corner thrown up to 10 pixels while it
    /// shakes (two `rand()`s a corner, drawn whether it shakes or not; the
    /// y's first, as the oracle's compiler evaluates a call's arguments).
    fn draw(&mut self, out: &mut Vec<Quad>, tex: usize, pos: [f32; 2], amount: i32, col: [f32; 4], t: f32) {
        let a = if t < 0.2 {
            t / 0.2
        } else if t > 0.5 {
            1.0 - (t - 0.5) * 2.0
        } else {
            1.0
        };
        let shake = if t < 0.6 { 1.0 - t / 0.6 } else { 0.0 };
        let a = a.clamp(0.0, 1.0);
        let text = amount.wrapping_abs().to_string();
        let len = text.len() as f32;
        let scale = t * 1.2 + 0.9;
        let mut p = [pos[0] - (len * 10.0 * 0.5 * scale + scale), pos[1] - scale * 13.0];
        let corners = [[0.0, 0.0], [scale * 12.0, 0.0], [scale * 12.0, scale * 24.0], [0.0, scale * 24.0]];
        for c in text.bytes() {
            let uv = glyph_uv(c as i32 - '0' as i32);
            let mut q = [[0.0f32; 2]; 4];
            for (v, c) in q.iter_mut().zip(corners) {
                let y = ((self.rand.rand() & 0xFF) - 128) as f32 * (1.0 / 128.0) * 10.0 * shake;
                let x = ((self.rand.rand() & 0xFF) - 128) as f32 * (1.0 / 128.0) * 10.0 * shake;
                *v = [p[0] + c[0] + x, p[1] + c[1] + y];
            }
            out.push(Quad { tex, blend: 3, pos: q, uv, color: [col[0], col[1], col[2], a], clip: None });
            p[0] += scale * 10.0;
        }
    }
}

/// Inside the camera's frustum as `Process` tests it (its near 0.1 and far
/// 900), for a point in clip space.
fn in_frustum(h: glam::Vec4) -> bool {
    h.w > 0.1 && h.w <= 900.0 && h.x.abs() <= h.w && h.y.abs() <= h.w
}

/// `GlyphUV`: a digit's four corners in the 4 x 4 grid.
fn glyph_uv(digit: i32) -> [[f32; 2]; 4] {
    let v = (digit >> 2) as f32 * 0.25;
    let u = (digit % 4) as f32 * 0.25;
    [[u, v], [u + 0.25, v], [u + 0.25, v + 0.25], [u, v + 0.25]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(id: u32) -> impl Fn(u32) -> Option<Target> {
        move |e| (e == id).then_some(Target { pos: [10.0, 0.0, 20.0], slot4: Some([10.5, 2.0, 20.0]) })
    }

    #[test]
    fn numbers_cycle_their_bounce_and_the_list_holds_32() {
        let mut d = Damage::default();
        for k in 0..5 {
            d.add(7, -(k + 1), 0);
        }
        d.add(9, -3, 1);
        let slots: Vec<i16> = d.active.iter().map(|&i| d.numbers[i].slot).collect();
        assert_eq!(slots, [0, 1, 2, 0, 1, 0]);
        assert_eq!(d.slot, [2, 1]);
        for _ in 0..40 {
            d.add(7, -1, 0);
        }
        assert_eq!(d.len(), 32);
        // the 26 that fitted moved the slot; the 14 that did not, did not
        assert_eq!(d.slot[0], (2 + 26) % 3);
    }

    #[test]
    fn the_digit_grid_is_four_by_four() {
        assert_eq!(glyph_uv(0)[0], [0.0, 0.0]);
        assert_eq!(glyph_uv(5)[0], [0.25, 0.25]);
        assert_eq!(glyph_uv(9)[2], [0.5, 0.75]);
        // control: column-major would put 1 under 0
        assert_ne!(glyph_uv(1)[0], [0.0, 0.25]);
    }

    #[test]
    fn the_delta_is_capped_and_averaged_over_eight_frames() {
        let mut d = Damage::default();
        assert_eq!(d.delta(1000), 0.0);
        assert_eq!(d.delta(1033), 0.033 * 0.125);
        // a 2-second stall counts as 0.1
        let s = d.delta(3033);
        assert!((s - (0.033 + 0.1) * 0.125).abs() < 1e-7, "{s}");
    }

    #[test]
    fn a_number_floats_from_slot_4_for_a_second_then_frees() {
        let mut d = Damage::default();
        d.add(5, -12, 0);
        d.add(5, 40, 1);
        d.add(6, -1, 1);
        let mut frames = 0;
        let mut first = None;
        for f in 0..200 {
            let shown = d.step(f * 33, at(5));
            if f == 20 {
                first = Some(shown.clone());
            }
            if d.len() == 0 {
                break;
            }
            frames += 1;
        }
        // the one over entity 6 went at once; the others a second or so on
        assert!((28..=45).contains(&frames), "{frames} frames");
        assert_eq!(d.free.len(), 32);
        let s = first.unwrap();
        assert_eq!(s.len(), 2);
        // over the slot, bounced: entity 5 at (10, 0, 20), slot 4 at (10.5, 2, 20)
        assert_eq!((s[0].at.x, s[0].at.z), (10.5, 20.0));
        let bounce0 = (s[0].time * 3.1415 * 0.7).sin() * 0.8;
        assert_eq!(s[0].at.y, 2.0 + bounce0);
        assert_eq!((s[0].color, s[1].color), ([1.0, 1.0, 1.0, 1.0], [0.0, 1.0, 0.0, 1.0]));
        // the heal is its side's (z 1) first: bounce slot 0 as well
        let bounce1 = (s[1].time * 3.1415 * 0.7).sin() * 0.8;
        assert_eq!(s[1].at.y, 2.0 + bounce1);
    }

    /// A number a server's patch coloured (Sandstorm's S2) is drawn in that
    /// colour, hit or heal, made or taken; one with none in the game's (the
    /// control).
    #[test]
    fn a_patchs_colour_is_the_numbers() {
        let mut d = Damage::default();
        let at = |id: u32| move |e: u32| (e == id).then_some(Target { pos: [0.0; 3], slot4: None });
        d.add_colored(5, -30, 0, Some([255, 128, 0]));
        d.add_colored(5, 12, 1, Some([0, 0, 255]));
        d.add(5, -30, 0);
        let s = d.step(33, at(5));
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].color, [1.0, 128.0 / 255.0, 0.0, 1.0]);
        assert_eq!(s[1].color, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(s[2].color, [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn the_digits_grow_fade_and_stop_shaking() {
        let mut d = Damage::default();
        let quads = |d: &mut Damage, t: f32| {
            let mut out = Vec::new();
            d.draw(&mut out, 0, [320.0, 224.0], -123, [1.0; 4], t);
            out
        };
        // past 0.6 s, still: the corners exact
        let q = quads(&mut d, 0.7);
        assert_eq!(q.len(), 3);
        let s = 0.7f32 * 1.2 + 0.9;
        let p0 = [320.0 - (3.0 * 10.0 * 0.5 * s + s), 224.0 - s * 13.0];
        assert_eq!(q[0].pos[0], p0);
        assert_eq!(q[0].pos[2], [p0[0] + s * 12.0, p0[1] + s * 24.0]);
        assert_eq!(q[1].pos[0][0], p0[0] + s * 10.0);
        assert_eq!(q[0].uv[0], glyph_uv(1)[0]);
        assert!((q[0].color[3] - 0.6).abs() < 1e-6);
        // at 0.1 s it is fading in and every corner is thrown, by up to 10 x 5/6
        let q = quads(&mut d, 0.1);
        assert!((q[0].color[3] - 0.5).abs() < 1e-6);
        let s = 0.1f32 * 1.2 + 0.9;
        let p0 = [320.0 - (3.0 * 10.0 * 0.5 * s + s), 224.0 - s * 13.0];
        let off = [q[0].pos[0][0] - p0[0], q[0].pos[0][1] - p0[1]];
        assert!(off[0].abs() <= 10.0 * (1.0 - 0.1 / 0.6) && off[1].abs() <= 10.0 * (1.0 - 0.1 / 0.6));
        assert!(off != [0.0, 0.0], "a shaking digit's corner did not move");
        // eight rand()s a digit, shaking or not
        let mut a = Damage::default();
        let mut b = Damage::default();
        quads(&mut a, 0.7);
        for _ in 0..24 {
            b.rand.rand();
        }
        assert_eq!(a.rand.0, b.rand.0);
    }

    #[test]
    fn only_numbers_in_the_frustum_are_drawn() {
        let view = glam::camera::rh::view::look_to_mat4(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y);
        let proj = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), 4.0 / 3.0, 0.3, 6000.0);
        let vp = proj * view;
        let mut d = Damage::default();
        let show = |z: f32, x: f32| Shown { at: Vec3::new(x, 0.0, z), amount: -5, color: [1.0; 4], time: 0.7, defender: 0, from: Vec3::ZERO };
        // ahead, behind, past the far plane, off to the side
        let q = d.quads(&[show(-10.0, 0.0), show(10.0, 0.0), show(-901.0, 0.0), show(-10.0, 50.0)], vp, (640, 480), 0);
        assert_eq!(q.len(), 1);
        // straight ahead is the frame's centre, less the number's offset
        let s = 0.7f32 * 1.2 + 0.9;
        assert!((q[0].pos[0][0] - (320.0 - (10.0 * 0.5 * s + s))).abs() < 1e-3);
    }

    /// Laid out on a frame of the game's and placed through the same view,
    /// the quads are `Draw`'s through that view, its `rand()`s and all; placed
    /// through a view turned since, they follow the number on the screen
    /// (where the game's frame left them they would lag the creature).
    #[test]
    fn numbers_laid_out_once_a_frame_follow_the_view_between() {
        let proj = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), 4.0 / 3.0, 0.3, 6000.0);
        let vp = proj * glam::camera::rh::view::look_to_mat4(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y);
        let find = |id: u32| (id == 3).then_some(Target { pos: [1.0, 0.0, -12.0], slot4: Some([1.0, 2.0, -12.0]) });
        let (mut a, mut b) = (Damage::default(), Damage::default());
        a.add(3, -42, 0);
        b.add(3, -42, 0);
        // the same frames through `step` + `quads` and through `frame` + `place`
        let mut drawn = Vec::new();
        let mut placed = Vec::new();
        for t in [1000, 1033, 1067, 1100] {
            let shown = a.step(t, find);
            drawn = a.quads(&shown, vp, (640, 448), 7);
            b.frame(Some(7), t, vp, find);
            placed = b.place(vp, (640, 448), |id| find(id).map(|t| t.pos));
        }
        assert_eq!(drawn.len(), 2, "two digits");
        for (d, p) in drawn.iter().zip(&placed) {
            for k in 0..4 {
                assert!((d.pos[k][0] - p.pos[k][0]).abs() < 1e-3 && (d.pos[k][1] - p.pos[k][1]).abs() < 1e-3, "{:?} {:?}", d.pos, p.pos);
            }
            assert_eq!(d.color, p.color);
        }
        assert_eq!(a.rand.0, b.rand.0, "the same rand()s");
        // the view turned a little before the next of the game's frames: the
        // digits move with the point they float over
        let turned = proj * glam::camera::rh::view::look_to_mat4(Vec3::ZERO, Vec3::new(0.05, 0.0, -1.0), Vec3::Y);
        let moved = b.place(turned, (640, 448), |id| find(id).map(|t| t.pos));
        let dx = moved[0].pos[0][0] - placed[0].pos[0][0];
        assert!(dx < -5.0, "the view turned right, the number moves left ({dx})");
        for (m, p) in moved.iter().zip(&placed) {
            assert!((m.pos[2][0] - m.pos[0][0] - (p.pos[2][0] - p.pos[0][0])).abs() < 1e-3, "the same digit, moved whole");
        }
    }

    /// A creature that runs on between two of the game's frames (a display
    /// faster than 30 Hz) takes its number with it: laid out where it was
    /// and placed where it is drawn now, the number is where one laid out
    /// there would be, digits, bounce and shake alike (F3 20261024-201347,
    /// "pop-up numbers very jittery when moving"). The control: placed from
    /// where it was laid (a creature not found, as every number was placed
    /// before), it stays behind its runner.
    #[test]
    fn a_number_keeps_to_its_creature_between_the_games_frames() {
        let proj = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), 4.0 / 3.0, 0.3, 6000.0);
        let vp = proj * glam::camera::rh::view::look_to_mat4(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y);
        let at = |x: f32| move |id: u32| (id == 3).then_some(Target { pos: [x, 0.0, -12.0], slot4: Some([x, 2.0, -12.0]) });
        let (mut was, mut now) = (Damage::default(), Damage::default());
        was.add(3, -42, 0);
        now.add(3, -42, 0);
        for t in [1000, 1033] {
            was.frame(Some(7), t, vp, at(1.0));
            now.frame(Some(7), t, vp, at(1.5));
        }
        let here = |x: f32| move |id: u32| at(x)(id).map(|t| t.pos);
        let ran = was.place(vp, (640, 448), here(1.5));
        let there = now.place(vp, (640, 448), here(1.5));
        assert_eq!(ran.len(), 2);
        for (a, b) in ran.iter().zip(&there) {
            for k in 0..4 {
                assert!((a.pos[k][0] - b.pos[k][0]).abs() < 1e-3 && (a.pos[k][1] - b.pos[k][1]).abs() < 1e-3, "{:?} {:?}", a.pos, b.pos);
            }
        }
        // the control: from where it was laid, half a unit behind
        let left = was.place(vp, (640, 448), |_| None);
        let dx = there[0].pos[0][0] - left[0].pos[0][0];
        assert!(dx > 5.0, "left behind by {dx} pixels");
        assert_eq!(left.iter().map(|q| q.pos).collect::<Vec<_>>(), was.place(vp, (640, 448), here(1.0)).iter().map(|q| q.pos).collect::<Vec<_>>());
    }
}
