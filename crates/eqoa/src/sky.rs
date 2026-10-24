//! The sky, as `VIAtmosphere::RenderSky` draws it: the dome of the plane of
//! sky (`sky`, or `EQOA_Skydome_360` on profile 20) through the camera's
//! rotation, its materials as the atmosphere leaves them (the sky colour on
//! every layer, the sky type's texture, a third layer while a change of sky
//! fades), then, under a clear sky on plane 0, the sun, corona and moon at
//! their rotations and alphas. All of it lives in DATA2/SKY.CSF; the stars
//! and the lightning patches are billboards (`weather.rs`).

use esf::{Library, Model};
use render::{GpuModel, Renderer, wgpu};
use vi::atmosphere::{Pal, SkyRotations};
use vi::fpu::Ee;
use vi::hsprite::hash_ref_id;

pub const FILE: &str = "DATA2/SKY.CSF";

/// The sky's sprites, in upload order (and as the atmosphere numbers them).
#[derive(Clone, Copy)]
pub enum Sprites {
    Sky,
    Sky360,
    Sun,
    Corona,
    Moon,
}

/// One frame's sky: the plane of sky, whether the sun, corona and moon are
/// drawn, their placements, and every sprite's material palette.
pub struct SkyFrame {
    pub plane: i32,
    pub lights: bool,
    pub rot: SkyRotations<Ee>,
    pub pals: Vec<Pal>,
}

/// The sky's models in `Sprites` order (`EQOA_Skydome_360` is `sky` when a
/// disc has none).
pub fn models(lib: &Library) -> Option<Vec<Model>> {
    let find = |s: &str| lib.lookup(hash_ref_id(s)).and_then(|r| lib.build_model(r));
    let sky = find("sky")?;
    let sky360 = find("EQOA_Skydome_360").unwrap_or_else(|| sky.clone());
    let mut out = vec![sky, sky360];
    for s in ["Sun", "Corona", "Moon"] {
        out.push(find(s)?);
    }
    Some(out)
}

/// Uploads the sky; returns where in `gpu` it starts. A dome's batches carry
/// a third layer pass, a copy of the last, for the fade to use.
pub fn upload(r: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue, lib: &Library, gpu: &mut Vec<GpuModel>) -> Option<usize> {
    let mut ms = models(lib)?;
    for d in &mut ms[..2] {
        for b in &mut d.batches {
            if let Some(l) = b.layers.last().cloned() {
                while b.layers.len() < 3 {
                    b.layers.push(l.clone());
                }
            }
        }
    }
    let textures = render::library_textures(lib);
    let at = gpu.len();
    for m in &ms {
        let mut g = r.upload(device, queue, m, &[render::IDENTITY], &textures);
        g.sky = true;
        gpu.push(g);
    }
    Some(at)
}

/// Shows this frame's dome and places the sun, corona and moon, every
/// sprite's layers as its palette says (a palette entry per batch).
pub fn update(r: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue, gpu: &mut [GpuModel], at: usize, f: &SkyFrame, textures: &render::Textures) {
    for (k, pal) in f.pals.iter().enumerate().take(5) {
        let g = &mut gpu[at + k];
        g.visible = match k {
            0 | 1 => k as i32 == f.plane,
            _ => f.lights,
        };
        if !g.visible {
            continue;
        }
        let Some(first) = pal.first() else { continue };
        g.passes_used = first.n;
        for (b, m) in (0..g.batch_count()).map(|b| (b, pal.get(b).unwrap_or(first))) {
            for (j, l) in m.used().iter().enumerate() {
                let uv = usize::try_from(l.uv).ok();
                r.set_layer(device, queue, g, b, j, l.texture, l.blend.clamp(0, 4) as usize, l.color.map(|c| c as f32 / 255.0), uv, textures);
            }
        }
    }
    for (k, m) in [f.rot.sun, f.rot.corona, f.rot.moon].iter().enumerate() {
        r.set_instances(queue, &gpu[at + 2 + k], &[m.to_cols()]);
    }
}
