//! Zone streaming for the walk: the zones around the player come and go as
//! `VIWorld::StreamLeaf` decides (`vi::world::World::stream`), each with its
//! terrain, its placed actors and its animated props. The collision world is
//! still the whole file's (it loads in two seconds); only drawing streams.

use crate::scene;
use crate::walker::AnimatedProps;
use esf::Library;
use glam::Vec3;
use render::{GpuModel, Renderer, wgpu};
use std::collections::{BTreeMap, BTreeSet};
use vi::fpu::Ee;
use vi::world::World;

struct Loaded {
    models: Vec<GpuModel>,
    props: AnimatedProps,
    prop_models: Vec<GpuModel>,
}

pub struct Streamer {
    file: usize,
    zones: BTreeMap<usize, Loaded>,
}

pub struct Gpu<'a> {
    pub renderer: &'a mut Renderer,
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
}

impl Streamer {
    pub fn new(file: usize) -> Self {
        Streamer { file, zones: BTreeMap::new() }
    }

    /// Lets go of the zones past the outer radius of `at` and loads up to
    /// `budget` of those inside the inner one (all, if `None`), nearest
    /// first. Returns whether anything changed.
    pub fn update(&mut self, at: Vec3, world: &mut World<Ee>, lib: &Library, g: Gpu, budget: Option<usize>) -> bool {
        let loaded: BTreeSet<usize> = self.zones.keys().copied().collect();
        let (load, release) = world.stream(&vi::vect::Vect3::of(at.x, at.y, at.z), &loaded);
        for z in &release {
            self.zones.remove(z);
        }
        let all = lib.file(self.file).zones();
        let textures = render::library_textures(lib);
        let n = budget.unwrap_or(usize::MAX).min(load.len());
        for &z in &load[..n] {
            let s = scene::zones(lib, self.file, &[all[z]], &format!("zone {z}"), true);
            let models = s.parts.iter().map(|(m, mats)| g.renderer.upload(g.device, g.queue, m, mats, &textures)).collect();
            let props = AnimatedProps::new(world, lib, &s.hsprites);
            let prop_models = props
                .groups
                .iter()
                .zip(props.matrices(world))
                .map(|(group, mats)| g.renderer.upload(g.device, g.queue, &group.model, &mats, &textures))
                .collect();
            self.zones.insert(z, Loaded { models, props, prop_models });
        }
        !release.is_empty() || n > 0
    }

    /// Poses every loaded zone's props for this frame.
    pub fn animate(&self, world: &mut World<Ee>, r: &Renderer, queue: &wgpu::Queue) {
        for l in self.zones.values() {
            for (m, mats) in l.prop_models.iter().zip(l.props.matrices(world)) {
                r.set_instances(queue, m, &mats);
            }
        }
    }

    pub fn models(&self) -> impl Iterator<Item = &GpuModel> {
        self.zones.values().flat_map(|l| l.models.iter().chain(l.prop_models.iter()))
    }

    pub fn describe(&self) -> String {
        format!("{} zones streamed in", self.zones.len())
    }
}
