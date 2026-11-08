//! `eqoa dress <body> <out.png> [look...]`: one body off the disc dressed in
//! a look as the game dresses a creature (`play::dress_look`: the text slots,
//! and `slotless` on a body with none), drawn standing (its idle, as
//! `SetLocomotion(0)` leaves it) from the front, the front a little to its
//! left, its left and behind, side by side, under the GM preview's daylight
//! (the sun over the camera's shoulder), with no window, framed on the body
//! (not its wings or tail). For judging armour on bodies by eye.
//!
//! The look, as words: `set=N` (the five worn slots in armour set N),
//! `armor=a,b,c,d,e` (chest, bracer, gloves, legs, boots), `tint=T` (every
//! slot's tint), `robe=R`, `rtint=T`, `dye=RRGGBB` (the live game's colour on
//! every slot and the robe), `hair=c,l,s`, `face=F`, `at=ms` (the idle's
//! time), `size=WxH` (one view's). None of them: the body bare.

use esf::Library;
use glam::{Mat4, Vec3};
use render::{Renderer, View, wgpu};
use vi::character::{AnimCache, load_csprite};
use vi::csprite::Rand;
use vi::fpu::{Ee, F};
use vi::geom::Matrix44;
use vi::hsprite::AnimScene;
use vi::vect::Vect3;

/// The files a body needs, in the order the game registers them.
pub const FILES: &[&str] = &["CHARCUST.CSF", "CHARFACE.CSF", "CHAR.ESF"];

/// The look a list of words names, the idle's time, and one view's size.
pub fn parse(words: &[String]) -> Result<(vi::cust::Look, i64, (u32, u32)), String> {
    let mut look = vi::cust::Look { robe: -1, ..Default::default() };
    let (mut at, mut size) = (1500i64, (360u32, 540u32));
    let nums = |v: &str| v.split(',').map(|n| n.trim().parse::<u8>().map_err(|e| format!("{v}: {e}"))).collect::<Result<Vec<u8>, String>>();
    for w in words {
        let (k, v) = w.split_once('=').ok_or_else(|| format!("{w}: a look is words like set=3, tint=8, robe=1"))?;
        match k {
            "set" => {
                let n = nums(v)?[0];
                look.armor[..5].fill(n);
            }
            "armor" => {
                for (i, n) in nums(v)?.into_iter().take(6).enumerate() {
                    look.armor[i] = n;
                }
            }
            "tint" => look.armor_tint.fill(nums(v)?[0]),
            "robe" => look.robe = v.parse::<i8>().map_err(|e| format!("{w}: {e}"))?,
            "rtint" => look.robe_tint = nums(v)?[0],
            "dye" => {
                let c = u32::from_str_radix(v.trim_start_matches('#'), 16).map_err(|e| format!("{w}: {e}"))?;
                let rgba = [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255];
                look.armor_color = Some([rgba; 6]);
                look.robe_color = Some(rgba);
            }
            "hair" => {
                for (i, n) in nums(v)?.into_iter().take(3).enumerate() {
                    look.hair[i] = n;
                }
            }
            "face" => look.face = nums(v)?[0],
            "at" => at = v.parse().map_err(|e| format!("{w}: {e}"))?,
            "size" => {
                let (a, b) = v.split_once('x').ok_or_else(|| format!("{w}: WxH"))?;
                size = (a.parse().map_err(|e| format!("{w}: {e}"))?, b.parse().map_err(|e| format!("{w}: {e}"))?);
            }
            _ => return Err(format!("{w}: not a word of a look")),
        }
    }
    Ok((look, at, size))
}

/// The body drawn in the look from four sides into `out`.
#[cfg(not(target_arch = "wasm32"))]
pub fn shot(lib: &Library, model: i32, look: &vi::cust::Look, at: i64, (w, h): (u32, u32), out: &str) -> Result<(), String> {
    let r = lib.lookup(model).ok_or_else(|| format!("no body {:#010x} in CHAR.ESF", model as u32))?;
    let mut anims = AnimScene::<Ee>::default();
    let mut rand = Rand(1);
    let mut cs = load_csprite(lib, r, &mut anims, &mut AnimCache::new(), &mut rand).ok_or("not a character")?;
    let mut mesh = lib.build_model(r).ok_or("no mesh")?;
    let before = mesh.batches.len();
    crate::play::dress_look(lib, model, &mut mesh, &cs, look, false, &vi::cust::Tables::retail(), true);
    let tris = |b: &[esf::Batch]| b.iter().map(|b| b.indices.len() / 3).sum::<usize>();
    // `EQOA_DRESS_DEBUG=slots`: the made-up batches flat, a colour a slot
    // (chest red, bracer yellow, gloves green, legs blue, boots magenta);
    // `parts`: the body's own triangles a colour by where they are fitted
    // (body red, arms yellow and orange, legs blue and cyan, head grey, off
    // the spine light grey, wearing nothing black, a part the body keeps its
    // own look on purple)
    let debug = std::env::var("EQOA_DRESS_DEBUG").unwrap_or_default();
    let flat = |l: &mut esf::Layer, c: [u8; 3]| {
        l.texture = 0;
        l.color = [c[0], c[1], c[2], 255];
    };
    if debug == "slots" {
        const C: [[u8; 3]; 5] = [[220, 40, 40], [230, 210, 40], [40, 200, 60], [50, 90, 230], [210, 50, 210]];
        for b in &mut mesh.batches[before..] {
            let k = (b.material - crate::slotless::ENTRY) as usize;
            b.layers.iter_mut().for_each(|l| flat(l, C[k]));
        }
        // a body's own slots alike
        for b in &mut mesh.batches[..before] {
            if let Some(k) = (0..5).find(|&i| cs.text_slots[i].0 == b.material) {
                b.layers.iter_mut().for_each(|l| flat(l, C[k]));
            }
        }
    }
    if debug == "parts"
        && let Some(groups) = crate::slotless::debug_groups(lib, model, &mesh, &cs)
    {
        const C: [[u8; 3]; 9] = [[220, 40, 40], [230, 210, 40], [240, 130, 20], [50, 90, 230], [40, 200, 220], [130, 130, 130], [10, 10, 10], [190, 190, 190], [150, 60, 200]];
        let mut split = Vec::new();
        for (b, g) in mesh.batches[..before].iter().zip(&groups) {
            for code in 0..9u8 {
                let indices: Vec<u32> = b.indices.chunks_exact(3).zip(g).filter(|(_, c)| **c == code).flat_map(|(t, _)| t.to_vec()).collect();
                if !indices.is_empty() {
                    let mut layer = b.layers.first().cloned().unwrap_or(esf::Layer { flags: 0, texture: 0, wrap: 0, blend: 0, color: [255; 4], uv_transform: [0.0; 9], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] });
                    flat(&mut layer, C[code as usize]);
                    layer.blend = 0;
                    split.push(esf::Batch { material: b.material, layers: vec![layer], indices, lod: b.lod });
                }
            }
        }
        mesh.batches = split;
    }
    eprintln!(
        "{:#010x}: {} text slots, {} batches dressed, {} made-up batches ({} triangles of {})",
        model as u32,
        cs.text_slots.iter().filter(|s| s.0 != -1).count(),
        before,
        mesh.batches.len() - before,
        tris(&mesh.batches[before..]),
        tris(&mesh.batches[..before])
    );
    // standing, at `at` ms of its idle
    let mut t = 0;
    while t <= at {
        anims.time = t;
        cs.set_locomotion(0, &anims, &mut rand);
        cs.h.process(0, &anims);
        t += 16;
    }
    let centre = cs.h.calc_trans_about_center(&Matrix44::identity(), &anims);
    cs.h.set_blend_matrices();
    // framed on the posed body's bounds: its torso, limbs and head where it
    // has them (not wings or a tail), else everything
    let palette: Vec<[[f32; 4]; 4]> = cs.h.blend.iter().map(|m| m.to_cols()).collect();
    let world = Mat4::from_cols_array_2d(&centre.to_cols());
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let mut framed = vec![true; mesh.pos.len()];
    if let Some(groups) = crate::slotless::debug_groups(lib, model, &mesh, &cs) {
        framed.fill(false);
        for (b, g) in mesh.batches.iter().zip(&groups) {
            for (t, &code) in b.indices.chunks_exact(3).zip(g) {
                if code <= 5 || code == 8 {
                    t.iter().for_each(|&v| framed[v as usize] = true);
                }
            }
        }
        if !framed.contains(&true) {
            framed.fill(true);
        }
    }
    for (i, p) in mesh.pos.iter().enumerate().filter(|(i, _)| framed[*i]) {
        let (b, wt) = (mesh.bones[i], mesh.weights[i]);
        let mut q = Vec3::ZERO;
        let mut sum = 0.0;
        for k in 0..4 {
            if b[k] != 255 && (b[k] as usize) < palette.len() && wt[k] > 0 {
                q += Mat4::from_cols_array_2d(&palette[b[k] as usize]).transform_point3(Vec3::from(*p)) * wt[k] as f32;
                sum += wt[k] as f32;
            }
        }
        let q = world.transform_point3(if sum > 0.0 { q / sum } else { Vec3::from(*p) });
        lo = lo.min(q);
        hi = hi.max(q);
    }
    let mid = (lo + hi) * 0.5;
    let fov = 30f32.to_radians();
    let reach = ((hi.y - lo.y) * 0.5 / 0.86).max((hi.x - lo.x).max(hi.z - lo.z) * 0.5 / 0.7 / (w as f32 / h as f32)) / (fov * 0.5).tan();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).map_err(|e| e.to_string())?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).map_err(|e| e.to_string())?;
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    let textures = render::library_textures(lib);
    let mut gpu = r.upload(&device, &queue, &mesh, &[render::IDENTITY], &textures);
    r.set_instances(&queue, &gpu, &[centre.to_cols()]);
    if !palette.is_empty() {
        r.set_palette(&queue, &gpu, &palette);
    }
    // from the front, from the front a little to its left, from its left,
    // from behind
    let yaws = [0.0f32, 0.6, std::f32::consts::FRAC_PI_2, std::f32::consts::PI];
    let mut sheet = vec![0u8; (w * yaws.len() as u32 * h * 4) as usize];
    for (k, yaw) in yaws.iter().enumerate() {
        let dir = Vec3::new(yaw.sin() * 0.97, 0.24, yaw.cos() * 0.97).normalize();
        let eye = mid + dir * reach;
        let v = glam::camera::rh::view::look_at_mat4(eye, mid, Vec3::Y);
        let p = glam::camera::rh::proj::directx::perspective(fov, w as f32 / h as f32, reach * 0.05, reach * 4.0);
        let mut view = View::daylight(p * v);
        view.eye = eye;
        // the sun from over the camera's shoulder, so each side is seen lit
        view.sun_dir = (Vec3::new(-dir.x, 0.0, -dir.z) * 0.6 + Vec3::new(dir.z, 0.0, -dir.x) * 0.25 + Vec3::new(0.0, -0.75, 0.0)).normalize();
        crate::lod::place(&mut gpu, &centre, &Vect3::new(F::new(eye.x), F::new(eye.y), F::new(eye.z)));
        let rgba = render::capture(&device, &queue, &mut r, w, h, &view, &[render::Draw::Model(&gpu)], None);
        for y in 0..h as usize {
            let src = &rgba[y * w as usize * 4..(y + 1) * w as usize * 4];
            let o = (y * (w as usize * yaws.len()) + k * w as usize) * 4;
            sheet[o..o + w as usize * 4].copy_from_slice(src);
        }
    }
    let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(out).map_err(|e| format!("{out}: {e}"))?), w * yaws.len() as u32, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut wr| wr.write_image_data(&sheet)).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The words make the look they say; a stray word is refused (the control).
    #[test]
    fn looks_from_words() {
        let words = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
        let (l, at, size) = parse(&words("set=3 tint=8 robe=1 rtint=2 at=900 size=200x300")).unwrap();
        assert_eq!((l.armor, l.armor_tint, l.robe, l.robe_tint, at, size), ([3, 3, 3, 3, 3, 0], [8; 6], 1, 2, 900, (200, 300)));
        let (l, _, _) = parse(&words("armor=1,2,3,4,5 dye=c03020")).unwrap();
        assert_eq!(l.armor, [1, 2, 3, 4, 5, 0]);
        assert_eq!(l.armor_color, Some([[0xC0, 0x30, 0x20, 255]; 6]));
        let (bare, _, _) = parse(&[]).unwrap();
        assert_eq!((bare.armor, bare.robe), ([0; 6], -1));
        assert!(parse(&words("sett=3")).is_err());
    }
}
