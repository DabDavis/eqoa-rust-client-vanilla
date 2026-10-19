//! A STAND-IN for `VIScene`'s collision, so the ported walk and camera code
//! (crate `vi`) can run before the game's own scene walk is ported.
//!
//! It is NOT the game's algorithm. The game sweeps a sphere through the world
//! and zone BSPs into each room's collision buffers and its actors'
//! (`viscene.cpp`, `vicollide.cpp`; ported to Go, console-checked, in
//! `go-eqoa-vi/vicoll`). This sweeps a RAY against the zone's terrain
//! triangles only, double-sided, with the radius approximated by moving each
//! face toward the ray by that much along its normal (so edges and corners are
//! not rounded). No props, no water: `env_flags` is 0 everywhere.

use std::collections::HashMap;
use vi::coll::{CollSphere, EnvRay};
use vi::fpu::{Ee, F};
use vi::scene::{Filter, Scene};
use vi::vect::Vect3;

const CELL: f32 = 8.0;

pub struct TerrainScene {
    tris: Vec<[[f32; 3]; 3]>,
    normals: Vec<[f32; 3]>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    /// `VIScene::GetTime`, in ms; the caller advances it.
    pub time: i64,
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cell(v: f32) -> i32 {
    (v / CELL).floor() as i32
}

impl TerrainScene {
    /// From a model's world-space triangles (the zone's terrain).
    pub fn new(m: &esf::Model) -> Self {
        let mut s = TerrainScene { tris: Vec::new(), normals: Vec::new(), grid: HashMap::new(), time: 0 };
        for b in &m.batches {
            for t in b.indices.chunks(3) {
                let tri = [m.pos[t[0] as usize], m.pos[t[1] as usize], m.pos[t[2] as usize]];
                let n = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]));
                let len = dot(n, n).sqrt();
                if len == 0.0 {
                    continue;
                }
                let id = s.tris.len() as u32;
                s.tris.push(tri);
                s.normals.push([n[0] / len, n[1] / len, n[2] / len]);
                let (lo_x, hi_x) = (tri.iter().map(|p| p[0]).fold(f32::MAX, f32::min), tri.iter().map(|p| p[0]).fold(f32::MIN, f32::max));
                let (lo_z, hi_z) = (tri.iter().map(|p| p[2]).fold(f32::MAX, f32::min), tri.iter().map(|p| p[2]).fold(f32::MIN, f32::max));
                for gx in cell(lo_x)..=cell(hi_x) {
                    for gz in cell(lo_z)..=cell(hi_z) {
                        s.grid.entry((gx, gz)).or_default().push(id);
                    }
                }
            }
        }
        s
    }

    pub fn triangles(&self) -> usize {
        self.tris.len()
    }

    /// The earliest hit of the segment a→b, with faces moved `radius` toward
    /// it: (t in 0..1, normal facing the ray, triangle).
    fn sweep(&self, a: [f32; 3], b: [f32; 3], radius: f32) -> Option<(f32, [f32; 3], u32)> {
        let d = sub(b, a);
        let pad = radius + 0.01;
        let (lo_x, hi_x) = (a[0].min(b[0]) - pad, a[0].max(b[0]) + pad);
        let (lo_z, hi_z) = (a[2].min(b[2]) - pad, a[2].max(b[2]) + pad);
        let mut best: Option<(f32, [f32; 3], u32)> = None;
        let mut seen = std::collections::HashSet::new();
        for gx in cell(lo_x)..=cell(hi_x) {
            for gz in cell(lo_z)..=cell(hi_z) {
                for &id in self.grid.get(&(gx, gz)).map_or(&[][..], |v| v) {
                    if !seen.insert(id) {
                        continue;
                    }
                    let mut n = self.normals[id as usize];
                    if dot(n, d) > 0.0 {
                        n = [-n[0], -n[1], -n[2]];
                    }
                    // The face, moved toward the ray by the radius.
                    let off = [n[0] * radius, n[1] * radius, n[2] * radius];
                    let t3 = self.tris[id as usize].map(|p| [p[0] + off[0], p[1] + off[1], p[2] + off[2]]);
                    // Möller–Trumbore, both sides.
                    let (e1, e2) = (sub(t3[1], t3[0]), sub(t3[2], t3[0]));
                    let p = cross(d, e2);
                    let det = dot(e1, p);
                    if det.abs() < 1e-12 {
                        continue;
                    }
                    let inv = 1.0 / det;
                    let s = sub(a, t3[0]);
                    let u = dot(s, p) * inv;
                    if !(0.0..=1.0).contains(&u) {
                        continue;
                    }
                    let q = cross(s, e1);
                    let v = dot(d, q) * inv;
                    if v < 0.0 || u + v > 1.0 {
                        continue;
                    }
                    let t = dot(e2, q) * inv;
                    if (0.0..=1.0).contains(&t) && best.is_none_or(|(bt, _, _)| t < bt) {
                        best = Some((t, n, id));
                    }
                }
            }
        }
        best
    }

    /// The terrain's height under (x, z), from above.
    pub fn ground(&self, x: f32, z: f32) -> Option<f32> {
        let (top, bottom) = ([x, 5000.0, z], [x, -5000.0, z]);
        self.sweep(top, bottom, 0.0).map(|(t, _, _)| 5000.0 - t * 10000.0)
    }
}

impl Scene<Ee> for TerrainScene {
    fn time(&self) -> i64 {
        self.time
    }

    fn collide_sphere(&mut self, s: &mut CollSphere<Ee>, filter: &mut Filter) -> bool {
        let (a, b) = (s.ray.start.to_array(), s.ray.end.to_array());
        match self.sweep(a, b, s.radius.0) {
            Some((t, n, id)) if F::new(t) < s.ray.hit_time => {
                s.ray.hit_time = F::new(t);
                s.ray.hit_normal = Vect3::of(n[0], n[1], n[2]);
                s.ray.hit_idx = id as i32;
                filter.hit_actor = -1;
                filter.hit_sprite = -1;
                true
            }
            _ => false,
        }
    }

    fn collide_env(&mut self, _ray: &mut EnvRay<Ee>) -> bool {
        false
    }

    fn env_flags(&self, _at: &Vect3<Ee>) -> u32 {
        0
    }
}
