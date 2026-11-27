//! The radial flora (`VIRadialFloraSystem`, `vi::flora`, held to the decomp
//! by the decomp's oracle, `oracle_flora`): the plants the game grows round the camera
//! out of each zone's flora tables, a 3 x 3 grid of 40-unit swatches that
//! follows it, the plants within 40 units drawn farthest first and faded out
//! from 10. `VIScene::Render` updates it at the camera before the world draws
//! and draws it between the rooms' deferred list and the actors': here its
//! draws go after the zone groups and before the moving models (`viewer`).
//!
//! The client's: zones stream in after arriving, where CLIENT resets the
//! flora once, after `EnterWorld` has loaded the zones round the player. So
//! a swatch made over a zone not in yet (and grown empty) is let go when the
//! zone comes in, and the next update makes it again as the game would have
//! made it (each swatch is seeded off its own sub-cell). Each flora sprite is
//! a model of its own on the GPU (`Renderer::upload_flora`), made when a
//! plant of it is first drawn, with room for Render's 700; a run of plants of
//! one model in the drawing order is one draw (`Draw::Instances`), so they
//! are drawn in the game's order.
//!
//! Two of the client's own (Heirloom petition 3197, 2026-11-26, Graphics >
//! Foliage), the game's by default: Foliage Density, more plants grown by
//! the game's own rule (`vi::flora::System::density`: the game's plants
//! first, then a pass or two more off seeds of their own), with room for
//! that many more drawn; and Foliage Reacts, the plants round a player or a
//! creature leaning away from it and easing back once it has passed
//! (`React`): each plant's instance matrix sheared at its root, so the
//! vertex shader bends it (its top furthest) with nothing new on the GPU.
//! A third (Heirloom petition 3356, 2026-11-27): Foliage Distance, the
//! plants drawn out to 1.5, 2 or 3 times the game's 40 units on a grid of as
//! many more swatches, the game's fade stretched to the radius and the outer
//! quarter grown in from nothing (`vi::flora::System::distance`); Retail,
//! the game's, exactly as before.

use esf::Library;
use glam::Mat4;
use render::wgpu;
use std::collections::HashMap;
use vi::flora::{Drawn, Frustum, System};
use vi::fpu::{Ee, F};
use vi::geom::Plane;
use vi::vect::Vect3;
use vi::world::World;

/// The plants of `drawn` that the options' Flora draws at `share`
/// (`settings::share_keeps`): each by where it stands, so a plant is drawn
/// at every frame or at none, whatever the order `Render` sorts them in; at
/// all of it `drawn` as it is (the game's), at 0 none. The flora's state
/// (`vi::flora`, its swatches and `rand()`) is the game's either way.
pub fn kept(mut drawn: Vec<Drawn<Ee>>, share: u8) -> Vec<Drawn<Ee>> {
    if share < crate::settings::SHARE_FULL {
        drawn.retain(|d| crate::settings::share_keeps(plant_key(d), share));
    }
    drawn
}

/// A plant's key: where it stands (its world matrix's place, to the bit).
fn plant_key(d: &Drawn<Ee>) -> u64 {
    let t = &d.world.m[3];
    let (x, y, z) = (t.x.0.to_bits() as u64, t.y.0.to_bits() as u64, t.z.0.to_bits() as u64);
    x ^ z.rotate_left(32) ^ y.rotate_left(16)
}

#[derive(Default)]
pub struct Flora {
    sys: System<Ee>,
    /// Each flora sprite's model (by scene sprite), or None where it builds none.
    models: HashMap<usize, Option<render::GpuModel>>,
    /// Which zones were in at the last frame (`World::loaded`).
    loaded: Vec<bool>,
    /// This frame's draws: a model and its instances' range.
    runs: Vec<(usize, std::ops::Range<u32>)>,
    /// The plants grown and drawn at the last frame (`describe`).
    grown: usize,
    drawn: usize,
    /// Foliage Reacts' leans, and this frame's, one a plant drawn (empty
    /// with it off).
    react: React,
    leans: Vec<[f32; 2]>,
}

impl Flora {
    pub fn new() -> Self {
        Self::default()
    }

    /// `VIScene::Render`'s two calls: `Update(cam.m_position)`, then
    /// `Render(cam)` through the view's frustum (`view_proj`'s planes, the
    /// window's own shape: a wide window sees the plants at its sides).
    /// `movers`: the players and creatures round the eye, which the plants
    /// lean from with Foliage Reacts on.
    pub fn frame(&mut self, world: &mut World<Ee>, eye: &Vect3<Ee>, view_proj: Mat4, movers: &[Mover]) -> Vec<Drawn<Ee>> {
        let c = crate::settings::client();
        // Foliage Density: every swatch grown again, and the models made
        // again with room for as many more drawn
        let density = c.foliage_density.clamp(1, vi::flora::DENSITY_MOST);
        // and Foliage Distance: the grid grown again, its models with room
        // for the radius's area
        let distance = crate::settings::foliage_distance_drawn(&c);
        if self.sys.density != density || self.sys.distance != distance {
            self.sys.density = density;
            self.sys.distance = distance;
            self.sys.reset();
            self.models.clear();
            self.runs.clear();
        }
        let came: Vec<i32> = world.loaded.iter().enumerate().filter(|&(z, &now)| now && !self.loaded.get(z).copied().unwrap_or(false)).map(|(z, _)| z as i32).collect();
        if !came.is_empty() {
            for s in self.sys.swatches.iter_mut() {
                if s.as_ref().is_some_and(|s| came.contains(&s.loc.zone)) {
                    *s = None;
                }
            }
        }
        self.loaded.clone_from(&world.loaded);
        self.sys.update(world, eye);
        let mut fr = Frustum::<Ee>::default();
        for (k, p) in crate::particles::frustum(view_proj).iter().enumerate() {
            fr.planes[k] = Plane { n: Vect3::of(-p.x, -p.y, -p.z), d: F::new(-p.w) };
        }
        let drawn = kept(self.sys.render(&fr, eye), c.flora);
        self.grown = self.sys.swatches.iter().flatten().map(|s| s.insts.len()).sum();
        self.drawn = drawn.len();
        self.leans.clear();
        if c.foliage_reacts {
            let at = [eye.x.0, eye.y.0, eye.z.0];
            self.leans = self.react.frame(&drawn, &nearest(movers, at, self.sys.radius().0), web_time::Instant::now());
        } else {
            self.react = React::default();
        }
        drawn
    }

    /// The plants `frame` gave to the GPU: each model made the first time a
    /// plant of it is drawn (`world.sprite_refs`: the flora sprite's record),
    /// its instances and their fades written in the drawing order, and a run
    /// of one model a draw.
    pub fn upload(&mut self, renderer: &mut render::Renderer, device: &wgpu::Device, queue: &wgpu::Queue, lib: &Library, world: &World<Ee>, drawn: &[Drawn<Ee>]) {
        let mut per: HashMap<usize, (Vec<[[f32; 4]; 4]>, Vec<[f32; 4]>)> = HashMap::new();
        self.runs.clear();
        for (k, d) in drawn.iter().enumerate() {
            let Ok(s) = usize::try_from(d.prim) else { continue };
            let e = per.entry(s).or_default();
            let at = e.0.len() as u32;
            let cols = d.world.to_cols();
            e.0.push(match self.leans.get(k) {
                Some(&l) => bent(cols, l),
                None => cols,
            });
            e.1.push([0.0, 0.0, 0.0, d.alpha.0]);
            match self.runs.last_mut() {
                Some((m, r)) if *m == s && r.end == at => r.end += 1,
                _ => self.runs.push((s, at..at + 1)),
            }
        }
        for (s, (instances, fades)) in &per {
            let model = self.models.entry(*s).or_insert_with(|| {
                let r = world.sprite_refs.get(*s)?;
                let m = lib.file(r.file).build_model(r.node)?;
                Some(renderer.upload_flora(device, queue, &m, self.sys.drawn_most(), &render::library_textures(lib)))
            });
            if let Some(g) = model {
                renderer.set_instances(queue, g, instances);
                renderer.set_static_lights(queue, g, fades);
            }
        }
        let models = &self.models;
        self.runs.retain(|(s, _)| models.get(s).is_some_and(|m| m.is_some()));
    }

    /// This frame's draws, farthest plant first.
    pub fn draws(&self) -> impl Iterator<Item = render::Draw<'_>> {
        self.runs.iter().filter_map(|(s, r)| self.models.get(s)?.as_ref().map(|m| render::Draw::Instances(m, r.clone())))
    }

    /// Lets every plant go (Flora turned off): the swatches are grown again
    /// when it is turned on.
    pub fn clear(&mut self) {
        self.sys.reset();
        self.runs.clear();
        self.grown = 0;
        self.drawn = 0;
        self.react = React::default();
        self.leans.clear();
    }

    pub fn describe(&self) -> String {
        let swatches = self.sys.swatches.iter().flatten().count();
        let leaning = self.leans.iter().filter(|l| **l != [0.0, 0.0]).count();
        format!("flora: {} plants in {swatches} swatches, {} drawn in {} runs, {} models, density {}, distance {}%, {leaning} leaning", self.grown, self.drawn, self.runs.len(), self.models.len(), self.sys.density(), self.sys.distance())
    }
}

/// A body the plants lean from (Foliage Reacts): where its feet are and how
/// far round it reaches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mover {
    pub at: [f32; 3],
    pub reach: f32,
}

/// The most bodies the plants lean from at once: the nearest the eye.
pub const MOVERS: usize = 8;
/// How far past the flora's radius (the game's 40, or Foliage Distance's) a
/// body is looked for round the eye: a body's reach.
const MOVER_PAST: f32 = 8.0;
/// A plant's lean at its most: its top this many of its heights to the side
/// (about 35 degrees).
pub const LEAN_MOST: f32 = 0.7;
/// How far past a body's reach the plants still feel it (their own spread).
const SPREAD: f32 = 1.0;
/// A body this far above or below a plant's root leaves it (on a bridge, a
/// roof, a ledge).
const OVER: f32 = 5.0;
/// The ease's time constants, in seconds: quick to lean, slower back.
const EASE_IN: f32 = 0.08;
const EASE_OUT: f32 = 0.6;

/// The `MOVERS` bodies nearest `eye`, within the flora's `radius` and
/// `MOVER_PAST`, nearest first.
pub fn nearest(movers: &[Mover], eye: [f32; 3], radius: f32) -> Vec<Mover> {
    let d2 = |m: &Mover| (0..3).map(|k| (m.at[k] - eye[k]) * (m.at[k] - eye[k])).sum::<f32>();
    let range = radius + MOVER_PAST;
    let mut v: Vec<(f32, Mover)> = movers.iter().map(|m| (d2(m), *m)).filter(|(d, m)| *d < range * range && m.reach > 0.0 && d.is_finite()).collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v.into_iter().take(MOVERS).map(|(_, m)| m).collect()
}

/// The lean a plant rooted at `root` takes from `movers` (x and z, in its
/// heights): away from each body within its reach and `SPREAD`, by
/// smoothstep of how near (all of `LEAN_MOST` at its feet), summed and held
/// to `LEAN_MOST`.
pub fn lean(root: [f32; 3], movers: &[Mover]) -> [f32; 2] {
    let mut l = [0.0f32; 2];
    for m in movers {
        if (root[1] - m.at[1]).abs() > OVER {
            continue;
        }
        let (dx, dz) = (root[0] - m.at[0], root[2] - m.at[2]);
        let r = m.reach + SPREAD;
        let d = (dx * dx + dz * dz).sqrt();
        if d >= r {
            continue;
        }
        let t = 1.0 - d / r;
        let k = LEAN_MOST * t * t * (3.0 - 2.0 * t);
        // right under it: away along x
        let (ux, uz) = if d > 1e-4 { (dx / d, dz / d) } else { (1.0, 0.0) };
        l[0] += ux * k;
        l[1] += uz * k;
    }
    let n = (l[0] * l[0] + l[1] * l[1]).sqrt();
    if n > LEAN_MOST {
        l = [l[0] * LEAN_MOST / n, l[1] * LEAN_MOST / n];
    }
    l
}

/// `now` eased toward `to` over `dt` seconds: at `EASE_IN` while leaning
/// further, at `EASE_OUT` on the way back.
pub fn ease(now: [f32; 2], to: [f32; 2], dt: f32) -> [f32; 2] {
    let n2 = |v: [f32; 2]| v[0] * v[0] + v[1] * v[1];
    let tau = if n2(to) > n2(now) { EASE_IN } else { EASE_OUT };
    let a = 1.0 - (-dt.max(0.0) / tau).exp();
    [now[0] + (to[0] - now[0]) * a, now[1] + (to[1] - now[1]) * a]
}

/// A plant's instance matrix (column-major: the columns its local x, y, z
/// in the world and its root) leaning by `l`: its up column sheared toward
/// (l.x, 0, l.z) by its own length and shortened back to that length, so a
/// vertex moves by its height times the lean, the root stays and the plant
/// keeps its height along its stem. No lean: the matrix as it was, to the bit.
pub fn bent(mut cols: [[f32; 4]; 4], l: [f32; 2]) -> [[f32; 4]; 4] {
    if l == [0.0, 0.0] {
        return cols;
    }
    let up = cols[1];
    let len = (up[0] * up[0] + up[1] * up[1] + up[2] * up[2]).sqrt();
    let s = 1.0 / (1.0 + l[0] * l[0] + l[1] * l[1]).sqrt();
    cols[1] = [(up[0] + l[0] * len) * s, up[1] * s, (up[2] + l[1] * len) * s, up[3]];
    cols
}

/// Foliage Reacts' plants: each drawn one's lean, by where it stands
/// (`plant_key`), eased from frame to frame; a plant first drawn (grown,
/// come into the view, or in a shot) starts at its lean, as it would stand
/// with the body there; one no longer drawn is let go.
#[derive(Default)]
pub struct React {
    bent: HashMap<u64, [f32; 2]>,
    last: Option<web_time::Instant>,
}

impl React {
    /// The leans of `drawn` (one each, in its order) at `now`, from `movers`.
    pub fn frame(&mut self, drawn: &[Drawn<Ee>], movers: &[Mover], now: web_time::Instant) -> Vec<[f32; 2]> {
        let dt = self.last.map_or(0.0, |t| now.saturating_duration_since(t).as_secs_f32()).min(0.25);
        self.last = Some(now);
        self.step(drawn.iter().map(|d| (plant_key(d), d.world.m[3].xyz())).map(|(k, p)| (k, [p.x.0, p.y.0, p.z.0])), movers, dt)
    }

    /// `frame`'s, by each plant's key and root, `dt` seconds on.
    pub fn step(&mut self, plants: impl Iterator<Item = (u64, [f32; 3])>, movers: &[Mover], dt: f32) -> Vec<[f32; 2]> {
        let mut out = Vec::new();
        let mut kept = HashMap::with_capacity(self.bent.len());
        for (key, root) in plants {
            let to = lean(root, movers);
            let mut now = match self.bent.get(&key) {
                Some(&was) => ease(was, to, dt),
                None => to,
            };
            if to == [0.0, 0.0] && now[0].abs() < 1e-3 && now[1].abs() < 1e-3 {
                now = [0.0, 0.0];
            }
            kept.insert(key, now);
            out.push(now);
        }
        self.bent = kept;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vi::geom::Matrix44;

    fn plant(x: f32, z: f32) -> Drawn<Ee> {
        let mut world = Matrix44::<Ee>::identity();
        world.m[3].x = F::new(x);
        world.m[3].z = F::new(z);
        Drawn { world, prim: 0, pal: 0, alpha: F::ONE }
    }

    /// The lean: none out of reach, above or below; away from the body,
    /// most at its feet; held to `LEAN_MOST` with several; two bodies on
    /// either side cancel. The control: the body's side decides the way.
    #[test]
    fn plants_lean_away_from_a_body() {
        let m = |x: f32, z: f32| Mover { at: [x, 10.0, z], reach: 1.5 };
        assert_eq!(lean([0.0, 10.0, 0.0], &[m(5.0, 0.0)]), [0.0, 0.0], "out of reach");
        assert_eq!(lean([0.0, 10.0, 0.0], &[Mover { at: [1.0, 20.0, 0.0], reach: 1.5 }]), [0.0, 0.0], "a body overhead");
        let l = lean([0.0, 10.0, 0.0], &[m(1.0, 0.0)]);
        assert!(l[0] < 0.0 && l[1] == 0.0, "away, to -x: {l:?}");
        let r = lean([0.0, 10.0, 0.0], &[m(-1.0, 0.0)]);
        assert!(r[0] > 0.0, "the control: the other side, to +x: {r:?}");
        let near = lean([0.0, 10.0, 0.0], &[m(0.3, 0.0)]);
        assert!(near[0].abs() > l[0].abs(), "nearer, further over");
        let at = lean([0.0, 10.0, 0.0], &[m(0.0, 0.0)]);
        assert!((at[0] - LEAN_MOST).abs() < 1e-6, "at its feet, all of it: {at:?}");
        let many = lean([0.0, 10.0, 0.0], &[m(0.2, 0.0), m(0.0, 0.2), m(0.2, 0.2)]);
        assert!((many[0] * many[0] + many[1] * many[1]).sqrt() <= LEAN_MOST + 1e-6, "held: {many:?}");
        let both = lean([0.0, 10.0, 0.0], &[m(1.0, 0.0), m(-1.0, 0.0)]);
        assert!(both[0].abs() < 1e-6, "either side: {both:?}");
    }

    /// The shear: no lean leaves the matrix to the bit; a lean moves a
    /// vertex at height h by h times it, the root not at all, and keeps the
    /// up column's length.
    #[test]
    fn the_bend_shears_from_the_root() {
        let s = 1.7f32;
        let cols = [[s, 0.0, 0.0, 0.0], [0.0, s, 0.0, 0.0], [0.0, 0.0, s, 0.0], [100.0, 5.0, -40.0, 1.0]];
        assert_eq!(bent(cols, [0.0, 0.0]), cols, "no lean: as it was");
        let b = bent(cols, [0.5, -0.25]);
        let at = |c: &[[f32; 4]; 4], p: [f32; 3]| [0, 1, 2].map(|k| c[0][k] * p[0] + c[1][k] * p[1] + c[2][k] * p[2] + c[3][k]);
        assert_eq!(at(&b, [0.0, 0.0, 0.0]), at(&cols, [0.0, 0.0, 0.0]), "the root stays");
        let (top, was) = (at(&b, [0.0, 1.0, 0.0]), at(&cols, [0.0, 1.0, 0.0]));
        let k = 1.0 / (1.0f32 + 0.25 + 0.0625).sqrt();
        assert!((top[0] - (100.0 + 0.5 * s * k)).abs() < 1e-4 && (top[2] - (-40.0 - 0.25 * s * k)).abs() < 1e-4, "{top:?}");
        assert!(top[1] < was[1], "it bows");
        let len = |c: [f32; 4]| (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        assert!((len(b[1]) - s).abs() < 1e-4, "its height along the stem kept");
        assert_eq!((b[0], b[2]), (cols[0], cols[2]), "its width as it was");
    }

    /// The ease: toward the lean quickly, back slowly, never past it; a plant
    /// first drawn at its lean; one no longer drawn let go; with nobody round
    /// every lean is none.
    #[test]
    fn leans_ease_in_and_back() {
        let mut r = React::default();
        let body = [Mover { at: [1.0, 0.0, 0.0], reach: 1.5 }];
        let plant = || std::iter::once((7u64, [0.0f32, 0.0, 0.0]));
        let to = lean([0.0, 0.0, 0.0], &body);
        assert_eq!(React::default().step(plant(), &body, 0.0), vec![to], "first drawn: at its lean");
        assert_eq!(r.step(plant(), &[], 1.0 / 60.0), vec![[0.0, 0.0]], "upright with nobody round");
        let first = r.step(plant(), &body, 1.0 / 60.0)[0];
        assert!(first[0] < 0.0 && first[0] > to[0], "on its way: {first:?} to {to:?}");
        let mut l = first;
        for _ in 0..60 {
            l = r.step(plant(), &body, 1.0 / 60.0)[0];
        }
        assert!((l[0] - to[0]).abs() < 1e-3, "there after a second: {l:?}");
        let back = r.step(plant(), &[], 1.0 / 60.0)[0];
        assert!(back[0] < 0.0 && back[0] > l[0], "easing back, not snapped: {back:?}");
        let in_step = (first[0] / to[0]).abs();
        let out_step = ((back[0] - l[0]) / l[0]).abs();
        assert!(out_step < in_step, "slower back ({out_step}) than in ({in_step})");
        for _ in 0..300 {
            l = r.step(plant(), &[], 1.0 / 60.0)[0];
        }
        assert_eq!(l, [0.0, 0.0], "upright again");
        r.step(std::iter::empty(), &[], 1.0 / 60.0);
        assert!(r.bent.is_empty(), "let go once not drawn");
        let mut fresh = React::default();
        assert_eq!(fresh.step(plant(), &[], 0.5), vec![[0.0, 0.0]], "nobody round: none");
    }

    /// The nearest bodies to the eye, `MOVERS` at most, none out of range.
    #[test]
    fn the_nearest_bodies() {
        let all: Vec<Mover> = (0..20).map(|k| Mover { at: [k as f32 * 4.0, 0.0, 0.0], reach: 1.0 }).collect();
        let n = nearest(&all, [0.0, 0.0, 0.0], 40.0);
        assert_eq!(n.len(), MOVERS);
        assert_eq!(n[0].at[0], 0.0);
        assert!(n.iter().all(|m| m.at[0] < 48.0));
        assert!(nearest(&all, [1000.0, 0.0, 0.0], 40.0).is_empty(), "none in range");
        // Foliage Distance: the bodies out at the farther plants are felt
        let one = [Mover { at: [100.0, 0.0, 0.0], reach: 1.0 }];
        assert!(nearest(&one, [0.0, 0.0, 0.0], 40.0).is_empty(), "past the game's 40");
        assert_eq!(nearest(&one, [0.0, 0.0, 0.0], 120.0).len(), 1, "inside 3x's 120");
    }

    /// Flora at all of it is `Render`'s list as it is, in its order; below,
    /// the plants kept are the same whatever order they come in, fewer at a
    /// lower share and none at 0. The control: half keeps some, not all.
    #[test]
    fn flora_keeps_the_same_plants_at_any_order() {
        let all: Vec<Drawn<Ee>> = (0..400).map(|k| plant((k % 20) as f32 * 1.7, (k / 20) as f32 * 2.3)).collect();
        let at = |v: &[Drawn<Ee>]| v.iter().map(|d| (d.world.m[3].x.0.to_bits(), d.world.m[3].z.0.to_bits())).collect::<Vec<_>>();
        assert_eq!(at(&kept(all.clone(), 100)), at(&all), "all: the game's list, in its order");
        assert!(kept(all.clone(), 0).is_empty(), "none");
        let half = kept(all.clone(), 50);
        assert!(!half.is_empty() && half.len() < all.len(), "the control: {}", half.len());
        let mut back = all.clone();
        back.reverse();
        let mut b = at(&kept(back, 50));
        b.reverse();
        assert_eq!(b, at(&half), "the same plants, sorted otherwise");
        let fewer = kept(all.clone(), 20);
        assert!(fewer.len() < half.len(), "fewer at 20 than at 50");
        let h = at(&half);
        assert!(at(&fewer).iter().all(|p| h.contains(p)), "a plant drawn at 20 is drawn at 50");
    }
}
