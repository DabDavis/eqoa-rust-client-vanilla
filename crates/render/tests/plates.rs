//! The name plates against the particles, as `VIScene::Render` and the GS
//! draw them: the plates after the world and before the particles (which
//! blend over them), the screen's text after everything; and a particle
//! whose texture blends writes no depth (`VIMaterial::ConstructTextPass`:
//! ATST never, AFAIL RGB only), whatever its definition's Z-write flag, while
//! one with no texture writes as the flag says. Needs a graphics adapter
//! (any backend); without one it says so and passes.

use glam::{Mat4, Vec3};
use render::billboard::{Batch, Quad};
use render::text::Label;
use render::{Renderer, View};

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

fn gpu() -> Option<Gpu> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return None;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    Some(Gpu { device, queue })
}

/// Texture 7 is one opaque white texel; any other id is missing (drawn white,
/// untextured).
fn tex(id: i32) -> Option<esf::Image> {
    (id == 7).then(|| esf::Image { width: 1, height: 1, rgba: vec![255, 255, 255, 255], alpha: true })
}

/// A particle over the whole view at clip depth `z` (the view and the
/// projection are the identity).
fn batch(texture: i32, blend: usize, ztest: bool, zwrite: bool, z: f32, color: [f32; 4]) -> Batch {
    Batch { texture, blend, ztest, zwrite, quads: vec![Quad { center: [0.0, 0.0, z], half: [3.0, 3.0], frame: 0, color }] }
}

const CLEAR: Vec3 = Vec3::new(0.2, 0.4, 0.6);

/// The pixel at (x, y) of a `size` frame: the clear, the plates and labels
/// set on `r`, the particles over them.
fn pixel(g: &Gpu, r: &mut Renderer, size: u32, at: (u32, u32), batches: &[Batch]) -> [f32; 3] {
    let view = View { clear: CLEAR, ..View::daylight(Mat4::IDENTITY) };
    let px = render::capture(&g.device, &g.queue, r, size, size, &view, &[], Some((Mat4::IDENTITY, Mat4::IDENTITY, batches, &tex)));
    let i = ((at.1 * size + at.0) * 4) as usize;
    [px[i] as f32, px[i + 1] as f32, px[i + 2] as f32]
}

fn close(got: [f32; 3], want: [f32; 3], what: &str) {
    assert!((0..3).all(|k| (got[k] - want[k]).abs() <= 1.5), "{what}: {got:?}, want {want:?}");
}

#[test]
fn a_textured_particle_that_blends_writes_no_depth() {
    let Some(g) = gpu() else { return };
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    let c = CLEAR * 255.0;
    // in front: added red, Z test off, its definition asking to write Z (as
    // 27 of SPELLFX.CSF's do); behind it: green at half alpha (a blended
    // layer's alpha is a x 64 of 128)
    let front = |texture| batch(texture, 1, false, true, 0.3, [0.25, 0.0, 0.0, 1.0]);
    let behind = batch(7, 3, true, false, 0.6, [0.0, 1.0, 0.0, 1.0]);
    // added at the blended alpha (a x 64 of 128): the GS's (Cs - 0) As + Cd
    let added = [c.x + 0.25 * 255.0 * 0.5, c.y, c.z];
    let over = |p: [f32; 3]| [p[0] * 0.5, p[1] * 0.5 + 255.0 * 0.5, p[2] * 0.5];
    // textured: no depth written, the one behind blends over it
    close(pixel(&g, &mut r, 8, (4, 4), &[front(7), behind.clone()]), over(added), "textured, blended");
    // the control: untextured (a solid layer), the flag holds and the one
    // behind fails its depth test
    close(pixel(&g, &mut r, 8, (4, 4), &[front(8), behind.clone()]), added, "untextured");
    // and a textured one that asks for no Z write writes none either way
    let no_z = batch(7, 1, false, false, 0.3, [0.25, 0.0, 0.0, 1.0]);
    close(pixel(&g, &mut r, 8, (4, 4), &[no_z, behind]), over(added), "no Z write asked");
}

#[test]
fn the_plates_go_under_the_particles_and_the_screens_text_over_them() {
    let Some(g) = gpu() else { return };
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    // a font of one character: 'A', 8 by 8, every texel opaque white
    let font = esf::font::Font { id: 1, height: 8, glyphs: vec![esf::font::Glyph { code: b'A' as u16, width: 8, rgba: vec![255; 8 * 8 * 4] }] };
    r.set_font(&g.device, &g.queue, &font);
    // at 448 high a PS2 pixel is a pixel: the glyph covers the 8x8 about the
    // label's centre
    let size = 448;
    let blue = [0.0, 0.0, 1.0, 1.0];
    let red = batch(7, 1, true, false, 0.3, [0.25, 0.0, 0.0, 1.0]);
    let t = r.text.as_mut().unwrap();
    // a plate at the middle, behind the particle; a label of the screen's
    // (the chat's, at depth 0) on the left
    t.plates = vec![Label { at: [0.0, 0.0, 0.5], color: blue, text: "A".into() }];
    t.labels = vec![Label { at: [-0.5, 0.0, 0.0], color: blue, text: "A".into() }];
    // the plate drawn first, the red added over it
    close(pixel(&g, &mut r, size, (224, 224), std::slice::from_ref(&red)), [0.25 * 255.0 * 0.5, 0.0, 255.0], "plate");
    // the screen's text over the particle
    close(pixel(&g, &mut r, size, (112, 224), std::slice::from_ref(&red)), [0.0, 0.0, 255.0], "label");
    // the control: with the particle gone, the plate is its own colour
    close(pixel(&g, &mut r, size, (224, 224), &[]), [0.0, 0.0, 255.0], "plate alone");
}
