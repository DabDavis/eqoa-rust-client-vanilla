//! Light Shadows (the options' Extended > Lighting, which work only with
//! Dynamic Lighting on; off unless chosen): the point lights' light stopped
//! by what stands between a surface and the light, as Sun Shadows stop the
//! sun's (`sunshadow`). Dynamic Lighting's lights (`lit`: the fires, the
//! personal light over the player, the spells' and the glowing weapons')
//! light through everything, walls and bodies alike, as the strip programs'
//! one point light does on a PS2; with these, a character standing by a
//! brazier casts a shadow away from it.
//!
//! A light's shadow map is a cube round it (six faces, each a 90° view along
//! a world axis widened by [`GUARD`] texels each side, so the filter's taps
//! never leave the face they are looked up in), [`map_size`] a side, layers
//! of a 2D depth array (a cube array is not a WebGL2 texture). The frame
//! shadows the [`slots`] most important of its lights: by brightness times
//! reach over distance from the eye, as `eqoa`'s `lights::choose` ranks them
//! (by reach over distance) times how bright each is; a light holding a
//! map keeps it until another's rank passes its by [`HOLD`], and when a map
//! changes hands the leaving light's shadow fades out over [`FADE`] before
//! the coming one's fades in (the budget is never passed), so a shadow never
//! comes or goes in a frame. A light new to the frame's lights brings its
//! shadow with it.
//!
//! The maps are drawn from what the frame draws, as Sun Shadows' are: every
//! opaque layer of a material whose first layer is opaque, both faces, a
//! vertex turned from the light put back [`BACK_PUSH`] along its ray; not the
//! radial flora, the sky, nor any blended layer, particle, billboard, fan,
//! plate or the HUD, nor a model cleared of casting (`GpuModel::casts`: a
//! Wisp's). What does not move (the zones' groups, but their
//! animated props) is drawn once into a cache of each map ([`Points`]'s
//! `cache` array) once its light has stood still a moment ([`SETTLE`]), and
//! again only when a zone within its reach comes or goes; each frame a face
//! that holds a mover (a character, what it holds, a prop) is copied from the
//! cache (a pass that writes the cache's depth: wgpu's GL backend copies a
//! texture through a colour attachment, which a depth texture cannot be) and
//! the movers drawn over it. A light that moves (the personal light as the
//! player walks, a spell's) is drawn whole every frame. A zone's members are
//! culled by each instance's box against the light's sphere and each face.
//!
//! What is nearer a light than [`NEAR`] casts nothing for it (a torch's own
//! stick, a lamp's housing round its flame), nor does a body whose own box
//! holds the light (a spell's light inside its target), nor, for the
//! personal light, the body under it (the player's: its light 3 over the
//! player's feet would shadow the ground round them).
//!
//! The receivers are Dynamic Lighting's pipelines with the point term
//! taking the light's visibility (`lightshadow.wgsl`), with the sun's map too
//! while Sun Shadows are on (one module for every frame: by night the sun's
//! strength is 0 there). The visibility is a tent of the map's texels round
//! the point ([`REACH`]), wide enough that a shadow's edge is soft and glides
//! as its light or its caster moves rather than stepping a texel at a time;
//! and a light of the moment's map stays where it was drawn while the light
//! sways about it ([`MOMENT_HOLD`]). Off (or with Dynamic Lighting off)
//! nothing here is made or run, and every frame is the build's before it,
//! byte for byte.

use crate::{Draw, GpuGroup, GpuModel, Material, Member, Renderer, View, lit, sunshadow};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};

/// The most lights a frame shadows (the receivers' uniform holds this many):
/// every light Dynamic Lighting draws (`lit::MAX_LIGHTS`).
pub const MAX_SLOTS: usize = lit::MAX_LIGHTS;

/// How many lights a frame shadows at the options' Shadow Distance's game
/// value: natively 8 (the personal light and seven more), half that in a page
/// and on Android, where a phone's GPU pays for every comparison at every
/// fragment a light reaches.
pub fn slots() -> usize {
    if cfg!(any(target_arch = "wasm32", target_os = "android")) { 4 } else { 8 }
}

/// The most it shadows when the Shadow Distance is raised all the way: every
/// light Dynamic Lighting draws (natively 16, in a page and on Android 8).
pub fn most_slots() -> usize {
    if cfg!(any(target_arch = "wasm32", target_os = "android")) { MAX_SLOTS / 2 } else { MAX_SLOTS }
}

/// How many lights a frame shadows at `scale` times the game's Shadow
/// Distance (1 to 4): `slots()` at 1, `most_slots()` at 4, evenly between;
/// the more cast, the farther from the eye a light still casts.
pub fn slots_at(scale: f32) -> usize {
    let (lo, hi) = (slots(), most_slots());
    (lo + (((hi - lo) as f32) * ((scale - 1.0) / 3.0).clamp(0.0, 1.0)).round() as usize).min(hi)
}

/// A side of each face's map: 512 natively, a texel of 2 cm at 5 from the
/// light; 256 in a page and on Android.
pub fn map_size(device: &wgpu::Device) -> u32 {
    let want = if cfg!(any(target_arch = "wasm32", target_os = "android")) { 256 } else { 512 };
    want.min(device.limits().max_texture_dimension_2d)
}

/// The maps' format: 16 bits are enough over a light's reach (a depth step
/// of 1.2 cm at 20 from the light, under its texel there), and half the
/// memory of the frame's 32.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth16Unorm;

/// Each face's near plane: what is nearer the light than this along the
/// face's axis casts nothing (a torch's stick, a lamp's housing).
pub const NEAR: f32 = 0.5;

/// The receivers' filter (`lightshadow.wgsl`): a tent of the map's texels
/// this many texels each way round the point, every texel within it by its
/// weight, two texels a bilinear comparison (natively 3: nine comparisons; in
/// a page and on Android 2: four), looked up from a point moved this many
/// texels off the surface along its normal (times the sine of the angle to
/// the light), so no texel of the tent finds the surface itself in front of
/// it. The shadow's edge is soft over about the reach, and moves as smoothly
/// as a light or a caster does: through the four comparisons half a texel
/// apart before it (a reach of 1.5) each texel that flipped as a light moved
/// by a hair moved the shadow a quarter of its depth at once, a row of them
/// along the map's grid half (a page's one comparison, all of it), so every
/// edge crawled a texel at a time under a moving light and showed the
/// texels' steps up close (the owner's "blocky and also jittery"); through
/// the tent a texel is at most a ninth, a row a third.
pub const REACH: f32 = if cfg!(any(target_arch = "wasm32", target_os = "android")) { 2.0 } else { 3.0 };

/// The texels each side of a face past its 90°: the filter's texels
/// ([`REACH`] round the point, half a texel more to their edges) stay
/// inside the face the point is looked up in.
pub const GUARD: f32 = REACH + 1.0;

/// How far a caster's vertex whose normal is turned from the light is put
/// back along its ray (as Sun Shadows' `BACK_PUSH`): the underside of a thin
/// thing does not shadow its own top, and a closed body still casts.
pub const BACK_PUSH: f32 = 0.5;

/// The most a placed thing whose own box holds a light may measure across
/// and still be that light's fixture, which casts nothing for it (`near`): a
/// street lamp's light sits at its halo's middle, inside its housing, whose
/// solid bottom plate filled the downward face's whole view and darkened the
/// ground under the lamp in that face's square (the owner's "the shadow
/// around the light shadows is square"). A building or a courtyard's walls
/// round their torches are larger, and cast.
pub const FIXTURE: f32 = 6.0;

/// How long, in seconds, a shadow takes to come or go when a map changes
/// hands (`eqoa`'s `lights::BUDGET_FADE`).
pub const FADE: f32 = 0.25;

/// How much a light holding a map is ranked over the rest: another takes
/// its place only past this many times its rank.
pub const HOLD: f32 = 1.5;

/// How far a light may move between frames and keep its map (its origin
/// stays where the map was drawn).
pub const STILL: f32 = 1e-3;

/// How far a light of the moment (`eqoa`'s `lights::Moment`: a held item's
/// glow, a spell's particles, at its particles' middle eased from frame to
/// frame) may wander from where its map was drawn before the map follows it,
/// on a leash this long: a glowing weapon swaying in an idle stance moved
/// its map by a few millimetres a frame, every edge it cast crawling a texel
/// at a time (a sway of 3 cm at 0.7 a second: 2,107 pixels of a 960x540
/// frame going back and forth three times or more in two seconds); held,
/// none. Its light is drawn where it is; its shadow is cast from no more
/// than this far off, and its particles' middle lags 0.2 s behind them as it
/// is.
pub const MOMENT_HOLD: f32 = 0.15;

/// The personal light's leash (`MOMENT_HOLD`'s): it rides the walk's place
/// 3 over the feet, which is still while the player stands; walking, its map
/// follows a centimetre behind.
pub const PERSONAL_HOLD: f32 = 0.01;

/// A light of the moment (its reach eased from frame to frame as its
/// particles spread and gather) has its map drawn this much past its reach,
/// and drawn again only when its reach passes the map's or falls under it by
/// this twice: not every frame its reach moves.
pub const ROOM: f32 = 1.1;

/// Where a map stays drawn from as its light moves: where it was while the
/// light is within `hold` of it, else `hold` behind the light on the way it
/// went.
fn leash(map: Vec3, light: Vec3, hold: f32) -> Vec3 {
    let off = light - map;
    let d = off.length();
    if d <= hold { map } else { light - off * (hold / d) }
}

/// How far `l`'s map is drawn: a light of the moment's, the map's own `kept`
/// while it covers the light's reach ([`ROOM`]); a fixed light's and the
/// personal light's, exactly its reach.
fn map_range(l: &lit::PointLight, kept: Option<f32>) -> f32 {
    match kept {
        _ if l.fixed || l.key == lit::PERSONAL_KEY => l.range,
        Some(r) if l.range <= r && l.range * ROOM * ROOM >= r => r,
        _ => l.range * ROOM,
    }
}

/// How far `l`'s map may stay from it (`leash`).
fn hold(l: &lit::PointLight) -> f32 {
    match l.key {
        _ if l.fixed => 0.0,
        lit::PERSONAL_KEY => PERSONAL_HOLD,
        _ => MOMENT_HOLD,
    }
}

/// How long, in seconds, a light stands still before what does not move is
/// drawn into its cache: the personal light moves on the game's frames (30 a
/// second), every other frame drawn at 60.
pub const SETTLE: f32 = 0.1;

/// The faces, each its view's forward and up axes (right is forward × up):
/// +x, −x, +y, −y, +z, −z, as `lightshadow.wgsl`'s `light_face` picks them.
pub const FACES: [(Vec3, Vec3); 6] = [(Vec3::X, Vec3::Y), (Vec3::NEG_X, Vec3::Y), (Vec3::Y, Vec3::Z), (Vec3::NEG_Y, Vec3::NEG_Z), (Vec3::Z, Vec3::Y), (Vec3::NEG_Z, Vec3::Y)];

/// The face a point `v` (less the light) is looked up in: its largest
/// axis's, the earlier axis on a tie (`light_face`).
pub fn face_of(v: Vec3) -> usize {
    let a = v.abs();
    if a.x >= a.y && a.x >= a.z {
        if v.x >= 0.0 { 0 } else { 1 }
    } else if a.y >= a.z {
        if v.y >= 0.0 { 2 } else { 3 }
    } else if v.z >= 0.0 {
        4
    } else {
        5
    }
}

/// The faces' half width, a little over a 90° view's 1: [`GUARD`] texels
/// more each side at `size`.
pub fn widen(size: u32) -> f32 {
    1.0 / (1.0 - 2.0 * GUARD / size.max(16) as f32)
}

/// The depth a face stores for a point `m` along its axis, `a − b / m`
/// (0 at [`NEAR`], 1 at `far`): (a, b).
pub fn depth_map(far: f32) -> (f32, f32) {
    let far = far.max(2.0 * NEAR);
    (far / (far - NEAR), far * NEAR / (far - NEAR))
}

/// Face `k`'s matrix: a point less the light to the face's clip space, out
/// to `far`, the faces widened to `g` (`widen`).
pub fn face_matrix(k: usize, far: f32, g: f32) -> Mat4 {
    let (f, u) = FACES[k];
    let r = f.cross(u);
    let (a, b) = depth_map(far);
    Mat4::from_cols(Vec4::new(r.x / g, u.x / g, a * f.x, f.x), Vec4::new(r.y / g, u.y / g, a * f.y, f.y), Vec4::new(r.z / g, u.z / g, a * f.z, f.z), Vec4::new(0.0, 0.0, -b, 0.0))
}

/// Whether any of the box `(lo, hi)` (less the light) is inside face `k`'s
/// view out to `far`, widened to `g`.
pub fn in_face(k: usize, (lo, hi): (Vec3, Vec3), far: f32, g: f32) -> bool {
    let (f, u) = FACES[k];
    let r = f.cross(u);
    let planes = [(f, -NEAR), (-f, far), (f * g - r, 0.0), (f * g + r, 0.0), (f * g - u, 0.0), (f * g + u, 0.0)];
    planes.iter().all(|&(n, d)| {
        let c = Vec3::new(if n.x >= 0.0 { hi.x } else { lo.x }, if n.y >= 0.0 { hi.y } else { lo.y }, if n.z >= 0.0 { hi.z } else { lo.z });
        n.dot(c) + d >= 0.0
    })
}

/// Whether the box `b` meets the sphere at `c` of radius `r`.
pub fn meets(b: (Vec3, Vec3), c: Vec3, r: f32) -> bool {
    let q = c.clamp(b.0, b.1);
    (q - c).length_squared() <= r * r
}

/// How important a light's shadow is: how bright it is times its reach over
/// its distance from the eye (`lights::choose`'s key, times brightness).
pub fn rank(l: &lit::PointLight, eye: Vec3) -> f32 {
    l.color.max_element() * l.range / (l.at - eye).length().max(1.0)
}

/// `x` moved toward `to` by at most `by`.
fn approach(x: f32, to: f32, by: f32) -> f32 {
    if x < to { (x + by).min(to) } else { (x - by).max(to) }
}

/// One map as the frame has it.
#[derive(Clone, Debug)]
struct Slot {
    key: u64,
    /// The map's origin and reach: the light's where its map was drawn.
    at: Vec3,
    range: f32,
    /// How much of its shadow is drawn (0 to 1), and whether the budget
    /// wants it (else it fades out, and the place is freed at 0).
    strength: f32,
    wanted: bool,
    /// How long its light has stood still, in seconds.
    still: f32,
    /// The groups within its reach (by id) its cache holds; `None` while it
    /// has none.
    cache: Option<Vec<u64>>,
    /// The live faces drawn other than as the cache holds them (movers over
    /// it, or the whole of them): copied from it before they are used again.
    dirty: [bool; 6],
    /// Its light's place in the frame's list.
    j: usize,
}

#[derive(Default)]
struct State {
    slots: [Option<Slot>; MAX_SLOTS],
    /// The last frame's time, and the keys of its lights.
    last: Option<f32>,
    seen: Vec<u64>,
}

/// What a frame drew into the maps (`Renderer::light_shadow_drawn`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Drawn {
    /// Maps in use, and how many of their faces were drawn: whole (from
    /// the lights' casters), into the cache, copied from it.
    pub maps: u32,
    pub whole: u32,
    pub cached: u32,
    pub copied: u32,
    /// Draw calls in all.
    pub draws: u32,
}

/// `lightshadow.wgsl`'s `LightShadows`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ShadowUniform {
    /// Each light of the list (`lit::listed`) its map's place + 1, 0 none.
    slot: [[u32; 4]; lit::MAX_LIGHTS / 4],
    /// Each map's origin less the eye, and its strength.
    origin: [[f32; 4]; MAX_SLOTS],
    /// Each map's depth mapping (a, b: `depth_map`).
    depth: [[f32; 4]; MAX_SLOTS],
    /// The faces' width (`widen`), 1 / the size, a texel's world size per
    /// unit of distance along a face's axis, the filter's reach ([`REACH`]).
    params: [f32; 4],
}

/// `lightcast.wgsl`'s `Face`: one face of one map.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FaceUniform {
    m: [[f32; 4]; 4],
    /// the light less the eye; w: how far a face turned from it is put back
    origin: [f32; 4],
    /// x: the face's layer (the cache's, which the copy reads)
    layer: [u32; 4],
}

/// lit.wgsl's point terms, as the receivers change them (`receivers_wgsl`).
const POINT_LINE: &str = "        light += p.color.rgb * listed_term(l, d2, abs(p.color.w), n);\n";
const POINT_LINE_SHADOWED: &str = "        let term = listed_term(l, d2, abs(p.color.w), n);\n        if (term > 0.0) {\n            light += p.color.rgb * (term * light_shadow(j, i, n));\n        }\n";
const PLANT_LINE: &str = "            light += p.color.rgb * ((1.0 - min(d * abs(p.color.w), 1.0)) * k);\n";
const PLANT_LINE_SHADOWED: &str =
    "            let fall = 1.0 - min(d * abs(p.color.w), 1.0);\n            if (fall > 0.0) {\n                light += p.color.rgb * (fall * k * light_shadow(j, i, vec3<f32>(0.0, 1.0, 0.0)));\n            }\n";

/// lightshadow.wgsl's filter: the tent of radius 3 ([`REACH`]), nine
/// bilinear comparisons; in a page and on Android the tent of radius 2
/// ([`NARROW`]), four: at a page's 256 a side a little wider in the world
/// than the nine at 512 (four of 512's texels each way to their three), and
/// under half their texture reads at every fragment a shadowed light
/// reaches.
const WIDE: &str = "    // the tent of radius 3: texels −2 to 3 from b weigh (1 − |k − fr| / 3) / 3,
    // paired (−2, −1) (0, 1) (2, 3): weights (3 − 2fr, 5, 1 + 2fr) / 9
    let w0 = 3.0 - 2.0 * fr;
    let w2 = 1.0 + 2.0 * fr;
    let a0 = (b - 1.5 + (2.0 - fr) / w0) * e;
    let a1 = (b + 0.5 + (2.0 + fr) / 5.0) * e;
    let a2 = (b + 2.5 + fr / w2) * e;
    var row = w0.x * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a0.x, a0.y), layer, z);
    row += 5.0 * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a1.x, a0.y), layer, z);
    row += w2.x * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a2.x, a0.y), layer, z);
    var vis = w0.y * row;
    row = w0.x * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a0.x, a1.y), layer, z);
    row += 5.0 * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a1.x, a1.y), layer, z);
    row += w2.x * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a2.x, a1.y), layer, z);
    vis += 5.0 * row;
    row = w0.x * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a0.x, a2.y), layer, z);
    row += 5.0 * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a1.x, a2.y), layer, z);
    row += w2.x * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a2.x, a2.y), layer, z);
    vis += w2.y * row;
    return mix(1.0, vis / 81.0, o.w);
";
const NARROW: &str = "    // the tent of radius 2: texels −1 to 2 from b weigh (1 − |k − fr| / 2) / 2,
    // paired (−1, 0) (1, 2): weights (3 − 2fr, 1 + 2fr) / 4
    let w0 = 3.0 - 2.0 * fr;
    let w1 = 1.0 + 2.0 * fr;
    let a0 = (b - 0.5 + (2.0 - fr) / w0) * e;
    let a1 = (b + 1.5 + fr / w1) * e;
    var vis = w0.y * (w0.x * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a0.x, a0.y), layer, z) + w1.x * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a1.x, a0.y), layer, z));
    vis += w1.y * (w0.x * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a0.x, a1.y), layer, z) + w1.x * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a1.x, a1.y), layer, z));
    return mix(1.0, vis / 16.0, o.w);
";

/// Whether the receivers take the narrow filter ([`NARROW`], [`REACH`] 2).
fn narrow() -> bool {
    REACH < 3.0
}

/// The receivers' module: shader.wgsl, lit.wgsl with its point terms taking
/// the maps' visibility (with `sun`, its sun line taking the sun's map's
/// too, `sunshadow::shadowed_lit`), then `sunshadow.wgsl` with the sun and
/// `lightshadow.wgsl` (with the narrow filter where `narrow`).
/// `None` if lit.wgsl's lines are not there to change
/// (tests::the_receivers_take_the_point_lines holds them to it).
pub(crate) fn receivers_wgsl(sun: bool, narrow: bool) -> Option<String> {
    let mut lit = include_str!("lit.wgsl").to_string();
    if sun {
        lit = sunshadow::shadowed_lit(&lit)?;
    }
    if lit.matches(POINT_LINE).count() != 1 || lit.matches(PLANT_LINE).count() != 1 {
        return None;
    }
    let lit = lit.replace(POINT_LINE, POINT_LINE_SHADOWED).replace(PLANT_LINE, PLANT_LINE_SHADOWED);
    let shadow = include_str!("lightshadow.wgsl");
    if shadow.matches(WIDE).count() != 1 {
        return None;
    }
    let shadow = if narrow { shadow.replace(WIDE, NARROW) } else { shadow.to_string() };
    let mut parts = vec![include_str!("shader.wgsl"), &lit];
    if sun {
        parts.push(include_str!("sunshadow.wgsl"));
    }
    parts.push(&shadow);
    Some(parts.join("\n"))
}

/// The casters' module: shader.wgsl's structs and bindings, then
/// `lightcast.wgsl`.
const CAST_WGSL: &str = concat!(include_str!("shader.wgsl"), "\n", include_str!("lightcast.wgsl"));

/// The models' vertex buffers as `model_pipeline` lays them out.
const VERTEX: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x3, 3 => Float32x4, 8 => Uint8x4, 9 => Uint8x4];
const INSTANCE: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4];
const STATIC: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![10 => Float32x4];

fn vertex_buffers() -> [Option<wgpu::VertexBufferLayout<'static>>; 3] {
    let layout = |stride: usize, step_mode, attributes| Some(wgpu::VertexBufferLayout { array_stride: stride as u64, step_mode, attributes });
    [
        layout(size_of::<crate::Vertex>(), wgpu::VertexStepMode::Vertex, &VERTEX[..]),
        layout(64, wgpu::VertexStepMode::Instance, &INSTANCE[..]),
        layout(16, wgpu::VertexStepMode::Instance, &STATIC[..]),
    ]
}

/// A mover this frame: a model, or a group's animated prop.
enum Mover<'a> {
    Model(&'a GpuModel),
    Prop(&'a GpuGroup, &'a Member),
}

/// The receivers of one kind: their pipelines by baked and blend mode
/// (`lit::Lit`'s), and their group 3.
pub(crate) struct Receivers {
    pub(crate) pipelines: [Vec<wgpu::RenderPipeline>; 2],
    pub(crate) bg: wgpu::BindGroup,
}

/// The maps, their pipelines and uniforms, made the first time Light
/// Shadows are drawn with Dynamic Lighting on (`Renderer::set_light_shadows`)
/// and kept: the bundles a group records name them.
pub(crate) struct Points {
    /// Dynamic Lighting's with the maps.
    pub(crate) receivers: Receivers,
    /// And with the sun's map too, made once Sun Shadows are (`attach_sun`).
    pub(crate) with_sun: Option<Receivers>,
    buf: wgpu::Buffer,
    live_view: wgpu::TextureView,
    cmp: wgpu::Sampler,
    /// Each layer of the live maps and of the caches, as an attachment.
    live: Vec<wgpu::TextureView>,
    cache: Vec<wgpu::TextureView>,
    /// The casters: alpha tested, and a solid layer's, depth alone.
    caster: wgpu::RenderPipeline,
    solid: wgpu::RenderPipeline,
    /// The copy from a cache's face to its live one.
    copy: wgpu::RenderPipeline,
    copy_bg: wgpu::BindGroup,
    /// Each face of each map's uniform, at `stride`, and a bind group each
    /// (the casters' group 3, the copy's group 0).
    faces: wgpu::Buffer,
    face_bgs: Vec<wgpu::BindGroup>,
    stride: u64,
    size: u32,
    slots: usize,
    state: std::cell::RefCell<State>,
    pub(crate) drawn: std::cell::Cell<Drawn>,
}

impl Points {
    pub(crate) fn new(device: &wgpu::Device, layouts: [&wgpu::BindGroupLayout; 3], lit: &lit::Lit, sun: Option<&sunshadow::Sun>, format: wgpu::TextureFormat) -> Option<Points> {
        let size = map_size(device);
        let slots = most_slots();
        // a layer more than the maps use: wgpu's GL backend makes a square 2D
        // texture of a multiple of six layers a cube map array (its "cube
        // compatible" guess), which then samples as nothing through a 2D
        // array's comparison (every light wholly in shadow, on GL and in a
        // page's WebGL2)
        let layers = (6 * slots + 1) as u32;
        let array = |label: &str| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: layers },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let (live_map, cache_map) = (array("light shadows"), array("light shadow caches"));
        let layer_views = |t: &wgpu::Texture| {
            (0..layers)
                .map(|l| t.create_view(&wgpu::TextureViewDescriptor { label: Some("light shadow face"), dimension: Some(wgpu::TextureViewDimension::D2), base_array_layer: l, array_layer_count: Some(1), ..Default::default() }))
                .collect::<Vec<_>>()
        };
        let (live, cache) = (layer_views(&live_map), layer_views(&cache_map));
        let whole = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor { label: Some("light shadows"), dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        let (live_view, cache_view) = (whole(&live_map), whole(&cache_map));
        // compared, bilinear (four texels a tap), clamped
        let cmp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("light shadows"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("light shadows"),
            size: size_of::<ShadowUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let receivers = Self::receivers(device, layouts, lit, None, &live_view, &cmp, &buf, format)?;
        // the faces' uniforms, at the device's uniform alignment
        let stride = (size_of::<FaceUniform>() as u64).next_multiple_of(device.limits().min_uniform_buffer_offset_alignment as u64);
        let faces = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("light shadow faces"),
            size: stride * layers as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let face_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("light shadow face"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: wgpu::BufferSize::new(size_of::<FaceUniform>() as u64) },
                count: None,
            }],
        });
        let face_bgs = (0..layers as u64)
            .map(|l| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("light shadow face"),
                    layout: &face_layout,
                    entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &faces, offset: l * stride, size: wgpu::BufferSize::new(size_of::<FaceUniform>() as u64) }) }],
                })
            })
            .collect();
        let cast_module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("light shadow casters"), source: wgpu::ShaderSource::Wgsl(CAST_WGSL.into()) });
        let cast_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("light shadow casters"),
            bind_group_layouts: &[Some(layouts[0]), Some(layouts[1]), Some(layouts[2]), Some(&face_layout)],
            immediate_size: 0,
        });
        let depth_state = |compare| {
            Some(wgpu::DepthStencilState { format: FORMAT, depth_write_enabled: Some(true), depth_compare: Some(compare), stencil: Default::default(), bias: Default::default() })
        };
        let cast_pipeline = |vs: &str, fs: Option<&str>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("light shadow casters"),
                layout: Some(&cast_layout),
                vertex: wgpu::VertexState { module: &cast_module, entry_point: Some(vs), compilation_options: Default::default(), buffers: &vertex_buffers() },
                // both faces, a vertex turned from the light put back
                // (`BACK_PUSH`), as Sun Shadows' casters
                primitive: wgpu::PrimitiveState { front_face: crate::FRONT_FACE, cull_mode: None, ..Default::default() },
                depth_stencil: depth_state(wgpu::CompareFunction::LessEqual),
                multisample: Default::default(),
                fragment: fs.map(|fs| wgpu::FragmentState { module: &cast_module, entry_point: Some(fs), compilation_options: Default::default(), targets: &[] }),
                multiview_mask: None,
                cache: None,
            })
        };
        let (caster, solid) = (cast_pipeline("vs_light_cast", Some("fs_light_cast")), cast_pipeline("vs_light_cast_solid", None));
        // the copy: the cache's depth through a nearest sampler (a depth
        // texture binds as an unfilterable float; wgpu's GL backend leaves a
        // depth texture's own filters linear, which GLES 3.0 counts as
        // incomplete for a texelFetch, as ao.wgsl found)
        let copy_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("light shadow copy"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering), count: None },
            ],
        });
        let nearest = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("light shadow copy"), ..Default::default() });
        let copy_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("light shadow copy"),
            layout: &copy_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&cache_view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&nearest) }],
        });
        let copy_module = device.create_shader_module(wgpu::include_wgsl!("lightcopy.wgsl"));
        let copy_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("light shadow copy"),
            bind_group_layouts: &[Some(&face_layout), Some(&copy_layout)],
            immediate_size: 0,
        });
        let copy = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("light shadow copy"),
            layout: Some(&copy_pipeline_layout),
            vertex: wgpu::VertexState { module: &copy_module, entry_point: Some("vs_copy"), compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(),
            depth_stencil: depth_state(wgpu::CompareFunction::Always),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &copy_module, entry_point: Some("fs_copy"), compilation_options: Default::default(), targets: &[] }),
            multiview_mask: None,
            cache: None,
        });
        let mut p = Points {
            receivers,
            with_sun: None,
            buf,
            live_view,
            cmp,
            live,
            cache,
            caster,
            solid,
            copy,
            copy_bg,
            faces,
            face_bgs,
            stride,
            size,
            slots,
            state: Default::default(),
            drawn: Default::default(),
        };
        if let Some(s) = sun {
            p.attach_sun(device, layouts, lit, s, format);
        }
        Some(p)
    }

    /// The receivers' pipelines and group 3: the lights (Dynamic Lighting's
    /// buffer), with `sun` its map, sampler and uniform, then the maps, their
    /// comparison sampler and uniform.
    #[allow(clippy::too_many_arguments)]
    fn receivers(device: &wgpu::Device, layouts: [&wgpu::BindGroupLayout; 3], lit: &lit::Lit, sun: Option<&sunshadow::Sun>, maps: &wgpu::TextureView, cmp: &wgpu::Sampler, buf: &wgpu::Buffer, format: wgpu::TextureFormat) -> Option<Receivers> {
        let fs = wgpu::ShaderStages::FRAGMENT;
        let depth = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: fs,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false },
            count: None,
        };
        let comparison = |binding| wgpu::BindGroupLayoutEntry { binding, visibility: fs, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison), count: None };
        let mut entries = vec![crate::uniform_entry(0, fs)];
        if sun.is_some() {
            entries.extend([depth(1), comparison(2), crate::uniform_entry(3, fs)]);
        }
        entries.extend([depth(4), comparison(5), crate::uniform_entry(6, fs)]);
        let layout3 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("lights and light shadows"), entries: &entries });
        let mut bound = vec![wgpu::BindGroupEntry { binding: 0, resource: lit.buf.as_entire_binding() }];
        if let Some(s) = sun {
            bound.extend([
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&s.whole) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&s.cmp) },
                wgpu::BindGroupEntry { binding: 3, resource: s.buf.as_entire_binding() },
            ]);
        }
        bound.extend([
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(maps) },
            wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::Sampler(cmp) },
            wgpu::BindGroupEntry { binding: 6, resource: buf.as_entire_binding() },
        ]);
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("lights and light shadows"), layout: &layout3, entries: &bound });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("light shadows"), source: wgpu::ShaderSource::Wgsl(receivers_wgsl(sun.is_some(), narrow())?.into()) });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("light shadowed model"),
            bind_group_layouts: &[Some(layouts[0]), Some(layouts[1]), Some(layouts[2]), Some(&layout3)],
            immediate_size: 0,
        });
        let make = |fs: &str| (0..5).map(|m| crate::model_pipeline(device, &layout, &module, ("vs_lit", fs), m, false, format)).collect();
        Some(Receivers { pipelines: [make("fs_lit"), make("fs_lit_baked")], bg })
    }

    /// The receivers with the sun's map too, once Sun Shadows are made.
    pub(crate) fn attach_sun(&mut self, device: &wgpu::Device, layouts: [&wgpu::BindGroupLayout; 3], lit: &lit::Lit, sun: &sunshadow::Sun, format: wgpu::TextureFormat) {
        if self.with_sun.is_none() {
            self.with_sun = Self::receivers(device, layouts, lit, Some(sun), &self.live_view, &self.cmp, &self.buf, format);
        }
    }

    /// The frame's maps for `view` (its lights, `lit::listed`'s order) at
    /// the time `now` (seconds): the budget's choice held from frame to
    /// frame, each map in use drawn as much as it needs (`Points`), and the
    /// receivers' uniform set.
    pub(crate) fn draw(&self, r: &Renderer, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, view: &View, items: &[Draw], now: f32) {
        let list = lit::listed(view.point, view.lights.as_ref().unwrap_or(&lit::Lights::default()));
        let lights = list.as_slice();
        let mut st = self.state.borrow_mut();
        let dt = st.last.map_or(0.0, |t| (now - t).clamp(0.0, 0.25));
        st.last = Some(now);
        let step = dt / FADE;
        self.choose(&mut st, lights, view.eye, step, slots_at(r.shadow_scale).min(self.slots));
        let mut drawn = Drawn::default();
        let g = widen(self.size);
        let mut u = ShadowUniform::zeroed();
        // every face's uniform, written at once (the maps in use)
        let mut faces = vec![0u8; self.slots * 6 * self.stride as usize];
        u.params = [g, 1.0 / self.size as f32, 2.0 * g / self.size as f32, REACH];
        // what moves this frame: the models (not one cleared of casting,
        // `GpuModel::casts`: a Wisp's), and the groups' animated props
        let mut movers = Vec::new();
        let mut groups = Vec::new();
        for item in items {
            match item {
                Draw::Model(m) if Renderer::draws_anything(m) && m.casts => movers.push(Mover::Model(m)),
                Draw::Group(g) => {
                    groups.push(*g);
                    movers.extend(g.members.iter().filter(|m| m.moves && m.instances > 0).map(|m| Mover::Prop(g, m)));
                }
                _ => {}
            }
        }
        for (k, slot) in st.slots.iter_mut().enumerate() {
            let Some(s) = slot else { continue };
            let l = lights[s.j];
            // drawn again from where the light is (on its leash) and as far
            // as it reaches, when either has moved
            let (at, range) = (leash(s.at, l.at, hold(&l)), map_range(&l, Some(s.range)));
            if (at - s.at).length() > STILL || (range - s.range).abs() > STILL {
                (s.at, s.range, s.still, s.cache) = (at, range, 0.0, None);
            } else {
                s.still += dt;
            }
            drawn.maps += 1;
            u.slot[s.j / 4][s.j % 4] = k as u32 + 1;
            u.origin[k] = (s.at - view.eye).extend(s.strength).to_array();
            let (a, b) = depth_map(s.range);
            u.depth[k] = [a, b, 0.0, 0.0];
            for face in 0..6 {
                let f = FaceUniform { m: face_matrix(face, s.range, g).to_cols_array_2d(), origin: (s.at - view.eye).extend(BACK_PUSH).to_array(), layer: [(k * 6 + face) as u32, 0, 0, 0] };
                let at = (k * 6 + face) * self.stride as usize;
                faces[at..at + size_of::<FaceUniform>()].copy_from_slice(bytemuck::bytes_of(&f));
            }
            let (at, range) = (s.at, s.range);
            let reached: Vec<&GpuGroup> = groups.iter().copied().filter(|g| g.bounds.is_some_and(|b| meets(b, at, range))).collect();
            let ids: Vec<u64> = reached.iter().map(|g| g.id).collect();
            let personal = s.key == lit::PERSONAL_KEY;
            // a light of the moment (a held item's glow, a spell's particles)
            // rides the very body or weapon that would shadow it, following
            // its particles 0.2 s behind: in and out of its carrier's box and
            // within 0.5 of it by turns, it flipped that carrier's shadow on
            // and off from frame to frame (Bam's two glowing swords). It casts
            // what does not move alone; a fire and the personal light cast the
            // movers too
            let casting: Vec<&Mover> = if l.fixed || personal { movers.iter().filter(|m| self.casts(m, at, range, personal)).collect() } else { Vec::new() };
            let mover_box = |m: &Mover| match m {
                Mover::Model(m) => m.world.get(),
                Mover::Prop(_, p) => p.posed.get(),
            };
            let in_face = |face: usize, b: Option<(Vec3, Vec3)>| b.is_none_or(|(lo, hi)| in_face(face, (lo - at, hi - at), range, g));
            if s.still < SETTLE {
                // still moving: drawn whole, every face
                let near = near(&reached, at, range);
                for face in 0..6 {
                    let here: Vec<&&Mover> = casting.iter().filter(|m| in_face(face, mover_box(m))).collect();
                    let mut pass = self.pass(encoder, &self.live[k * 6 + face]);
                    let mut current = None;
                    drawn.draws += self.statics(r, &mut pass, &near, at, range, face, g, k * 6 + face, &mut current);
                    drawn.draws += self.movers(r, &mut pass, &here, k * 6 + face, &mut current);
                    drawn.whole += 1;
                }
                s.dirty = [true; 6];
                continue;
            }
            if s.cache.as_ref() != Some(&ids) {
                let near = near(&reached, at, range);
                for face in 0..6 {
                    let mut pass = self.pass(encoder, &self.cache[k * 6 + face]);
                    let mut current = None;
                    drawn.draws += self.statics(r, &mut pass, &near, at, range, face, g, k * 6 + face, &mut current);
                    drawn.cached += 1;
                }
                s.cache = Some(ids);
                s.dirty = [true; 6];
            }
            for face in 0..6 {
                let here: Vec<&&Mover> = casting.iter().filter(|m| in_face(face, mover_box(m))).collect();
                if here.is_empty() && !s.dirty[face] {
                    continue;
                }
                let mut pass = self.pass(encoder, &self.live[k * 6 + face]);
                pass.set_pipeline(&self.copy);
                pass.set_bind_group(0, &self.face_bgs[k * 6 + face], &[]);
                pass.set_bind_group(1, &self.copy_bg, &[]);
                pass.draw(0..3, 0..1);
                drawn.copied += 1;
                drawn.draws += 1;
                let mut current = None;
                drawn.draws += self.movers(r, &mut pass, &here, k * 6 + face, &mut current);
                s.dirty[face] = !here.is_empty();
            }
        }
        if drawn.maps > 0 {
            queue.write_buffer(&self.faces, 0, &faces);
        }
        queue.write_buffer(&self.buf, 0, bytemuck::bytes_of(&u));
        self.drawn.set(drawn);
    }

    /// The budget's choice for the frame's `lights`: the `slots()` most by
    /// `rank` (a light holding a map at `HOLD` times its own) want a map;
    /// one that holds a map and is not wanted fades out by `step`, its place
    /// freed at nothing; one wanted takes a free place, at once if it is new
    /// to the frame's lights (or has no key), else fading in by `step`.
    fn choose(&self, st: &mut State, lights: &[lit::PointLight], eye: Vec3, step: f32, budget: usize) {
        // each map's light this frame, by its key; a map whose light is gone
        // (or had no key to be found by) goes with it
        for slot in st.slots.iter_mut() {
            let found = slot.as_ref().filter(|s| s.key != 0).and_then(|s| lights.iter().position(|l| l.key == s.key));
            match (slot.as_mut(), found) {
                (Some(s), Some(j)) => s.j = j,
                _ => *slot = None,
            }
        }
        let held = |st: &State, j: usize| st.slots.iter().flatten().any(|s| s.j == j && s.wanted);
        let mut ranked: Vec<(f32, usize)> = lights
            .iter()
            .enumerate()
            .filter(|(_, l)| l.range > 2.0 * NEAR && l.color.max_element() > 0.0)
            .map(|(j, l)| (rank(l, eye) * if held(st, j) { HOLD } else { 1.0 }, j))
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let wanted: Vec<usize> = ranked.iter().take(budget).map(|&(_, j)| j).collect();
        for slot in st.slots.iter_mut() {
            let Some(s) = slot else { continue };
            s.wanted = wanted.contains(&s.j);
            s.strength = approach(s.strength, if s.wanted { 1.0 } else { 0.0 }, step);
            if !s.wanted && s.strength <= 0.0 {
                *slot = None;
            }
        }
        for &j in &wanted {
            if st.slots.iter().flatten().any(|s| s.j == j) {
                continue;
            }
            let Some(free) = st.slots[..budget].iter().position(Option::is_none) else { break };
            let l = lights[j];
            let new = l.key == 0 || !st.seen.contains(&l.key);
            st.slots[free] = Some(Slot { key: l.key, at: l.at, range: map_range(&l, None), strength: if new { 1.0 } else { approach(0.0, 1.0, step) }, wanted: true, still: 0.0, cache: None, dirty: [true; 6], j });
        }
        st.seen = lights.iter().map(|l| l.key).filter(|&k| k != 0).collect();
    }

    /// Whether a mover casts for the light at `at` reaching `range`: in its
    /// reach, and not round it (a body whose own box holds the light; for the
    /// personal light, the body under it).
    fn casts(&self, m: &Mover, at: Vec3, range: f32, personal: bool) -> bool {
        match m {
            Mover::Model(m) => {
                if !m.batches.iter().any(|(_, _, passes, lod)| passes.first().is_some_and(|p| p.blend == 0) && crate::level_drawn(*lod, m.level)) {
                    return false;
                }
                if m.world.get().is_some_and(|b| !meets(b, at, range)) {
                    return false;
                }
                match m.core.get() {
                    // the personal light's own body (a skinned one, and what
                    // its palette carries: a cape) by its footprint; a held
                    // sword's box straddles the lantern's line in an idle
                    // stance, and was taken for the body: weapons cast only
                    // while walking (petition 1389)
                    Some((lo, hi)) if personal && m.skinned => !(lo.x <= at.x && at.x <= hi.x && lo.z <= at.z && at.z <= hi.z),
                    Some((lo, hi)) => !(lo.cmple(at).all() && at.cmple(hi).all()),
                    None => true,
                }
            }
            Mover::Prop(_, p) => p.posed.get().is_some_and(|b| meets(b, at, range)) && p.batches.iter().any(|(_, ps)| opaque(ps)),
        }
    }

    /// A render pass into one face, cleared.
    fn pass<'e>(&self, encoder: &'e mut wgpu::CommandEncoder, view: &wgpu::TextureView) -> wgpu::RenderPass<'e> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("light shadow face"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    /// The instances near a light (`near`) that are in face `face` of its
    /// map (its uniform `layer`), in runs of neighbouring instances of a
    /// member. Returns the draw calls.
    #[allow(clippy::too_many_arguments)]
    fn statics(&self, r: &Renderer, pass: &mut wgpu::RenderPass<'_>, near: &[(&GpuGroup, &Member, usize)], at: Vec3, range: f32, face: usize, g: f32, layer: usize, current: &mut Option<bool>) -> u32 {
        let here = |&(_, m, i): &(&GpuGroup, &Member, usize)| {
            let b = m.boxes[i];
            in_face(face, (b.0 - at, b.1 - at), range, g)
        };
        let mut draws = 0;
        let mut bound: Option<*const GpuGroup> = None;
        let mut layer_bound = false;
        let mut k = 0;
        while k < near.len() {
            if !here(&near[k]) {
                k += 1;
                continue;
            }
            // a run: the same member, instance after instance
            let (gr, m, first) = near[k];
            let mut n = 1;
            while k + n < near.len() && std::ptr::eq(near[k + n].1, m) && near[k + n].2 == first + n && here(&near[k + n]) {
                n += 1;
            }
            k += n;
            if bound.is_none() {
                pass.set_bind_group(0, &r.frame_bg, &[]);
                pass.set_bind_group(2, &r.identity_palette, &[]);
                pass.set_bind_group(3, &self.face_bgs[layer], &[]);
            }
            if bound != Some(gr as *const GpuGroup) {
                pass.set_vertex_buffer(0, gr.vertices.slice(..));
                pass.set_index_buffer(gr.indices.slice(..), wgpu::IndexFormat::Uint32);
                bound = Some(gr as *const GpuGroup);
            }
            let (i0, n) = ((m.instance0 as usize + first) as u64, n as u64);
            pass.set_vertex_buffer(1, gr.instances.slice(i0 * 64..(i0 + n) * 64));
            pass.set_vertex_buffer(2, gr.statics.slice(i0 * 16..(i0 + n) * 16));
            for (indices, passes) in m.batches.iter().filter(|(_, p)| opaque(p)) {
                for p in passes.iter().filter(|p| p.blend == 0) {
                    if *current != Some(p.solid) {
                        pass.set_pipeline(if p.solid { &self.solid } else { &self.caster });
                        *current = Some(p.solid);
                    }
                    // a solid layer's vertex reads no layer: any bound will do
                    if !p.solid || !layer_bound {
                        pass.set_bind_group(1, &p.bg, &[]);
                        layer_bound = true;
                    }
                    pass.draw_indexed(indices.clone(), 0, 0..n as u32);
                    draws += 1;
                }
            }
        }
        draws
    }

    /// The movers `here` into a face (its uniform `layer`): a model's opaque
    /// layers of the level it draws, a prop's every instance. Returns the
    /// draw calls.
    fn movers(&self, r: &Renderer, pass: &mut wgpu::RenderPass<'_>, here: &[&&Mover], layer: usize, current: &mut Option<bool>) -> u32 {
        if here.is_empty() {
            return 0;
        }
        pass.set_bind_group(0, &r.frame_bg, &[]);
        pass.set_bind_group(3, &self.face_bgs[layer], &[]);
        let mut set_pipeline = |pass: &mut wgpu::RenderPass<'_>, solid: bool| {
            if *current != Some(solid) {
                pass.set_pipeline(if solid { &self.solid } else { &self.caster });
                *current = Some(solid);
            }
        };
        let mut draws = 0;
        let mut layer_bound = false;
        for m in here {
            match **m {
                Mover::Model(m) => {
                    pass.set_vertex_buffer(0, m.vertices.slice(..));
                    pass.set_vertex_buffer(1, m.instances.slice(..));
                    pass.set_vertex_buffer(2, m.statics.slice(..));
                    pass.set_bind_group(2, &m.palette_bg, &[]);
                    for (indices, count, passes, lod) in &m.batches {
                        if !passes.first().is_some_and(|p| p.blend == 0) || !crate::level_drawn(*lod, m.level) {
                            continue;
                        }
                        pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                        for p in passes.iter().take(m.passes_used).filter(|p| p.blend == 0) {
                            let solid = p.solid();
                            set_pipeline(pass, solid);
                            if !solid || !layer_bound {
                                pass.set_bind_group(1, &p.bg, &[]);
                                layer_bound = true;
                            }
                            pass.draw_indexed(0..*count, 0, 0..m.instance_count);
                            draws += 1;
                        }
                    }
                }
                Mover::Prop(gr, p) => {
                    pass.set_vertex_buffer(0, gr.vertices.slice(..));
                    pass.set_index_buffer(gr.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.set_bind_group(2, &r.identity_palette, &[]);
                    let (i0, n) = (p.instance0 as u64, p.instances as u64);
                    pass.set_vertex_buffer(1, gr.instances.slice(i0 * 64..(i0 + n) * 64));
                    pass.set_vertex_buffer(2, gr.statics.slice(i0 * 16..(i0 + n) * 16));
                    for (indices, passes) in p.batches.iter().filter(|(_, ps)| opaque(ps)) {
                        for q in passes.iter().filter(|q| q.blend == 0) {
                            set_pipeline(pass, q.solid);
                            if !q.solid || !layer_bound {
                                pass.set_bind_group(1, &q.bg, &[]);
                                layer_bound = true;
                            }
                            pass.draw_indexed(indices.clone(), 0, 0..p.instances);
                            draws += 1;
                        }
                    }
                }
            }
        }
        draws
    }
}

/// The instances of the groups' members that do not move whose boxes meet
/// the light's sphere at `at` reaching `range` (a member's whole box tested
/// first), member by member, instance by instance, but the light's own
/// fixture: one whose box holds the light and is no more than [`FIXTURE`]
/// across.
fn near<'g>(groups: &[&'g GpuGroup], at: Vec3, range: f32) -> Vec<(&'g GpuGroup, &'g Member, usize)> {
    let fixture = |b: &(Vec3, Vec3)| b.0.cmple(at).all() && at.cmple(b.1).all() && (b.1 - b.0).max_element() <= FIXTURE;
    let mut out = Vec::new();
    for gr in groups {
        for m in &gr.members {
            if m.moves || m.instances == 0 || m.bounds.is_some_and(|b| !meets(b, at, range)) || !m.batches.iter().any(|(_, p)| opaque(p)) {
                continue;
            }
            out.extend(m.boxes.iter().enumerate().filter(|(_, b)| meets(**b, at, range) && !fixture(b)).map(|(i, _)| (*gr, m, i)));
        }
    }
    out
}

/// A group's batch casts when its first layer is opaque.
fn opaque(passes: &[Material]) -> bool {
    passes.first().is_some_and(|p| p.blend == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// lit.wgsl's point lines are there to change, once each, alone and
    /// after the sun's change: a change to either (another branch's) fails
    /// here, not as shadows that never come.
    #[test]
    fn the_receivers_take_the_point_lines() {
        for (sun, narrow) in [(false, false), (true, false), (false, true), (true, true)] {
            let s = receivers_wgsl(sun, narrow).expect("lit.wgsl's point lines");
            assert_eq!((s.contains(WIDE), s.contains(NARROW)), (!narrow, narrow), "the wide filter or the narrow");
            assert!(s.contains("light_shadow(j, i, n)") && s.contains("light_shadow(j, i, vec3<f32>(0.0, 1.0, 0.0))"));
            assert!(!s.contains(POINT_LINE) && !s.contains(PLANT_LINE), "the unshadowed lines gone");
            assert!(s.contains("fn light_shadow") && s.contains("fn fs_lit_baked"));
            assert_eq!(s.contains("fn sun_shadow"), sun, "the sun's with it alone");
            assert_eq!(s.contains("min(i.base.w, sun_shadow(i, n))"), sun);
        }
        assert!(CAST_WGSL.contains("fn vs_light_cast") && CAST_WGSL.contains("fn vs_light_cast_solid"));
    }

    /// Every receivers' module (with the sun's map or not, the wide filter
    /// or the narrow), the casters' and the copy's parse and validate, the
    /// narrow filter's modules too, which only a page and Android make.
    #[test]
    fn every_module_validates() {
        use wgpu::naga;
        let check = |name: &str, src: &str| {
            let module = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        };
        for (sun, narrow) in [(false, false), (true, false), (false, true), (true, true)] {
            check(&format!("receivers sun {sun} narrow {narrow}"), &receivers_wgsl(sun, narrow).unwrap());
        }
        check("casters", CAST_WGSL);
        check("copy", include_str!("lightcopy.wgsl"));
    }

    /// Every direction falls in the face its largest axis names, and the
    /// face's matrix puts it inside the face's middle 90° (the guard band
    /// round it), at the depth `depth_map` gives its distance along the
    /// face's axis; the control, the face opposite, puts it behind.
    #[test]
    fn a_point_lands_in_its_face_at_its_depth() {
        let g = widen(512);
        assert!(g > 1.0 && g < 1.02, "{g}");
        let far = 19.2;
        let (a, b) = depth_map(far);
        let mut n = 0;
        for i in 0..2000 {
            let t = i as f32 * 0.618034;
            let v = Vec3::new((t * 7.1).sin(), (t * 3.3).cos(), (t * 5.7).sin() * (t * 1.9).cos()) * (1.0 + (i % 17) as f32);
            if v.length() < 1.0 {
                continue;
            }
            let k = face_of(v);
            let c = face_matrix(k, far, g) * v.extend(1.0);
            assert!(c.w > 0.0, "{v} in face {k}: w {}", c.w);
            let (x, y, z) = (c.x / c.w, c.y / c.w, c.z / c.w);
            assert!(x.abs() <= 1.0 / g + 1e-5 && y.abs() <= 1.0 / g + 1e-5, "{v} in face {k}: ({x}, {y}) past its 90°");
            let m = v.dot(FACES[k].0);
            assert!((z - (a - b / m)).abs() < 1e-5, "{v}: depth {z} against {}", a - b / m);
            let back = face_matrix(k ^ 1, far, g) * v.extend(1.0);
            assert!(back.w < 0.0, "the control: {v} behind the opposite face");
            n += 1;
        }
        assert!(n > 1500);
        assert!(depth_map(far).0 - depth_map(far).1 / NEAR == 0.0 && (a - b / far - 1.0).abs() < 1e-6, "0 at the near plane, 1 at the light's reach");
    }

    /// A map stays where it was drawn while its light is within its hold
    /// and follows that far behind it past it; a light of the moment's
    /// reach is drawn with room and drawn again only past it either way,
    /// a fire's and the personal light's exactly theirs.
    #[test]
    fn a_map_holds_its_place_and_reach() {
        let map = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(leash(map, map + Vec3::new(0.1, 0.0, 0.05), MOMENT_HOLD), map, "within the hold: where it was");
        let light = map + Vec3::new(0.0, 0.0, 1.0);
        let held = leash(map, light, MOMENT_HOLD);
        assert!((held - Vec3::new(1.0, 2.0, 3.0 + 1.0 - MOMENT_HOLD)).length() < 1e-6, "past it: the hold behind the light, {held}");
        assert_eq!(leash(map, light, 0.0), light, "no hold: where the light is");
        let moment = |range: f32| lit::PointLight { at: map, range, color: Vec3::ONE, fixed: false, key: (1 << 40) | 3 };
        let drawn = map_range(&moment(10.0), None);
        assert_eq!(drawn, 10.0 * ROOM, "drawn with room");
        assert_eq!(map_range(&moment(10.5), Some(drawn)), drawn, "a reach eased a little: the map's");
        assert_eq!(map_range(&moment(9.5), Some(drawn)), drawn, "and a little down");
        assert_eq!(map_range(&moment(11.5), Some(drawn)), 11.5 * ROOM, "past the map's: drawn again");
        assert_eq!(map_range(&moment(8.0), Some(drawn)), 8.0 * ROOM, "well under it: drawn again");
        let fire = lit::PointLight { fixed: true, ..moment(10.0) };
        let personal = lit::PointLight { key: lit::PERSONAL_KEY, ..moment(10.0) };
        assert_eq!((map_range(&fire, Some(11.0)), map_range(&personal, Some(11.0))), (10.0, 10.0), "a fire's and the lantern's exactly");
        assert_eq!((hold(&fire), hold(&personal), hold(&moment(10.0))), (0.0, PERSONAL_HOLD, MOMENT_HOLD));
    }

    /// A box is in a face where any of it is in the face's view: a box
    /// straight down the face's axis, one off to its side past the guard
    /// band out of it, one behind the near plane out of it, one past the
    /// light's reach out of it; and one across the seam with the next face
    /// in both.
    #[test]
    fn a_box_is_in_the_faces_it_meets() {
        let g = widen(512);
        let b = |c: Vec3, h: f32| (c - Vec3::splat(h), c + Vec3::splat(h));
        assert!(in_face(0, b(Vec3::new(5.0, 0.0, 0.0), 0.5), 19.0, g));
        assert!(!in_face(0, b(Vec3::new(5.0, 7.0, 0.0), 0.5), 19.0, g), "off to its side");
        assert!(!in_face(0, b(Vec3::new(0.2, 0.0, 0.0), 0.1), 19.0, g), "behind the near plane");
        assert!(!in_face(0, b(Vec3::new(25.0, 0.0, 0.0), 0.5), 19.0, g), "past its reach");
        let seam = b(Vec3::new(5.0, 5.0, 0.0), 0.3);
        assert!(in_face(0, seam, 19.0, g) && in_face(2, seam, 19.0, g), "across the seam: in both");
        assert!(meets(b(Vec3::new(10.0, 0.0, 0.0), 1.0), Vec3::ZERO, 9.5) && !meets(b(Vec3::new(10.0, 0.0, 0.0), 1.0), Vec3::ZERO, 8.5));
    }
}
