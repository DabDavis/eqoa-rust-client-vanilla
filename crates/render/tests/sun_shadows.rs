//! Sun Shadows (`render::sunshadow`): a box over a floor seen from above,
//! the sun at 45° (its light travelling +x and down), so the box's shadow
//! lands on the floor beside it, x 1 to 5.
//!
//! - The shadow is there (ambient alone) and the open floor round it is
//!   lit exactly as Dynamic Lighting alone lights it, no acne, no stripes:
//!   near the origin and out at Freeport's x 25,300, and at a sun ten
//!   degrees up over an open floor; the control, Sun Shadows off, lights
//!   the shadow's place.
//! - A cut-out casts its shape (the alpha test), a blended layer, a radial
//!   flora plant and the sky cast nothing, a skinned model casts where its
//!   palette puts it, a ceiling (a face turned from the sun) casts and the
//!   underside of a thin thing does not shadow its own top.
//! - A baked visibility stays the upper bound: half the sun outside the
//!   shadow, none in it (the lesser of the two).
//! - No shadow while the sun is under the horizon or black, and every frame
//!   with Sun Shadows off (or Dynamic Lighting off) byte for byte a
//!   renderer's that never had them.
//! - A group casts once relit, and at once when uploaded with them on.
//! - A model cleared of casting (`GpuModel::casts`: a Wisp's) casts nothing
//!   and is drawn into no cascade; the control, the same box casting.
//! Needs a graphics adapter (any backend); without one it says so and passes.

use glam::{Mat4, Vec3};
use render::{Draw, GroupPart, Lights, Renderer, View};

const SIZE: u32 = 128;
/// The camera's height over the floor; a 90° view takes in 2·HALF.
const HALF: f32 = 20.0;
const FREEPORT: Vec3 = Vec3::new(25_300.0, 60.0, 15_700.0);

fn layer(texture: i32, blend: i32) -> esf::Layer {
    esf::Layer { flags: 0, texture, wrap: 1, blend, color: [255; 4], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] }
}

/// A quad of corners `c` facing `n` (its winding made front that way), its
/// UVs (0,0) (0,1) (1,1) (1,0) in corner order.
fn quad(c: [Vec3; 4], n: Vec3, alpha: f32, l: esf::Layer) -> esf::Model {
    let front = (c[1] - c[0]).cross(c[2] - c[0]).dot(n) > 0.0;
    let indices = if front { vec![0, 1, 2, 0, 2, 3] } else { vec![0, 2, 1, 0, 3, 2] };
    esf::Model {
        pos: c.iter().map(|p| p.to_array()).collect(),
        uv: vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
        normal: vec![n.to_array(); 4],
        color: vec![[0.0, 0.0, 0.0, alpha]; 4],
        batches: vec![esf::Batch { material: 0, layers: vec![l], indices, lod: None }],
        ..Default::default()
    }
}

/// The floor, `half` out each way at y 0, facing up, its sun visibility
/// `alpha`.
fn floor(half: f32, alpha: f32) -> esf::Model {
    let s = half;
    quad([Vec3::new(-s, 0.0, -s), Vec3::new(-s, 0.0, s), Vec3::new(s, 0.0, s), Vec3::new(s, 0.0, -s)], Vec3::Y, alpha, layer(0, 0))
}

/// A box round `c`, `h` out each way, its six faces outward.
fn cube(c: Vec3, h: f32, l: esf::Layer) -> esf::Model {
    let mut m = esf::Model::default();
    let mut indices = Vec::new();
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            let mut n = Vec3::ZERO;
            n[axis] = sign;
            let (u, v) = (Vec3::new(n.y.abs() + n.z.abs(), n.x.abs(), 0.0), Vec3::new(0.0, n.z.abs(), n.x.abs() + n.y.abs()));
            let corners = [c + (n - u - v) * h, c + (n - u + v) * h, c + (n + u + v) * h, c + (n + u - v) * h];
            let q = quad(corners, n, 1.0, l.clone());
            let base = m.pos.len() as u32;
            indices.extend(q.batches[0].indices.iter().map(|i| i + base));
            m.pos.extend(q.pos);
            m.uv.extend(q.uv);
            m.normal.extend(q.normal);
            m.color.extend(q.color);
        }
    }
    m.batches = vec![esf::Batch { material: 0, layers: vec![l], indices, lod: None }];
    m
}

/// Looking straight down from HALF over `origin`, +x to the right; the sun
/// red alone, its light along `sun_dir`; ambient 0.1; Dynamic Lighting's
/// lights (none) on.
fn view(origin: Vec3, sun_dir: Vec3) -> View {
    let proj = glam::camera::rh::proj::directx::perspective(90f32.to_radians(), 1.0, 0.3, 800.0);
    let eye = origin + Vec3::new(0.0, HALF, 0.0);
    let v = glam::camera::rh::view::look_at_mat4(eye, origin, Vec3::new(0.0, 0.0, -1.0));
    View { eye, sun: Vec3::X, sun_dir: sun_dir.normalize(), ambient: Vec3::splat(0.1), clear: Vec3::ZERO, lights: Some(Lights::default()), ..View::daylight(proj * v) }
}

const SUN45: Vec3 = Vec3::new(1.0, -1.0, 0.0);

/// The pixel a floor point (x, z) is drawn at.
fn pixel(x: f32, z: f32) -> (u32, u32) {
    let px = ((x / HALF) * 0.5 + 0.5) * SIZE as f32;
    let py = ((z / HALF) * 0.5 + 0.5) * SIZE as f32;
    (px as u32, py as u32)
}

fn at(origin: Vec3, m: Mat4) -> [[f32; 4]; 4] {
    (Mat4::from_translation(origin) * m).to_cols_array_2d()
}

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

fn shot(g: &Gpu, r: &mut Renderer, view: &View, draws: &[Draw]) -> Vec<u8> {
    render::capture(&g.device, &g.queue, r, SIZE, SIZE, view, draws, None)
}

fn red(px: &[u8], (x, y): (u32, u32)) -> f32 {
    px[((y * SIZE + x) * 4) as usize] as f32 / 255.0
}

fn close(got: f32, want: f32) -> bool {
    (got - want).abs() <= 2.5 / 255.0
}

/// A renderer with Dynamic Lighting on and Sun Shadows as `sun`.
fn renderer(g: &Gpu, sun: bool) -> Renderer {
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    r.set_dynamic_lighting(&g.device, true);
    r.set_sun_shadows(&g.device, sun);
    r
}

/// Under the shadow, floor (3.5, 0); in the open, (−8, 0).
const SHADE: (f32, f32) = (3.5, 0.0);
const OPEN: (f32, f32) = (-8.0, 0.0);

#[test]
fn a_box_shadows_the_floor_beside_it_and_nothing_else() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let lit = 0.1 + std::f32::consts::FRAC_1_SQRT_2;
    for origin in [Vec3::ZERO, FREEPORT] {
        let frame = |sun: bool| {
            let mut r = renderer(&g, sun);
            let f = r.upload(&g.device, &g.queue, &floor(HALF, 1.0), &[at(origin, Mat4::IDENTITY)], &none);
            let b = r.upload(&g.device, &g.queue, &cube(Vec3::new(0.0, 3.0, 0.0), 1.0, layer(0, 0)), &[at(origin, Mat4::IDENTITY)], &none);
            shot(&g, &mut r, &view(origin, SUN45), &[Draw::Model(&f), Draw::Model(&b)])
        };
        let (on, off) = (frame(true), frame(false));
        let p = |(x, z): (f32, f32)| pixel(x, z);
        assert!(close(red(&on, p(SHADE)), 0.1), "at {origin}: the shadow, the ambient alone: {}", red(&on, p(SHADE)));
        assert!(close(red(&on, p(OPEN)), lit), "at {origin}: the open floor: {}", red(&on, p(OPEN)));
        assert!(close(red(&off, p(SHADE)), lit), "the control at {origin}: Sun Shadows off light the shadow's place: {}", red(&off, p(SHADE)));
        // the open floor (all but round the box and its shadow) as Dynamic
        // Lighting alone draws it: no acne, no stripes
        let (lo, hi) = (pixel(-2.0, -2.5), pixel(6.0, 2.5));
        let mut worst = 0u8;
        for y in 0..SIZE {
            for x in 0..SIZE {
                if (lo.0..=hi.0).contains(&x) && (lo.1..=hi.1).contains(&y) {
                    continue;
                }
                let k = ((y * SIZE + x) * 4) as usize;
                worst = worst.max(on[k].abs_diff(off[k]));
            }
        }
        assert!(worst <= 1, "at {origin}: the open floor off Dynamic Lighting's by {worst}");
    }
}

/// A box cleared of casting (`GpuModel::casts` false, as a Wisp's body and
/// all it wears and holds are: petition 1449) shadows nothing: every pixel,
/// the floor beside it and the box itself, within a level of Dynamic
/// Lighting alone, and the cascades drawn without it (the floor alone, once
/// each). The control: the same box casting shadows the floor beside it, and
/// is drawn into every cascade.
#[test]
fn a_model_cleared_of_casting_casts_nothing() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let lit = 0.1 + std::f32::consts::FRAC_1_SQRT_2;
    let frame = |sun: bool, casts: bool| {
        let mut r = renderer(&g, sun);
        let f = r.upload(&g.device, &g.queue, &floor(HALF, 1.0), &[at(Vec3::ZERO, Mat4::IDENTITY)], &none);
        let mut b = r.upload(&g.device, &g.queue, &cube(Vec3::new(0.0, 3.0, 0.0), 1.0, layer(0, 0)), &[at(Vec3::ZERO, Mat4::IDENTITY)], &none);
        b.casts = casts;
        let px = shot(&g, &mut r, &view(Vec3::ZERO, SUN45), &[Draw::Model(&f), Draw::Model(&b)]);
        (px, r.sun_shadow_drawn())
    };
    let ((cleared, drawn), (casting, cast), (alone, _)) = (frame(true, false), frame(true, true), frame(false, true));
    let worst = cleared.iter().zip(&alone).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
    assert!(worst <= 1, "cleared of casting: every pixel within {worst} of Dynamic Lighting alone");
    assert!(close(red(&cleared, pixel(SHADE.0, SHADE.1)), lit), "its shadow's place lit: {}", red(&cleared, pixel(SHADE.0, SHADE.1)));
    assert!(close(red(&casting, pixel(SHADE.0, SHADE.1)), 0.1), "the control: the box casting shadows it: {}", red(&casting, pixel(SHADE.0, SHADE.1)));
    let ((_, cleared_draws), (_, casting_draws)) = (drawn.expect("drawn"), cast.expect("drawn"));
    assert_eq!(casting_draws - cleared_draws, 4, "the box once a cascade, only while it casts ({cleared_draws} draws cleared, {casting_draws} casting)");
}

/// A sun ten degrees up over an open floor out at Freeport (the grazing
/// light acne shows under first): every pixel as Dynamic Lighting alone.
#[test]
fn a_low_sun_over_an_open_floor_far_out_leaves_it_whole() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let low = Vec3::new(10f32.to_radians().cos(), -10f32.to_radians().sin(), 0.3);
    let frame = |sun: bool| {
        let mut r = renderer(&g, sun);
        let f = r.upload(&g.device, &g.queue, &floor(HALF * 4.0, 1.0), &[at(FREEPORT, Mat4::IDENTITY)], &none);
        shot(&g, &mut r, &view(FREEPORT, low), &[Draw::Model(&f)])
    };
    let (on, off) = (frame(true), frame(false));
    let worst = on.iter().zip(&off).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
    assert!(worst <= 1, "the open floor off Dynamic Lighting's by {worst}");
    assert!(red(&on, (64, 64)) > 0.2, "lit by the low sun: {}", red(&on, (64, 64)));
}

/// A quad half see-through (its texture's right half clear) casts its left
/// half alone; the control, an opaque texture, casts both.
#[test]
fn a_cut_out_casts_its_shape() {
    let Some(g) = gpu() else { return };
    // 2x2: the u < 0.5 column opaque, the other clear
    let tex = |id: i32| {
        (id == 1 || id == 2).then(|| {
            let clear = if id == 1 { 0 } else { 255 };
            esf::Image { width: 2, height: 2, rgba: vec![255, 255, 255, 255, 255, 255, 255, clear, 255, 255, 255, 255, 255, 255, 255, clear].chunks(4).flat_map(|c| c.to_vec()).collect(), alpha: true }
        })
    };
    // UV u runs along x (corners (0,0) (0,1) (1,1) (1,0) at x −1, −1, 1, 1)
    let card = |id| quad([Vec3::new(-1.0, 3.0, -1.0), Vec3::new(-1.0, 3.0, 1.0), Vec3::new(1.0, 3.0, 1.0), Vec3::new(1.0, 3.0, -1.0)], Vec3::Y, 1.0, layer(id, 0));
    // as a model, and as a group's member (its layers by cluster: a solid
    // one depth alone, an alpha tested one with its texture)
    let frame = |id, grouped: bool| {
        let mut r = renderer(&g, true);
        let f = r.upload(&g.device, &g.queue, &floor(HALF, 1.0), &[render::IDENTITY], &|_| None);
        let model = card(id);
        let c = r.upload(&g.device, &g.queue, &model, &[render::IDENTITY], &tex);
        let grp = r.upload_group(&g.device, &g.queue, &[GroupPart { model: &model, instances: &[render::IDENTITY], statics: &[], moves: false, baked: false }], &tex);
        let d = if grouped { Draw::Group(&grp) } else { Draw::Model(&c) };
        shot(&g, &mut r, &view(Vec3::ZERO, SUN45), &[Draw::Model(&f), d])
    };
    // the card's left half (x −1..0) falls on x 2..3, its right on 3..4
    let (left, right) = (pixel(2.5, 0.0), pixel(3.6, 0.0));
    for grouped in [false, true] {
        let cut = frame(1, grouped);
        assert!(close(red(&cut, left), 0.1), "the opaque half's shadow (grouped {grouped}): {}", red(&cut, left));
        assert!(red(&cut, right) > 0.7, "the clear half casts nothing (grouped {grouped}): {}", red(&cut, right));
        let whole = frame(2, grouped);
        assert!(close(red(&whole, right), 0.1), "the control: an opaque card casts there (grouped {grouped}): {}", red(&whole, right));
    }
}

/// A blended layer (water, glass) casts nothing, nor a radial flora plant
/// (`Draw::Instances`), nor the sky; the control, the same box opaque.
#[test]
fn blended_flora_and_sky_cast_nothing() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let lit = 0.1 + std::f32::consts::FRAC_1_SQRT_2;
    let frame = |blend: i32, how: u8| {
        let mut r = renderer(&g, true);
        let f = r.upload(&g.device, &g.queue, &floor(HALF, 1.0), &[render::IDENTITY], &none);
        let mut b = r.upload(&g.device, &g.queue, &cube(Vec3::new(0.0, 3.0, 0.0), 1.0, layer(0, blend)), &[render::IDENTITY], &none);
        b.sky = how == 2;
        let d = if how == 1 { Draw::Instances(&b, 0..1) } else { Draw::Model(&b) };
        shot(&g, &mut r, &view(Vec3::ZERO, SUN45), &[Draw::Model(&f), d])
    };
    for (what, blend, how) in [("a blended box", 3, 0), ("a plant", 0, 1), ("the sky", 0, 2)] {
        let px = frame(blend, how);
        assert!(close(red(&px, pixel(SHADE.0, SHADE.1)), lit), "{what} casts nothing: {}", red(&px, pixel(SHADE.0, SHADE.1)));
    }
    let px = frame(0, 0);
    assert!(close(red(&px, pixel(SHADE.0, SHADE.1)), 0.1), "the control: an opaque box: {}", red(&px, pixel(SHADE.0, SHADE.1)));
}

/// A skinned box casts where its palette puts it: moved 8 toward −z, its
/// shadow goes with it; the control, the palette's identity, casts at z 0.
#[test]
fn a_skinned_model_casts_where_its_skin_puts_it() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let mut body = cube(Vec3::new(0.0, 3.0, 0.0), 1.0, layer(0, 0));
    body.bones = vec![[0, 255, 255, 255]; body.pos.len()];
    body.weights = vec![[255, 0, 0, 0]; body.pos.len()];
    body.skinned = true;
    let frame = |moved: bool| {
        let mut r = renderer(&g, true);
        let f = r.upload(&g.device, &g.queue, &floor(HALF, 1.0), &[render::IDENTITY], &none);
        let b = r.upload(&g.device, &g.queue, &body, &[render::IDENTITY], &none);
        let m = if moved { Mat4::from_translation(Vec3::new(0.0, 0.0, -8.0)) } else { Mat4::IDENTITY };
        r.set_palette(&g.queue, &b, &[m.to_cols_array_2d()]);
        shot(&g, &mut r, &view(Vec3::ZERO, SUN45), &[Draw::Model(&f), Draw::Model(&b)])
    };
    let moved = frame(true);
    assert!(close(red(&moved, pixel(3.5, -8.0)), 0.1), "the shadow under the moved body: {}", red(&moved, pixel(3.5, -8.0)));
    assert!(red(&moved, pixel(3.5, 0.0)) > 0.7, "none where it was: {}", red(&moved, pixel(3.5, 0.0)));
    let still = frame(false);
    assert!(close(red(&still, pixel(3.5, 0.0)), 0.1), "the control: unmoved, at z 0: {}", red(&still, pixel(3.5, 0.0)));
}

/// A face turned from the sun casts (a ceiling's, a cave roof's, seen from
/// above), and the underside of a thin thing does not shadow its own top: a
/// card facing up at y 3 with a face turned down 0.1 over it is lit, where
/// that face drawn at its own depth would shadow it; the same face alone, a
/// ceiling over the floor, shadows the floor.
#[test]
fn a_ceiling_casts_and_an_underside_does_not() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let lit = 0.1 + std::f32::consts::FRAC_1_SQRT_2;
    let square = |y: f32, n: Vec3| quad([Vec3::new(-1.0, y, -1.0), Vec3::new(-1.0, y, 1.0), Vec3::new(1.0, y, 1.0), Vec3::new(1.0, y, -1.0)], n, 1.0, layer(0, 0));
    let mut r = renderer(&g, true);
    let f = r.upload(&g.device, &g.queue, &floor(HALF, 1.0), &[render::IDENTITY], &none);
    let ceiling = r.upload(&g.device, &g.queue, &square(3.0, Vec3::NEG_Y), &[render::IDENTITY], &none);
    let px = shot(&g, &mut r, &view(Vec3::ZERO, SUN45), &[Draw::Model(&f), Draw::Model(&ceiling)]);
    assert!(close(red(&px, pixel(3.0, 0.0)), 0.1), "a ceiling's shadow on the floor: {}", red(&px, pixel(3.0, 0.0)));
    let top = r.upload(&g.device, &g.queue, &square(3.0, Vec3::Y), &[render::IDENTITY], &none);
    let under = r.upload(&g.device, &g.queue, &square(3.1, Vec3::NEG_Y), &[render::IDENTITY], &none);
    let px = shot(&g, &mut r, &view(Vec3::ZERO, SUN45), &[Draw::Model(&f), Draw::Model(&top), Draw::Model(&under)]);
    // the card's middle, seen from 17 over it
    assert!(close(red(&px, (SIZE / 2, SIZE / 2)), lit), "the card's top under its own underside: {}", red(&px, (SIZE / 2, SIZE / 2)));
    assert!(close(red(&px, pixel(3.0, 0.0)), 0.1), "and the pair's shadow on the floor: {}", red(&px, pixel(3.0, 0.0)));
}

/// A group's casters go by where they are: forty boxes a part each, 50
/// apart along x, drawn into a cascade only where its box reaches them
/// (the near cascades a box or two, the last the boxes within its reach
/// and its 500 upwind), where drawn by the group's runs (32 members, in
/// order) every cascade drew every box; the box by the camera still casts.
#[test]
fn a_cascade_draws_the_casters_it_reaches() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let bx = cube(Vec3::new(0.0, 3.0, 0.0), 1.0, layer(0, 0));
    let at: Vec<[[f32; 4]; 4]> = (0..40).map(|k| Mat4::from_translation(Vec3::new(-1000.0 + 50.0 * k as f32, 0.0, 0.0)).to_cols_array_2d()).collect();
    let parts: Vec<GroupPart> = at.iter().map(|m| GroupPart { model: &bx, instances: std::slice::from_ref(m), statics: &[], moves: false, baked: false }).collect();
    let mut r = renderer(&g, true);
    let f = r.upload(&g.device, &g.queue, &floor(HALF, 1.0), &[render::IDENTITY], &none);
    let grp = r.upload_group(&g.device, &g.queue, &parts, &none);
    let px = shot(&g, &mut r, &view(Vec3::ZERO, SUN45), &[Draw::Model(&f), Draw::Group(&grp)]);
    assert!(close(red(&px, pixel(SHADE.0, SHADE.1)), 0.1), "the box at the camera casts: {}", red(&px, pixel(SHADE.0, SHADE.1)));
    let (_, draws) = r.sun_shadow_drawn().expect("drawn");
    // the floor once a cascade, and the boxes each cascade reaches
    assert!(draws < 40, "{draws} draws into the cascades, of 160 by run");
    assert!(draws > 4 + 4, "and the boxes near enough drawn: {draws}");
}

/// Every cascade is drawn every frame, on texels that hold still while the
/// sun does: with nothing moving, the frames after the first are byte for
/// byte the first; and after the camera has turned away and back (150
/// degrees a frame) and a little a frame, each frame is byte for byte what a
/// renderer drawing that view first draws, the shadow of a box 70 off in it.
#[test]
fn every_cascade_is_drawn_every_frame() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let far_view = |turn: f32| {
        let proj = glam::camera::rh::proj::directx::perspective(55f32.to_radians(), 1.6, 0.3, 800.0);
        let eye = Vec3::new(0.0, 25.0, 70.0);
        let look = Mat4::from_rotation_y(turn).transform_vector3(Vec3::new(0.0, -25.0, -70.0)).normalize();
        let v = glam::camera::rh::view::look_to_mat4(eye, look, Vec3::Y);
        View { eye, sun: Vec3::X, sun_dir: SUN45.normalize(), ambient: Vec3::splat(0.1), clear: Vec3::ZERO, lights: Some(Lights::default()), ..View::daylight(proj * v) }
    };
    let scene = |r: &mut Renderer| {
        let f = r.upload(&g.device, &g.queue, &floor(HALF * 8.0, 1.0), &[render::IDENTITY], &none);
        let b = r.upload(&g.device, &g.queue, &cube(Vec3::new(0.0, 3.0, 0.0), 1.0, layer(0, 0)), &[render::IDENTITY], &none);
        (f, b)
    };
    let mut r = renderer(&g, true);
    let (f, b) = scene(&mut r);
    let first = shot(&g, &mut r, &far_view(0.0), &[Draw::Model(&f), Draw::Model(&b)]);
    for k in 1..4 {
        assert!(shot(&g, &mut r, &far_view(0.0), &[Draw::Model(&f), Draw::Model(&b)]) == first, "frame {k}: nothing moving, the same picture");
    }
    // the box's shadow on the floor, 70 off: where each frame draws it
    let shadow_px = |v: &View| {
        let c = v.view_proj.project_point3(Vec3::new(3.5, 0.0, 0.0));
        (((c.x * 0.5 + 0.5) * SIZE as f32) as u32, ((0.5 - c.y * 0.5) * SIZE as f32) as u32)
    };
    for (before, turn) in [(2.6f32, 0.3f32), (-2.6, -0.3), (0.13, 0.15), (0.15, 0.17), (0.17, 0.19)] {
        shot(&g, &mut r, &far_view(before), &[Draw::Model(&f), Draw::Model(&b)]);
        let v = far_view(turn);
        let after = shot(&g, &mut r, &v, &[Draw::Model(&f), Draw::Model(&b)]);
        let mut fresh = renderer(&g, true);
        let (f2, b2) = scene(&mut fresh);
        let whole = shot(&g, &mut fresh, &v, &[Draw::Model(&f2), Draw::Model(&b2)]);
        let p = shadow_px(&v);
        assert!(red(&whole, p) < 0.3, "turned {turn}: the shadow at {p:?}: {}", red(&whole, p));
        assert!(after == whole, "turned {turn}: the frame as a renderer drawing it first draws it");
    }
}

/// The floor's baked visibility (vertex alpha 0.5) stays the upper bound:
/// half the sun in the open, none in the shadow (the lesser of the two).
#[test]
fn a_baked_visibility_is_the_upper_bound() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let mut r = renderer(&g, true);
    let f = r.upload(&g.device, &g.queue, &floor(HALF, 0.5), &[render::IDENTITY], &none);
    let b = r.upload(&g.device, &g.queue, &cube(Vec3::new(0.0, 3.0, 0.0), 1.0, layer(0, 0)), &[render::IDENTITY], &none);
    let px = shot(&g, &mut r, &view(Vec3::ZERO, SUN45), &[Draw::Model(&f), Draw::Model(&b)]);
    let half = 0.1 + 0.5 * std::f32::consts::FRAC_1_SQRT_2;
    assert!(close(red(&px, pixel(OPEN.0, OPEN.1)), half), "the bake's half in the open: {}", red(&px, pixel(OPEN.0, OPEN.1)));
    assert!(close(red(&px, pixel(SHADE.0, SHADE.1)), 0.1), "none in the shadow: {}", red(&px, pixel(SHADE.0, SHADE.1)));
}

/// Byte for byte: Sun Shadows off with Dynamic Lighting on, a renderer that
/// had them on and turned them off, and Sun Shadows on under a sun below
/// the horizon or a black one, each as a renderer that never had them; and
/// with Dynamic Lighting off, Sun Shadows on draw as the PS2's renderer.
#[test]
fn off_and_the_sun_down_draw_as_before() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let fl = floor(HALF, 0.75);
    let bx = cube(Vec3::new(0.0, 3.0, 0.0), 1.0, layer(0, 0));
    let draw = |r: &mut Renderer, v: &View| {
        let f = r.upload(&g.device, &g.queue, &fl, &[render::IDENTITY], &none);
        let grp = r.upload_group(&g.device, &g.queue, &[GroupPart { model: &bx, instances: &[render::IDENTITY], statics: &[], moves: false, baked: false }], &none);
        shot(&g, r, v, &[Draw::Model(&f), Draw::Group(&grp)])
    };
    let night = Vec3::new(0.3, 0.9, 0.0);
    for (what, sun_dir, sun) in [("noon", SUN45, Vec3::X), ("the sun under the horizon", night, Vec3::X), ("a black sun", SUN45, Vec3::ZERO)] {
        let v = View { sun, ..view(Vec3::ZERO, sun_dir) };
        let mut plain = renderer(&g, false);
        let want = draw(&mut plain, &v);
        let mut toggled = renderer(&g, true);
        toggled.set_sun_shadows(&g.device, false);
        assert!(draw(&mut toggled, &v) == want, "{what}: turned off again");
        if what != "noon" {
            let mut on = renderer(&g, true);
            assert!(draw(&mut on, &v) == want, "{what}: on, nothing to cast");
        }
    }
    // Dynamic Lighting off: the PS2's way, Sun Shadows or not
    let ps2 = View { lights: None, ..view(Vec3::ZERO, SUN45) };
    let mut plain = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    let want = draw(&mut plain, &ps2);
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    r.set_sun_shadows(&g.device, true);
    assert!(draw(&mut r, &ps2) == want, "Dynamic Lighting off");
    let mut r = renderer(&g, true);
    r.set_dynamic_lighting(&g.device, false);
    assert!(draw(&mut r, &View { lights: None, ..ps2 }) == want, "Dynamic Lighting turned off with them on");
}

/// A group uploaded before Sun Shadows came on casts nothing until relit;
/// one uploaded with them on casts at once.
#[test]
fn a_group_casts_once_relit() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let bx = cube(Vec3::new(0.0, 3.0, 0.0), 1.0, layer(0, 0));
    let part = || GroupPart { model: &bx, instances: &[render::IDENTITY], statics: &[], moves: false, baked: false };
    let mut r = renderer(&g, false);
    let f = r.upload(&g.device, &g.queue, &floor(HALF, 1.0), &[render::IDENTITY], &none);
    let mut before = r.upload_group(&g.device, &g.queue, &[part()], &none);
    r.set_sun_shadows(&g.device, true);
    let v = view(Vec3::ZERO, SUN45);
    let shade = pixel(SHADE.0, SHADE.1);
    let px = shot(&g, &mut r, &v, &[Draw::Model(&f), Draw::Group(&before)]);
    assert!(red(&px, shade) > 0.7, "not yet relit: no shadow from it ({})", red(&px, shade));
    r.relight(&g.device, &mut before);
    let px = shot(&g, &mut r, &v, &[Draw::Model(&f), Draw::Group(&before)]);
    assert!(close(red(&px, shade), 0.1), "relit: its shadow ({})", red(&px, shade));
    let after = r.upload_group(&g.device, &g.queue, &[part()], &none);
    let px = shot(&g, &mut r, &v, &[Draw::Model(&f), Draw::Group(&after)]);
    assert!(close(red(&px, shade), 0.1), "uploaded with them on: its shadow ({})", red(&px, shade));
}

/// The shadows move with the sun as it is: as the sun turns a hundredth of
/// a degree a frame out at Freeport (six frames of the game's day), the
/// shadow of a tall box turned across the map's texels lengthens a little at
/// almost every frame, no move more than a few times the mean; the control,
/// the same turn taken in eighths of a degree (the build before), stands
/// still and then jumps. The shadow's place is its darkness's centroid along
/// the light (Dynamic Lighting alone less Sun Shadows, each its own
/// renderer, frame by frame).
#[test]
fn the_shadows_move_with_the_sun_as_it_is() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    const N: u32 = 256;
    let floor_m = floor(HALF, 1.0);
    let pole = cube(Vec3::new(0.0, 1.0, 0.0), 1.0, layer(0, 0));
    let pole_at = at(FREEPORT, Mat4::from_translation(Vec3::new(-10.0, 0.0, 0.0)) * Mat4::from_rotation_y(0.5) * Mat4::from_scale(Vec3::new(1.0, 5.0, 1.0)));
    // the game's path: 30 degrees up at 240, its light travelling +x and down
    let sun_at = |deg: f32| {
        let a = deg.to_radians();
        Vec3::new(-a.sin(), a.cos(), 0.0)
    };
    let places = |step: Option<f32>| -> Vec<f64> {
        let (mut on, mut alone) = (renderer(&g, true), renderer(&g, false));
        let scene = |r: &mut Renderer| (r.upload(&g.device, &g.queue, &floor_m, &[at(FREEPORT, Mat4::IDENTITY)], &none), r.upload(&g.device, &g.queue, &pole, &[pole_at], &none));
        let (a, b) = (scene(&mut on), scene(&mut alone));
        (0..=25)
            .map(|k| {
                let deg = 240.0 + 0.01 * k as f32;
                let v = view(FREEPORT, sun_at(step.map_or(deg, |s| (deg / s).round() * s)));
                let lit = render::capture(&g.device, &g.queue, &mut on, N, N, &v, &[Draw::Model(&a.0), Draw::Model(&a.1)], None);
                let plain = render::capture(&g.device, &g.queue, &mut alone, N, N, &v, &[Draw::Model(&b.0), Draw::Model(&b.1)], None);
                let (mut sum, mut moment) = (0.0f64, 0.0f64);
                for y in 0..N {
                    for x in 0..N {
                        let k = ((y * N + x) * 4) as usize;
                        let dark = (plain[k] as f64 - lit[k] as f64 - 2.0).max(0.0);
                        sum += dark;
                        moment += dark * x as f64;
                    }
                }
                assert!(sum > 0.0, "a shadow at {deg}");
                moment / sum
            })
            .collect()
    };
    let moves = |p: &[f64]| -> Vec<f64> { p.windows(2).map(|w| w[1] - w[0]).collect() };
    let smooth = moves(&places(None));
    let mean = smooth.iter().map(|m| m.abs()).sum::<f64>() / smooth.len() as f64;
    let moved = smooth.iter().filter(|m| m.abs() > 1e-4).count();
    let largest = smooth.iter().fold(0.0f64, |a, m| a.max(m.abs()));
    assert!(mean > 0.005 && moved >= 20 && largest < 4.0 * mean, "the sun as it is: moved at {moved} of 25 frames, the largest move {largest} px, the mean {mean} ({smooth:?})");
    let stepped = moves(&places(Some(0.125)));
    let mean_s = stepped.iter().map(|m| m.abs()).sum::<f64>() / stepped.len() as f64;
    let moved_s = stepped.iter().filter(|m| m.abs() > 1e-4).count();
    let largest_s = stepped.iter().fold(0.0f64, |a, m| a.max(m.abs()));
    assert!(moved_s <= 3 && largest_s > 8.0 * mean_s, "the control, in eighths of a degree: moved at {moved_s} of 25 frames, the largest move {largest_s} px, the mean {mean_s}");
}

/// A character's shadow keeps up with it wherever there are shadows: a box
/// (turned across the map's texels) running 0.15 a frame (a run at 60 frames
/// a second) 74 off, in the third cascade, and 149 off, in the last (a lens
/// half as wide, so it is as large in the picture), moves its shadow with it
/// at every frame, its darkness's centroid (Dynamic Lighting alone less Sun
/// Shadows) on by no less than half the mean move any frame. The far two
/// cascades drawn in turn, as at first, held the shadow where it had been
/// at every other frame and the box's sunlit side in it: the centroid went
/// back 4.4 pixels and on 6.4 by turns at 74, back 1.2 and on 3.4 at 149.
#[test]
fn a_running_box_casts_where_it_is_each_frame() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    const N: u32 = 512;
    let floor_m = floor(HALF * 8.0, 1.0);
    let bx = cube(Vec3::new(0.0, 3.0, 0.0), 1.0, layer(0, 0));
    for (cascade, far) in [(2usize, 1.0f32), (3, 2.0)] {
        let proj = glam::camera::rh::proj::directx::perspective((55.0 / far).to_radians(), 1.0, 0.3, 800.0);
        let eye = Vec3::new(0.0, 25.0, 70.0) * far;
        let v = View { eye, sun: Vec3::X, sun_dir: SUN45.normalize(), ambient: Vec3::splat(0.1), clear: Vec3::ZERO, lights: Some(Lights::default()), ..View::daylight(proj * glam::camera::rh::view::look_at_mat4(eye, Vec3::ZERO, Vec3::Y)) };
        let depth = (v.view_proj * Vec3::new(3.0, 0.0, 0.0).extend(1.0)).w;
        assert!(depth > sunshadow_end(cascade - 1) && depth < sunshadow_end(cascade), "the shadow in cascade {cascade}: {depth}");
        let (mut on, mut alone) = (renderer(&g, true), renderer(&g, false));
        let scene = |r: &mut Renderer| (r.upload(&g.device, &g.queue, &floor_m, &[render::IDENTITY], &none), r.upload(&g.device, &g.queue, &bx, &[render::IDENTITY], &none));
        let (a, b) = (scene(&mut on), scene(&mut alone));
        let places: Vec<f64> = (0..12)
            .map(|k| {
                let m = (Mat4::from_translation(Vec3::new(-0.9 + 0.15 * k as f32, 0.0, 0.0)) * Mat4::from_rotation_y(0.5)).to_cols_array_2d();
                on.set_instances(&g.queue, &a.1, &[m]);
                alone.set_instances(&g.queue, &b.1, &[m]);
                let lit = render::capture(&g.device, &g.queue, &mut on, N, N, &v, &[Draw::Model(&a.0), Draw::Model(&a.1)], None);
                let plain = render::capture(&g.device, &g.queue, &mut alone, N, N, &v, &[Draw::Model(&b.0), Draw::Model(&b.1)], None);
                let (mut sum, mut moment) = (0.0f64, 0.0f64);
                for y in 0..N {
                    for x in 0..N {
                        let i = ((y * N + x) * 4) as usize;
                        let dark = (plain[i] as f64 - lit[i] as f64 - 2.0).max(0.0);
                        sum += dark;
                        moment += dark * x as f64;
                    }
                }
                assert!(sum > 0.0, "cascade {cascade}: a shadow at frame {k}");
                moment / sum
            })
            .collect();
        let moves: Vec<f64> = places.windows(2).map(|w| w[1] - w[0]).collect();
        let mean = moves.iter().sum::<f64>() / moves.len() as f64;
        assert!(mean > 0.3 && moves.iter().all(|m| *m > 0.5 * mean), "cascade {cascade}: the shadow's moves, frame by frame: {moves:?}");
    }
}

/// Where cascade `c` ends before the camera (`render::sunshadow::ENDS`).
fn sunshadow_end(c: usize) -> f32 {
    render::sunshadow::ENDS[c]
}
