//! The minimap indoors (petition 1494, a player's: "Mini map should map the
//! dungeons and inside building similar to the way frontiersforge minimap
//! did"; the server owner's call, 2026-11-11). The minimap shows the map's
//! pictures, each zone drawn from straight above: inside a building that is
//! its roof, in a cave the hill over it. FrontiersForge's minimap
//! (`scripts/mini_map`) draws the geometry round the player from above with a
//! "ceiling cut": what starts over a ceiling found above the player is left
//! out, so the floor and the walls' feet show instead of the roof. This is
//! that, in the game's own pieces:
//!
//! - the ceiling: the collision world probed straight up over the player and
//!   four points 3 units round it (`ceiling`; FrontiersForge's five probes, its
//!   2 of head room, 250 of search), a surface facing down hit going up or one
//!   facing up hit coming down (a roof, a floor of a storey above), the lowest
//!   at each point; under one at all five the player is in, and a change in or
//!   out holds `HOLD` before it counts, so a doorway does not flip it;
//! - the picture: inside, the zones round the player as the game draws them
//!   (the streamer's groups, in the game's own renderer), from straight above
//!   with the eye just under the ceiling (`MARGIN` under it), so the near
//!   plane cuts away the roof, every storey over the player and the hill over
//!   a cave, the floors under it fading into the dark by their depth; drawn
//!   into a picture of its own (`PIXELS` across) only when the player has
//!   moved a fifth of it, changed floor, or the ceiling moved, and every two
//!   seconds for doors and zones that came in, never a frame for nothing. It
//!   is drawn before the frame's own draw (in its own submit), so it costs one
//!   more pass over the few zones in view at those times;
//! - the minimap: in, it shows that picture at its own zoom (`SCALE`, a
//!   room's worth; the wheel's own limits), out, the map's pictures at the
//!   minimap's, as before. Marks, the player, the group and the rim are drawn
//!   over either as ever.

use render::map::MapLayer;
use render::{Renderer, wgpu};
use vi::coll::CollRay;
use vi::fpu::Ee;
use vi::scene::{Filter, FilterKind};
use vi::vect::Vect3;
use vi::world::World;

/// Head room before a surface over the player counts, and how far up one is
/// looked for (FrontiersForge's `head_clearance` and `ceiling_search`).
pub const CLEARANCE: f32 = 2.0;
pub const SEARCH: f32 = 250.0;
/// How far round the player the four other probes go.
pub const PROBE_R: f32 = 3.0;
/// The cut's distance under the ceiling found (FrontiersForge's
/// `ceiling_margin`).
pub const MARGIN: f32 = 3.0;
/// How long a change in or out must hold (seconds).
pub const HOLD: f32 = 0.5;
/// How far under the player floors are drawn, fading out toward it.
pub const BELOW: f32 = 90.0;
/// The picture's pixels across.
pub const PIXELS: u32 = 512;
/// The picture covers this many times what the minimap shows, so the player
/// walks a while before it is drawn again.
pub const COVER: f32 = 1.6;
/// The minimap's zoom indoors (world units a frame pixel: 120 units across)
/// and the wheel's limits there.
pub const SCALE: f32 = 1.2;
pub const SCALE_LIMITS: (f32, f32) = (0.5, 6.0);
/// The picture's light: the ambient high, the art nearly as it is (the
/// strip programs' light is clamped at 1), a little of the map's sun.
pub const INSIDE_AMBIENT: f32 = 0.75;
pub const INSIDE_SUN: f32 = 0.3;
/// What the picture clears to, and fades into below (the map's void).
pub const DARK: [f32; 3] = super::build::VOID;

/// A collision ray from `a` to `b` through the scene's world geometry (no
/// creature, `WalkFloor`'s filter): the height it hit at.
fn ray(w: &mut World<Ee>, a: [f32; 3], b: [f32; 3]) -> Option<f32> {
    let mut r = CollRay::<Ee>::default();
    r.init(Vect3::of(a[0], a[1], a[2]), Vect3::of(b[0], b[1], b[2]));
    let mut f = Filter::new(FilterKind::WalkFloor, -1);
    w.collide_ray(&mut r, &mut f).then(|| r.hit_point().y.0)
}

/// The lowest surface over (x, z) between `y + CLEARANCE` and `y + SEARCH`:
/// one facing down met going up (a cave's roof, a ceiling), or one facing up
/// met coming down (a roof seen from above, the floor of a storey over the
/// player), followed on down to the lowest. The game's collision is one-sided
/// (`collide_ray_tri`: a face is hit from its front), hence both ways.
fn ceiling_at(w: &mut World<Ee>, x: f32, y: f32, z: f32) -> Option<f32> {
    let (lo, hi) = (y + CLEARANCE, y + SEARCH);
    let up = ray(w, [x, lo, z], [x, hi, z]);
    let mut down = None;
    let mut from = hi;
    for _ in 0..6 {
        let Some(h) = ray(w, [x, from, z], [x, lo, z]) else { break };
        down = Some(h);
        from = h - 0.05;
        if from <= lo {
            break;
        }
    }
    [up, down].into_iter().flatten().reduce(f32::min)
}

/// The ceiling over the player at `p`: a surface over it at the middle and at
/// each of the four probes round it, the highest of those (FrontiersForge's
/// `CeilingAbove`); none where any probe has open sky.
pub fn ceiling(w: &mut World<Ee>, p: [f32; 3]) -> Option<f32> {
    let mut best: Option<f32> = None;
    for (dx, dz) in [(0.0, 0.0), (1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        let c = ceiling_at(w, p[0] + dx * PROBE_R, p[1], p[2] + dz * PROBE_R)?;
        best = Some(best.map_or(c, |b: f32| b.max(c)));
    }
    best
}

/// The height the picture is cut at under ceiling `c` for a player at `y`:
/// `MARGIN` under it, but never under the player's head room (a low ceiling
/// cut just under itself).
pub fn cut(c: f32, y: f32) -> f32 {
    let at = c - MARGIN;
    if at > y + CLEARANCE { at } else { (c - 0.25).max(y + 0.5) }
}

/// What the picture shows: the world rectangle (x0, z0, x1, z1, north up),
/// its cut, the player's height and the clock when drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slice {
    pub rect: [f32; 4],
    pub cut: f32,
    pub y: f32,
    pub at: f32,
}

impl Slice {
    /// Whether a player at `p` under cut `cut`, the minimap showing `half`
    /// each way, needs it drawn again at clock `now`.
    pub fn stale(&self, p: [f32; 3], cut: f32, half: f32, now: f32) -> bool {
        let c = [(self.rect[0] + self.rect[2]) / 2.0, (self.rect[1] + self.rect[3]) / 2.0];
        let have = (self.rect[2] - self.rect[0]) / 2.0;
        let moved = (p[0] - c[0]).hypot(p[2] - c[1]);
        moved > have * 0.2 || have < half * 1.05 || have > half * COVER * 1.5 || (cut - self.cut).abs() > 2.0 || (p[1] - self.y).abs() > 4.0 || now - self.at > 2.0
    }
}

/// The view a slice is drawn through: `rect` from straight above, north (-z)
/// up, the eye at `cut` (the near plane: nothing over it is drawn), floors
/// down to `BELOW` under the player at `y`, fading toward `DARK` from a
/// little under the player down.
pub fn slice_view(rect: [f32; 4], cut: f32, y: f32) -> render::View {
    use glam::Vec3;
    let c = Vec3::new((rect[0] + rect[2]) / 2.0, cut, (rect[1] + rect[3]) / 2.0);
    let (hw, hh) = ((rect[2] - rect[0]) / 2.0, (rect[3] - rect[1]) / 2.0);
    let view = glam::camera::rh::view::look_at_mat4(c, c - Vec3::Y, -Vec3::Z);
    let deep = (cut - y) + BELOW;
    let proj = glam::camera::rh::proj::directx::orthographic(-hw, hw, -hh, hh, 0.01, deep);
    render::View {
        view_proj: proj * view,
        eye: c,
        // the art nearly unlit (FrontiersForge's `world_lighting` off): a
        // dungeon's baked light is dark, by night most of all; slopes still
        // a little shaded
        sun_dir: Vec3::from(super::build::SUN_DIR),
        sun: Vec3::splat(INSIDE_SUN),
        ambient: Vec3::splat(INSIDE_AMBIENT),
        fog: Some(((cut - y) + 6.0, deep, Vec3::from(DARK))),
        point: None,
        clear: Vec3::from(DARK),
        sky_view_proj: None,
        lights: None,
    }
}

/// The minimap's indoor state, kept by the map.
#[derive(Default)]
pub struct Indoor {
    /// the minimap's zoom inside (`SCALE` until the wheel moves it)
    pub scale: Option<f32>,
    /// the seconds counted (the probes' and the picture's ages)
    clock: f32,
    /// the last probe: where, when, and the ceiling it found
    probed: Option<([f32; 3], f32, Option<f32>)>,
    /// in or out as it counts (held `HOLD`), the ceiling then, and since when
    /// the probes have said otherwise
    inside: bool,
    held_ceiling: Option<f32>,
    flip_since: Option<f32>,
    /// the picture (the map's layer's texture) and its depth buffer
    tex: Option<usize>,
    depth: Option<wgpu::TextureView>,
    /// what the picture shows, once drawn
    drawn: Option<Slice>,
    /// how many pictures were drawn, for the log
    pub draws: u64,
}

impl Indoor {
    /// The minimap's zoom inside.
    pub fn scale(&self) -> f32 {
        self.scale.unwrap_or(SCALE)
    }

    /// Whether the player counts as inside.
    pub fn inside(&self) -> bool {
        self.inside
    }

    /// The probe's answer for a player at `p` after `dt` seconds (`probe`
    /// asks the world when the last one is stale: a move of 2, a step of 2 up
    /// or down, half a second): in or out, held `HOLD` before it counts.
    pub fn probe(&mut self, w: &mut World<Ee>, p: [f32; 3], dt: f32) {
        self.clock += dt.clamp(0.0, 0.25);
        let stale = self.probed.is_none_or(|(q, t, _)| (p[0] - q[0]).hypot(p[2] - q[2]) > 2.0 || (p[1] - q[1]).abs() > 2.0 || self.clock - t > 0.5);
        if stale {
            let c = ceiling(w, p);
            if super::build::trace() {
                let each: Vec<String> = [(0.0, 0.0), (1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]
                    .iter()
                    .map(|(dx, dz)| {
                        let (x, z) = (p[0] + dx * PROBE_R, p[2] + dz * PROBE_R);
                        let up = ray(w, [x, p[1] + CLEARANCE, z], [x, p[1] + SEARCH, z]);
                        let down = ray(w, [x, p[1] + SEARCH, z], [x, p[1] + CLEARANCE, z]);
                        format!("{up:?}/{down:?}")
                    })
                    .collect();
                crate::say(&format!("minimap: probe at ({:.1}, {:.1}, {:.1}): {c:?} (up/down first hits {})", p[0], p[1], p[2], each.join(" ")));
            }
            self.probed = Some((p, self.clock, c));
        }
        let found = self.probed.and_then(|x| x.2);
        self.take(found);
    }

    /// A probe's answer `found` (the ceiling, none: open sky) taken into the
    /// held state at the clock now.
    pub fn take(&mut self, found: Option<f32>) {
        let raw = found.is_some();
        if raw == self.inside {
            self.flip_since = None;
            if raw {
                self.held_ceiling = found;
            }
            return;
        }
        match self.flip_since {
            None => self.flip_since = Some(self.clock),
            Some(t) if self.clock - t >= HOLD => {
                self.inside = raw;
                self.held_ceiling = found;
                self.flip_since = None;
                if !raw {
                    self.drawn = None;
                }
                crate::say(&format!("minimap: {}", if raw { "inside: the floors under the ceiling" } else { "outside: the map's pictures" }));
            }
            Some(_) => {}
        }
    }

    /// The clock moved on `dt` with no probe (the minimap not up): out, as at
    /// the start, the picture let go of when it was drawn.
    pub fn idle(&mut self) {
        self.inside = false;
        self.held_ceiling = None;
        self.flip_since = None;
        self.probed = None;
        self.drawn = None;
    }

    /// The layer the picture was on is gone (the art turned, the map off).
    pub fn forget(&mut self) {
        self.tex = None;
        self.depth = None;
        self.drawn = None;
    }

    /// The picture and the world rectangle it shows, while inside and drawn.
    pub fn picture(&self) -> Option<(usize, [f32; 4])> {
        Some((self.tex?, self.drawn.filter(|_| self.inside)?.rect))
    }

    /// The picture drawn again when it is stale (`Slice::stale`) for the player
    /// at `p`, the minimap showing `half` each way: the zones `items` through
    /// `renderer` (the game's own, before its frame: its own submit) into the
    /// layer's texture. Nothing outside.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, layer: &mut MapLayer, renderer: &Renderer, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, items: &[render::Draw], p: [f32; 3], half: f32) {
        let (true, Some(c)) = (self.inside, self.held_ceiling) else { return };
        let cut = cut(c, p[1]);
        if self.drawn.is_some_and(|d| !d.stale(p, cut, half, self.clock)) && self.tex.is_some() {
            return;
        }
        let tex = *self.tex.get_or_insert_with(|| layer.target(device, PIXELS, PIXELS, format));
        let Some(target) = layer.target_view(tex) else { return };
        let depth = self.depth.get_or_insert_with(|| render::depth_view(device, PIXELS, PIXELS));
        let h = half * COVER;
        let rect = [p[0] - h, p[2] - h, p[0] + h, p[2] + h];
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("minimap inside") });
        renderer.draw(queue, &mut enc, &target, depth, &slice_view(rect, cut, p[1]), items);
        queue.submit([enc.finish()]);
        self.drawn = Some(Slice { rect, cut, y: p[1], at: self.clock });
        self.draws += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A change in or out counts only once it has held `HOLD`: a doorway's
    /// flicker (in for a moment) leaves the minimap out; in for longer, it is
    /// in, with the ceiling found; out again the same way. The control: the
    /// raw answer alone would have flipped at the first probe.
    #[test]
    fn in_and_out_hold_before_they_count() {
        let mut d = Indoor::default();
        let step = |d: &mut Indoor, found: Option<f32>, dt: f32| {
            d.clock += dt;
            d.take(found);
        };
        step(&mut d, Some(30.0), 0.1);
        step(&mut d, Some(30.0), 0.2);
        assert!(!d.inside(), "not yet");
        step(&mut d, None, 0.1);
        step(&mut d, Some(30.0), 0.1);
        step(&mut d, Some(30.0), 0.3);
        assert!(!d.inside(), "the flicker started the hold again");
        step(&mut d, Some(31.0), 0.3);
        assert!(d.inside() && d.held_ceiling == Some(31.0));
        step(&mut d, None, 0.2);
        assert!(d.inside());
        step(&mut d, None, 0.4);
        assert!(d.inside(), "not held long enough yet");
        step(&mut d, None, 0.1);
        assert!(!d.inside(), "out once it held");
        assert!(d.picture().is_none());
    }

    /// The cut is `MARGIN` under the ceiling, but under a low one never into
    /// the player's head room.
    #[test]
    fn the_cut_stays_over_the_players_head() {
        assert_eq!(cut(40.0, 10.0), 37.0);
        assert_eq!(cut(14.0, 10.0), 13.75);
        assert!(cut(10.6, 10.0) >= 10.5);
    }

    /// A slice is drawn again when the player walks a fifth of it, steps a
    /// floor, the ceiling moves, the zoom outgrows it, or two seconds pass;
    /// not for a step inside it. The control: each change alone.
    #[test]
    fn a_slice_is_drawn_again_only_when_it_is_stale() {
        let s = Slice { rect: [0.0, 0.0, 192.0, 192.0], cut: 30.0, y: 10.0, at: 0.0 };
        let p = [96.0, 10.0, 96.0];
        assert!(!s.stale(p, 30.0, 60.0, 1.0));
        assert!(!s.stale([100.0, 10.0, 100.0], 30.0, 60.0, 1.0), "a step");
        assert!(s.stale([96.0 + 25.0, 10.0, 96.0], 30.0, 60.0, 1.0), "a fifth of it");
        assert!(s.stale([96.0, 15.0, 96.0], 30.0, 60.0, 1.0), "a floor down");
        assert!(s.stale(p, 33.0, 60.0, 1.0), "the ceiling");
        assert!(s.stale(p, 30.0, 100.0, 1.0), "zoomed out past it");
        assert!(s.stale(p, 30.0, 20.0, 1.0), "zoomed far in");
        assert!(s.stale(p, 30.0, 60.0, 2.5), "two seconds");
    }

    /// The slice's view: the eye at the cut looking down, north up; a point
    /// just under the cut is in front of the near plane and one just over it
    /// is behind it (cut away); one `BELOW` under the player is past the far
    /// plane. The control: a point at the player's floor is drawn.
    #[test]
    fn the_slice_cuts_away_what_is_over_the_ceiling() {
        let v = slice_view([0.0, 0.0, 100.0, 100.0], 30.0, 10.0);
        let depth = |y: f32| {
            let c = v.view_proj * glam::Vec4::new(50.0, y, 50.0, 1.0);
            c.z / c.w
        };
        assert!((0.0..=1.0).contains(&depth(29.0)), "under the cut");
        assert!(depth(31.0) < 0.0, "over the cut: cut away");
        assert!(depth(10.0 - BELOW - 1.0) > 1.0, "past the far plane");
        assert!((0.0..=1.0).contains(&depth(10.0)), "the control: the floor");
        // north (-z) is up, east (+x) right
        let at = |x: f32, z: f32| {
            let c = v.view_proj * glam::Vec4::new(x, 20.0, z, 1.0);
            [c.x / c.w, c.y / c.w]
        };
        assert!(at(50.0, 10.0)[1] > at(50.0, 90.0)[1] && at(90.0, 50.0)[0] > at(10.0, 50.0)[0]);
    }
}
