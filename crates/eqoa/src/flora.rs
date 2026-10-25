//! The radial flora (`VIRadialFloraSystem`, `vi::flora`, held to the decomp
//! by eqoa-port's `oracle_flora`): the plants the game grows round the camera
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

use esf::Library;
use glam::Mat4;
use render::wgpu;
use std::collections::HashMap;
use vi::flora::{Drawn, Frustum, System};
use vi::fpu::{Ee, F};
use vi::geom::Plane;
use vi::vect::Vect3;
use vi::world::World;

/// The most plants `Render` draws a frame (`m_sorted`).
const MOST: usize = 700;

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
}

impl Flora {
    pub fn new() -> Self {
        Self::default()
    }

    /// `VIScene::Render`'s two calls: `Update(cam.m_position)`, then
    /// `Render(cam)` through the view's frustum (`view_proj`'s planes, the
    /// window's own shape: a wide window sees the plants at its sides).
    pub fn frame(&mut self, world: &mut World<Ee>, eye: &Vect3<Ee>, view_proj: Mat4) -> Vec<Drawn<Ee>> {
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
        let drawn = self.sys.render(&fr, eye);
        self.grown = self.sys.swatches.iter().flatten().map(|s| s.insts.len()).sum();
        self.drawn = drawn.len();
        drawn
    }

    /// The plants `frame` gave to the GPU: each model made the first time a
    /// plant of it is drawn (`world.sprite_refs`: the flora sprite's record),
    /// its instances and their fades written in the drawing order, and a run
    /// of one model a draw.
    pub fn upload(&mut self, renderer: &mut render::Renderer, device: &wgpu::Device, queue: &wgpu::Queue, lib: &Library, world: &World<Ee>, drawn: &[Drawn<Ee>]) {
        let mut per: HashMap<usize, (Vec<[[f32; 4]; 4]>, Vec<[f32; 4]>)> = HashMap::new();
        self.runs.clear();
        for d in drawn {
            let Ok(s) = usize::try_from(d.prim) else { continue };
            let e = per.entry(s).or_default();
            let at = e.0.len() as u32;
            e.0.push(d.world.to_cols());
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
                Some(renderer.upload_flora(device, queue, &m, MOST, &render::library_textures(lib)))
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
    }

    pub fn describe(&self) -> String {
        let swatches = self.sys.swatches.iter().flatten().count();
        format!("flora: {} plants in {swatches} swatches, {} drawn in {} runs, {} models", self.grown, self.drawn, self.runs.len(), self.models.len())
    }
}
