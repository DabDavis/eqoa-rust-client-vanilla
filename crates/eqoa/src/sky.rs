//! The sky, as `VIAtmosphere::RenderSky` draws it in its settled state: the
//! dome (`EQOA_Skydome_360`, else `sky`) with the sky type's texture on both
//! its layers and the sky colour on every layer, then, under a clear sky, the
//! sun, corona and moon. All of it lives in DATA2/SKY.CSF.

use esf::{Library, Model};
use glam::Vec3;
use render::{GpuModel, Renderer, wgpu};
use vi::atmosphere::SkyRotations;
use vi::fpu::Ee;
use vi::hsprite::hash_ref_id;

pub const FILE: &str = "DATA2/SKY.CSF";

/// One frame's sky: its type, colour and the sun's placement.
pub struct SkyFrame {
    pub ty: i32,
    pub rgb: Vec3,
    pub rot: SkyRotations<Ee>,
}

/// The sky's models in upload order: a dome per sky type (0 sunny, 1 the
/// dome's own texture, 2 dark clouds), then sun, corona, moon.
pub fn models(lib: &Library) -> Option<Vec<Model>> {
    let find = |s: &str| lib.lookup(hash_ref_id(s)).and_then(|r| lib.build_model(r));
    let dome = find("EQOA_Skydome_360").or_else(|| find("sky"))?;
    let mut out = Vec::new();
    for tex in [hash_ref_id("PS2EQSky_Sunny.tga"), 0, hash_ref_id("PS2EQSky_DarkClouds.tga")] {
        let mut d = dome.clone();
        if tex != 0 {
            for b in &mut d.batches {
                for l in b.layers.iter_mut().take(2) {
                    l.texture = tex;
                }
            }
        }
        out.push(d);
    }
    for s in ["Sun", "Corona", "Moon"] {
        out.push(find(s)?);
    }
    Some(out)
}

/// Uploads the sky; returns where in `gpu` it starts.
pub fn upload(r: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue, lib: &Library, gpu: &mut Vec<GpuModel>) -> Option<usize> {
    let ms = models(lib)?;
    let textures = render::library_textures(lib);
    let at = gpu.len();
    for m in &ms {
        let mut g = r.upload(device, queue, m, &[render::IDENTITY], &textures);
        g.sky = true;
        gpu.push(g);
    }
    Some(at)
}

/// Shows the dome of this frame's type in this frame's colour and places
/// the sun, corona and moon (hidden under dark clouds).
pub fn update(r: &Renderer, queue: &wgpu::Queue, gpu: &mut [GpuModel], at: usize, f: &SkyFrame) {
    let ty = f.ty.clamp(0, 2) as usize;
    for k in 0..3 {
        gpu[at + k].visible = k == ty;
    }
    r.set_layer_rgb(queue, &gpu[at + ty], f.rgb);
    for (k, m) in [f.rot.sun, f.rot.corona, f.rot.moon].iter().enumerate() {
        let g = &mut gpu[at + 3 + k];
        g.visible = ty < 2;
        r.set_instances(queue, g, &[m.to_cols()]);
    }
}
