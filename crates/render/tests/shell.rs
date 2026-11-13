//! A glowing creature's aura shell (`render::shell`): its mesh drawn again,
//! pushed out along its normals, added in the glow's colour as a rim. A model
//! with no shell is drawn bit for bit as before (a shell set and taken away
//! leaves the picture as it was, and a model beside it is untouched); with
//! one, one draw call more, red added and nothing else changed, strongest at
//! the silhouette. Needs a GPU adapter (any backend); without one it says so
//! and passes.

use render::{Draw, Renderer, View};

/// A right-handed perspective (depth 0..1) from `z` on the +z axis looking
/// down -z at the origin, `fovy` high, `aspect` wide.
fn camera(z: f32, fovy: f32, aspect: f32) -> glam::Mat4 {
    let (near, far) = (0.1f32, 100.0f32);
    let f = 1.0 / (fovy / 2.0).tan();
    let proj = glam::Mat4::from_cols_array_2d(&[[f / aspect, 0.0, 0.0, 0.0], [0.0, f, 0.0, 0.0], [0.0, 0.0, far / (near - far), -1.0], [0.0, 0.0, near * far / (near - far), 0.0]]);
    proj * glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, -z))
}

/// A sphere of radius `r` at `at`, its normals outward, its winding the
/// renderer's front face from outside.
fn sphere(r: f32, at: [f32; 3]) -> esf::Model {
    let (rings, segs) = (16u32, 24u32);
    let mut m = esf::Model::default();
    for i in 0..=rings {
        let th = std::f32::consts::PI * i as f32 / rings as f32;
        for j in 0..=segs {
            let ph = 2.0 * std::f32::consts::PI * j as f32 / segs as f32;
            let n = [th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin()];
            m.pos.push([at[0] + r * n[0], at[1] + r * n[1], at[2] + r * n[2]]);
            m.normal.push(n);
            m.uv.push([0.5, 0.5]);
            m.color.push([0.0, 0.0, 0.0, 1.0]);
        }
    }
    let mut idx = Vec::new();
    for i in 0..rings {
        for j in 0..segs {
            let a = i * (segs + 1) + j;
            let b = a + segs + 1;
            idx.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    let layer = esf::Layer { flags: 0, texture: 0, wrap: 0, blend: 0, color: [128, 128, 128, 255], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] };
    m.batches.push(esf::Batch { material: 0, layers: vec![layer], indices: idx, lod: None });
    m
}

#[test]
fn a_shell_adds_one_draw_and_a_rim_and_nothing_else() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    r.scroll_time = Some(1.0);
    let tex = |_: i32| None;
    let body = sphere(1.0, [-1.3, 0.0, 0.0]);
    let beside = sphere(1.0, [1.3, 0.0, 0.0]);
    let mut m = r.upload(&device, &queue, &body, &[render::IDENTITY], &tex);
    let other = r.upload(&device, &queue, &beside, &[render::IDENTITY], &tex);
    // looking down -z at both from 6 away
    let vp = camera(6.0, 0.9, 2.0);
    let view = View { eye: glam::Vec3::new(0.0, 0.0, 6.0), clear: glam::Vec3::ZERO, ..View::daylight(vp) };
    let (w, h) = (128u32, 64u32);
    let shot = |r: &mut Renderer, m: &render::GpuModel| {
        let px = render::capture(&device, &queue, r, w, h, &view, &[Draw::Model(m), Draw::Model(&other)], None);
        (px, r.drawn.get().1)
    };
    let (before, draws) = shot(&mut r, &m);
    r.set_shell(&device, &mut m, &body, Some([255, 0, 0]));
    assert_eq!(m.shell.as_ref().map(|s| s.colors()), Some(vec![[255, 0, 0]]));
    let (with, draws_with) = shot(&mut r, &m);
    assert_eq!(draws_with, draws + 1, "one draw call more");
    r.set_shell(&device, &mut m, &body, None);
    let (after, draws_after) = shot(&mut r, &m);
    assert_eq!(draws_after, draws);
    assert!(before == after, "no shell: bit for bit as before");
    // the shell adds red alone, and only about the left sphere
    let at = |x: u32, y: u32| ((y * w + x) * 4) as usize;
    let mut added = 0u32;
    for y in 0..h {
        for x in 0..w {
            let i = at(x, y);
            assert_eq!((with[i + 1], with[i + 2]), (before[i + 1], before[i + 2]), "green and blue unchanged at {x},{y}");
            assert!(with[i] >= before[i], "added at {x},{y}");
            if with[i] > before[i] {
                added += 1;
                assert!(x < 66, "the right sphere untouched at {x},{y}");
            }
        }
    }
    assert!(added > 200, "{added} pixels lit");
    // a rim: the left sphere's centre (x 50: 64 less 1.3 of the 5.8 the half
    // width spans) gets little, its edge (about x 39) more
    let red = |x: u32| with[at(x, h / 2)] as i32 - before[at(x, h / 2)] as i32;
    let centre = red(50);
    let edge = (30..50).map(red).max().unwrap();
    eprintln!("added red: centre {centre}, edge {edge}, {added} pixels");
    assert!(edge > centre * 2 && edge > 40, "the rim is strongest at the silhouette: centre {centre}, edge {edge}");
}

/// A shell in parts (a worn piece's each): a sphere's top half and bottom
/// half two batches, a red part over the top alone and a blue over the
/// bottom alone: red is added only above the middle, blue only below, two
/// draw calls; one part alone, the other half as before.
#[test]
fn each_part_its_own_batches_and_colour() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    r.scroll_time = Some(1.0);
    let tex = |_: i32| None;
    // the sphere's triangles split at its equator into two batches
    let mut body = sphere(1.0, [0.0, 0.0, 0.0]);
    let whole = std::mem::take(&mut body.batches[0].indices);
    let (mut top, mut bottom) = (Vec::new(), Vec::new());
    for t in whole.chunks_exact(3) {
        let y = t.iter().map(|&i| body.pos[i as usize][1]).sum::<f32>();
        if y > 0.0 { top.extend_from_slice(t) } else { bottom.extend_from_slice(t) }
    }
    let layers = body.batches[0].layers.clone();
    body.batches = vec![esf::Batch { material: 0, layers: layers.clone(), indices: top, lod: None }, esf::Batch { material: 1, layers, indices: bottom, lod: None }];
    let mut m = r.upload(&device, &queue, &body, &[render::IDENTITY], &tex);
    let vp = camera(5.0, 0.9, 1.0);
    let view = View { eye: glam::Vec3::new(0.0, 0.0, 5.0), clear: glam::Vec3::ZERO, ..View::daylight(vp) };
    let n = 64u32;
    let shot = |r: &mut Renderer, m: &render::GpuModel| (render::capture(&device, &queue, r, n, n, &view, &[Draw::Model(m)], None), r.drawn.get().1);
    let (before, draws) = shot(&mut r, &m);
    use render::shell::PartOf;
    r.set_shells(&device, &mut m, &body, &[PartOf { color: [255, 0, 0], batches: Some(vec![0]) }, PartOf { color: [0, 0, 255], batches: Some(vec![1]) }]);
    let (both, draws_both) = shot(&mut r, &m);
    assert_eq!(draws_both, draws + 2, "a draw call a part");
    let at = |x: u32, y: u32| ((y * n + x) * 4) as usize;
    let (mut red_above, mut blue_below) = (0, 0);
    for y in 0..n {
        for x in 0..n {
            let i = at(x, y);
            assert_eq!(both[i + 1], before[i + 1], "no green at {x},{y}");
            // a row of the image from the top: above the middle, the top half
            let (red, blue) = (both[i] > before[i], both[i + 2] > before[i + 2]);
            if y + 3 < n / 2 {
                assert!(!blue, "blue above the middle at {x},{y}");
                red_above += red as u32;
            } else if y > n / 2 + 3 {
                assert!(!red, "red below the middle at {x},{y}");
                blue_below += blue as u32;
            }
        }
    }
    assert!(red_above > 100 && blue_below > 100, "{red_above} red above, {blue_below} blue below");
    // one part alone: the bottom half as before
    r.set_shells(&device, &mut m, &body, &[PartOf { color: [255, 0, 0], batches: Some(vec![0]) }]);
    let (top_only, draws_top) = shot(&mut r, &m);
    assert_eq!(draws_top, draws + 1);
    for y in n / 2 + 4..n {
        for x in 0..n {
            let i = at(x, y);
            assert_eq!(&top_only[i..i + 4], &before[i..i + 4], "the bottom untouched at {x},{y}");
        }
    }
    // a part of no batch draws nothing: no shell at all
    r.set_shells(&device, &mut m, &body, &[PartOf { color: [255, 0, 0], batches: Some(Vec::new()) }]);
    assert!(m.shell.is_none());
}
