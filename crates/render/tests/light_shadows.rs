//! Light Shadows (`render::lightshadow`): a floor seen from above, a point
//! light over it, and what stands between them.
//!
//! - A box beside a light shadows the floor beyond it, away from the light,
//!   and the open floor round them is lit exactly as Dynamic Lighting alone
//!   lights it (no acne, no rings), near the origin and out at Freeport's
//!   x 25,300; the control, Light Shadows off, lights the shadow's place.
//! - A body under the personal light does not shadow the ground round its
//!   own feet; the same light as a fire's shadows it. A body round a light
//!   casts nothing for it.
//! - A zone's group casts from its cache once its light has stood still, a
//!   character moving past casts where it is each frame and nowhere it was,
//!   and the cache is drawn once.
//! - More lights than maps: the budget hands maps between lights as the
//!   eye moves, each shadow fading in and out, no pixel changing by more than
//!   a few levels a frame at 60 frames a second; the control, frames a
//!   second apart, jumps.
//! - With Sun Shadows on too: both shadows, and by night the sun's term as
//!   the strip programs' (the map's last day not left in it).
//! - Every frame with Light Shadows off (or Dynamic Lighting off) byte for
//!   byte a renderer's that never had them.
//! - A model cleared of casting (`GpuModel::casts`: a Wisp's) casts nothing
//!   for a fire or the personal light; the control, the same box casting.
//! Needs a graphics adapter (any backend); without one it says so and passes.

use glam::{Mat4, Vec3};
use render::{Draw, GroupPart, Lights, PointLight, Renderer, View};

const SIZE: u32 = 128;
/// The camera's height over the floor; a 90° view takes in 2·HALF.
const HALF: f32 = 10.0;
const FREEPORT: Vec3 = Vec3::new(25_300.0, 60.0, 15_700.0);

fn layer(texture: i32, blend: i32) -> esf::Layer {
    esf::Layer { flags: 0, texture, wrap: 1, blend, color: [255; 4], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] }
}

/// A quad of corners `c` facing `n` (its winding made front that way).
fn quad(c: [Vec3; 4], n: Vec3, l: esf::Layer) -> esf::Model {
    let front = (c[1] - c[0]).cross(c[2] - c[0]).dot(n) > 0.0;
    let indices = if front { vec![0, 1, 2, 0, 2, 3] } else { vec![0, 2, 1, 0, 3, 2] };
    esf::Model {
        pos: c.iter().map(|p| p.to_array()).collect(),
        uv: vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
        normal: vec![n.to_array(); 4],
        color: vec![[0.0, 0.0, 0.0, 1.0]; 4],
        batches: vec![esf::Batch { material: 0, layers: vec![l], indices, lod: None }],
        ..Default::default()
    }
}

/// The floor, `half` out each way at y 0, facing up.
fn floor(half: f32) -> esf::Model {
    let s = half;
    quad([Vec3::new(-s, 0.0, -s), Vec3::new(-s, 0.0, s), Vec3::new(s, 0.0, s), Vec3::new(s, 0.0, -s)], Vec3::Y, layer(0, 0))
}

/// A box from `lo` to `hi`, its six faces outward.
fn cuboid(lo: Vec3, hi: Vec3, l: esf::Layer) -> esf::Model {
    let (c, h) = ((lo + hi) * 0.5, (hi - lo) * 0.5);
    let mut m = esf::Model::default();
    let mut indices = Vec::new();
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            let mut n = Vec3::ZERO;
            n[axis] = sign;
            let (u, v) = (Vec3::new(n.y.abs() + n.z.abs(), n.x.abs(), 0.0), Vec3::new(0.0, n.z.abs(), n.x.abs() + n.y.abs()));
            let corners = [c + (n - u - v) * h, c + (n - u + v) * h, c + (n + u + v) * h, c + (n + u - v) * h];
            let q = quad(corners, n, l.clone());
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

/// Looking straight down from HALF over `origin`, +x to the right; no sun
/// (its colour black), ambient 0.1, Dynamic Lighting's `lights` on, and the
/// personal light `point`.
fn view(origin: Vec3, lights: &[PointLight], point: Option<(Vec3, f32, Vec3)>) -> View {
    let proj = glam::camera::rh::proj::directx::perspective(90f32.to_radians(), 1.0, 0.3, 800.0);
    let eye = origin + Vec3::new(0.0, HALF, 0.0);
    let v = glam::camera::rh::view::look_at_mat4(eye, origin, Vec3::new(0.0, 0.0, -1.0));
    View { eye, sun: Vec3::ZERO, sun_dir: Vec3::NEG_Y, ambient: Vec3::splat(0.1), clear: Vec3::ZERO, lights: Some(Lights::new(lights)), point, ..View::daylight(proj * v) }
}

/// A fire's light (fixed), red, at `at`.
fn fire(at: Vec3, range: f32, key: u64) -> PointLight {
    PointLight { at, range, color: Vec3::X, fixed: true, key }
}

/// The pixel a floor point (x, z) (from the view's origin) is drawn at.
fn pixel(x: f32, z: f32) -> (u32, u32) {
    let px = ((x / HALF) * 0.5 + 0.5) * SIZE as f32;
    let py = ((z / HALF) * 0.5 + 0.5) * SIZE as f32;
    (px as u32, py as u32)
}

fn at(origin: Vec3) -> [[f32; 4]; 4] {
    Mat4::from_translation(origin).to_cols_array_2d()
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

/// A renderer with Dynamic Lighting `dl`, Sun Shadows `sun` and Light
/// Shadows `light`.
fn renderer(g: &Gpu, dl: bool, sun: bool, light: bool) -> Renderer {
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    r.set_dynamic_lighting(&g.device, dl);
    r.set_sun_shadows(&g.device, sun);
    r.set_light_shadows(&g.device, light);
    r
}

/// The light at height `h` over a floor point `x` from under it, red: its
/// falloff times its cosine, and the ambient.
fn lit_floor(x: f32, h: f32, range: f32) -> f32 {
    let d = (x * x + h * h).sqrt();
    0.1 + (1.0 - d / range) * (h / d)
}

/// The largest difference between two frames, past the pixels inside
/// `(lo, hi)` (pixel corners), and how many pixels differ there.
fn worst_outside(a: &[u8], b: &[u8], lo: (u32, u32), hi: (u32, u32)) -> u8 {
    let mut worst = 0u8;
    for y in 0..SIZE {
        for x in 0..SIZE {
            if (lo.0..=hi.0).contains(&x) && (lo.1..=hi.1).contains(&y) {
                continue;
            }
            let k = ((y * SIZE + x) * 4) as usize;
            worst = worst.max(a[k].abs_diff(b[k]));
        }
    }
    worst
}

/// The light 3 over the origin, a box 1.5 to 2.5 along x and 0.5 to 1.5 up
/// beside it: its shadow on the floor from x 1.8 out to 5, the floor at
/// (3.5, 0) in it, (−4, 0) and (0, 4) open.
#[test]
fn a_box_shadows_the_floor_away_from_a_light() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let (h, range) = (3.0, 20.0);
    for origin in [Vec3::ZERO, FREEPORT] {
        let lights = [fire(origin + Vec3::new(0.0, h, 0.0), range, 1)];
        let frame = |light: bool| {
            let mut r = renderer(&g, true, false, light);
            let f = r.upload(&g.device, &g.queue, &floor(HALF), &[at(origin)], &none);
            let b = r.upload(&g.device, &g.queue, &cuboid(Vec3::new(1.5, 0.5, -0.5), Vec3::new(2.5, 1.5, 0.5), layer(0, 0)), &[at(origin)], &none);
            let px = shot(&g, &mut r, &view(origin, &lights, None), &[Draw::Model(&f), Draw::Model(&b)]);
            (px, r.light_shadow_drawn())
        };
        let ((on, drawn), (off, _)) = (frame(true), frame(false));
        let shade = red(&on, pixel(3.5, 0.0));
        assert!((shade - 0.1).abs() <= 2.5 / 255.0, "at {origin}: the shadow, the ambient alone: {shade}");
        assert!((red(&off, pixel(3.5, 0.0)) - lit_floor(3.5, h, range)).abs() <= 3.0 / 255.0, "the control at {origin}: Light Shadows off light it: {}", red(&off, pixel(3.5, 0.0)));
        for open in [(-4.0, 0.0), (0.0, 4.0), (0.0, 0.0)] {
            let (x, z): (f32, f32) = open;
            let want = lit_floor((x * x + z * z).sqrt(), h, range);
            assert!((red(&on, pixel(x, z)) - want).abs() <= 3.0 / 255.0, "at {origin}: the open floor at {open:?}: {} against {want}", red(&on, pixel(x, z)));
        }
        // the open floor (all but round the box and its shadow) as Dynamic
        // Lighting alone draws it: no acne, no rings
        let worst = worst_outside(&on, &off, pixel(1.2, -1.6), pixel(5.6, 1.6));
        assert!(worst <= 1, "at {origin}: the open floor off Dynamic Lighting's by {worst}");
        let d = drawn.expect("the maps made");
        assert_eq!((d.maps, d.whole), (1, 6), "at {origin}: one map, drawn whole: {d:?}");
    }
}

/// A box beside a fire and one beside the personal light, cleared of
/// casting (`GpuModel::casts` false, as a Wisp's body and all it wears and
/// holds are: petition 1449), shadow nothing: every pixel within a level of
/// Light Shadows off, and the maps drawn without them (the floor alone). The
/// control: the same boxes casting shadow the floor beyond them.
#[test]
fn a_model_cleared_of_casting_casts_nothing() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let (h, range) = (3.0, 20.0);
    let lamp = Vec3::new(0.0, h, 0.0);
    for (what, lights, point) in [("a fire", vec![fire(lamp, range, 1)], None), ("the personal light", Vec::new(), Some((lamp, range, Vec3::X)))] {
        let frame = |light: bool, casts: bool| {
            let mut r = renderer(&g, true, false, light);
            let f = r.upload(&g.device, &g.queue, &floor(HALF), &[at(Vec3::ZERO)], &none);
            // a character's armour is skinned, and stands beside the lantern
            let mut b = r.upload(&g.device, &g.queue, &esf::Model { skinned: true, ..cuboid(Vec3::new(1.5, 0.5, -0.5), Vec3::new(2.5, 1.5, 0.5), layer(0, 0)) }, &[at(Vec3::ZERO)], &none);
            b.casts = casts;
            let px = shot(&g, &mut r, &view(Vec3::ZERO, &lights, point), &[Draw::Model(&f), Draw::Model(&b)]);
            (px, r.light_shadow_drawn())
        };
        let ((cleared, drawn), (casting, cast), (off, _)) = (frame(true, false), frame(true, true), frame(false, true));
        let worst = cleared.iter().zip(&off).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
        assert!(worst <= 1, "{what}: cleared of casting, every pixel within {worst} of Light Shadows off");
        assert!(red(&cleared, pixel(3.5, 0.0)) > 0.1 + 20.0 / 255.0, "{what}: its shadow's place lit: {}", red(&cleared, pixel(3.5, 0.0)));
        assert!((red(&casting, pixel(3.5, 0.0)) - 0.1).abs() <= 2.5 / 255.0, "the control, {what}: the box casting shadows it: {}", red(&casting, pixel(3.5, 0.0)));
        let (cleared, casting) = (drawn.expect("the maps made"), cast.expect("the maps made"));
        assert!(cleared.maps == 1 && cleared.draws < casting.draws, "{what}: the box drawn into the map only while it casts ({cleared:?} cleared, {casting:?} casting)");
    }
}

/// The personal light 3 over a body's feet (the player's lantern) leaves
/// the ground round them lit; the same light as a fire's shadows the floor
/// past the body's edge (1.5 out, where the light's ray passes through it);
/// a fire inside the body casts nothing of it.
#[test]
fn a_body_does_not_shadow_its_own_light() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let warm = Vec3::X;
    let lamp = Vec3::new(0.0, 3.0, 0.0);
    let frame = |lights: &[PointLight], point: Option<(Vec3, f32, Vec3)>, light: bool| {
        let mut r = renderer(&g, true, false, light);
        let f = r.upload(&g.device, &g.queue, &floor(HALF), &[at(Vec3::ZERO)], &none);
        // a character's body is skinned (its footprint rule's)
        let body = r.upload(&g.device, &g.queue, &esf::Model { skinned: true, ..cuboid(Vec3::new(-1.0, 0.0, -1.0), Vec3::new(1.0, 2.0, 1.0), layer(0, 0)) }, &[at(Vec3::ZERO)], &none);
        shot(&g, &mut r, &view(Vec3::ZERO, lights, point), &[Draw::Model(&f), Draw::Model(&body)])
    };
    let past = pixel(1.5, 0.0);
    let personal = frame(&[], Some((lamp, 20.0, warm)), true);
    let personal_off = frame(&[], Some((lamp, 20.0, warm)), false);
    assert!((red(&personal, past) - red(&personal_off, past)).abs() <= 1.0 / 255.0, "the lantern's own body: no shadow at its feet ({} against {})", red(&personal, past), red(&personal_off, past));
    let fixed = frame(&[fire(lamp, 20.0, 7)], None, true);
    assert!((red(&fixed, past) - 0.1).abs() <= 2.5 / 255.0, "the control: a fire there shadows past the body's edge: {}", red(&fixed, past));
    // a fire inside the body
    let inside = Vec3::new(0.0, 1.0, 0.0);
    let held = frame(&[fire(inside, 20.0, 7)], None, true);
    let held_off = frame(&[fire(inside, 20.0, 7)], None, false);
    assert!((red(&held, pixel(4.0, 0.0)) - red(&held_off, pixel(4.0, 0.0))).abs() <= 1.0 / 255.0, "a body round a light casts nothing for it");
    assert!((red(&held, pixel(4.0, 0.0)) - lit_floor(4.0, 1.0, 20.0)).abs() <= 3.0 / 255.0, "and it lights the floor: {}", red(&held, pixel(4.0, 0.0)));
}

/// A zone's post (a group) and a character (a model) walking past a light:
/// the first frame draws the map whole; once the light has stood still the
/// post goes into its cache, drawn once, and each frame after copies the
/// faces the character is in and draws it over them; its shadow is where it
/// is and gone from where it was.
#[test]
fn a_group_casts_from_its_cache_and_a_mover_where_it_is() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let mut r = renderer(&g, true, false, true);
    let lamp = Vec3::new(0.0, 3.0, 0.0);
    let lights = [fire(lamp, 20.0, 3)];
    let post = cuboid(Vec3::new(-2.5, 0.5, -0.5), Vec3::new(-1.5, 1.5, 0.5), layer(0, 0));
    let flat = floor(HALF);
    let group = r.upload_group(
        &g.device,
        &g.queue,
        &[GroupPart { model: &flat, instances: &[render::IDENTITY], statics: &[], moves: false, baked: false }, GroupPart { model: &post, instances: &[render::IDENTITY], statics: &[], moves: false, baked: false }],
        &none,
    );
    let walker = r.upload(&g.device, &g.queue, &cuboid(Vec3::new(-0.5, 0.5, -0.5), Vec3::new(0.5, 1.5, 0.5), layer(0, 0)), &[at(Vec3::new(2.0, 0.0, 2.0))], &none);
    let mut drawn = Vec::new();
    let mut frames = Vec::new();
    for k in 0..12 {
        r.scroll_time = Some(k as f32 / 30.0);
        // the character from z 2 to z −2 along x 2
        let z = 2.0 - 4.0 * k as f32 / 11.0;
        r.set_instances(&g.queue, &walker, &[at(Vec3::new(2.0, 0.0, z))]);
        frames.push((z, shot(&g, &mut r, &view(Vec3::ZERO, &lights, None), &[Draw::Group(&group), Draw::Model(&walker)])));
        drawn.push(r.light_shadow_drawn().unwrap());
    }
    assert_eq!((drawn[0].whole, drawn[0].cached), (6, 0), "the first frame whole: {:?}", drawn[0]);
    let cached: u32 = drawn.iter().map(|d| d.cached).sum();
    assert_eq!(cached, 6, "the cache drawn once: {drawn:?}");
    assert!(drawn[11].whole == 0 && drawn[11].copied >= 1 && drawn[11].copied < 6, "the last frame copies the faces the character is in: {:?}", drawn[11]);
    // the post's shadow on (−3.5, 0) every frame
    for (z, f) in &frames {
        assert!((red(f, pixel(-3.5, 0.0)) - 0.1).abs() <= 2.5 / 255.0, "the post's shadow with the character at z {z}: {}", red(f, pixel(-3.5, 0.0)));
    }
    // the character's at (3.5, 1.75·z) where it is, lit where it was
    let (z0, first) = &frames[0];
    let (z1, last) = &frames[11];
    let shadow = |z: f32| pixel(3.5, z * 1.75);
    assert!(red(first, shadow(*z0)) < 0.15 && red(last, shadow(*z1)) < 0.15, "the character's shadow where it is: {} {}", red(first, shadow(*z0)), red(last, shadow(*z1)));
    assert!(red(last, shadow(*z0)) > 0.3, "and gone from where it was: {}", red(last, shadow(*z0)));
}

/// Ten lights in a row 12 apart, each with a post beside it (its shadow
/// across the row, in the camera's sight), more than the maps: as the eye moves along the row (the camera high over it, still) the
/// budget hands the maps on, the nearest lights' posts casting, each shadow
/// fading in and out, no pixel changing by more than 20 of 255 a frame at 60
/// frames a second (a shadow's whole contrast is about 105). The control,
/// the same walk a second a frame, changes pixels by more than 60 at once.
#[test]
fn the_budget_hands_maps_on_without_a_jump() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let n = render::lightshadow::slots() + 2;
    let lights: Vec<PointLight> = (0..n).map(|k| PointLight { color: Vec3::X * 2.0, ..fire(Vec3::new(k as f32 * 12.0, 3.0, 0.0), 6.0, 100 + k as u64) }).collect();
    let posts: Vec<[[f32; 4]; 4]> = (0..n).map(|k| at(Vec3::new(k as f32 * 12.0, 0.0, 0.0))).collect();
    let span = (n - 1) as f32 * 12.0;
    // the camera over the row's middle, taking it all in, three times as
    // wide as high
    let (w, h) = (3 * SIZE, SIZE);
    let proj = glam::camera::rh::proj::directx::perspective(90f32.to_radians(), 3.0, 0.3, 800.0);
    let camera = Vec3::new(span * 0.5, span / 6.0 + 4.0, 0.0);
    let vp = proj * glam::camera::rh::view::look_at_mat4(camera, Vec3::new(span * 0.5, 0.0, 0.0), Vec3::new(0.0, 0.0, -1.0));
    // a post's shadow on the floor, 3 across the row from its light, and its red
    let shadow_of = |k: usize| {
        let c = vp * Vec3::new(k as f32 * 12.0, 0.0, 3.0).extend(1.0);
        ((((c.x / c.w) * 0.5 + 0.5) * w as f32) as u32, (((0.5 - (c.y / c.w) * 0.5)) * h as f32) as u32)
    };
    let red = |px: &[u8], (x, y): (u32, u32)| px[((y * w + x) * 4) as usize] as f32 / 255.0;
    let walk = |dt: f32| {
        let mut r = renderer(&g, true, false, true);
        let f = r.upload(&g.device, &g.queue, &floor(200.0), &[at(Vec3::ZERO)], &none);
        let p = r.upload(&g.device, &g.queue, &cuboid(Vec3::new(-0.5, 0.5, 1.0), Vec3::new(0.5, 1.5, 2.0), layer(0, 0)), &posts, &none);
        let mut frames: Vec<Vec<u8>> = Vec::new();
        let mut worst = 0u8;
        for k in 0..120 {
            r.scroll_time = Some(k as f32 * dt);
            // the eye from the row's start to its end: the same picture,
            // each light's rank by its distance from it
            let eye = Vec3::new(k as f32 / 119.0 * span, 1.0, 0.0);
            let v = View { view_proj: vp, eye, ..view(Vec3::ZERO, &lights, None) };
            let px = render::capture(&g.device, &g.queue, &mut r, w, h, &v, &[Draw::Model(&f), Draw::Model(&p)], None);
            if let Some(l) = frames.last() {
                worst = worst.max(px.iter().zip(l).map(|(a, b)| a.abs_diff(*b)).max().unwrap());
            }
            frames.push(px);
        }
        (worst, frames)
    };
    let (smooth, frames) = walk(1.0 / 60.0);
    let (first, last) = (&frames[0], &frames[119]);
    assert!(red(first, shadow_of(0)) < 0.15 && red(first, shadow_of(n - 1)) > 0.3, "at the start the near end's posts cast: {} {}", red(first, shadow_of(0)), red(first, shadow_of(n - 1)));
    assert!(red(last, shadow_of(n - 1)) < 0.15 && red(last, shadow_of(0)) > 0.3, "at the end the far end's: {} {}", red(last, shadow_of(n - 1)), red(last, shadow_of(0)));
    assert!(smooth <= 20, "a frame at 60 a second changed a pixel by {smooth}");
    let (jumps, _) = walk(1.0);
    assert!(jumps > 60, "the control, a second a frame: {jumps}");
}

/// The options' Shadow Distance raised all the way: twelve lights in view
/// all get a map where the game's distance gives eight (natively; a page's
/// four and eight), the farthest of them casting too; lowered again, the
/// four farthest give theirs back.
#[test]
fn the_shadow_distance_shadows_more_lights() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let lights: Vec<PointLight> = (0..12).map(|k| fire(Vec3::new(-8.0 + (k % 4) as f32 * 5.0, 3.0, -6.0 + (k / 4) as f32 * 6.0), 10.0, 200 + k as u64)).collect();
    let mut r = renderer(&g, true, false, true);
    let f = r.upload(&g.device, &g.queue, &floor(HALF), &[at(Vec3::ZERO)], &none);
    let maps = |r: &mut Renderer, t: f32| {
        r.scroll_time = Some(t);
        shot(&g, r, &view(Vec3::ZERO, &lights, None), &[Draw::Model(&f)]);
        r.light_shadow_drawn().unwrap().maps as usize
    };
    assert_eq!(maps(&mut r, 0.0), render::lightshadow::slots().min(12), "the game's distance");
    r.set_shadow_distance(4.0);
    assert_eq!(maps(&mut r, 0.1), render::lightshadow::most_slots().min(12), "raised all the way: the farthest cast too");
    r.set_shadow_distance(1.0);
    // the extra maps fade out over a quarter of a second, then are let go
    let _ = maps(&mut r, 0.2);
    assert_eq!(maps(&mut r, 0.6), render::lightshadow::slots().min(12), "lowered: given back");
    assert_eq!((render::lightshadow::slots_at(1.0), render::lightshadow::slots_at(4.0)), (render::lightshadow::slots(), render::lightshadow::most_slots()));
}

/// A lamp's light inside its housing (a street lamp's: its light at its
/// halo's middle, a solid plate under it 0.7 down, glass round it): the plate
/// filled the downward face's view and darkened the floor under the lamp in
/// that face's square (the owner's "the shadow around the light shadows is
/// square"). The housing, a placed thing no more than `FIXTURE` across whose
/// box holds the light, casts nothing for it: the floor is lit as the light's
/// own falloff lights it, every pixel within a level of Light Shadows off (the
/// control); and a room of walls 12 across round the light still shadows the
/// floor outside it.
#[test]
fn a_lamps_housing_casts_no_square() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let lamp = Vec3::new(0.0, 3.0, 0.0);
    let lights = [fire(lamp, 20.0, 11)];
    // the housing's two plates, 1.4 across, 0.7 under and over the light
    let plate = |y: f32, n: Vec3| quad([Vec3::new(-0.7, y, -0.7), Vec3::new(-0.7, y, 0.7), Vec3::new(0.7, y, 0.7), Vec3::new(0.7, y, -0.7)], n, layer(0, 0));
    // one model, as a lamp's housing is
    let (mut housing, over, flat) = (plate(2.3, Vec3::NEG_Y), plate(3.7, Vec3::Y), floor(HALF));
    let base = housing.pos.len() as u32;
    housing.pos.extend(over.pos);
    housing.uv.extend(over.uv);
    housing.normal.extend(over.normal);
    housing.color.extend(over.color);
    let more: Vec<u32> = over.batches[0].indices.iter().map(|i| i + base).collect();
    housing.batches[0].indices.extend(more);
    let frame = |light: bool| {
        let mut r = renderer(&g, true, false, light);
        let part = |m| GroupPart { model: m, instances: &[render::IDENTITY], statics: &[], moves: false, baked: false };
        let gr = r.upload_group(&g.device, &g.queue, &[part(&flat), part(&housing)], &none);
        shot(&g, &mut r, &view(Vec3::ZERO, &lights, None), &[Draw::Group(&gr)])
    };
    let (on, off) = (frame(true), frame(false));
    // the floor round the housing's own pixels, every pixel
    let worst = worst_outside(&on, &off, pixel(-1.0, -1.0), pixel(1.0, 1.0));
    assert!(worst <= 1, "the floor under the lamp off its falloff alone by {worst} (a square under the housing)");
    // a room 12 across round the light, walls 0.5 thick and 1.5 high: the
    // floor 8 out, past its wall, in the wall's shadow
    // one model, two walls: its box holds the light
    let mut room = cuboid(Vec3::new(5.5, 0.0, -6.0), Vec3::new(6.0, 1.5, 6.0), layer(0, 0));
    let other = cuboid(Vec3::new(-6.0, 0.0, -6.0), Vec3::new(-5.5, 1.5, 6.0), layer(0, 0));
    let base = room.pos.len() as u32;
    room.pos.extend(other.pos);
    room.uv.extend(other.uv);
    room.normal.extend(other.normal);
    room.color.extend(other.color);
    let more: Vec<u32> = other.batches[0].indices.iter().map(|i| i + base).collect();
    room.batches[0].indices.extend(more);
    let mut r = renderer(&g, true, false, true);
    let part = |m| GroupPart { model: m, instances: &[render::IDENTITY], statics: &[], moves: false, baked: false };
    let gr = r.upload_group(&g.device, &g.queue, &[part(&flat), part(&room)], &none);
    let px = shot(&g, &mut r, &view(Vec3::ZERO, &lights, None), &[Draw::Group(&gr)]);
    assert!((red(&px, pixel(8.0, 0.0)) - 0.1).abs() <= 2.5 / 255.0, "past the room's wall: its shadow ({})", red(&px, pixel(8.0, 0.0)));
}

/// A character standing still under its own personal light (the lantern 3
/// over its feet), a sword held across its front as an idle stance holds it:
/// the sword's box straddled the lantern's line, so the footprint rule meant
/// for the body took the sword too and it cast nothing, while walking (the
/// sword at its side) it did: weapon shadows only while moving (petition
/// 1389). The body (skinned) still casts nothing for its own lantern; the
/// sword casts in every one of ten idle frames, as it does held at the side
/// (the control, walking).
#[test]
fn a_still_characters_sword_casts_for_its_lantern() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let lamp = (Vec3::new(0.0, 3.0, 0.0), 20.0, Vec3::X);
    let probe = pixel(1.3, 0.0);
    let run = |sword: (Vec3, Vec3)| {
        let mut r = renderer(&g, true, false, true);
        let f = r.upload(&g.device, &g.queue, &floor(HALF), &[at(Vec3::ZERO)], &none);
        let body = r.upload(&g.device, &g.queue, &esf::Model { skinned: true, ..cuboid(Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 2.0, 0.5), layer(0, 0)) }, &[at(Vec3::ZERO)], &none);
        let blade = r.upload(&g.device, &g.queue, &cuboid(sword.0, sword.1, layer(0, 0)), &[at(Vec3::ZERO)], &none);
        (0..10)
            .map(|k| {
                r.scroll_time = Some(k as f32 / 60.0);
                let px = shot(&g, &mut r, &view(Vec3::ZERO, &[], Some(lamp)), &[Draw::Model(&f), Draw::Model(&body), Draw::Model(&blade)]);
                red(&px, probe)
            })
            .collect::<Vec<f32>>()
    };
    let idle = run((Vec3::new(-0.9, 0.9, -0.2), Vec3::new(0.9, 1.1, 0.2)));
    assert!(idle.iter().all(|&v| v < 0.15), "the idle sword's shadow from its lantern, every frame: {idle:?}");
    let walking = run((Vec3::new(0.6, 0.5, -0.2), Vec3::new(0.8, 1.5, 0.2)));
    assert!(walking.iter().all(|&v| v < 0.15), "the control, the sword at its side: {walking:?}");
}

/// A light of the moment riding a body (a weapon's glow in its hand, a
/// spell's particles on it), a hair in and out of the body's box from frame
/// to frame as its particles move: the body cast no shadow for it every other
/// frame and a long one in between, the floor behind the body dark and lit 30
/// times a second (Bam's two glowing swords' shadows "appearing disappearing
/// quickly"). A light of the moment casts what does not move alone, so the
/// floor behind the body keeps the light's own falloff; the control, a fire
/// moved the same way, still flips the body's shadow (a fire never moves).
#[test]
fn a_light_riding_a_body_does_not_flicker() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let behind = pixel(-3.0, 0.0);
    let run = |fixed: bool| {
        let mut r = renderer(&g, true, false, true);
        let f = r.upload(&g.device, &g.queue, &floor(HALF), &[at(Vec3::ZERO)], &none);
        let body = r.upload(&g.device, &g.queue, &cuboid(Vec3::new(-1.0, 0.0, -1.0), Vec3::new(1.0, 2.0, 1.0), layer(0, 0)), &[at(Vec3::ZERO)], &none);
        let mut seen = Vec::new();
        for k in 0..60 {
            r.scroll_time = Some(k as f32 / 60.0);
            // just inside the body's side, then just outside, by turns
            let x = if k % 2 == 0 { 0.95 } else { 1.05 };
            let light = PointLight { at: Vec3::new(x, 1.5, 0.0), range: 10.0, color: Vec3::X, fixed, key: if fixed { 5 } else { (1 << 40) | 5 } };
            let px = shot(&g, &mut r, &view(Vec3::ZERO, &[light], None), &[Draw::Model(&f), Draw::Model(&body)]);
            seen.push(red(&px, behind));
        }
        let lo = seen.iter().copied().fold(f32::MAX, f32::min);
        let hi = seen.iter().copied().fold(f32::MIN, f32::max);
        let flips = seen.windows(2).filter(|w| (w[1] - w[0]).abs() > 0.1).count();
        (hi - lo, flips)
    };
    let (spread, flips) = run(false);
    assert!(spread < 8.0 / 255.0 && flips == 0, "a light of the moment riding a body: the floor behind it changed by {spread} with {flips} flips in a second");
    let (spread, flips) = run(true);
    assert!(spread > 0.1 && flips > 20, "the control, a fire moved so: the body's shadow flips ({spread}, {flips})");
}

/// The floor round a post 1.5 to 2.3 out from a light 3 up, seen from 3
/// over x 4 (60°, a texel of its map 1.5 to 2.5 pixels there), each frame
/// with the light at `light(k)`, 60 frames a second: the frames, and the most
/// any pixel changed from one frame to the next.
fn moving_light_frames(g: &Gpu, light: impl Fn(usize) -> PointLight, frames: usize) -> (Vec<Vec<u8>>, u8) {
    let none = |_: i32| None;
    let mut r = renderer(g, true, false, true);
    let part = |m| GroupPart { model: m, instances: &[render::IDENTITY], statics: &[], moves: false, baked: false };
    let (flat, post) = (floor(HALF), cuboid(Vec3::new(1.5, 0.0, -0.4), Vec3::new(2.3, 2.0, 0.4), layer(0, 0)));
    let gr = r.upload_group(&g.device, &g.queue, &[part(&flat), part(&post)], &none);
    let eye = Vec3::new(4.0, 3.0, 0.0);
    let vp = glam::camera::rh::proj::directx::perspective(60f32.to_radians(), 1.0, 0.1, 100.0) * glam::camera::rh::view::look_at_mat4(eye, Vec3::new(4.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -1.0));
    let (mut out, mut worst): (Vec<Vec<u8>>, u8) = (Vec::new(), 0);
    for k in 0..frames {
        r.scroll_time = Some(k as f32 / 60.0);
        let v = View { view_proj: vp, eye, ..view(Vec3::ZERO, &[light(k)], None) };
        let px = render::capture(&g.device, &g.queue, &mut r, 256, 256, &v, &[Draw::Group(&gr)], None);
        if let Some(l) = out.last() {
            worst = worst.max(px.iter().zip(l).map(|(a, b)| a.abs_diff(*b)).max().unwrap());
        }
        out.push(px);
    }
    (out, worst)
}

/// A light moving a millimetre a frame beside a post (a walking lantern's
/// map is drawn again from where it is every frame): the post's shadow on the
/// floor follows it, and as the light moves the map's texels flip one by one
/// along the edge, a row of them at a time where the edge lies along the
/// map's grid. Through the tent of radius 3 a row is a third of the filter's
/// weight: no pixel changes by 48 levels of the shadow's 112 in a frame (42
/// at most). Through the four comparisons half a texel apart before it a row
/// was half: 3,351 changes of 48 or more in the second, up to 64, every edge
/// crawling a texel at a time (the owner's "jittery"). The control, the light
/// 2.5 cm a frame, changes pixels by more than a flip: the measure sees a
/// move.
#[test]
fn a_moving_lights_shadow_glides() {
    let Some(g) = gpu() else { return };
    let at = |dz: f32| fire(Vec3::new(0.0, 3.0, dz), 20.0, 4);
    let (frames, worst) = moving_light_frames(&g, |k| at(k as f32 * 0.001), 60);
    assert!(worst < 48, "a frame changed a pixel by {worst} (of the shadow's 112) as the light moved a millimetre");
    // and the shadow moved: its edge 7 cm on, some pixels lit that were dark
    let moved = frames[0].iter().zip(&frames[59]).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
    assert!(moved > 40, "the shadow moved with the light: {moved}");
    let (_, jumps) = moving_light_frames(&g, |k| at(k as f32 * 0.025), 12);
    assert!(jumps >= 48, "the control, 2.5 cm a frame: {jumps}");
}

/// A light of the moment swaying 3 cm about its place (a glowing weapon in
/// an idle stance, its particles' middle following the hand): its map stays
/// where it was drawn (`MOMENT_HOLD`), so the post's shadow does not move at
/// all, only the light's own falloff by under a level; drawn again from each
/// frame's place, the shadow's edges crawled back and forth. The control: a
/// fire swayed the same way (a fixed light's map goes where it is) changes
/// its shadow's edge pixels by more.
#[test]
fn a_swaying_light_of_the_moment_holds_its_shadow() {
    let Some(g) = gpu() else { return };
    let sway = |k: usize, fixed: bool| {
        let t = k as f32 / 60.0 * 0.7 * std::f32::consts::TAU;
        let at = Vec3::new(0.0, 3.0, 0.0) + Vec3::new(t.sin(), 0.6 * (2.0 * t).sin(), t.cos()) * 0.03;
        PointLight { at, range: 20.0, color: Vec3::X, fixed, key: if fixed { 6 } else { (1 << 40) | 6 } }
    };
    let (frames, worst) = moving_light_frames(&g, |k| sway(k, false), 60);
    assert!(worst <= 2, "a frame changed a pixel by {worst} as a light of the moment swayed");
    assert!(red_at(&frames[30], 128, 128) < 0.15, "the post's shadow there: {}", red_at(&frames[30], 128, 128));
    let (_, crawl) = moving_light_frames(&g, |k| sway(k, true), 60);
    assert!(crawl > 2, "the control, a fire swayed so: {crawl}");
}

/// The red of pixel (x, y) of a 256-wide frame.
fn red_at(px: &[u8], x: usize, y: usize) -> f32 {
    px[(y * 256 + x) * 4] as f32 / 255.0
}

/// With Sun Shadows on too: at noon the box casts the sun's shadow and the
/// light's; by night (the game's sun under the ground, its light travelling
/// up, a ceiling lit) the sun's term is the strip programs' alone, the last
/// day's map not in it: the frame Light Shadows alone draw, to a level.
#[test]
fn the_sun_and_a_light_shadow_together() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let lamp = Vec3::new(0.0, 3.0, 0.0);
    let lights = [fire(lamp, 20.0, 9)];
    let scene = |r: &mut Renderer| {
        let f = r.upload(&g.device, &g.queue, &floor(HALF), &[at(Vec3::ZERO)], &none);
        // a ceiling over the floor's left half, facing down, the sun's by night
        let c = r.upload(&g.device, &g.queue, &quad([Vec3::new(-9.0, 6.0, -9.0), Vec3::new(-9.0, 6.0, 9.0), Vec3::new(-6.0, 6.0, 9.0), Vec3::new(-6.0, 6.0, -9.0)], Vec3::NEG_Y, layer(0, 0)), &[at(Vec3::ZERO)], &none);
        let b = r.upload(&g.device, &g.queue, &cuboid(Vec3::new(1.5, 0.5, -0.5), Vec3::new(2.5, 1.5, 0.5), layer(0, 0)), &[at(Vec3::ZERO)], &none);
        (f, c, b)
    };
    let noon = |v: View| View { sun: Vec3::new(0.0, 0.0, 1.0), sun_dir: Vec3::new(1.0, -1.0, 0.0).normalize(), ..v };
    let night = |v: View| View { sun: Vec3::new(0.0, 0.0, 1.0), sun_dir: Vec3::new(0.0, 1.0, 0.0), ..v };
    let mut both = renderer(&g, true, true, true);
    let (f, c, b) = scene(&mut both);
    let items = [Draw::Model(&f), Draw::Model(&c), Draw::Model(&b)];
    let day = shot(&g, &mut both, &noon(view(Vec3::ZERO, &lights, None)), &items);
    // the light's shadow (red, x 1.8 to 5) and the sun's (blue: the box's
    // shadow down the light, x 1.5 to 4)
    let px = |f: &[u8], (x, y): (u32, u32), c: usize| f[((y * SIZE + x) * 4) as usize + c] as f32 / 255.0;
    assert!(px(&day, pixel(3.5, 0.0), 0) < 0.15, "the light's shadow by day: {}", px(&day, pixel(3.5, 0.0), 0));
    assert!(px(&day, pixel(3.0, 0.0), 2) < 0.15, "the sun's: {}", px(&day, pixel(3.0, 0.0), 2));
    assert!(px(&day, pixel(-4.0, 0.0), 2) > 0.7, "the open floor in the sun: {}", px(&day, pixel(-4.0, 0.0), 2));
    let dark = shot(&g, &mut both, &night(view(Vec3::ZERO, &lights, None)), &items);
    let mut alone = renderer(&g, true, false, true);
    let (f, c, b) = scene(&mut alone);
    let want = shot(&g, &mut alone, &night(view(Vec3::ZERO, &lights, None)), &[Draw::Model(&f), Draw::Model(&c), Draw::Model(&b)]);
    let worst = dark.iter().zip(&want).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
    assert!(worst <= 1, "by night the frame Light Shadows alone draw: off by {worst}");
    assert!(px(&dark, pixel(3.5, 0.0), 0) < 0.15, "the light's shadow by night");
}

/// Off (and on without Dynamic Lighting) every frame is byte for byte a
/// renderer's that never had them: by day and night, with and without the
/// sun's shadows, a group and a model.
#[test]
fn off_draws_as_before() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let lights = [fire(Vec3::new(0.0, 3.0, 0.0), 20.0, 1), fire(Vec3::new(-5.0, 2.0, 3.0), 12.0, 2)];
    let flat = floor(HALF);
    let post = cuboid(Vec3::new(1.5, 0.5, -0.5), Vec3::new(2.5, 1.5, 0.5), layer(0, 0));
    let frames = |r: &mut Renderer| {
        let gr = r.upload_group(&g.device, &g.queue, &[GroupPart { model: &flat, instances: &[render::IDENTITY], statics: &[], moves: false, baked: false }], &none);
        let b = r.upload(&g.device, &g.queue, &post, &[at(Vec3::ZERO)], &none);
        let mut out = Vec::new();
        for sun in [Vec3::new(1.0, -1.0, 0.3), Vec3::Y] {
            let v = View { sun: Vec3::splat(0.6), sun_dir: sun.normalize(), point: Some((Vec3::new(0.0, 4.0, 1.0), 20.0, Vec3::new(1.0, 0.69, 0.36))), ..view(Vec3::ZERO, &lights, None) };
            out.push(shot(&g, r, &v, &[Draw::Group(&gr), Draw::Model(&b)]));
        }
        out
    };
    for (dl, sun) in [(false, false), (true, false), (true, true), (false, true)] {
        let mut before = renderer(&g, dl, sun, false);
        let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
        r.set_dynamic_lighting(&g.device, dl);
        r.set_sun_shadows(&g.device, sun);
        let want = frames(&mut before);
        // turned on and off again, and on without Dynamic Lighting
        r.set_light_shadows(&g.device, true);
        r.set_light_shadows(&g.device, false);
        assert_eq!(frames(&mut r), want, "Light Shadows off, Dynamic Lighting {dl}, Sun Shadows {sun}");
        if !dl {
            let mut on = renderer(&g, false, sun, true);
            assert_eq!(frames(&mut on), want, "Light Shadows on without Dynamic Lighting, Sun Shadows {sun}");
        }
    }
}
