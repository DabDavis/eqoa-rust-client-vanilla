//! The renderer, on wgpu.
//!
//! Models out of the containers, textured through the dictionary, drawn as the
//! game draws them: the VU1 strip programs' lighting, one PASS PER MATERIAL
//! LAYER with the GS state `VIMaterial::ConstructTextPass` builds for it
//! (alpha test or blend mode, wrap, the UV transform and its scroll), and GPU
//! skinning as `VIRStrip4` does it.
//!
//! Two passes, as the scene draws them: a material whose first layer is
//! opaque draws every layer at once, and one whose first layer blends waits
//! for the second pass (`MaterialHasDepthSort`, `RenderDeferredList`). The
//! second pass goes in model order, where the game's goes rooms, flora, then
//! actors in the order its tree walk met them.

pub mod ao;
pub mod billboard;
pub mod curve;
pub mod fans;
pub mod fog16;
pub mod lightshadow;
pub mod lit;
pub mod map;
pub mod overlay;
pub mod packed;
pub mod packmem;
pub mod petart;
pub mod recolor;
pub mod scale;
pub mod shadow;
pub mod shell;
pub mod standin;
pub mod sunshadow;
pub mod tattoo;
pub mod text;
pub mod trimart;
pub mod trimglow;
pub mod ui;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};


/// `create_buffer_init` that does not panic on a lost device: wgpu's maps the
/// buffer at its creation and panics when it cannot, which on a device the
/// driver or the browser has just lost it cannot (the native F3 crash
/// 20261024-001640, an upload in the frame that lost it). Here the buffer is
/// left as the lost device makes it (invalid, drawn nowhere) and the client
/// goes on to make a new device (eqoa's `gpulost`).
pub trait BufferInit {
    fn buffer_init(&self, d: &wgpu::util::BufferInitDescriptor) -> wgpu::Buffer;
}

impl BufferInit for wgpu::Device {
    fn buffer_init(&self, d: &wgpu::util::BufferInitDescriptor) -> wgpu::Buffer {
        if d.contents.is_empty() {
            return self.create_buffer(&wgpu::BufferDescriptor { label: d.label, size: 0, usage: d.usage, mapped_at_creation: false });
        }
        let n = d.contents.len() as u64;
        let mask = wgpu::COPY_BUFFER_ALIGNMENT - 1;
        let size = ((n + mask) & !mask).max(wgpu::COPY_BUFFER_ALIGNMENT);
        let buffer = self.create_buffer(&wgpu::BufferDescriptor { label: d.label, size, usage: d.usage, mapped_at_creation: true });
        // In a browser on WebGPU the mapping is written through the page's
        // own `getMappedRange` (`web_write_mapped`), which says when it has
        // no bytes to give; wgpu's view of it is made by a call that cannot
        // fail and throws instead, through the whole frame (the page's "The
        // client stopped: Uint8Array@[native code]
        // __wbg_new_with_byte_offset_and_length", petition 2097).
        #[cfg(target_arch = "wasm32")]
        if let Some(gpu) = buffer.as_webgpu() {
            // unmapped whatever came of it: a buffer drawn while mapped is a
            // validation error, which wgpu raises as a panic
            web_write_mapped(gpu, d.contents);
            buffer.unmap();
            return buffer;
        }
        let mapped = match buffer.get_mapped_range_mut(..) {
            Ok(mut m) => {
                m.slice(..n as usize).copy_from_slice(d.contents);
                true
            }
            Err(_) => false,
        };
        if mapped {
            buffer.unmap();
        }
        buffer
    }
}

/// A buffer mapped at its creation written with `contents` through the
/// browser's WebGPU itself: whether it was. The
/// mapping's `ArrayBuffer` comes back short, or detached (no bytes at all),
/// when the browser could not give the buffer its memory (its GPU process
/// out of it, the device lost): then nothing is written, and the buffer is
/// left as the browser made it, drawn nowhere, as a lost device's are.
#[cfg(target_arch = "wasm32")]
fn web_write_mapped(gpu: &wgpu::webgpu::GpuBuffer, contents: &[u8]) -> bool {
    let Ok(range) = gpu.get_mapped_range() else { return false };
    if !mapping_holds(range.byte_length() as u64, contents.len() as u64) {
        return false;
    }
    js_sys::Uint8Array::new_with_byte_offset_and_length(&range, 0, contents.len() as u32).copy_from(contents);
    true
}

/// A draw's buffers made a frame (the 2D layer's and the text's vertices),
/// destroyed `RETIRE_AFTER` draws later, when the frames that drew them
/// are long submitted: dropped, a browser's WebGPU keeps each until its
/// garbage collector comes by (wgpu destroys nothing on a drop), 60 to 120
/// a second of a frame's quads.
#[derive(Default)]
pub struct Retire(std::sync::Mutex<std::collections::VecDeque<wgpu::Buffer>>);

/// How many draws a buffer `Retire` keeps outlives: a layer draws two or
/// three times a frame, so its buffer goes ten frames or more after its own.
pub const RETIRE_AFTER: usize = 32;

impl Retire {
    /// `b`, drawn now, kept until `RETIRE_AFTER` draws on; the oldest past
    /// that destroyed.
    pub fn keep(&self, b: wgpu::Buffer) {
        let Ok(mut q) = self.0.lock() else { return };
        q.push_back(b);
        while q.len() > RETIRE_AFTER {
            if let Some(old) = q.pop_front() {
                old.destroy();
            }
        }
    }

    /// How many are kept.
    pub fn len(&self) -> usize {
        self.0.lock().map_or(0, |q| q.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Whether a mapping of `have` bytes takes the `want` written into it (a
/// detached one has none).
pub fn mapping_holds(have: u64, want: u64) -> bool {
    have >= want
}

/// A mapped `buffer`'s bytes, read through the browser's WebGPU itself
/// where it is the page's (`web_write_mapped`'s reasons): None when the
/// mapping has fewer bytes than the buffer; natively wgpu's.
pub fn read_mapped(buffer: &wgpu::Buffer) -> Option<Vec<u8>> {
    #[cfg(target_arch = "wasm32")]
    if let Some(gpu) = buffer.as_webgpu() {
        let range = gpu.get_mapped_range().ok()?;
        if !mapping_holds(range.byte_length() as u64, buffer.size()) {
            return None;
        }
        return Some(js_sys::Uint8Array::new(&range).to_vec());
    }
    buffer.slice(..).get_mapped_range().ok().map(|d| d.to_vec())
}

pub use lit::{Lights, MAX_LIGHTS, PointLight};
pub use wgpu;

/// Which winding is front, in esf's strip-to-list order. Settled by looking.
pub const FRONT_FACE: wgpu::FrontFace = wgpu::FrontFace::Ccw;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Colour is NOT sRGB, anywhere: the GS multiplies, adds and fogs the bytes
/// as they are (modulate is `Ct·Cf/128`) and the TV shows them as they are,
/// so the shader does the same arithmetic on the same numbers. An sRGB
/// target would do it in linear light instead, which lightens every lit or
/// fogged surface.
pub const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Palette size: enough for every character skeleton. A human has 40 nodes;
/// the most on either disc (Frontiers' and 2002's CHAR.ESF) is 68, four tree
/// bodies, the training dummies' among them, whose hands are skinned to nodes
/// 64 and 65. A vertex on a node past the palette was drawn at the model's
/// origin (the shader skips that node but keeps its weight), a thin wedge
/// from the hand down to the feet (`tests/skin_palette.rs`).
pub const MAX_BONES: usize = 72;

/// The GS's level of a texel (`VIEESurfCache::Set`'s TEX1: LCM 0, L 0,
/// MMIN 5, MXL the levels kept less one): LOD = log2(1/|Q|) + K, where 1/Q
/// is the vertex's w, its depth before the camera in world units
/// (`VIRaster::Frustum`'s projection puts z in w, and clip-to-screen keeps
/// it), blended between the two levels round it; at 0 or under, the base
/// magnified. K is `(char)(int)(lodBias * -127.0f)` in sixteenths of a
/// level: the game's default bias 0.8 is -101/16, the base out to about 80
/// units, and a bias past 1.008 wraps round to a positive K, as the char
/// does. (The products of the disc's biases are nowhere near a whole number,
/// so the host's multiply truncates as the EE's does.)
pub fn gs_lod_k(lod_bias: f32) -> f32 {
    ((lod_bias * -127.0) as i32 as i8) as f32 / 16.0
}

/// The bias a layer the game makes in code starts with (`VIMaterial`'s
/// layers: `m_lodBias = 0.8f`): the effect volumes', particles', strips'
/// and the selection rings' materials.
pub const DEFAULT_LOD_BIAS: f32 = 0.8;

/// A K that keeps every texel on the base level (the options' Mipmaps off).
pub const BASE_LEVEL_K: f32 = -16.0;

/// The colour go-eqoa-server sends for a slot dyed White
/// (`database.WhiteDyeWire`, 0xFFFFFEFF; the user, 2026-11-08: "increase the
/// whiteness of the white dye option", "only the people selecting white").
/// A colour multiplies its texture, so white (an undyed slot's tint 0) leaves
/// the armour's own browns and greys as they are: a layer of this colour is
/// drawn bleached instead (`LayerUniform::emissive`'s w, shader.wgsl's
/// `tinted`), and every other colour as ever. A renderer that does not know
/// it draws it as white: a blue of 254 for 255.
pub const WHITE_DYE: [u8; 4] = [0xFF, 0xFF, 0xFE, 0xFF];

/// Whether the textures' mip levels are sampled (Options > Extended > Video
/// > Mipmaps; on, the game's).
static MIPMAPS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn set_mipmaps(on: bool) {
    MIPMAPS.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn mipmaps() -> bool {
    MIPMAPS.load(std::sync::atomic::Ordering::Relaxed)
}

/// K for a pass the game makes in code (`DEFAULT_LOD_BIAS`), or the base
/// level's with the mip levels off.
pub fn effects_lod_k() -> f32 {
    if mipmaps() { gs_lod_k(DEFAULT_LOD_BIAS) } else { BASE_LEVEL_K }
}

/// What stands in for the game's own textures (the options' Texture Packs:
/// `eqoa`'s `texpack`), asked about every surface a library's texture is
/// made from (`LibraryTextures`) while one is set (`set_pack`). With none
/// set, every texture is the surface's own, as ever.
pub trait Pack: Send + Sync {
    /// The levels to draw in place of the surface `s` (its record's bytes
    /// `c.data`, off the disc of `era`), the base first; `None` for its own.
    fn levels(&self, c: &esf::Container, s: &esf::Surface, era: esf::Era) -> Option<Vec<esf::Image>>;
    /// Whether `levels` has a stand-in for the surface `key` names (a
    /// library's `texture_key`), so that its texture is cached apart from
    /// the surface's own.
    fn replaces(&self, key: u64, c: &esf::Container, s: &esf::Surface, era: esf::Era) -> bool;
    /// What tells this pack's stand-ins apart from an earlier pack's: a
    /// replaced texture's key carries it (`LibraryTextures::key`).
    fn generation(&self) -> u64;
    /// A stand-in still being read for the surface (on the pack's own
    /// thread): its size and level count, `(w, h, n)`, as `levels` will
    /// give them, its read asked for under the texture key `key`
    /// (`take_ready` hands its levels back under it); `None` when the pack
    /// has none for the surface, or has its levels already (`levels`).
    /// With `defer`, one the pack has ready is handed back the same way
    /// (`take_ready`), its texture made now as a placeholder: the frame has
    /// made as many as it may at once (`Renderer::make_texture`).
    fn request(&self, _key: u64, _c: &esf::Container, _s: &esf::Surface, _era: esf::Era, _defer: bool) -> Option<(u32, u32, u32)> {
        None
    }
    /// The stand-ins read since the last call, by the key each was asked
    /// for under (`request`).
    fn take_ready(&self) -> standin::Ready {
        Vec::new()
    }
    /// The stand-in for the surface already in the pack's memory in a
    /// compressed form (`packed`), its levels as `levels` would give them:
    /// made a texture of that form where the device has it, never decoded.
    /// `None` when there is none, or it is RGBA (`levels` then).
    fn packed(&self, _c: &esf::Container, _s: &esf::Surface, _era: esf::Era) -> Option<std::sync::Arc<packed::Packed>> {
        None
    }
}

static PACK: std::sync::RwLock<Option<std::sync::Arc<dyn Pack>>> = std::sync::RwLock::new(None);

/// Sets what stands in for the game's textures, or with `None` nothing.
/// What is drawn changes as each texture is next made: the textures made
/// already stay until their models are made again (`eqoa`'s
/// `Play::art_changed` makes them again).
pub fn set_pack(p: Option<std::sync::Arc<dyn Pack>>) {
    *PACK.write().unwrap_or_else(|e| e.into_inner()) = p;
}

/// The pack set, if any.
pub fn pack() -> Option<std::sync::Arc<dyn Pack>> {
    PACK.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// How much graphics memory a texture pack's stand-ins may take, and the
/// largest side one is drawn at (Hoofbite's crashes of 2026-11-17, an Iris
/// Xe out of memory in Freeport, the owner: "texture packs are needing
/// cacheing"): every texture a renderer makes is kept for the run, and a
/// pack's are many times a surface's (a 2048 x 2048 one with its levels is
/// 22 MB, the surface it stands for 0.09). A larger stand-in is drawn halved
/// until it fits (its file's levels past it, `eqoa`'s `texpack`); past the
/// budget the least recently drawn are let go, and where what is drawn
/// fills it a new one is made smaller (`packmem`). Set by the adapter
/// (`set_pack_limits`): a graphics chip sharing the system's memory (an
/// integrated one, a page's) gets the small ones. Each time the device runs
/// out of memory the budget halves (`out_of_memory`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackLimits {
    pub budget: u64,
    pub max_side: u32,
}

impl PackLimits {
    /// The limits for a device of this type: a discrete card's 1 GB and
    /// 2048, anything else's (integrated, virtual, software, a page's)
    /// 256 MB and 1024. `EQOA_PACK_BUDGET_MB` and `EQOA_PACK_MAX_SIDE` set
    /// either.
    pub fn for_device(kind: wgpu::DeviceType) -> PackLimits {
        PackLimits::for_adapter(kind, None)
    }

    /// `for_device`, with the card's own memory where its driver says it
    /// (`device_memory`: Vulkan's largest device-local heap): a discrete
    /// card's budget a third of it, from 1 GB to 4 GB (a 4 GB card 1.3 GB,
    /// an 8 GB one 2.7 GB, 12 GB and more 4 GB). An integrated chip's memory
    /// is the system's, so it keeps 256 MB whatever it says.
    pub fn for_adapter(kind: wgpu::DeviceType, memory: Option<u64>) -> PackLimits {
        let discrete = kind == wgpu::DeviceType::DiscreteGpu && cfg!(not(target_arch = "wasm32"));
        let mut l = if discrete { PackLimits { budget: 1 << 30, max_side: 2048 } } else { PackLimits { budget: 256 << 20, max_side: 1024 } };
        if discrete && let Some(m) = memory {
            l.budget = (m / 3).clamp(1 << 30, 4 << 30);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(mb) = std::env::var("EQOA_PACK_BUDGET_MB").ok().and_then(|v| v.parse::<u64>().ok()) {
                l.budget = mb << 20;
            }
            if let Some(n) = std::env::var("EQOA_PACK_MAX_SIDE").ok().and_then(|v| v.parse::<u32>().ok()).filter(|&n| n >= 1) {
                l.max_side = n;
            }
        }
        l
    }
}

/// The card's own memory where its driver says it: on Vulkan the largest
/// heap local to the device (its VRAM on a discrete card); `None` on any
/// other backend, or where it cannot be asked.
pub fn device_memory(device: &wgpu::Device) -> Option<u64> {
    #[cfg(all(not(target_arch = "wasm32"), not(target_vendor = "apple")))]
    {
        // SAFETY: the device's own Vulkan handles, only read from (a
        // physical device's memory properties), nothing made or destroyed
        let hal = unsafe { device.as_hal::<wgpu::hal::api::Vulkan>() }?;
        let instance = hal.shared_instance().raw_instance();
        let props = unsafe { instance.get_physical_device_memory_properties(hal.raw_physical_device()) };
        // VK_MEMORY_HEAP_DEVICE_LOCAL_BIT
        props.memory_heaps[..props.memory_heap_count as usize].iter().filter(|h| h.flags.as_raw() & 1 != 0).map(|h| h.size).max()
    }
    #[cfg(any(target_arch = "wasm32", target_vendor = "apple"))]
    {
        let _ = device;
        None
    }
}

static PACK_LIMITS: std::sync::Mutex<PackLimits> = std::sync::Mutex::new(PackLimits { budget: 256 << 20, max_side: 1024 });
/// How many times the device has run out of memory this run.
static OUT_OF_MEMORY: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// The options' Texture Pack Memory in MB (`set_pack_memory`): 0 for Auto,
/// the device's (`PackLimits::for_adapter`).
static PACK_MEMORY_MB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Sets the pack limits for the device `Renderer::new` is made on.
pub fn set_pack_limits(l: PackLimits) {
    *PACK_LIMITS.lock().unwrap_or_else(|e| e.into_inner()) = l;
}

/// The options' Texture Pack Memory: the pack's budget `mb` MB whatever the
/// device, or with 0 (Auto) the device's. From the next stand-in made.
pub fn set_pack_memory(mb: u64) {
    PACK_MEMORY_MB.store(mb, std::sync::atomic::Ordering::Relaxed);
}

/// How many game windows share the graphics card (`set_pack_sharers`).
static PACK_SHARERS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// The game windows open on this computer, this one with them (`eqoa`'s
/// `multibox`: each a program of its own, whose textures no other can draw
/// with): Auto's budget is the device's split between them, so four windows
/// do not each take a third of the card (the owner's of 2026-11-18, "if
/// multiple vanilla clients open they should share the same texture cache
/// so not duplicating vram"). A budget the options set is each window's.
pub fn set_pack_sharers(n: u64) {
    PACK_SHARERS.store(n.max(1), std::sync::atomic::Ordering::Relaxed);
}

/// How many game windows share the card now (`set_pack_sharers`).
pub fn pack_sharers() -> u64 {
    PACK_SHARERS.load(std::sync::atomic::Ordering::Relaxed)
}

/// The limits now: the options' budget or the device's (split between the
/// windows open, `set_pack_sharers`), halved for each time the memory ran
/// out.
pub fn pack_limits() -> PackLimits {
    let l = *PACK_LIMITS.lock().unwrap_or_else(|e| e.into_inner());
    let n = OUT_OF_MEMORY.load(std::sync::atomic::Ordering::Relaxed).min(16);
    let budget = match PACK_MEMORY_MB.load(std::sync::atomic::Ordering::Relaxed) {
        0 => (l.budget / PACK_SHARERS.load(std::sync::atomic::Ordering::Relaxed).clamp(1, 16)).max(128 << 20).min(l.budget),
        mb => mb << 20,
    };
    PackLimits { budget: budget >> n, max_side: l.max_side }
}

/// The main renderer's pack memory as it last was (`Renderer::publish_pack_memory`),
/// for the Textures page.
static PACK_MEMORY: std::sync::Mutex<packmem::PackMemory> = std::sync::Mutex::new(packmem::PackMemory { used: 0, budget: 0, textures: 0, reduced: 0, evicted: 0 });

/// What a texture pack's stand-ins take now, as the game's renderer last
/// said (`Renderer::publish_pack_memory`).
pub fn pack_memory() -> packmem::PackMemory {
    *PACK_MEMORY.lock().unwrap_or_else(|e| e.into_inner())
}

/// The device ran out of memory (a texture refused, or `eqoa`'s `gpulost`
/// told of it): the pack's budget halves from now on, this device and the
/// next. How many times so far.
pub fn out_of_memory() -> u32 {
    OUT_OF_MEMORY.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}

/// How many levels a texture's base is past the size its surface gives
/// (`TextureSource::native_size`): log2 of the larger ratio of their sides,
/// 0 for the surface's own size. A texture twice its surface's size each way
/// is sampled a level farther on at every depth (`fs_main`'s level plus it),
/// so a pack's larger texture keeps the game's texel density at a distance
/// and adds detail only near, where the game magnifies its base.
pub fn level_shift(base: (u32, u32), native: Option<(u32, u32)>) -> f32 {
    match native {
        Some((w, h)) if w > 0 && h > 0 && base.0 > 0 && base.1 > 0 => {
            let r = (base.0 as f32 / w as f32).max(base.1 as f32 / h as f32);
            if r == 1.0 { 0.0 } else { r.log2() }
        }
        _ => 0.0,
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    uv: [f32; 2],
    normal: [f32; 3],
    color: [f32; 4],
    /// Skinning: four palette indices (255 unused) and their weights.
    bones: [u8; 4],
    weights: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameUniform {
    /// `View::view_proj` with the camera moved to the origin: the shader
    /// projects positions relative to `eye`.
    view_proj: [[f32; 4]; 4],
    /// The camera, and w = 1 when the textures' mip levels are sampled
    /// (`mipmaps`).
    eye: [f32; 4],
    sun_dir: [f32; 4],
    /// rgb, and w = the time in seconds (for UV scrolling).
    sun: [f32; 4],
    ambient: [f32; 4],
    /// rgb, and w = 1 when fog is on.
    fog_color: [f32; 4],
    /// near, far.
    fog: [f32; 4],
    /// The point light: position and range (0: none), and its colour.
    point: [f32; 4],
    point_color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LayerUniform {
    tint: [f32; 4],
    /// x: 1 textured, y: 1 blended (no alpha test).
    flags: [f32; 4],
    /// (m0, m1, m3, m4) and (m6, m7, rate u, rate v): u' = u·m0 + v·m3 + m6 +
    /// scroll, v' = u·m1 + v·m4 + m7 + scroll.
    uv0: [f32; 4],
    uv1: [f32; 4],
    /// The material's emissive colour, which the light adds to the ambient
    /// (`VIRaster::UploadPerMaterial`); w (its alpha, which nothing reads):
    /// 1 for a layer dyed White (`WHITE_DYE`), drawn bleached.
    emissive: [f32; 4],
}

/// One layer's pass over a batch.
struct Pass {
    blend: usize,
    bg: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    base: LayerUniform,
    /// the DictID bound, and the sampler (0 repeat, 1 clamp, 2 mirror)
    texture: i32,
    sampler: usize,
    /// The UV mapping the pass was made with (`uv0`, `uv1` and the sampler),
    /// which another pass of the batch can be given (`set_layer`).
    made: UvMapping,
    /// The texture's level shift (`level_shift`), in `base`'s K.
    shift: f32,
    /// The least alpha of its texture's texels, every level (255 for none):
    /// Sun Shadows draw a layer whose alpha test can let nothing through as
    /// depth alone (`Pass::solid`).
    alpha_min: u8,
}

impl Pass {
    /// Whether no texel of the layer fails the alpha test (`solid`).
    fn solid(&self) -> bool {
        solid(self.alpha_min, self.base.tint[3])
    }
}

/// Whether a layer whose texture's least alpha is `alpha_min` and whose
/// colour's alpha is `tint_a` lets every texel through fs_main's alpha test
/// (texel × tint ≥ 0.5 at every level and every filtered place between):
/// Sun Shadows draw it as depth alone, with no fragment to run.
pub(crate) fn solid(alpha_min: u8, tint_a: f32) -> bool {
    alpha_min as f32 / 255.0 * tint_a >= 0.5
}

/// A layer's UV mapping: the transform and scroll (`LayerUniform::uv0`,
/// `uv1`) and its wrap's sampler.
type UvMapping = ([f32; 4], [f32; 4], usize);

/// A layer `VIMaterial::SetNumLayers` made: an identity transform, no scroll,
/// wrapping.
const FRESH_UV: UvMapping = ([1.0, 0.0, 0.0, 1.0], [0.0; 4], 0);

/// One model on the GPU, drawn once per instance (a model matrix each).
pub struct GpuModel {
    vertices: wgpu::Buffer,
    /// The skin palette and its bind group; for an unskinned model, all
    /// identity and never written.
    palette: wgpu::Buffer,
    palette_bg: wgpu::BindGroup,
    pub skinned: bool,
    instances: wgpu::Buffer,
    /// Each instance's static light, rgb added and alpha scaling the sun
    /// term; (0, 0, 0, 1), none, unless set.
    statics: wgpu::Buffer,
    instance_count: u32,
    /// Each batch's indices and their count, its layer passes and its LOD
    /// level (`esf::Batch::lod`).
    batches: Vec<(wgpu::Buffer, u32, Vec<Pass>, Option<u8>)>,
    /// Bounds of the model itself, before any instance transform.
    pub bounds: (Vec3, Vec3),
    /// Part of the sky: drawn first, through `View::sky_view_proj`, with no
    /// depth test or write and its layers in order (`VIAtmosphere::RenderSky`;
    /// `SetResources` turns Z off on the sky's materials).
    pub sky: bool,
    /// Skipped by `draw` when false.
    pub visible: bool,
    /// How many of each batch's layer passes are drawn (`VIMaterial`'s
    /// layer count, which the sky's fade changes).
    pub passes_used: usize,
    /// The LOD level drawn of a skinned LOD body (`VILODSprite::Raster` draws
    /// one): batches of any other level are skipped, and with `None` every
    /// level is. Level 0, the near mesh, unless set.
    pub level: Option<u8>,
    /// The levels' distances (`esf::Model::lod_dists`), for whoever sets
    /// `level`; empty for a model with no levels.
    pub lod_dists: Vec<f32>,
    /// Its instances' vertex colours are a zone's baked lighting (colour
    /// buffers): Dynamic Lighting adds no fixed light on it (`lit`).
    pub baked: bool,
    /// Casts into Sun Shadows' and Light Shadows' maps while it is drawn
    /// (`sunshadow`, `lightshadow`); true unless cleared: `eqoa` clears it on
    /// a Wisp's body and everything it wears and holds (`wisp`), whose shadow
    /// would give away the human under its armour.
    pub casts: bool,
    /// The world box its instances cover as last set (`set_instances`), for
    /// Sun Shadows' casters (`sunshadow::world_box`).
    world: std::cell::Cell<Option<(Vec3, Vec3)>>,
    /// The same without a skinned model's room to move: the box its own
    /// bounds make at its instances, which Light Shadows ask whether it holds
    /// a light (`lightshadow`).
    core: std::cell::Cell<Option<(Vec3, Vec3)>>,
    /// A glowing creature's aura shell (`shell`, `Renderer::set_shell`),
    /// drawn after its blended layers; none for every other model.
    pub shell: Option<shell::Shell>,
    /// Its armour trims' auras (`trimglow`, `Renderer::set_trims`), drawn
    /// after its blended layers; none for every other model.
    pub trim: Option<trimglow::Trim>,
    /// The pack's stand-ins it is drawn with, kept while it is and stamped
    /// at each draw (`packmem`): never let go under it.
    holds: Vec<packmem::Hold>,
}

impl GpuModel {
    /// Destroys the model's buffers (its vertices, palette, instances,
    /// static lights, and each batch's indices and layers' uniforms), for a
    /// model let go once the work drawing it is submitted (the map's tiles,
    /// `forget_textures`): their memory is the GPU's again when that work is
    /// done. Dropped alone, a model's buffers wait for the page's garbage
    /// collector in a browser (wgpu's WebGPU backend destroys nothing on a
    /// drop), tens of thousands of them a minute while the map is drawn.
    /// Never for a model still to be drawn: a destroyed buffer in a
    /// submission is a validation error.
    pub fn destroy(&self) {
        for b in [&self.vertices, &self.palette, &self.instances, &self.statics] {
            b.destroy();
        }
        for (indices, _, passes, _) in &self.batches {
            indices.destroy();
            for p in passes {
                p.uniform.destroy();
            }
        }
    }

    /// How many batches (each with its layer passes) the model has.
    pub fn batch_count(&self) -> usize {
        self.batches.len()
    }
}

/// Models uploaded together (`Renderer::upload_group`), a streamed zone's:
/// one vertex, index, instance and static-light buffer for all of them, the
/// materials shared with every other group's that are the same, and each
/// pass's commands recorded once into render bundles, validated then and
/// replayed every frame. What changes afterwards is buffer contents (the
/// props' instances, `set_group_instances`), which a bundle reads as they
/// are at the draw.
///
/// It is what wgpu's per-frame cost is counted in: a model of its own costs
/// a vertex, instance, static-light and palette buffer, an index buffer a
/// batch and a uniform buffer and bind group a layer, each tracked again at
/// every submit (Tunaria round Freeport: 1,134 models, ~14,000 resources).
///
/// The members are recorded in runs of `RUN` (in their order, so the order
/// they are drawn in is the order given) and a run wholly outside the view is
/// skipped (`Run::bounds`).
pub struct GpuGroup {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    instances: wgpu::Buffer,
    statics: wgpu::Buffer,
    members: Vec<Member>,
    runs: Vec<Run>,
    /// Its casters for Sun Shadows (`sunshadow`), recorded while they are on.
    sun: Option<sunshadow::GroupSun>,
    /// Which group it is, and what its members can cover in the world: Light
    /// Shadows' maps keep what the groups within a light's reach cast while
    /// they are the same ones (`lightshadow`).
    id: u64,
    bounds: Option<(Vec3, Vec3)>,
    /// The pack's stand-ins its members are drawn with (`GpuModel::holds`).
    holds: Vec<packmem::Hold>,
}

/// The next group's id (`GpuGroup::id`).
static GROUP_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Members a run of a group.
const RUN: usize = 32;

/// Consecutive members of a group, recorded and drawn as one.
struct Run {
    /// What they can cover in the world: the static ones' transformed bounds,
    /// and round each moving one's instances a sphere of its size and
    /// `MOVING_SLACK` more.
    bounds: Option<(Vec3, Vec3)>,
    /// the first pass's commands and the deferred pass's, if it draws any
    bundles: [Option<wgpu::RenderBundle>; 2],
    /// models with anything in either pass, and each pass's draw calls
    drawn: (u32, [u32; 2]),
    /// The same through Dynamic Lighting's pipelines (`lit`), recorded while
    /// it is on (`Renderer::relight`); none until then.
    lit: Option<[Option<wgpu::RenderBundle>; 2]>,
    /// The same through Sun Shadows' pipelines (`sunshadow`), recorded while
    /// they are on; none until then.
    sun: Option<[Option<wgpu::RenderBundle>; 2]>,
    /// The same through Light Shadows' (`lightshadow`), alone and with the
    /// sun's map (while Sun Shadows are on too), recorded while they are on.
    points: Option<[Option<wgpu::RenderBundle>; 2]>,
    both: Option<[Option<wgpu::RenderBundle>; 2]>,
}

/// One model of a group: its first instance and their count, and each
/// batch's index range in the group's buffer with its layer passes.
struct Member {
    instance0: u32,
    instances: u32,
    batches: Vec<(std::ops::Range<u32>, Vec<Material>)>,
    /// `GroupPart::baked`
    baked: bool,
    /// What it can cover in the world (`part_bounds`), for Sun Shadows'
    /// casters, which draw the members near one another together.
    bounds: Option<(Vec3, Vec3)>,
    /// Its instances move after the upload (`GroupPart::moves`), and the box
    /// they cover as last set (`set_group_instances`); its model's own box;
    /// and, for one that does not move, each instance's box: Light Shadows'
    /// casters, culled instance by instance (`lightshadow`).
    moves: bool,
    posed: std::cell::Cell<Option<(Vec3, Vec3)>>,
    own: Option<(Vec3, Vec3)>,
    boxes: Vec<(Vec3, Vec3)>,
}

/// A layer pass whose bind group (texture, sampler, uniform) every layer
/// with the same three shares.
#[derive(Clone)]
struct Material {
    blend: usize,
    bg: wgpu::BindGroup,
    /// No texel fails its alpha test (`solid`).
    solid: bool,
}

/// A model as `upload_group` takes it: the mesh, a matrix per instance
/// (column-major) and their static lights (fewer: (0, 0, 0, 1) for the rest).
pub struct GroupPart<'a> {
    pub model: &'a esf::Model,
    pub instances: &'a [[[f32; 4]; 4]],
    pub statics: &'a [[f32; 4]],
    /// Its instances move after the upload (`set_group_instances`: a zone's
    /// animated props), so its bounds are taken loosely.
    pub moves: bool,
    /// Its vertex colours are baked lighting (`GpuModel::baked`).
    pub baked: bool,
}

/// How far past its own size a moving part's bounds reach from where its
/// instances were at the upload (a prop's parts swing about their nodes).
const MOVING_SLACK: f32 = 50.0;

/// A model's own box, its vertices' least and most.
fn model_box(m: &esf::Model) -> Option<(Vec3, Vec3)> {
    let (first, rest) = m.pos.split_first()?;
    Some(rest.iter().fold((Vec3::from(*first), Vec3::from(*first)), |(lo, hi), q| (lo.min(Vec3::from(*q)), hi.max(Vec3::from(*q)))))
}

/// The world box the box `b` makes at each of `instances` (column-major).
fn instance_boxes(b: Option<(Vec3, Vec3)>, instances: &[[[f32; 4]; 4]]) -> Vec<(Vec3, Vec3)> {
    let Some(b) = b else { return Vec::new() };
    instances.iter().map(|m| sunshadow::world_box(b, false, std::slice::from_ref(m)).unwrap_or(b)).collect()
}

/// The world-space box a part's instances cover (`GpuGroup::bounds`).
fn part_bounds(p: &GroupPart) -> Option<(Vec3, Vec3)> {
    let (first, rest) = p.model.pos.split_first()?;
    let (lo, hi) = rest.iter().fold((Vec3::from(*first), Vec3::from(*first)), |(lo, hi), q| (lo.min(Vec3::from(*q)), hi.max(Vec3::from(*q))));
    let mut out: Option<(Vec3, Vec3)> = None;
    for m in p.instances {
        let m = Mat4::from_cols_array_2d(m);
        let (a, b) = if p.moves {
            let scale = m.x_axis.truncate().length().max(m.y_axis.truncate().length()).max(m.z_axis.truncate().length());
            let r = scale * lo.abs().max(hi.abs()).length() + MOVING_SLACK;
            let c = m.w_axis.truncate();
            (c - Vec3::splat(r), c + Vec3::splat(r))
        } else {
            (0..8).map(|k| m.transform_point3(Vec3::new([lo.x, hi.x][k & 1], [lo.y, hi.y][k >> 1 & 1], [lo.z, hi.z][k >> 2]))).fold(
                (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
                |(a, b), w| (a.min(w), b.max(w)),
            )
        };
        out = Some(out.map_or((a, b), |(lo, hi)| (lo.min(a), hi.max(b))));
    }
    out
}

/// The planes a box must be inside (left, right, bottom, top, far) for any of
/// it to be drawn through `view_proj`, as (normal, distance) with the inside
/// positive; the near plane is left out (it would save little).
fn frustum(view_proj: Mat4) -> [glam::Vec4; 5] {
    let (r0, r1, r2, r3) = (view_proj.row(0), view_proj.row(1), view_proj.row(2), view_proj.row(3));
    [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r3 - r2]
}

/// Whether the box `(lo, hi)` may show through the planes: outside one of
/// them wholly, it cannot.
fn in_view(planes: &[glam::Vec4; 5], (lo, hi): (Vec3, Vec3)) -> bool {
    planes.iter().all(|p| {
        let far = Vec3::new(if p.x >= 0.0 { hi.x } else { lo.x }, if p.y >= 0.0 { hi.y } else { lo.y }, if p.z >= 0.0 { hi.z } else { lo.z });
        p.truncate().dot(far) + p.w >= 0.0
    })
}

/// What the world is drawn through (`Renderer::pipeline_of`): the PS2's
/// pipelines, Dynamic Lighting's (`lit`), Sun Shadows' (`sunshadow`), or
/// Light Shadows' (`lightshadow`; with the sun's map too when `true`).
#[derive(Clone, Copy)]
enum Shade<'a> {
    Ps2,
    Lit(&'a lit::Lit),
    Sun(&'a sunshadow::Sun),
    Points(&'a lightshadow::Receivers, bool),
}

impl<'a> Shade<'a> {
    /// The group 3 its pipelines take (the lights, and the maps), if any.
    fn group3(self) -> Option<&'a wgpu::BindGroup> {
        match self {
            Shade::Ps2 => None,
            Shade::Lit(l) => Some(&l.bg),
            Shade::Sun(s) => Some(&s.bg),
            Shade::Points(p, _) => Some(&p.bg),
        }
    }
}

/// What `draw` draws, in order.
pub enum Draw<'a> {
    Model(&'a GpuModel),
    Group(&'a GpuGroup),
    /// Some of a model's instances, in their order (a run of the radial
    /// flora's plants, drawn farthest first: `Renderer::upload_flora`).
    Instances(&'a GpuModel, std::ops::Range<u32>),
}

/// A texture source: the decoded image a DictID names, if any, and what the
/// renderer's cache keys it by.
pub trait TextureSource {
    fn image(&self, id: i32) -> Option<esf::Image>;
    /// The image's mip levels, the base first (`esf::Surface::decode_levels`:
    /// those the game keeps, which the GS samples); the base alone for a
    /// source that has no others.
    fn levels(&self, id: i32) -> Vec<esf::Image> {
        self.image(id).into_iter().collect()
    }
    /// What tells this source's image for `id` apart from another image a
    /// source names by the same id: equal keys are one texture. A plain
    /// function's is the id.
    fn key(&self, id: i32) -> u64 {
        id as u32 as u64
    }
    /// The size the game's own texture for `id` has (its surface's), which
    /// `levels` may hand out larger (a pack's: `level_shift`); `None` when
    /// the levels are always the source's own.
    fn native_size(&self, _id: i32) -> Option<(u32, u32)> {
        None
    }
    /// A pack's stand-in for `id` still being read (`Pack::request`, asked
    /// under the texture key `key`): its size and level count, which its
    /// texture is made at at once, the surface's own levels drawn into it
    /// (`standin::placeholder`) until it is read (`Renderer::take_stand_ins`).
    /// `None`: `levels` are the texture's now.
    /// With `defer`, a stand-in the pack has ready is pending too (the
    /// frame's direct uploads are spent: `Pack::request`).
    fn pending(&self, _id: i32, _key: u64, _defer: bool) -> Option<(u32, u32, u32)> {
        None
    }
    /// The game's own levels for `id`, never a pack's: what is drawn in
    /// place of a stand-in past the pack's budget (`PackLimits`) or one the
    /// device had no memory for.
    fn own_levels(&self, id: i32) -> Vec<esf::Image> {
        self.levels(id)
    }
    /// A pack's stand-in for `id` in a compressed form (`Pack::packed`),
    /// made a texture as it is where the device has BCn.
    fn packed(&self, _id: i32) -> Option<std::sync::Arc<packed::Packed>> {
        None
    }
}

impl<F: Fn(i32) -> Option<esf::Image>> TextureSource for F {
    fn image(&self, id: i32) -> Option<esf::Image> {
        self(id)
    }
}

/// What every upload takes its textures from.
pub type Textures<'a> = dyn TextureSource + 'a;

pub struct Renderer {
    /// Indexed by blend mode: 0 opaque, 1 add, 2 subtract, 3 alpha, 4 multiply.
    pipelines: Vec<wgpu::RenderPipeline>,
    /// The same with Z test and write off, for the sky.
    sky_pipelines: Vec<wgpu::RenderPipeline>,
    frame_buf: wgpu::Buffer,
    frame_bg: wgpu::BindGroup,
    sky_frame_buf: wgpu::Buffer,
    sky_frame_bg: wgpu::BindGroup,
    material_layout: wgpu::BindGroupLayout,
    palette_layout: wgpu::BindGroupLayout,
    /// The frame uniform's layout, which Dynamic Lighting's pipelines share.
    frame_layout: wgpu::BindGroupLayout,
    /// Dynamic Lighting's pipelines and lights (`lit`), made when it is first
    /// turned on, and whether it is on.
    lit: Option<lit::Lit>,
    lit_on: bool,
    /// Sun Shadows (`sunshadow`): the map and its pipelines, made when they
    /// are first on with Dynamic Lighting, and whether they are on.
    sun: Option<sunshadow::Sun>,
    sun_on: bool,
    /// Light Shadows (`lightshadow`): the maps and their pipelines, made when
    /// they are first on with Dynamic Lighting, and whether they are on.
    points: Option<lightshadow::Points>,
    points_on: bool,
    /// Which of Dynamic Lighting, Sun Shadows and Light Shadows a page's
    /// WebGL2 refused (`guarded`): not made again while the page lives.
    refused: Vec<&'static str>,
    /// What each refusal said, until the client takes it (`refusals`).
    refusals: Vec<String>,
    /// The options' Shadow Distance, times the game's (1 to 4,
    /// `set_shadow_distance`): the sun's cascades' far edge, and how many
    /// lights cast.
    shadow_scale: f32,
    /// repeat, clamp, mirror.
    samplers: [wgpu::Sampler; 3],
    white: wgpu::TextureView,
    /// A source's key for a DictID (`TextureSource::key`) -> uploaded texture
    /// and its level shift (`level_shift`; `None`: the id resolved to nothing).
    textures: std::collections::HashMap<u64, Option<(wgpu::TextureView, f32)>>,
    /// Each uploaded texture's least texel alpha over its levels, by the same
    /// key (`Pass::alpha_min`).
    texture_alpha: std::collections::HashMap<u64, u8>,
    /// The textures of stand-ins being read, by key, with the size and level
    /// count each was made at (`standin`) and how many levels smaller than
    /// the pack's it is (`packmem::step_down`), and the stand-ins read and
    /// not yet written into them, in order (`take_stand_ins`).
    stand_ins: std::collections::HashMap<u64, (wgpu::Texture, (u32, u32, u32), u32)>,
    stand_ins_ready: std::collections::VecDeque<(u64, std::sync::Arc<Vec<esf::Image>>)>,
    /// What draws a placeholder's levels, made with the first.
    blit: Option<standin::Blit>,
    /// The pack's stand-ins made, their bytes and when each was last drawn,
    /// kept within the budget by what is drawn (`packmem`, `PackLimits`).
    pack: packmem::Budget,
    /// While a model or group is uploaded, the stand-ins it is made with,
    /// for it to hold (`GpuModel::holds`).
    collecting: Option<std::collections::HashMap<u64, packmem::Hold>>,
    /// Whether `ran_out` has said so.
    said_out_of_memory: bool,
    /// Whether the device makes BCn textures (`packed::bc_supported`): a
    /// pack's compressed stand-ins made as they are.
    bc: bool,
    /// The bytes of stand-ins made at once (ready when asked for) this
    /// frame: past `standin::BUDGET` the rest are made as placeholders and
    /// written over later frames, so a zone whose stand-ins were all ready
    /// (prefetched, off the disk cache) does not upload them in one frame.
    /// A compressed one counts a quarter of its bytes (`make_packed`).
    direct: u64,
    /// DictIDs a batch asked for that no loaded file holds.
    pub missing_textures: std::collections::BTreeSet<i32>,
    start: web_time::Instant,
    /// The time the layers' UVs scroll by, in seconds: the wall clock since
    /// the renderer was made, unless set (`shot` sets its script's time, so a
    /// shot is the same picture every run).
    pub scroll_time: Option<f32>,
    format: wgpu::TextureFormat,
    /// Labels (name plates), once a font is set.
    pub text: Option<text::Text>,
    /// The 2D layer (the HUD), drawn over everything.
    pub ui: Option<ui::Ui>,
    /// The particles' pipelines, made at the first particle.
    billboards: Option<billboard::Billboards>,
    /// The immediate-mode fans' (`set_fans`), made at the first fan.
    fans: Option<fans::Fans>,
    /// Whether a fan run marked `ps2` is drawn as the PS2's 16-bit frame
    /// keeps it (`fog16`): only where the adapter can add up its sums
    /// (`set_ps2_sums`, `fog16::supported`); off until told.
    ps2_sums: bool,
    /// The PS2 runs drawn a fan at a time, the GS's arithmetic exactly
    /// (`set_ps2_sequential`): the reference the sums are measured against.
    ps2_sequential: bool,
    /// Whether the world's particles of blend mode 3 are blended as the
    /// PS2's 16-bit frame keeps them (`set_ps2_particles`, `billboard`), as
    /// the fans marked `ps2` are; off until told.
    ps2_particles: bool,
    /// The copy and the sums the PS2's blends are run over (`fog16`), the
    /// fans' and the particles' in turn: one set, made at the first.
    fog16: Option<fog16::Fog16>,
    /// The characters' blob shadows' (`set_shadows`), made at the first.
    shadows: Option<shadow::Shadows>,
    /// What is drawn over the scene with Z off (`set_overlay`), made at the
    /// first.
    overlay: Option<overlay::Overlay>,
    /// Strips drawn after the particles and the shadows, in the order given:
    /// the spell effects' lightning (`VISpellEffectSystem::Process`), then the
    /// weapon trails (`VITrailFxRender`), as `VIScene::Render` draws them after
    /// `VIParticleSystem::Render` and `RenderShadows` and before the weather
    /// (`draw_billboards_and_shadows`, `draw_queued_strips`).
    pub strips: Vec<billboard::Strips>,
    /// This frame's decorations' strips (a server's lights' cords and its
    /// presents): solid, drawn with the world before the name plates
    /// (`draw_decor`).
    pub decor: Vec<billboard::Strips>,
    decor_globes: Vec<billboard::DecorBatch>,
    decor_worn_glass: Vec<billboard::Strips>,
    /// How many of the batches `draw_billboards_and_shadows` is handed are
    /// the particle system's: the shadows and the strips go after them, the
    /// rest (the weather's) after those. Set for each frame.
    pub particles_end: usize,
    /// The last `draw`'s models and draw calls (`EQOA_PERF`).
    pub drawn: std::cell::Cell<(u32, u32)>,
    /// Groups' materials by texture (its key), sampler and uniform bytes.
    materials: std::collections::HashMap<(u64, usize, [u32; 20]), wgpu::BindGroup>,
    /// The palette every group's models draw with: theirs is never set.
    identity_palette: wgpu::BindGroup,
    /// The options' Render Scale (`scale`): 1 until set.
    pub scale: scale::Scale,
    /// The options' Brightness over the scene (`curve`): none until set.
    pub curve: curve::Curve,
    /// The options' Ambient Occlusion (`ao`): off until set.
    pub ao: ao::Ao,
    /// The aura shells' pipeline (`shell`), made at the first shell.
    shell: Option<shell::Pipeline>,
    /// The trims' auras' pipelines (`trimglow`), made at the first trim.
    trim: Option<trimglow::Pipeline>,
}

/// What to draw this frame. The lights are the VU1 strip programs' uniforms:
/// the direction the sun's light TRAVELS, its colour, and ambient.
pub struct View {
    pub view_proj: Mat4,
    /// Where the camera is, in the world: what the models are drawn relative
    /// to (`draw`). The origin is right for a view near it.
    pub eye: Vec3,
    pub sun_dir: Vec3,
    pub sun: Vec3,
    pub ambient: Vec3,
    /// `VIRaster::SetFog`: linear in view depth from `near` (none) to `far`
    /// (all fog colour), as the strip programs' per-vertex fog value and the
    /// GS's FOGCOL mix.
    pub fog: Option<(f32, f32, Vec3)>,
    /// The point light (`VIRaster::SetPointLight` 0, the personal light):
    /// position, range, colour.
    pub point: Option<(Vec3, f32, Vec3)>,
    /// What the frame clears to: the sky colour (under the dome, if drawn).
    pub clear: Vec3,
    /// The view with its translation zeroed, for `GpuModel::sky` models.
    pub sky_view_proj: Option<Mat4>,
    /// How much of the fog's colour the sky is drawn in, whatever its depth
    /// (0 to 1): a storm's (the client's regional weather, `weather_zones`).
    /// The sky's own view puts the dome a few units off, so the distance fog
    /// barely reaches it, and its horizon (hills, sea) showed through a
    /// sandstorm that had turned everything else sand (the owner,
    /// 2026-11-16). 0: the sky as the distance fog leaves it, the game's.
    pub sky_fog: f32,
    /// The fog is a storm's (the client's regional weather): measured per
    /// fragment by the distance from the eye (the shaders' `storm_fog`), the
    /// same whichever way the camera faces. A storm's is a few tens of units,
    /// shorter than a ground triangle, and per vertex it came and went as the
    /// camera turned. false: the game's, per vertex in view depth.
    pub storm_fog: bool,
    /// Dynamic Lighting's point lights (`lit`): with them (and it on), the
    /// models drawn lit per fragment; `None`, the PS2's way.
    pub lights: Option<Lights>,
}

/// Where a storm's sky changes from all fog to its share (`storm_sky_share`):
/// the sine of the elevation, about 20° and 35°. The storm's fog pulls the
/// far plane in (eqoa's `fog::set_storm_far`), so a wall or a cliff past it
/// is not drawn, and the sky behind it showed through where it stood (the
/// owner, 2026-11-17: "i can see the stars through the sandstorm also where
/// the walls would be also"). Below the band the sky is the storm's whole.
pub const STORM_SKY_BAND: (f32, f32) = (0.34, 0.57);

/// How much of a storm's fog colour the sky takes at a direction of sine
/// `elevation` (its y over its length), for the storm's share `s` (0 to 1,
/// `View::sky_fog`): all of it to the band's foot, `s` above its top, a
/// smoothstep between (shader.wgsl's `storm_fog`, fog.w 2, computes the same).
pub fn storm_sky_share(s: f32, elevation: f32) -> f32 {
    let (lo, hi) = STORM_SKY_BAND;
    let t = ((elevation - lo) / (hi - lo)).clamp(0.0, 1.0);
    let t = t * t * (3.0 - 2.0 * t);
    1.0 + (s.clamp(0.0, 1.0) - 1.0) * t
}

/// The sky's fog (near, far) for `View::sky_fog` share `s`: a linear fog over
/// a span so long that the shader's (far - depth) / (far - near) is 1 - s for
/// any depth the dome is drawn at.
pub fn sky_fog_span(s: f32) -> [f32; 4] {
    const SPAN: f32 = 1.0e6;
    let far = (1.0 - s.clamp(0.0, 1.0)) * SPAN;
    [far - SPAN, far, 0.0, 0.0]
}

impl View {
    /// Fixed daylight, no fog: for looking at models on their own.
    pub fn daylight(view_proj: Mat4) -> Self {
        View {
            view_proj,
            eye: Vec3::ZERO,
            sun_dir: Vec3::new(0.35, -0.8, 0.45),
            sun: Vec3::splat(0.6),
            ambient: Vec3::splat(0.45),
            fog: None,
            point: None,
            clear: Vec3::new(0.18, 0.2, 0.24),
            sky_view_proj: None,
            sky_fog: 0.0,
            storm_fog: false,
            lights: None,
        }
    }
}

/// The GS ALPHA register of each blend mode, as a wgpu blend:
/// 1 `Cs·As + Cd`, 2 `Cd − Cs·As`, 3 `(Cs − Cd)·As + Cd`, 4 `Cd·As`.
pub(crate) fn blend_state(mode: usize) -> Option<wgpu::BlendState> {
    use wgpu::{BlendComponent as C, BlendFactor as Fa, BlendOperation as Op};
    let color = match mode {
        1 => C { src_factor: Fa::SrcAlpha, dst_factor: Fa::One, operation: Op::Add },
        2 => C { src_factor: Fa::SrcAlpha, dst_factor: Fa::One, operation: Op::ReverseSubtract },
        3 => C { src_factor: Fa::SrcAlpha, dst_factor: Fa::OneMinusSrcAlpha, operation: Op::Add },
        4 => C { src_factor: Fa::Zero, dst_factor: Fa::SrcAlpha, operation: Op::Add },
        _ => return None,
    };
    Some(wgpu::BlendState { color, alpha: C { src_factor: Fa::Zero, dst_factor: Fa::One, operation: Op::Add } })
}

/// A model layer's pipeline for blend `mode` (`blend_state`; the sky's with
/// no depth test or write), through `module`'s entry points `entries`
/// (vertex, fragment): the PS2's (shader.wgsl's `vs_main`, `fs_main`) and
/// Dynamic Lighting's (`lit`) are made alike, over the same vertices.
pub(crate) fn model_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    entries: (&str, &str),
    mode: usize,
    sky: bool,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let blend = blend_state(mode);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("layer"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(entries.0),
            compilation_options: Default::default(),
            buffers: &[
                Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x3, 3 => Float32x4, 8 => Uint8x4, 9 => Uint8x4],
                }),
                Some(wgpu::VertexBufferLayout {
                    array_stride: 64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4],
                }),
                // the instance's static light (VIRaster::SetStaticLighting)
                Some(wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![10 => Float32x4],
                }),
            ],
        },
        // Strips are lists with the winding made consistent; the
        // strip programs cull back faces (ADC), so this does too.
        primitive: wgpu::PrimitiveState {
            front_face: FRONT_FACE,
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        // Opaque: depth tested and written. Blended: TEST's
        // AFAIL = RGB_ONLY with a NEVER alpha test, i.e. colour
        // only, no depth write.
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(blend.is_none() && !sky),
            depth_compare: Some(if sky { wgpu::CompareFunction::Always } else { wgpu::CompareFunction::LessEqual }),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(entries.1),
            compilation_options: Default::default(),
            // Colour only, opaque or not: nothing reads the
            // target's alpha (no GS ALPHA setting the game uses
            // takes the destination's), and it must stay the
            // clear's 1. A page's WebGL2 canvas is composited
            // with it (wgpu makes the context with alpha), and
            // a picture of it came out with the colour
            // of every alpha-tested edge wrapped round: a cyan
            // fringe where red exceeded alpha.
            targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::COLOR })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        set_pack_limits(PackLimits::for_adapter(device.adapter_info().device_type, device_memory(device)));
        let bc = packed::bc_supported(device);
        packed::set_compressed_textures(bc);
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("layer"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                uniform_entry(2, wgpu::ShaderStages::VERTEX_FRAGMENT),
            ],
        });
        let palette_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("palette"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX)],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("model"),
            bind_group_layouts: &[Some(&frame_layout), Some(&material_layout), Some(&palette_layout)],
            immediate_size: 0,
        });
        let make = |mode: usize, sky: bool| model_pipeline(device, &layout, &shader, ("vs_main", "fs_main"), mode, sky, format);
        let pipelines = (0..5).map(|m| make(m, false)).collect();
        let sky_pipelines = (0..5).map(|m| make(m, true)).collect();
        let frame_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame"),
            size: std::mem::size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame"),
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: frame_buf.as_entire_binding() }],
        });
        let sky_frame_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky frame"),
            size: std::mem::size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sky_frame_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky frame"),
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: sky_frame_buf.as_entire_binding() }],
        });
        // TEX1's filters (`VIEESurfCache::Set`): MMAG linear, MMIN 5
        // (LINEAR_MIPMAP_LINEAR), the level the shader's (`gs_lod_k`)
        let sampler = |mode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: mode,
                address_mode_v: mode,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            })
        };
        let samplers = [
            sampler(wgpu::AddressMode::Repeat),
            sampler(wgpu::AddressMode::ClampToEdge),
            sampler(wgpu::AddressMode::MirrorRepeat),
        ];
        let white = upload_rgba(device, queue, 1, 1, &[255; 4]);
        let identity = device.buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("identity palette"),
            contents: bytemuck::cast_slice(&[IDENTITY; MAX_BONES]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let identity_palette = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("identity palette"),
            layout: &palette_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: identity.as_entire_binding() }],
        });
        Renderer {
            pipelines,
            sky_pipelines,
            frame_buf,
            frame_bg,
            sky_frame_buf,
            sky_frame_bg,
            material_layout,
            palette_layout,
            frame_layout,
            lit: None,
            lit_on: false,
            sun: None,
            sun_on: false,
            points: None,
            points_on: false,
            refused: Vec::new(),
            refusals: Vec::new(),
            shadow_scale: 1.0,
            samplers,
            white,
            textures: Default::default(),
            texture_alpha: Default::default(),
            stand_ins: Default::default(),
            stand_ins_ready: Default::default(),
            blit: None,
            pack: Default::default(),
            collecting: None,
            said_out_of_memory: false,
            bc,
            direct: 0,
            missing_textures: Default::default(),
            start: web_time::Instant::now(),
            scroll_time: None,
            format,
            text: None,
            ui: None,
            billboards: None,
            fans: None,
            ps2_sums: false,
            ps2_sequential: false,
            ps2_particles: false,
            fog16: None,
            shadows: None,
            overlay: None,
            strips: Vec::new(),
            decor: Vec::new(),
            decor_globes: Vec::new(),
            decor_worn_glass: Vec::new(),
            particles_end: 0,
            drawn: Default::default(),
            materials: Default::default(),
            identity_palette,
            scale: scale::Scale::new(format),
            curve: curve::Curve::new(format),
            ao: ao::Ao::new(format),
            shell: None,
            trim: None,
        }
    }

    fn texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, id: i32, src: &Textures) -> Option<wgpu::TextureView> {
        self.texture_shifted(device, queue, id, src).map(|(v, _)| v)
    }

    /// `texture`, with how many levels its base is past its surface's
    /// (`level_shift`: 0 but for a pack's larger texture).
    fn texture_shifted(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, id: i32, src: &Textures) -> Option<(wgpu::TextureView, f32)> {
        if id == 0 {
            return None;
        }
        let key = src.key(id);
        // a stand-in made smaller, or as the game's own, for want of room,
        // that nothing holds now: made again, at its size if there is room
        if self.pack.retry(key, pack_limits().budget) {
            self.let_go(key);
        }
        if let Some(t) = self.textures.get(&key) {
            let t = t.clone();
            self.collect(key);
            return t;
        }
        if self.blit.is_none() && pack().is_some() {
            self.blit = Some(standin::Blit::new(device));
        }
        let made = self.make_texture(device, queue, id, key, src);
        self.textures.insert(key, made.clone());
        self.collect(key);
        made
    }

    /// The stand-in under `key`, if it is one, stamped as asked for this
    /// frame and, while a model or group is uploaded, held by it.
    fn collect(&mut self, key: u64) {
        if let Some(h) = self.pack.touch(key)
            && let Some(c) = self.collecting.as_mut()
        {
            c.entry(key).or_insert(h);
        }
    }

    /// The stand-in `id` names, if it is one, held by `m` (a layer or trim
    /// bound to it after `m` was made).
    fn hold_in(&mut self, m: &mut GpuModel, id: i32, src: &Textures) {
        if let Some(h) = self.pack.touch(src.key(id))
            && !m.holds.iter().any(|x| std::sync::Arc::ptr_eq(x, &h))
        {
            m.holds.push(h);
        }
    }

    /// The stand-ins a model or group being uploaded is made with gathered
    /// from now (`end_holds`), any gathering already going on kept aside.
    fn begin_holds(&mut self) -> Option<std::collections::HashMap<u64, packmem::Hold>> {
        self.collecting.replace(Default::default())
    }

    /// The stand-ins gathered since `begin_holds`, the gathering before it
    /// (`outer`) going on with them too.
    fn end_holds(&mut self, outer: Option<std::collections::HashMap<u64, packmem::Hold>>) -> Vec<packmem::Hold> {
        let got = std::mem::replace(&mut self.collecting, outer).unwrap_or_default();
        if let Some(o) = self.collecting.as_mut() {
            for (k, h) in &got {
                o.entry(*k).or_insert_with(|| h.clone());
            }
        }
        got.into_values().collect()
    }

    /// The texture under `key` let go (a stand-in past the budget, nothing
    /// holding it: `packmem`): out of every cache that keeps it, so that its
    /// memory is the GPU's again once the work already recorded with it is
    /// done. Dropped, never destroyed: anything still bound to it keeps it
    /// valid. Asked for again, it is made again.
    fn let_go(&mut self, key: u64) {
        self.pack.remove(key);
        self.textures.remove(&key);
        self.texture_alpha.remove(&key);
        self.stand_ins.remove(&key);
        self.materials.retain(|k, _| k.0 != key);
        if let Some(b) = self.billboards.as_mut() {
            b.forget_texture(key);
        }
        if let Some(f) = self.fans.as_mut() {
            f.forget_textures();
        }
        if let Some(s) = self.shadows.as_mut() {
            s.forget_texture(key);
        }
    }

    /// Room made in the pack's budget for `need` more bytes: the least
    /// recently drawn stand-ins nothing holds let go (`packmem::Budget::make_room`),
    /// then how many levels smaller each way the new one is made to fit
    /// (`bytes(k)` its bytes so), no more than `most`; `None`: the game's own.
    fn room_for(&mut self, most: u32, bytes: impl Fn(u32) -> u64) -> Option<u32> {
        let budget = pack_limits().budget;
        for key in self.pack.make_room(bytes(0), budget) {
            self.let_go(key);
        }
        self.pack.choose_step(budget, most, bytes)
    }

    /// The texture `texture_shifted` keeps under `key`: a pack's stand-in
    /// within the pack's budget (`PackLimits`; made now at its size and
    /// written once read, `standin`), room made for it by letting go of the
    /// least recently drawn (`packmem`), made a level smaller each way at a
    /// time where what is held fills it, the surface's own only past its own
    /// size; nothing for an id of no levels. A texture the device has no
    /// memory for is the surface's own instead, or nothing (`made_safely`),
    /// never the panic wgpu raises by default.
    fn make_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, id: i32, key: u64, src: &Textures) -> Option<(wgpu::TextureView, f32)> {
        // a pack's stand-in still being read: its texture made at its size
        // now, the surface's own drawn into it, written over once read
        // (`standin`); asked first, so that a stand-in read meanwhile is
        // still written in
        let pending = src.pending(id, key, self.direct >= standin::BUDGET as u64);
        // a compressed stand-in ready (`packed`): made as it is, its blocks
        // written as they are (no decode, no copy: counted a quarter of its
        // bytes against the frame's)
        if pending.is_none()
            && self.bc
            && let Some(p) = src.packed(id).filter(|p| p.format.compressed())
            && let Some(made) = self.make_packed(device, queue, key, &p, src.native_size(id))
        {
            return Some(made);
        }
        let mut levels = src.levels(id);
        if levels.is_empty() {
            self.missing_textures.insert(id);
            return None;
        }
        let native = src.native_size(id);
        // how many levels a stand-in may be made smaller: to its surface's size
        let most = |base: (u32, u32)| level_shift(base, native).max(0.0).floor() as u32;
        // the game's own drawn for want of room: the stand-in's bytes
        let mut reduced = None;
        if let Some(size) = pending.filter(|_| self.blit.is_some()) {
            if let Some(k) = self.room_for(most((size.0, size.1)), |k| packmem::bytes(packmem::stepped(size, k))) {
                let made_at = packmem::stepped(size, k);
                let blit = self.blit.as_ref().expect("made above");
                let made = made_safely(device, || standin::placeholder(device, queue, blit, &levels, made_at));
                if let Some((tex, view)) = made {
                    self.pack.insert(key, packmem::bytes(made_at), packmem::bytes(size));
                    // its texels unknown until read: never drawn as one no
                    // texel of which fails its alpha test (Sun Shadows)
                    self.texture_alpha.insert(key, 0);
                    self.stand_ins.insert(key, (tex, made_at, k));
                    return Some((view, level_shift((made_at.0, made_at.1), native)));
                }
                self.ran_out("a texture pack's stand-in");
            }
            // past even its own size, or refused: the surface's own (the
            // stand-in, read, is let go: `take_stand_ins` finds no texture
            // for it), tried again once nothing holds it
            levels = src.own_levels(id);
            reduced = Some(packmem::bytes(size));
        } else if level_shift((levels[0].width, levels[0].height), native) > 0.0 {
            // a stand-in read already (larger than its surface)
            let size = (levels[0].width, levels[0].height, levels.len() as u32);
            match self.room_for(most((size.0, size.1)), |k| packmem::bytes(packmem::stepped(size, k))) {
                Some(k) => {
                    levels = packmem::step_down(levels, k);
                    let made = packmem::bytes((levels[0].width, levels[0].height, levels.len() as u32));
                    self.pack.insert(key, made, packmem::bytes(size));
                    self.direct += made;
                }
                None => {
                    levels = src.own_levels(id);
                    reduced = Some(packmem::bytes(size));
                }
            }
        }
        if let Some(want) = reduced {
            // counted as nothing, kept to be tried again (`packmem::Budget::retry`)
            self.pack.insert(key, 0, want);
        }
        if levels.is_empty() {
            self.missing_textures.insert(id);
            return None;
        }
        self.texture_alpha.insert(key, levels.iter().flat_map(|l| l.rgba.chunks_exact(4).map(|p| p[3])).min().unwrap_or(255));
        let shift = level_shift((levels[0].width, levels[0].height), native);
        if let Some(view) = made_safely(device, || upload_levels(device, queue, &levels)) {
            return Some((view, shift));
        }
        self.ran_out("a texture");
        // the surface's own, if that was a stand-in: a far smaller one
        if shift > 0.0 {
            let own = src.own_levels(id);
            if !own.is_empty()
                && let Some(view) = made_safely(device, || upload_levels(device, queue, &own))
            {
                let want = levels.iter().map(|l| u64::from(l.width) * u64::from(l.height) * 4).sum();
                self.pack.insert(key, 0, want);
                self.texture_alpha.insert(key, own.iter().flat_map(|l| l.rgba.chunks_exact(4).map(|p| p[3])).min().unwrap_or(255));
                return Some((view, 0.0));
            }
        }
        self.pack.remove(key);
        self.texture_alpha.remove(&key);
        None
    }

    /// A pack's stand-in in its compressed form (`packed`) made a texture of
    /// that form: room made for it as for any (`room_for`), made a level
    /// smaller each way at a time by dropping its first levels where what is
    /// held fills the budget, no smaller than its surface and no further than
    /// its blocks allow (`Packed::droppable`). `None` past that, or refused:
    /// `make_texture` goes on as for an RGBA one (decoded, then smaller still
    /// or the game's own).
    fn make_packed(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, key: u64, p: &packed::Packed, native: Option<(u32, u32)>) -> Option<(wgpu::TextureView, f32)> {
        let (w, h, _) = p.size();
        let most = (level_shift((w, h), native).max(0.0).floor() as u32).min(p.droppable());
        let k = self.room_for(most, |k| p.bytes_without(k))?;
        let q = p.without(k);
        let Some(view) = made_safely(device, || packed::upload(device, queue, &q)) else {
            self.ran_out("a texture pack's compressed stand-in");
            return None;
        };
        self.pack.insert(key, q.bytes(), p.bytes());
        self.direct += q.bytes() / 4;
        self.texture_alpha.insert(key, p.alpha_min);
        let (qw, qh, _) = q.size();
        Some((view, level_shift((qw, qh), native)))
    }

    /// The device refused `what` for want of memory: the pack's budget
    /// halved (`out_of_memory`), said once a renderer.
    fn ran_out(&mut self, what: &str) {
        let n = out_of_memory();
        // what this renderer has made counts against the smaller budget
        if !self.said_out_of_memory {
            self.said_out_of_memory = true;
            eprintln!("graphics: out of memory making {what} ({n} times this run): a texture pack's budget now {} MB, its textures past it drawn as the game's own", pack_limits().budget >> 20);
        }
    }

    /// The stand-ins a pack has read since (`Pack::take_ready`) written over
    /// their textures' placeholders (`standin`), as many as `standin::BUDGET`
    /// bytes allow (one at least), the rest left for the next frame: called
    /// once a frame, before it is drawn. How many were written.
    pub fn take_stand_ins(&mut self, queue: &wgpu::Queue) -> usize {
        self.direct = 0;
        if let Some(p) = pack() {
            self.stand_ins_ready.extend(p.take_ready());
            // an earlier pack's stand-ins nothing holds any more let go
            for key in self.pack.stale(p.generation()) {
                self.let_go(key);
            }
        }
        let sizes: Vec<usize> = self.stand_ins_ready.iter().map(|(_, l)| standin::bytes(l)).collect();
        let n = standin::due(&sizes, standin::BUDGET);
        for (key, levels) in self.stand_ins_ready.drain(..n) {
            let Some((tex, size, k)) = self.stand_ins.remove(&key) else { continue };
            // made smaller for want of room (`make_texture`): its levels so
            let smaller;
            let levels = if k > 0 {
                smaller = packmem::step_down(levels.as_ref().clone(), k);
                &smaller
            } else {
                levels.as_ref()
            };
            if standin::fits(levels, size) {
                standin::write(queue, &tex, levels);
            }
        }
        self.publish_pack_memory();
        n
    }

    /// Stand-ins read and not yet written (`take_stand_ins`).
    pub fn stand_ins_waiting(&self) -> usize {
        self.stand_ins_ready.len()
    }

    /// The least texel alpha of the texture `id` names in `src` (255 for none
    /// or one not uploaded).
    fn alpha_min(&self, id: i32, src: &Textures) -> u8 {
        if id == 0 { 255 } else { self.texture_alpha.get(&src.key(id)).copied().unwrap_or(255) }
    }

    /// Uploads a model, drawn once per matrix in `instances` (column-major),
    /// with its textures out of `textures` (cached by DictID).
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        m: &esf::Model,
        instances: &[[[f32; 4]; 4]],
        textures: &Textures,
    ) -> GpuModel {
        self.upload_with(device, queue, m, instances, textures, false)
    }

    /// Uploads a radial flora plant (a flora sprite's model) with room for
    /// `capacity` instances, lit as `VIRStrip6` lights it: no normal and no
    /// vertex colour, the ambient plus the sun and the point light's falloff
    /// each times 0.15, the falloff's distance in the plant's own space; the
    /// vertex alpha is the plant's fade, an instance's static-light alpha
    /// (`set_static_lights`). Draw its instances with `Draw::Instances`.
    pub fn upload_flora(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, m: &esf::Model, capacity: usize, textures: &Textures) -> GpuModel {
        self.upload_with(device, queue, m, &vec![IDENTITY; capacity.max(1)], textures, true)
    }

    fn upload_with(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        m: &esf::Model,
        instances: &[[[f32; 4]; 4]],
        textures: &Textures,
        flora: bool,
    ) -> GpuModel {
        let outer = self.begin_holds();
        let verts = vertices_of(m);
        let vertices = device.buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let identity = [IDENTITY; MAX_BONES];
        let palette = device.buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("palette"),
            contents: bytemuck::cast_slice(&identity),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let palette_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("palette"),
            layout: &self.palette_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: palette.as_entire_binding() }],
        });
        let mut batches = Vec::new();
        for b in &m.batches {
            if b.indices.is_empty() {
                continue;
            }
            let mut passes = Vec::new();
            for l in layers_of(b) {
                let (view, shift) = self.texture_shifted(device, queue, l.texture, textures).unzip();
                let (blend, mut base, si) = layer_state(l, view.is_some());
                base.flags[2] += shift.unwrap_or(0.0);
                base.flags[3] = flora as i32 as f32;
                let uniform = device.buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("layer"),
                    contents: bytemuck::bytes_of(&base),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
                let sampler = &self.samplers[si];
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("layer"),
                    layout: &self.material_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view.as_ref().unwrap_or(&self.white)),
                        },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                        wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
                    ],
                });
                let alpha_min = if view.is_some() { self.alpha_min(l.texture, textures) } else { 255 };
                passes.push(Pass { blend, bg, uniform, base, texture: l.texture, sampler: si, made: (base.uv0, base.uv1, si), shift: shift.unwrap_or(0.0), alpha_min });
            }
            let indices = device.buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("indices"),
                contents: bytemuck::cast_slice(&b.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            batches.push((indices, b.indices.len() as u32, passes, b.lod));
        }
        let lo = m.pos.iter().fold(Vec3::splat(f32::MAX), |a, p| a.min(Vec3::from(*p)));
        let hi = m.pos.iter().fold(Vec3::splat(f32::MIN), |a, p| a.max(Vec3::from(*p)));
        let instance_buf = device.buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("instances"),
            contents: bytemuck::cast_slice(instances),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let statics = device.buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("static lights"),
            contents: bytemuck::cast_slice(&vec![[0.0f32, 0.0, 0.0, 1.0]; instances.len().max(1)]),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let holds = self.end_holds(outer);
        GpuModel {
            vertices,
            palette,
            palette_bg,
            skinned: m.skinned,
            instances: instance_buf,
            statics,
            instance_count: instances.len() as u32,
            batches,
            bounds: (lo, hi),
            sky: false,
            visible: true,
            passes_used: usize::MAX,
            level: Some(0),
            lod_dists: m.lod_dists.clone(),
            baked: false,
            casts: true,
            world: std::cell::Cell::new(sunshadow::world_box((lo, hi), m.skinned, instances)),
            core: std::cell::Cell::new(sunshadow::world_box((lo, hi), false, instances)),
            shell: None,
            trim: None,
            holds,
        }
    }

    /// Lets go of every texture uploaded so far, and destroys them: for a
    /// renderer that draws one thing after another and keeps nothing from
    /// one to the next (the map's tiles), its models drawn with them already
    /// submitted and let go too (`GpuModel::destroy`). A destroyed texture's
    /// memory is the GPU's again once the work submitted with it is done; one
    /// only dropped waits for the page's garbage collector in a browser.
    pub fn forget_textures(&mut self) {
        for (view, _) in self.textures.values().flatten() {
            view.texture().destroy();
        }
        self.textures.clear();
        self.pack.clear();
        self.missing_textures.clear();
        self.materials.clear();
    }

    /// The textures this renderer keeps, as `(textures, bytes, of them a
    /// pack's larger than their surfaces: count, bytes)`: each at 4 bytes a
    /// texel over its levels (`TEXTURE_FORMAT`).
    pub fn texture_memory(&self) -> (usize, u64, usize, u64) {
        let (mut n, mut bytes, mut pn, mut pbytes) = (0, 0, 0, 0);
        for (view, shift) in self.textures.values().flatten() {
            let b = texture_bytes(view.texture());
            n += 1;
            bytes += b;
            if *shift > 0.0 {
                pn += 1;
                pbytes += b;
            }
        }
        (n, bytes, pn, pbytes)
    }

    /// What the pack's stand-ins take now against the budget (`packmem`).
    pub fn pack_memory(&self) -> packmem::PackMemory {
        self.pack.memory(pack_limits().budget)
    }

    /// `pack_memory` made what `render::pack_memory` says (for the Textures
    /// page): the game's renderer's, each frame (`take_stand_ins`).
    pub fn publish_pack_memory(&self) {
        *PACK_MEMORY.lock().unwrap_or_else(|e| e.into_inner()) = self.pack_memory();
    }

    /// The kept buffers of the particles' and strips' slots, in bytes
    /// (`billboard`).
    pub fn billboard_memory(&self) -> u64 {
        self.billboards.as_ref().map_or(0, |b| b.kept_bytes())
    }

    /// What the texture under `key` (`TextureSource::key`) was made as: None
    /// never asked for, `Some(false)` asked for and nothing, `Some(true)` a
    /// texture.
    pub fn texture_made(&self, key: u64) -> Option<bool> {
        self.textures.get(&key).map(Option::is_some)
    }

    /// Re-points one layer pass of one batch, as `VIAtmosphere` changes the
    /// sky's materials: its texture (rebound when it changes), blend mode and
    /// colour (0..1, alpha included).
    #[allow(clippy::too_many_arguments)]
    /// A layer pass as a material has it this frame: its texture, blend mode
    /// and colour (`rgba`), and its UV mapping, the one pass `uv` of the
    /// batch was made with (`VIMaterial::CopyLayer` carries a layer's
    /// transform, wrap and scroll with it), or with `None` a fresh layer's
    /// (`SetNumLayers`: identity, no scroll). The scroll's phase is the
    /// mapping's own, rate × time, as a copied offset keeps it.
    #[allow(clippy::too_many_arguments)]
    pub fn set_layer(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, m: &mut GpuModel, batch: usize, pass: usize, texture: i32, blend: usize, rgba: [f32; 4], uv: Option<usize>, textures: &Textures) {
        let Some(ps) = m.batches.get(batch).map(|b| &b.2) else { return };
        let Some(p) = ps.get(pass) else { return };
        let (uv0, uv1, si) = uv.and_then(|u| ps.get(u)).map_or(FRESH_UV, |src| src.made);
        let rebind = p.texture != texture || p.sampler != si;
        let (view, shift) = if rebind { self.texture_shifted(device, queue, texture, textures).unzip() } else { (None, None) };
        let alpha_min = if view.is_some() { self.alpha_min(texture, textures) } else { 255 };
        if rebind && view.is_some() {
            self.hold_in(m, texture, textures);
        }
        let Some(p) = m.batches.get_mut(batch).and_then(|b| b.2.get_mut(pass)) else { return };
        if rebind {
            let shift = shift.unwrap_or(0.0);
            p.base.flags[2] += shift - p.shift;
            p.shift = shift;
            p.bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("layer"),
                layout: &self.material_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view.as_ref().unwrap_or(&self.white)) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.samplers[si]) },
                    wgpu::BindGroupEntry { binding: 2, resource: p.uniform.as_entire_binding() },
                ],
            });
            p.texture = texture;
            p.sampler = si;
            p.base.flags[0] = view.is_some() as i32 as f32;
            p.alpha_min = alpha_min;
        }
        p.blend = blend.min(4);
        p.base.flags[1] = (p.blend != 0) as i32 as f32;
        p.base.tint = rgba;
        (p.base.uv0, p.base.uv1) = (uv0, uv1);
        queue.write_buffer(&p.uniform, 0, bytemuck::bytes_of(&p.base));
    }

    /// Sets every layer's colour to `rgb` (0..1), keeping each layer's own
    /// alpha (`VIAtmosphere::SetSkyColor`).
    pub fn set_layer_rgb(&self, queue: &wgpu::Queue, m: &GpuModel, rgb: Vec3) {
        for (_, _, passes, _) in &m.batches {
            for p in passes {
                let mut u = p.base;
                u.tint = [rgb.x, rgb.y, rgb.z, p.base.tint[3]];
                queue.write_buffer(&p.uniform, 0, bytemuck::bytes_of(&u));
            }
        }
    }

    /// The font labels are drawn in.
    pub fn set_font(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, font: &esf::font::Font) {
        self.text = Some(text::Text::new(device, queue, self.format, DEPTH_FORMAT, font));
    }

    /// The name plates (`text.plates`), into the scene at their anchors'
    /// depths, depth-tested against what `draw` drew and not writing it, as
    /// the font's material draws them (`VIRFont`: Z test on, alpha blended,
    /// so its TEST keeps colour only). `VIScene::Render` draws them after
    /// the world and before the particles, the shadows, the spell lightning,
    /// the trails and the weather, which blend over them: call it after
    /// `draw_fans` and before `draw_billboards`.
    pub fn draw_plates(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, size: (u32, u32)) {
        let Some(t) = self.text.as_ref().filter(|t| !t.plates.is_empty()) else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("plates"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        t.draw_plates(device, &mut pass, size);
    }

    /// The labels, over what `draw` drew (depth-tested against it, not
    /// writing it), then the overlay (`set_overlay`), then the 2D layer.
    /// `size` is the target's, in pixels.
    pub fn draw_text(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, size: (u32, u32)) {
        self.text_pass(device, encoder, target, depth, size, true, true);
    }

    /// `draw_text` in two passes with `between` drawn between them: the
    /// labels and the overlay, then `between`'s (a piece of the HUD another
    /// layer draws, the minimap: over the names, under the HUD and its
    /// windows), then the 2D layer's quads.
    pub fn draw_text_around(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, size: (u32, u32), between: impl FnOnce(&mut wgpu::CommandEncoder)) {
        self.text_pass(device, encoder, target, depth, size, true, false);
        between(encoder);
        self.text_pass(device, encoder, target, depth, size, false, true);
    }

    /// The text pass: the labels and the overlay (`over`), the 2D layer's
    /// quads (`hud`).
    #[allow(clippy::too_many_arguments)]
    fn text_pass(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, size: (u32, u32), over: bool, hud: bool) {
        let labels = self.text.as_ref().filter(|t| over && !t.labels.is_empty());
        let overlay = self.overlay.as_ref().filter(|o| over && !o.is_empty());
        let quads = self.ui.as_ref().filter(|u| hud && !u.quads.is_empty());
        if labels.is_none() && overlay.is_none() && quads.is_none() {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("text"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Some(t) = labels {
            t.draw(device, &mut pass, size);
        }
        if let Some(o) = overlay {
            o.draw(&mut pass);
        }
        if let Some(u) = quads {
            u.draw(device, &mut pass, size);
        }
    }

    /// Gives `m` (uploaded from `model`) an aura shell in `color` over the
    /// whole of it (`shell`: its mesh again, pushed out along its normals,
    /// drawn additively as a rim after its blended layers), or with `None`
    /// takes its shell away.
    pub fn set_shell(&mut self, device: &wgpu::Device, m: &mut GpuModel, model: &esf::Model, color: Option<[u8; 3]>) {
        let parts: Vec<shell::PartOf> = color.map(|color| shell::PartOf { color, batches: None, dark: false }).into_iter().collect();
        self.set_shells(device, m, model, &parts);
    }

    /// Gives `m` (uploaded from `model`) an aura shell in `parts`, each over
    /// some of its batches in a colour of its own (a worn piece's), or with
    /// none takes its shell away. The pipeline is made at the first shell; a
    /// page whose WebGL2 refuses it draws none.
    pub fn set_shells(&mut self, device: &wgpu::Device, m: &mut GpuModel, model: &esf::Model, parts: &[shell::PartOf]) {
        if parts.is_empty() {
            m.shell = None;
            return;
        }
        if self.shell.is_none() && !self.refused.contains(&SHELL) {
            let (frame, palette, format) = (&self.frame_layout, &self.palette_layout, self.format);
            self.shell = guarded(device, &mut self.refused, &mut self.refusals, SHELL, || Some(shell::Pipeline::new(device, frame, palette, format)));
        }
        m.shell = self.shell.as_ref().and_then(|p| p.shell(device, model, m.bounds, parts));
    }

    /// Gives `m` (uploaded from `model`) its armour trims' auras in `trims`
    /// (`trimglow`: each the trim of some of its batches lit in a colour, or
    /// darkened), their masks out of `textures`; with none (or none whose
    /// mask resolves) takes them away. The pipelines are made at the first
    /// trim; a page whose WebGL2 refuses them draws none.
    pub fn set_trims(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, m: &mut GpuModel, model: &esf::Model, trims: &[trimglow::TrimOf], textures: &Textures) {
        m.trim = None;
        if trims.iter().all(|t| t.batches.is_empty()) {
            return;
        }
        if self.trim.is_none() && !self.refused.contains(&TRIM) {
            let (frame, material, palette, format) = (&self.frame_layout, &self.material_layout, &self.palette_layout, self.format);
            self.trim = guarded(device, &mut self.refused, &mut self.refusals, TRIM, || Some(trimglow::Pipeline::new(device, frame, material, palette, format)));
        }
        let Some(p) = self.trim.take() else { return };
        let views: std::collections::HashMap<i32, Option<wgpu::TextureView>> = trimglow::by_mask(trims).into_iter().map(|(_, id, _)| (id, self.texture(device, queue, id, textures))).collect();
        for &id in views.keys() {
            self.hold_in(m, id, textures);
        }
        m.trim = p.trim(device, &self.material_layout, &self.samplers[0], model, m.bounds, trims, |id| views.get(&id).cloned().flatten());
        self.trim = Some(p);
    }

    /// The skin palette for a skinned model: one matrix per skeleton node
    /// (`VIHSprite::SetBlendMatrices`), as column-major arrays.
    pub fn set_palette(&self, queue: &wgpu::Queue, m: &GpuModel, palette: &[[[f32; 4]; 4]]) {
        let n = palette.len().min(MAX_BONES);
        queue.write_buffer(&m.palette, 0, bytemuck::cast_slice(&palette[..n]));
    }

    /// Sets a model's instances' static lights (as many as it has).
    pub fn set_static_lights(&self, queue: &wgpu::Queue, m: &GpuModel, lights: &[[f32; 4]]) {
        let n = lights.len().min(m.instance_count as usize);
        queue.write_buffer(&m.statics, 0, bytemuck::cast_slice(&lights[..n]));
    }

    /// Moves a model's instances (as many as it was uploaded with).
    pub fn set_instances(&self, queue: &wgpu::Queue, m: &GpuModel, instances: &[[[f32; 4]; 4]]) {
        let n = instances.len().min(m.instance_count as usize);
        queue.write_buffer(&m.instances, 0, bytemuck::cast_slice(&instances[..n]));
        m.world.set(sunshadow::world_box(m.bounds, m.skinned, &instances[..n]));
        m.core.set(sunshadow::world_box(m.bounds, false, &instances[..n]));
    }

    pub fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        view: &View,
        items: &[Draw],
    ) {
        self.draw_in(queue, encoder, target, depth, view, items, None);
    }

    /// `draw` into the rectangle `rect` of the target (x, y, width, height in
    /// its pixels), as `VIRaster::Viewport` puts a window's 3D into part of
    /// the screen: what the target holds is kept (no clear), depth is cleared,
    /// the view's projection fills the rectangle and nothing is drawn outside it.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_viewport(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, view: &View, items: &[Draw], rect: [u32; 4]) {
        self.draw_in(queue, encoder, target, depth, view, items, Some(rect));
    }

    /// The 2D layer's quads in `range` alone, over what the target holds
    /// (a window's quads under its 3D, then over it: `ui::Ui::split`).
    pub fn draw_ui(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, size: (u32, u32), range: std::ops::Range<usize>) {
        let Some(u) = self.ui.as_ref().filter(|u| !u.quads.is_empty()) else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui part"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        u.draw_range(device, &mut pass, size, range);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_in(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, view: &View, items: &[Draw], rect: Option<[u32; 4]>) {
        // a new frame for the pack's budget, and every stand-in the models
        // and groups drawn hold stamped as drawn in it (`packmem`)
        self.pack.next_frame();
        let frame = self.pack.frame();
        for item in items {
            let holds = match item {
                Draw::Model(m) | Draw::Instances(m, _) => &m.holds,
                Draw::Group(g) => &g.holds,
            };
            for h in holds {
                h.store(frame, std::sync::atomic::Ordering::Relaxed);
            }
        }
        // The view-projection relative to the eye, in doubles: `view_proj`'s
        // translation is the projection of the eye's world position, which
        // the eye's own translation here cancels (shader.wgsl, vs_main).
        let rel = view.view_proj.as_dmat4() * glam::DMat4::from_translation(view.eye.as_dvec3());
        let uniform = FrameUniform {
            view_proj: rel.as_mat4().to_cols_array_2d(),
            eye: view.eye.extend(mipmaps() as i32 as f32).to_array(),
            sun_dir: view.sun_dir.normalize().extend(0.0).to_array(),
            sun: view.sun.extend(self.scroll_time.unwrap_or_else(|| self.start.elapsed().as_secs_f32())).to_array(),
            ambient: view.ambient.extend(0.0).to_array(),
            fog_color: match view.fog {
                Some((_, _, c)) => c.extend(1.0).to_array(),
                None => [0.0; 4],
            },
            fog: match view.fog {
                Some((n, f, _)) => [n, f.max(n + 1e-3), 0.0, view.storm_fog as i32 as f32],
                None => [0.0, 1.0, 0.0, 0.0],
            },
            point: match view.point {
                Some((p, r, _)) => p.extend(r).to_array(),
                None => [0.0; 4],
            },
            point_color: match view.point {
                Some((_, _, c)) => c.extend(1.0).to_array(),
                None => [0.0; 4],
            },
        };
        queue.write_buffer(&self.frame_buf, 0, bytemuck::bytes_of(&uniform));
        if let Some(vp) = view.sky_view_proj {
            // the sky's view has no translation: its eye is the origin
            let mut sky = FrameUniform { view_proj: vp.to_cols_array_2d(), eye: [0.0, 0.0, 0.0, uniform.eye[3]], ..uniform };
            // the dome is fogged by its share below, never by its distance
            sky.fog[3] = 0.0;
            // a storm's share of the fog's colour by elevation (fog.w 2,
            // `storm_sky_share`): the whole of it low, where what the storm's
            // far plane left out stood, its share overhead; without a storm,
            // a share at every depth (the linear fog over a span so long the
            // dome's few units are nothing in it)
            if let (Some((_, _, c)), s) = (view.fog, view.sky_fog.clamp(0.0, 1.0)) {
                if view.storm_fog {
                    sky.fog = [0.0, 1.0, s, 2.0];
                    sky.fog_color = c.extend(1.0).to_array();
                } else if s > 0.0 {
                    sky.fog = sky_fog_span(s);
                    sky.fog_color = c.extend(1.0).to_array();
                }
            }
            queue.write_buffer(&self.sky_frame_buf, 0, bytemuck::bytes_of(&sky));
        }
        // Sun Shadows: the frame's casters into their map before the world,
        // and the world drawn through their pipelines while the sun casts
        // (`sunshadow`)
        let sun = match (view.lights.as_ref(), self.sun_drawn()) {
            (Some(_), Some((_, s))) if s.draw(self, queue, encoder, view, items) => Some(s),
            _ => None,
        };
        // Light Shadows: the frame's lights' maps, and the world drawn
        // through their receivers (with the sun's map too while Sun Shadows
        // are on, whose sun term is the strip programs' alone in a frame the
        // sun casts nothing: by night)
        let points = match (view.lights.as_ref(), self.points_drawn()) {
            (Some(_), Some(p)) => {
                p.draw(self, queue, encoder, view, items, self.scroll_time.unwrap_or_else(|| self.start.elapsed().as_secs_f32()));
                if sun.is_none()
                    && let Some((_, s)) = self.sun_drawn()
                {
                    s.unshadowed(queue);
                }
                Some(p)
            }
            _ => None,
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("models"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: match rect {
                        Some(_) => wgpu::LoadOp::Load,
                        None => wgpu::LoadOp::Clear(wgpu::Color { r: view.clear.x as f64, g: view.clear.y as f64, b: view.clear.z as f64, a: 1.0 }),
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Some([x, y, w, h]) = rect {
            pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
            pass.set_scissor_rect(x, y, w, h);
        }
        // The sky: its own view, no depth, every layer in order.
        if view.sky_view_proj.is_some() {
            pass.set_bind_group(0, &self.sky_frame_bg, &[]);
            let sky = items.iter().filter_map(|i| match i {
                Draw::Model(m) => Some(*m),
                Draw::Group(_) | Draw::Instances(..) => None,
            });
            for m in sky.filter(|m| m.sky && m.visible && m.instance_count > 0) {
                pass.set_vertex_buffer(0, m.vertices.slice(..));
                pass.set_vertex_buffer(1, m.instances.slice(..));
                pass.set_vertex_buffer(2, m.statics.slice(..));
                pass.set_bind_group(2, &m.palette_bg, &[]);
                for (indices, count, passes, _) in &m.batches {
                    pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                    for p in passes.iter().take(m.passes_used) {
                        pass.set_pipeline(&self.sky_pipelines[p.blend]);
                        pass.set_bind_group(1, &p.bg, &[]);
                        pass.draw_indexed(0..*count, 0, 0..m.instance_count);
                    }
                }
            }
        }
        // Dynamic Lighting: this view's lights, its models through the lit
        // pipelines and its groups' runs through their lit bundles (`lit`)
        let lit = match (view.lights.as_ref(), self.lit.as_ref()) {
            (Some(l), Some(lit)) if self.lit_on => {
                queue.write_buffer(&lit.buf, 0, bytemuck::bytes_of(&lit::LightsUniform::new(view.point, l, view.eye, view.view_proj)));
                Some(lit)
            }
            _ => None,
        };
        let shade = match (lit, sun, points) {
            (None, _, _) => Shade::Ps2,
            (Some(_), _, Some(p)) => match (self.sun_drawn(), p.with_sun.as_ref()) {
                (Some(_), Some(w)) => Shade::Points(w, true),
                _ => Shade::Points(&p.receivers, false),
            },
            (Some(_), Some(s), None) => Shade::Sun(s),
            (Some(l), None, None) => Shade::Lit(l),
        };
        let planes = frustum(view.view_proj);
        let shown = |r: &&Run| r.bounds.is_some_and(|b| in_view(&planes, b));
        let (mut drawn_models, mut draws) = (0u32, 0u32);
        for item in items {
            match item {
                Draw::Model(m) => drawn_models += Self::draws_anything(m) as u32,
                Draw::Instances(m, r) => drawn_models += (Self::draws_anything(m) && !r.is_empty()) as u32,
                Draw::Group(g) => drawn_models += g.runs.iter().filter(shown).map(|r| r.drawn.0).sum::<u32>(),
            }
        }
        // Two passes, as VIScene::Render's: a material whose layer 0 is
        // opaque (MaterialHasDepthSort) draws all its layers at once in the
        // first, one whose layer 0 blends is deferred whole to the second
        // (DrawPrimBuffer's flags; RenderDeferredList). The Ambient Occlusion
        // goes between them, over what the first wrote, in passes of its own
        // (`ao`: the frame `prepare` readied it for, never part of one).
        let ao = rect.is_none() && self.ao.take(depth);
        for deferred in [false, true] {
            if deferred && ao {
                drop(pass);
                self.ao.run(queue, encoder, target, rel, view.fog);
                pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("models, blended"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: depth,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            }
            // the pass's state, unset again by each bundle it executes
            let mut bound = false;
            let mut current = usize::MAX;
            for item in items {
                match item {
                    Draw::Model(m) => {
                        if !Self::draws_anything(m) {
                            continue;
                        }
                        if !bound {
                            pass.set_bind_group(0, &self.frame_bg, &[]);
                            if let Some(bg) = shade.group3() {
                                pass.set_bind_group(3, bg, &[]);
                            }
                            bound = true;
                            current = usize::MAX;
                        }
                        draws += self.encode_model(&mut pass, m, deferred, &mut current, 0..m.instance_count, shade);
                        // its aura shell after its blended layers (`shell`)
                        if deferred && let Some(n) = self.encode_shell(&mut pass, m) {
                            draws += n;
                            current = usize::MAX;
                            if let Some(bg) = shade.group3() {
                                pass.set_bind_group(3, bg, &[]);
                            }
                        }
                        // and its trims' auras (`trimglow`)
                        if deferred && let Some(n) = self.encode_trim(&mut pass, m) {
                            draws += n;
                            current = usize::MAX;
                            if let Some(bg) = shade.group3() {
                                pass.set_bind_group(3, bg, &[]);
                            }
                        }
                    }
                    Draw::Instances(m, r) => {
                        if !Self::draws_anything(m) || r.is_empty() {
                            continue;
                        }
                        if !bound {
                            pass.set_bind_group(0, &self.frame_bg, &[]);
                            if let Some(bg) = shade.group3() {
                                pass.set_bind_group(3, bg, &[]);
                            }
                            bound = true;
                            current = usize::MAX;
                        }
                        draws += self.encode_model(&mut pass, m, deferred, &mut current, r.start..r.end.min(m.instance_count), shade);
                    }
                    Draw::Group(g) => {
                        for r in g.runs.iter().filter(shown) {
                            let earlier = match (lit, sun, &r.sun, &r.lit) {
                                (Some(_), Some(_), Some(b), _) => b,
                                (Some(_), _, _, Some(b)) => b,
                                _ => &r.bundles,
                            };
                            // Light Shadows' where they are recorded (`relight`)
                            let bundles = match shade {
                                Shade::Points(_, true) => r.both.as_ref().unwrap_or(earlier),
                                Shade::Points(_, false) => r.points.as_ref().unwrap_or(earlier),
                                _ => earlier,
                            };
                            if let Some(b) = &bundles[deferred as usize] {
                                pass.execute_bundles([b]);
                                bound = false;
                            }
                            draws += r.drawn.1[deferred as usize];
                        }
                    }
                }
            }
        }
        self.drawn.set((drawn_models, draws));
    }

    /// A model's aura shell (`shell`), if it has one and a pipeline was
    /// made: its draw calls (one a part), or `None` when nothing was drawn.
    fn encode_shell<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, m: &'a GpuModel) -> Option<u32> {
        let (Some(s), Some(p)) = (m.shell.as_ref(), self.shell.as_ref()) else { return None };
        let mut draws = 0;
        for part in &s.parts {
            let Some((_, indices, count)) = part.run(m.level) else { continue };
            pass.set_pipeline(if part.dark { &p.dark } else { &p.pipeline });
            if draws == 0 {
                pass.set_bind_group(2, &m.palette_bg, &[]);
                pass.set_vertex_buffer(0, m.vertices.slice(..));
                pass.set_vertex_buffer(1, m.instances.slice(..));
                pass.set_vertex_buffer(2, s.normals.slice(..));
            }
            pass.set_bind_group(1, &part.bg, &[]);
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..*count, 0, 0..m.instance_count);
            draws += 1;
        }
        (draws > 0).then_some(draws)
    }

    /// A model's trims' auras (`trimglow`), if it has any and the pipelines
    /// were made: its draw calls (one a part), or `None` when nothing was
    /// drawn.
    fn encode_trim<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, m: &'a GpuModel) -> Option<u32> {
        let (Some(t), Some(p)) = (m.trim.as_ref(), self.trim.as_ref()) else { return None };
        let mut draws = 0;
        for part in &t.parts {
            let Some((_, indices, count)) = part.run(m.level) else { continue };
            pass.set_pipeline(if part.dark { &p.dark } else { &p.glow });
            if draws == 0 {
                pass.set_bind_group(2, &m.palette_bg, &[]);
                pass.set_vertex_buffer(0, m.vertices.slice(..));
                pass.set_vertex_buffer(1, m.instances.slice(..));
                pass.set_vertex_buffer(2, t.normals.slice(..));
            }
            pass.set_bind_group(1, &part.bg, &[]);
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..*count, 0, 0..m.instance_count);
            draws += 1;
        }
        (draws > 0).then_some(draws)
    }

    /// Whether `draw` draws `m` at all (the sky draws apart).
    fn draws_anything(m: &GpuModel) -> bool {
        m.instance_count > 0 && !m.sky && m.visible
    }

    /// One model's commands for one of the two passes, into a pass or a
    /// bundle; `current` is the pipeline set (`pipeline_of`'s key), through
    /// `shade`'s pipelines. Returns the draw calls.
    fn encode_model<'a, E: wgpu::util::RenderEncoder<'a>>(&'a self, e: &mut E, m: &'a GpuModel, deferred: bool, current: &mut usize, range: std::ops::Range<u32>, shade: Shade<'a>) -> u32 {
        let in_pass = |passes: &[Pass], lod: Option<u8>| passes.first().is_some_and(|p| (p.blend != 0) == deferred) && level_drawn(lod, m.level);
        if !m.batches.iter().any(|(_, _, passes, lod)| in_pass(passes, *lod)) {
            return 0;
        }
        e.set_vertex_buffer(0, Some(m.vertices.slice(..)));
        e.set_vertex_buffer(1, Some(m.instances.slice(..)));
        e.set_vertex_buffer(2, Some(m.statics.slice(..)));
        e.set_bind_group(2, Some(&m.palette_bg), &[]);
        let mut draws = 0;
        for (indices, count, passes, lod) in &m.batches {
            if !in_pass(passes, *lod) {
                continue;
            }
            e.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            for p in passes.iter().take(m.passes_used) {
                let (key, pipeline) = self.pipeline_of(p.blend, m.baked, shade);
                if key != *current {
                    e.set_pipeline(pipeline);
                    *current = key;
                }
                e.set_bind_group(1, Some(&p.bg), &[]);
                e.draw_indexed(0..*count, 0, range.clone());
                draws += 1;
            }
        }
        draws
    }

    /// The shared bind group of a layer (cached by texture, sampler and
    /// uniform), and its blend mode.
    fn material(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, l: &esf::Layer, textures: &Textures) -> Material {
        let (view, shift) = self.texture_shifted(device, queue, l.texture, textures).unzip();
        let (blend, mut base, si) = layer_state(l, view.is_some());
        base.flags[2] += shift.unwrap_or(0.0);
        let solid = solid(if view.is_some() { self.alpha_min(l.texture, textures) } else { 255 }, base.tint[3]);
        let key = (textures.key(l.texture), si, bytemuck::cast(base));
        if let Some(bg) = self.materials.get(&key) {
            return Material { blend, bg: bg.clone(), solid };
        }
        let uniform = device.buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("material"),
            contents: bytemuck::bytes_of(&base),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material"),
            layout: &self.material_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view.as_ref().unwrap_or(&self.white)) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.samplers[si]) },
                wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
            ],
        });
        self.materials.insert(key, bg.clone());
        Material { blend, bg, solid }
    }

    /// Uploads `parts` as one [`GpuGroup`], drawn as `draw` would draw them
    /// one by one as models, in this order.
    pub fn upload_group(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, parts: &[GroupPart], textures: &Textures) -> GpuGroup {
        let outer = self.begin_holds();
        // The buffers are sized first and each part written as it is built: a
        // whole zone's vertices at once (and the copies made of them on the
        // way to the GPU) would stay in a page's wasm heap, which never shrinks.
        let (mut nverts, mut nindices, mut ninstances) = (0, 0, 0);
        for p in parts {
            nverts += p.model.pos.len();
            nindices += p.model.batches.iter().filter(|b| group_drawn(b)).map(|b| b.indices.len()).sum::<usize>();
            ninstances += p.instances.len();
        }
        // a buffer of no bytes cannot be made; an empty group draws nothing
        let buffer = |label: &str, bytes: usize, usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: bytes.max(16) as u64,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let vertices = buffer("group vertices", nverts * size_of::<Vertex>(), wgpu::BufferUsages::VERTEX);
        let indices = buffer("group indices", nindices * 4, wgpu::BufferUsages::INDEX);
        let instances = buffer("group instances", ninstances * 64, wgpu::BufferUsages::VERTEX);
        let statics = buffer("group static lights", ninstances * 16, wgpu::BufferUsages::VERTEX);
        let mut members = Vec::new();
        let (mut base, mut first_index, mut instance0) = (0u32, 0u32, 0u32);
        for p in parts {
            let verts = vertices_of(p.model);
            let mut part_indices: Vec<u32> = Vec::new();
            let mut batches = Vec::new();
            for b in p.model.batches.iter().filter(|b| !b.indices.is_empty() && group_drawn(b)) {
                let first = first_index + part_indices.len() as u32;
                // rebased here: WebGL2 has no base vertex
                part_indices.extend(b.indices.iter().map(|&i| i + base));
                let passes = layers_of(b).iter().map(|l| self.material(device, queue, l, textures)).collect();
                batches.push((first..first_index + part_indices.len() as u32, passes));
            }
            let lights: Vec<[f32; 4]> = (0..p.instances.len()).map(|k| p.statics.get(k).copied().unwrap_or([0.0, 0.0, 0.0, 1.0])).collect();
            let write = |to: &wgpu::Buffer, at: u32, stride: usize, bytes: &[u8]| {
                if !bytes.is_empty() {
                    queue.write_buffer(to, at as u64 * stride as u64, bytes);
                }
            };
            write(&vertices, base, size_of::<Vertex>(), bytemuck::cast_slice(&verts));
            write(&indices, first_index, 4, bytemuck::cast_slice(&part_indices));
            write(&instances, instance0, 64, bytemuck::cast_slice(p.instances));
            write(&statics, instance0, 16, bytemuck::cast_slice(&lights));
            let own = model_box(p.model);
            members.push(Member {
                instance0,
                instances: p.instances.len() as u32,
                batches,
                baked: p.baked,
                bounds: part_bounds(p),
                moves: p.moves,
                posed: std::cell::Cell::new(if p.moves { own.and_then(|b| sunshadow::world_box(b, false, p.instances)) } else { None }),
                own,
                boxes: if p.moves { Vec::new() } else { instance_boxes(own, p.instances) },
            });
            base += verts.len() as u32;
            first_index += part_indices.len() as u32;
            instance0 += p.instances.len() as u32;
        }
        let bounds = members.iter().filter_map(|m| m.bounds).reduce(|(a, b), (c, d)| (a.min(c), b.max(d)));
        let id = GROUP_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let holds = self.end_holds(outer);
        let mut g = GpuGroup { vertices, indices, instances, statics, members, runs: Vec::new(), sun: None, id, bounds, holds };
        for at in (0..parts.len()).step_by(RUN) {
            let span = at..(at + RUN).min(parts.len());
            let bounds = parts[span.clone()].iter().filter_map(part_bounds).reduce(|(a, b), (c, d)| (a.min(c), b.max(d)));
            let drawn = g.members[span.clone()].iter().filter(|m| m.instances > 0 && !m.batches.is_empty()).count() as u32;
            let (bundles, n) = self.record(device, &g, span.clone(), Shade::Ps2);
            // and through Dynamic Lighting's pipelines while it is on
            let lit = self.lit.as_ref().filter(|_| self.lit_on).map(|l| self.record(device, &g, span.clone(), Shade::Lit(l)).0);
            // and Sun Shadows' while they are on
            let sun = self.sun_drawn().map(|(_, s)| self.record(device, &g, span.clone(), Shade::Sun(s)).0);
            // and Light Shadows' while they are on, with the sun's map while
            // Sun Shadows are too
            let [points, both] = self.points_shades().map(|sh| sh.map(|sh| self.record(device, &g, span.clone(), sh).0));
            g.runs.push(Run { bounds, bundles, drawn: (drawn, n), lit, sun, points, both });
        }
        // and its casters
        g.sun = self.sun_drawn().map(|(_, s)| s.casters(self, device, &g));
        g
    }

    /// The bundles of a group's members `span` for the two passes (none for
    /// a pass they draw nothing in), and each pass's draw calls, through
    /// `shade`'s pipelines.
    fn record(&self, device: &wgpu::Device, g: &GpuGroup, span: std::ops::Range<usize>, shade: Shade) -> ([Option<wgpu::RenderBundle>; 2], [u32; 2]) {
        let (mut bundles, mut drawn) = ([None, None], [0, 0]);
        for deferred in [false, true] {
            let mut e = device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
                label: Some("group"),
                color_formats: &[Some(self.format)],
                depth_stencil: Some(wgpu::RenderBundleDepthStencil { format: DEPTH_FORMAT, depth_read_only: false, stencil_read_only: true }),
                sample_count: 1,
                multiview: None,
            });
            let n = self.encode_run(&mut e, g, &g.members[span.clone()], deferred, shade);
            if n > 0 {
                bundles[deferred as usize] = Some(e.finish(&wgpu::RenderBundleDescriptor { label: Some("group") }));
            }
            drawn[deferred as usize] = n;
        }
        (bundles, drawn)
    }

    /// Turns Dynamic Lighting on or off (`lit`): on, its pipelines made the
    /// first time, a view's lights drawn, and a group uploaded from now on
    /// recorded for it as well; the groups uploaded before need `relight`.
    pub fn set_dynamic_lighting(&mut self, device: &wgpu::Device, on: bool) {
        if on && self.lit.is_none() && !self.refused.contains(&LIT) {
            self.lit = guarded(device, &mut self.refused, &mut self.refusals, LIT, || Some(lit::Lit::new(device, [&self.frame_layout, &self.material_layout, &self.palette_layout], self.format)));
        }
        self.lit_on = on;
        self.make_sun(device);
        self.make_points(device);
    }

    /// Turns Sun Shadows on or off (`sunshadow`): drawn only while Dynamic
    /// Lighting is on too, their map and pipelines made the first time both
    /// are, and a group uploaded from then on recorded for them; the groups
    /// uploaded before need `relight`.
    pub fn set_sun_shadows(&mut self, device: &wgpu::Device, on: bool) {
        self.sun_on = on;
        self.make_sun(device);
        self.make_points(device);
    }

    /// Turns Light Shadows on or off (`lightshadow`): drawn only while
    /// Dynamic Lighting is on too, their maps and pipelines made the first
    /// time both are, and a group uploaded from then on recorded for them;
    /// the groups uploaded before need `relight`.
    pub fn set_light_shadows(&mut self, device: &wgpu::Device, on: bool) {
        self.points_on = on;
        self.make_points(device);
    }

    /// The options' Shadow Distance, `scale` times the game's (1 to 4, as
    /// built at 1): Sun Shadows' far edge (`sunshadow::ends`) and how many
    /// lights Light Shadows shadow (`lightshadow::slots_at`), from the next
    /// frame.
    pub fn set_shadow_distance(&mut self, scale: f32) {
        self.shadow_scale = scale.clamp(1.0, 4.0);
    }

    /// Whether Light Shadows are on (`set_light_shadows`; drawn only with
    /// Dynamic Lighting on).
    pub fn light_shadows(&self) -> bool {
        self.points_on
    }

    /// What the last frame drew into Light Shadows' maps; `None` before they
    /// were first on.
    pub fn light_shadow_drawn(&self) -> Option<lightshadow::Drawn> {
        self.points.as_ref().map(|p| p.drawn.get())
    }

    /// What a page's WebGL2 refused of Dynamic Lighting, Sun Shadows and
    /// Light Shadows since the last call, each "<name>: <what it said>"
    /// (`guarded`): the client's to tell the player, the option left as
    /// they set it and the feature not drawn.
    pub fn refusals(&mut self) -> Vec<String> {
        std::mem::take(&mut self.refusals)
    }

    /// The comparisons Light Shadows' receivers take (`Filter::for_device`);
    /// `None` before they were first made.
    pub fn light_shadow_filter(&self) -> Option<lightshadow::Filter> {
        self.points.as_ref().map(lightshadow::Points::filter)
    }

    /// The maps and their pipelines, the first time Light Shadows and
    /// Dynamic Lighting are both on; and their receivers with the sun's map
    /// once Sun Shadows' are made.
    fn make_points(&mut self, device: &wgpu::Device) {
        let layouts = [&self.frame_layout, &self.material_layout, &self.palette_layout];
        if self.points_on
            && self.points.is_none()
            && !self.refused.contains(&POINTS)
            && let Some(l) = self.lit.as_ref().filter(|_| self.lit_on)
        {
            self.points = guarded(device, &mut self.refused, &mut self.refusals, POINTS, || lightshadow::Points::new(device, layouts, l, self.sun.as_ref(), self.format));
        }
        if let (Some(p), Some(l), Some(s)) = (self.points.as_mut(), self.lit.as_ref(), self.sun.as_ref())
            && p.with_sun.is_none()
            && guarded(device, &mut self.refused, &mut self.refusals, POINTS, || {
                p.attach_sun(device, layouts, l, s, self.format);
                Some(())
            })
            .is_none()
        {
            // refused with the sun's map: none at all, not maps whose
            // receivers would leave the sun's shadow out
            self.points = None;
        }
    }

    /// Light Shadows' while they and Dynamic Lighting are on.
    fn points_drawn(&self) -> Option<&lightshadow::Points> {
        self.lit.as_ref().filter(|_| self.lit_on)?;
        self.points.as_ref().filter(|_| self.points_on)
    }

    /// The shades a group records for Light Shadows while they are on: alone
    /// while Sun Shadows are off, with the sun's map while they are on.
    fn points_shades(&self) -> [Option<Shade<'_>>; 2] {
        let Some(p) = self.points_drawn() else { return [None, None] };
        match (self.sun_drawn(), p.with_sun.as_ref()) {
            (Some(_), Some(w)) => [None, Some(Shade::Points(w, true))],
            _ => [Some(Shade::Points(&p.receivers, false)), None],
        }
    }

    /// Whether Sun Shadows are on (`set_sun_shadows`; drawn only with
    /// Dynamic Lighting on).
    pub fn sun_shadows(&self) -> bool {
        self.sun_on
    }

    /// What the last frame drew into Sun Shadows' cascades, all four: the
    /// casters' bundles executed and the draw calls; `None` before they were
    /// first on.
    pub fn sun_shadow_drawn(&self) -> Option<(u32, u32)> {
        self.sun.as_ref().map(|s| s.drawn.get())
    }

    /// The sun's map and pipelines, the first time Sun Shadows and Dynamic
    /// Lighting are both on.
    fn make_sun(&mut self, device: &wgpu::Device) {
        if self.sun_on
            && self.sun.is_none()
            && !self.refused.contains(&SUN)
            && let Some(l) = self.lit.as_ref().filter(|_| self.lit_on)
        {
            self.sun = guarded(device, &mut self.refused, &mut self.refusals, SUN, || sunshadow::Sun::new(device, [&self.frame_layout, &self.material_layout, &self.palette_layout], l, self.format));
        }
    }

    /// Dynamic Lighting's and Sun Shadows' while both are on.
    fn sun_drawn(&self) -> Option<(&lit::Lit, &sunshadow::Sun)> {
        Some((self.lit.as_ref().filter(|_| self.lit_on)?, self.sun.as_ref().filter(|_| self.sun_on)?))
    }

    /// Whether Dynamic Lighting is on (`set_dynamic_lighting`).
    pub fn dynamic_lighting(&self) -> bool {
        self.lit_on
    }

    /// Records a group's lit bundles where it has none, while Dynamic
    /// Lighting is on (a group uploaded while it was off draws the PS2's way
    /// until then).
    pub fn relight(&self, device: &wgpu::Device, g: &mut GpuGroup) {
        let Some(l) = self.lit.as_ref().filter(|_| self.lit_on) else { return };
        let sun = self.sun.as_ref().filter(|_| self.sun_on);
        let [points, both] = self.points_shades();
        for k in 0..g.runs.len() {
            let span = k * RUN..((k + 1) * RUN).min(g.members.len());
            if g.runs[k].lit.is_none() {
                let (bundles, _) = self.record(device, g, span.clone(), Shade::Lit(l));
                g.runs[k].lit = Some(bundles);
            }
            // and Sun Shadows' while they are on
            if let Some(s) = sun.filter(|_| g.runs[k].sun.is_none()) {
                g.runs[k].sun = Some(self.record(device, g, span.clone(), Shade::Sun(s)).0);
            }
            // and Light Shadows' while they are on
            if let Some(sh) = points.filter(|_| g.runs[k].points.is_none()) {
                g.runs[k].points = Some(self.record(device, g, span.clone(), sh).0);
            }
            if let Some(sh) = both.filter(|_| g.runs[k].both.is_none()) {
                g.runs[k].both = Some(self.record(device, g, span, sh).0);
            }
        }
        if let Some(s) = sun.filter(|_| g.sun.is_none()) {
            g.sun = Some(s.casters(self, device, g));
        }
    }

    /// The pipeline a layer pass of blend `blend` is drawn through, and its
    /// key for a run of passes (a pipeline set once while the key stays):
    /// the PS2's, or Dynamic Lighting's, a baked instance's own, Sun Shadows'
    /// or Light Shadows' (`shade`).
    fn pipeline_of<'a>(&'a self, blend: usize, baked: bool, shade: Shade<'a>) -> (usize, &'a wgpu::RenderPipeline) {
        match shade {
            Shade::Ps2 => (blend, &self.pipelines[blend]),
            Shade::Sun(s) => (32 + blend + 8 * baked as usize, &s.pipelines[baked as usize][blend]),
            Shade::Lit(l) => (8 + blend + 8 * baked as usize, &l.pipelines[baked as usize][blend]),
            Shade::Points(p, sun) => (64 + 32 * sun as usize + blend + 8 * baked as usize, &p.pipelines[baked as usize][blend]),
        }
    }

    /// The commands of some of a group's members for one of the two passes,
    /// as `encode_model` issues a model's, through `shade`'s pipelines.
    /// Returns the draw calls.
    fn encode_run<'a>(&'a self, e: &mut wgpu::RenderBundleEncoder<'a>, g: &'a GpuGroup, members: &'a [Member], deferred: bool, shade: Shade<'a>) -> u32 {
        let in_pass = |passes: &[Material]| passes.first().is_some_and(|p| (p.blend != 0) == deferred);
        e.set_bind_group(0, &self.frame_bg, &[]);
        e.set_bind_group(2, &self.identity_palette, &[]);
        if let Some(bg) = shade.group3() {
            e.set_bind_group(3, bg, &[]);
        }
        e.set_vertex_buffer(0, g.vertices.slice(..));
        e.set_index_buffer(g.indices.slice(..), wgpu::IndexFormat::Uint32);
        let mut current = usize::MAX;
        let mut draws = 0;
        for m in members.iter().filter(|m| m.instances > 0) {
            if !m.batches.iter().any(|(_, passes)| in_pass(passes)) {
                continue;
            }
            let (i0, n) = (m.instance0 as u64, m.instances as u64);
            e.set_vertex_buffer(1, g.instances.slice(i0 * 64..(i0 + n) * 64));
            e.set_vertex_buffer(2, g.statics.slice(i0 * 16..(i0 + n) * 16));
            for (range, passes) in &m.batches {
                if !in_pass(passes) {
                    continue;
                }
                for p in passes {
                    let (key, pipeline) = self.pipeline_of(p.blend, m.baked, shade);
                    if key != current {
                        e.set_pipeline(pipeline);
                        current = key;
                    }
                    e.set_bind_group(1, &p.bg, &[]);
                    e.draw_indexed(range.clone(), 0, 0..m.instances);
                    draws += 1;
                }
            }
        }
        draws
    }

    /// Moves the instances of a group's `member`-th model (as many as it was
    /// uploaded with).
    pub fn set_group_instances(&self, queue: &wgpu::Queue, g: &GpuGroup, member: usize, instances: &[[[f32; 4]; 4]]) {
        let Some(m) = g.members.get(member) else { return };
        let n = instances.len().min(m.instances as usize);
        queue.write_buffer(&g.instances, m.instance0 as u64 * 64, bytemuck::cast_slice(&instances[..n]));
        // where it is now, for Light Shadows' casters (an animated prop)
        if m.moves {
            m.posed.set(m.own.and_then(|b| sunshadow::world_box(b, false, &instances[..n])));
        }
    }
}

/// Whether a batch of LOD level `lod` (`esf::Batch::lod`) is drawn while its
/// model's level is `level` (`GpuModel::level`): a batch in no level always,
/// one in a level only while that level is the model's.
fn level_drawn(lod: Option<u8>, level: Option<u8>) -> bool {
    lod.is_none() || lod == level
}

/// Whether a group draws a batch: a group's members are never moved closer or
/// further, so of a skinned LOD body only the near level, 0.
fn group_drawn(b: &esf::Batch) -> bool {
    b.lod.is_none_or(|l| l == 0)
}

/// The vertices of a model as the shader takes them.
fn vertices_of(m: &esf::Model) -> Vec<Vertex> {
    (0..m.pos.len())
        .map(|i| Vertex {
            pos: m.pos[i],
            uv: m.uv[i],
            normal: m.normal[i],
            color: m.color[i],
            bones: m.bones.get(i).copied().unwrap_or([255; 4]),
            weights: m.weights.get(i).copied().unwrap_or([0; 4]),
        })
        .collect()
}

/// A material with no layer at all draws once, untextured.
const UNTEXTURED: [esf::Layer; 1] = [esf::Layer {
    flags: 1,
    texture: 0,
    wrap: 0,
    blend: 0,
    color: [255; 4],
    uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    lod_bias: 0.8,
    uv_rate: [0.0, 0.0],
    emissive: [0; 4],
}];

fn layers_of(b: &esf::Batch) -> &[esf::Layer] {
    if b.layers.is_empty() { &UNTEXTURED } else { &b.layers }
}

/// A layer's blend mode, uniform and sampler (0 repeat, 1 clamp, 2 mirror),
/// `textured` if its texture resolved.
fn layer_state(l: &esf::Layer, textured: bool) -> (usize, LayerUniform, usize) {
    let blend = (l.blend.clamp(0, 4)) as usize;
    let k = l.uv_coeffs();
    // The layer colour reaches the GS as c/255 × 128 and modulates by /128:
    // a tint of c/255.
    let tint = l.color.map(|c| c as f32 / 255.0);
    let base = LayerUniform {
        tint,
        flags: [textured as i32 as f32, (blend != 0) as i32 as f32, gs_lod_k(l.lod_bias), 0.0],
        uv0: [k[0], k[1], k[2], k[3]],
        uv1: [k[4], k[5], l.uv_rate[0], l.uv_rate[1]],
        // as UploadPerMaterial converts it: c/255; w a White dye's flag
        emissive: {
            let e = l.emissive.map(|c| c as f32 / 255.0);
            [e[0], e[1], e[2], (l.color == WHITE_DYE) as i32 as f32]
        },
    };
    let si = match l.wrap {
        1 | 2 => 1,
        3 => 2,
        _ => 0,
    };
    (blend, base, si)
}

/// The lighting features' names, as `Renderer::refusals` says them.
const LIT: &str = "Dynamic Lighting";
const SUN: &str = "Sun Shadows";
const POINTS: &str = "Light Shadows";
const SHELL: &str = "Aura shells";
const TRIM: &str = "Trim auras";

/// What `make` makes (a lighting feature's maps, samplers and pipelines),
/// unless a page's WebGL2 refuses it. A browser's GL driver may refuse a
/// shader every other takes (its compiler's own limits and bugs), and a
/// refusal was an error nothing caught: a panic, the client stopped at every
/// world entry for as long as the option stayed on. On WebGL2 `make` runs
/// under error scopes (`caught`): refused, nothing is made, and `what` is
/// kept in `refused` and what it said in `said`. Elsewhere (natively, and a
/// page on WebGPU, whose scopes answer only later) `make` alone.
fn guarded<T>(device: &wgpu::Device, refused: &mut Vec<&'static str>, said: &mut Vec<String>, what: &'static str, make: impl FnOnce() -> Option<T>) -> Option<T> {
    if !(cfg!(target_arch = "wasm32") && device.adapter_info().backend == wgpu::Backend::Gl) {
        return make();
    }
    caught(device, refused, said, what, make)
}

/// `make` under error scopes (`guarded`): what it made, or `None` with
/// `what` kept in `refused` and its first error in `said` when it raised
/// any. wgpu's own backends (GL among them) answer a scope at once.
fn caught<T>(device: &wgpu::Device, refused: &mut Vec<&'static str>, said: &mut Vec<String>, what: &'static str, make: impl FnOnce() -> Option<T>) -> Option<T> {
    let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let made = make();
    // innermost first
    let errors = [ready(validation.pop()), ready(internal.pop()), ready(memory.pop())];
    let Some(e) = errors.into_iter().flatten().flatten().next() else { return made };
    let text: String = e.to_string().split_whitespace().collect::<Vec<_>>().join(" ").chars().take(400).collect();
    refused.push(what);
    said.push(format!("{what}: {text}"));
    None
}

/// `make` (a texture) under an out-of-memory and a validation scope: what it
/// made, or `None` if the device refused it (Hoofbite's Iris Xe of
/// 2026-11-17, whose refusal reached wgpu's default handler, a panic). A
/// scope that does not answer at once (a page's WebGPU) takes it as made.
pub(crate) fn made_safely<T>(device: &wgpu::Device, make: impl FnOnce() -> T) -> Option<T> {
    let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let made = make();
    let errors = [ready(validation.pop()), ready(memory.pop())];
    if let Some(e) = errors.into_iter().flatten().flatten().next() {
        let text: String = e.to_string().split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect();
        eprintln!("graphics: a texture refused: {text}");
        return None;
    }
    Some(made)
}

/// A future's output if it has one now, without waiting.
fn ready<F: std::future::Future>(f: F) -> Option<F::Output> {
    let mut f = std::pin::pin!(f);
    match f.as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop())) {
        std::task::Poll::Ready(v) => Some(v),
        std::task::Poll::Pending => None,
    }
}

/// A shader's text with its lines ended as this source's are (`\n`): a
/// Windows checkout (git's `core.autocrlf`, the default there and on
/// GitHub's Windows runners) ends `include_str!`'s lines `\r\n`, and the
/// receivers' modules are made by finding lit.wgsl's lines in it
/// (`sunshadow::shadowed_lit`, `lightshadow::receivers_wgsl`), which a `\r`
/// left unfound: Sun Shadows and Light Shadows were never made in the
/// Windows release (2026-11-12, a player's "the release shadows wasn't
/// working, built from source it worked").
pub(crate) fn lf(text: &str) -> String {
    text.replace("\r\n", "\n")
}

pub(crate) fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

/// A texture's bytes over its levels at 4 a texel.
pub fn texture_bytes(t: &wgpu::Texture) -> u64 {
    (0..t.mip_level_count()).map(|k| u64::from((t.width() >> k).max(1)) * u64::from((t.height() >> k).max(1)) * 4).sum()
}

/// A texture of `levels` (the base first, each half the one before), as
/// the surface cache puts a surface's levels in the GS's memory.
pub(crate) fn upload_levels(device: &wgpu::Device, queue: &wgpu::Queue, levels: &[esf::Image]) -> wgpu::TextureView {
    let base = &levels[0];
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("surface"),
        size: wgpu::Extent3d { width: base.width, height: base.height, depth_or_array_layers: 1 },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TEXTURE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (k, l) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: k as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &l.rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(l.width * 4), rows_per_image: Some(l.height) },
            wgpu::Extent3d { width: l.width, height: l.height, depth_or_array_layers: 1 },
        );
    }
    tex.create_view(&Default::default())
}

fn upload_rgba(device: &wgpu::Device, queue: &wgpu::Queue, w: u32, h: u32, rgba: &[u8]) -> wgpu::TextureView {
    let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("surface"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TEXTURE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        tex.as_image_copy(),
        rgba,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
        size,
    );
    tex.create_view(&Default::default())
}

/// A depth buffer the size of the target.
pub fn depth_view(device: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            // read by the Ambient Occlusion's passes (`ao`)
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

impl Renderer {
    /// Before `curve.scene_target`, for a frame drawn into `color`: where the
    /// fans set read the frame they are drawn over (`fans_read_frame`, the
    /// ground fog a PS2 frame keeps) and `color` cannot be copied (a page's
    /// canvas, a GL window), the scene goes through the curve's texture,
    /// which can (`curve::Curve::set_through`); otherwise as before.
    pub fn read_frame_through(&mut self, color: &wgpu::TextureView) {
        let through = self.fans_read_frame() && !color.texture().usage().contains(wgpu::TextureUsages::COPY_SRC);
        self.curve.set_through(through);
    }
}

/// Renders one frame offscreen and returns it as RGBA8 rows (for `--shot`),
/// with the particles' billboards (view, projection, batches) if any.
#[allow(clippy::too_many_arguments)]
pub fn capture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    w: u32,
    h: u32,
    view: &View,
    items: &[Draw],
    particles: Option<(Mat4, Mat4, &[billboard::Batch], &Textures)>,
) -> Vec<u8> {
    capture_with_sky(device, queue, renderer, w, h, view, items, particles, None)
}

/// `capture`, with the sky's billboards (the stars, the lightning patches)
/// drawn behind the world through the sky's view, and the fans `set_fans`
/// set after them.
#[allow(clippy::too_many_arguments)]
pub fn capture_with_sky(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    w: u32,
    h: u32,
    view: &View,
    items: &[Draw],
    particles: Option<(Mat4, Mat4, &[billboard::Batch], &Textures)>,
    sky: Option<(Mat4, Mat4, &[billboard::Batch], &Textures)>,
) -> Vec<u8> {
    capture_around(device, queue, renderer, w, h, view, items, particles, sky, |_, _, _, _| {})
}

/// `capture_with_sky` with `between` drawn between the labels and the 2D
/// layer, as `Renderer::draw_text_around` draws it (the minimap): given the
/// encoder, the target and its depth, and the target's size (the Render
/// Scale's, `w` x `h` times it).
#[allow(clippy::too_many_arguments)]
pub fn capture_around(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    w: u32,
    h: u32,
    view: &View,
    items: &[Draw],
    particles: Option<(Mat4, Mat4, &[billboard::Batch], &Textures)>,
    sky: Option<(Mat4, Mat4, &[billboard::Batch], &Textures)>,
    between: impl FnOnce(&mut wgpu::CommandEncoder, &wgpu::TextureView, &wgpu::TextureView, (u32, u32)),
) -> Vec<u8> {
    let format = TARGET_FORMAT;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("capture"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let row = (w * 4).div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let depth = depth_view(device, w, h);
    let mut enc = device.create_command_encoder(&Default::default());
    let tv = target.create_view(&Default::default());
    // the frame at the Render Scale's size (the target's own at 1), the scene
    // into the Brightness curve's texture when one is set
    let f = renderer.scale.targets(device, &tv, &depth, (w, h));
    renderer.read_frame_through(&f.color);
    let scene = renderer.curve.scene_target(device, f.size);
    let st = scene.as_ref().unwrap_or(&f.color);
    // the Ambient Occlusion's passes over its opaque half, while set (`ao`)
    renderer.ao.prepare(device, &f);
    renderer.draw(queue, &mut enc, st, &f.depth, view, items);
    if let Some((v, p, batches, textures)) = sky {
        renderer.draw_sky_billboards(device, queue, &mut enc, st, &f.depth, v, p, batches, textures);
    }
    if let Some((v, p, _, textures)) = particles {
        renderer.draw_decor(device, queue, &mut enc, st, &f.depth, v, p, textures);
    }
    renderer.draw_fans(device, queue, &mut enc, st, &f.depth, view);
    renderer.draw_plates(device, &mut enc, st, &f.depth, f.size);
    if let Some((v, p, batches, textures)) = particles {
        renderer.draw_billboards_and_shadows(device, queue, &mut enc, st, &f.depth, v, p, batches, textures, view);
    }
    renderer.curve.apply(queue, &mut enc, &f.color);
    renderer.draw_text_around(device, &mut enc, &f.color, &f.depth, f.size, |enc| between(enc, &f.color, &f.depth, f.size));
    renderer.scale.resolve(&mut enc, &tv);
    enc.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([enc.finish()]);
    readback.slice(..).map_async(wgpu::MapMode::Read, |r| r.expect("map readback"));
    device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
    let data = readback.slice(..).get_mapped_range().expect("mapped readback");
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        out.extend_from_slice(&data[y * row as usize..y * row as usize + w as usize * 4]);
    }
    out
}

/// One instance, untransformed.
pub const IDENTITY: [[f32; 4]; 4] = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

/// Textures out of a library: the surface a DictID names in the library's
/// view, wherever it was registered, decoded; keyed by the id and the disc
/// its record came off (`esf::Library::texture_key`), so a 2002 texture and
/// its Frontiers namesake are two textures, and an unchanged one is one.
pub struct LibraryTextures<'a>(pub &'a esf::Library);

/// Textures a server's decorations use that no file of the library holds (a
/// tent's cloth of another world's zone as a present's paper, a tree's leaves
/// as tinsel: `decorlights`'s assets), by DictID: their levels, decoded once.
/// A texture the library has is the library's; one of these is drawn only
/// where nothing loaded names its id, and keyed apart from any that does.
static DECOR_TEXTURES: std::sync::Mutex<Vec<(i32, std::sync::Arc<Vec<esf::Image>>)>> = std::sync::Mutex::new(Vec::new());

/// Keeps `levels` as decoration texture `id` (`DECOR_TEXTURES`); a second for
/// the same id takes the first's place.
pub fn set_decor_texture(id: i32, levels: Vec<esf::Image>) {
    let mut g = DECOR_TEXTURES.lock().unwrap_or_else(|e| e.into_inner());
    g.retain(|(i, _)| *i != id);
    g.push((id, std::sync::Arc::new(levels)));
}

/// Decoration texture `id`'s levels, if one is kept.
pub fn decor_texture(id: i32) -> Option<std::sync::Arc<Vec<esf::Image>>> {
    DECOR_TEXTURES.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(i, _)| *i == id).map(|(_, l)| l.clone())
}

/// The bit a decoration texture's key carries (`DECOR_TEXTURES`): never a
/// library texture's key.
const DECOR_KEY: u64 = 1 << 62;

/// The tattoos' patterns read so far, by pattern (`tattoo::Woad::of`, off the
/// disc's own face: never a texture pack's).
static WOADS: std::sync::Mutex<Vec<(u8, std::sync::Arc<tattoo::Woad>)>> = std::sync::Mutex::new(Vec::new());

impl LibraryTextures<'_> {
    /// Tattoo pattern `pattern` read off its face in this library; none for a
    /// pattern of none or a library without the face.
    fn woad(&self, pattern: u8) -> Option<std::sync::Arc<tattoo::Woad>> {
        let face = tattoo::pattern_face(pattern)?;
        let mut g = WOADS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, w)) = g.iter().find(|(p, _)| *p == pattern) {
            return Some(w.clone());
        }
        let (c, s) = self.0.surface(face as i32)?;
        let im = s.decode(&c.data)?;
        let w = std::sync::Arc::new(tattoo::Woad::of(pattern, &im.rgba, im.width, im.height)?);
        g.push((pattern, w.clone()));
        Some(w)
    }

    /// The surface a DictID names in the library's view, its container, and
    /// the disc its record came off.
    fn surface(&self, id: i32) -> Option<(&esf::Container, esf::Surface, esf::Era)> {
        let r = self.0.lookup(id)?;
        let (c, s) = self.0.surface(id)?;
        Some((c, s, self.0.era_of(r.file)))
    }

    /// A texture's trim (`trimart`) as this library draws it: every trim
    /// on Frontiers' art, the plate's gold alone on the 2002 disc's (F9:
    /// its other sets' art is other art, which the rules were not read off).
    fn trim_of(&self, id: i32) -> Option<trimart::Trim> {
        let t = trimart::trim_of(id as u32)?;
        match self.surface(id).map(|(_, _, era)| era) {
            Some(esf::Era::Y2002) if t != trimart::Trim::Gold => None,
            _ => Some(t),
        }
    }
}

/// With a pack set (`set_pack`), a surface it has a stand-in for is drawn
/// as the pack's levels, keyed apart (its generation in bits 40 and up);
/// every other texture, and every texture with none set, is the surface's
/// own under its own key.
///
/// A recoloured texture's id ([`recolor::recolored`]: a body's part a player
/// recoloured) is its source's levels, recoloured, under a key of its own; a
/// texture's own id resolves as it always did.
impl TextureSource for LibraryTextures<'_> {
    fn image(&self, id: i32) -> Option<esf::Image> {
        self.levels(id).into_iter().next()
    }
    fn levels(&self, id: i32) -> Vec<esf::Image> {
        let Some((c, s, era)) = self.surface(id) else {
            return match recolor::source_of(id) {
                Some((source, r)) if r >> 24 == recolor::TATTOO => {
                    // a barbarian's tattoo: the face's own woad off (the
                    // face it was made from at the first), the pattern's on
                    let mut levels = self.levels(source);
                    let root = recolor::root_of(source);
                    if root as u32 == tattoo::CHEST_ART {
                        for l in &mut levels {
                            tattoo::chest(&mut l.rgba, l.width, l.height, tattoo::pattern_of(r));
                        }
                        return levels;
                    }
                    let own = tattoo::own_pattern(root as u32).and_then(|p| self.woad(p));
                    let put = self.woad(tattoo::pattern_of(r));
                    for l in &mut levels {
                        tattoo::lay(&mut l.rgba, l.width, l.height, own.as_deref(), source == root, put.as_deref());
                    }
                    levels
                }
                Some((source, r)) if r >> 24 == recolor::TRIM_MASK => {
                    // a texture's trim as a mask (`trimglow`), each level its own
                    let mut levels = self.levels(source);
                    let trim = self.trim_of(source);
                    for l in &mut levels {
                        l.rgba = match trim {
                            Some(t) => recolor::trim_mask_of(&l.rgba, l.width, l.height, t),
                            None => recolor::trim_mask_of(&l.rgba, l.width, l.height, trimart::Trim::Parts(&[])),
                        };
                    }
                    levels
                }
                Some((source, r)) if matches!(r >> 24, recolor::CLOTH | recolor::BARE) => {
                    // a pet's clothing or body dye: the clothing read off the
                    // texture it was made from at the first (`petart`)
                    let mut levels = self.levels(source);
                    let root = recolor::root_of(source);
                    let Some(cloth) = petart::cloth_of(root as u32) else {
                        // a texture of no clothing: all body
                        if r >> 24 == recolor::BARE {
                            let mut rgba: Vec<Vec<u8>> = levels.iter_mut().map(|l| std::mem::take(&mut l.rgba)).collect();
                            recolor::recolor_levels(&mut rgba, recolor::MUL << 24 | (r & 0xFF_FFFF));
                            for (l, px) in levels.iter_mut().zip(rgba) {
                                l.rgba = px;
                            }
                        }
                        return levels;
                    };
                    let base: Vec<(Vec<u8>, u32)> = if root == source { levels.iter().map(|l| (l.rgba.clone(), l.width)).collect() } else { self.levels(root).into_iter().map(|l| (l.rgba, l.width)).collect() };
                    let mut rgba: Vec<Vec<u8>> = levels.iter_mut().map(|l| std::mem::take(&mut l.rgba)).collect();
                    recolor::cloth_levels(&mut rgba, &base, r, cloth);
                    for (l, px) in levels.iter_mut().zip(rgba) {
                        l.rgba = px;
                    }
                    levels
                }
                Some((source, r)) if r >> 24 == recolor::TRIM => {
                    // a piece's trim in a dye's colour: the trim read off the
                    // texture it was made from (a MUL before it changed the
                    // texels' colours, not which are trim)
                    let mut levels = self.levels(source);
                    let root = recolor::root_of(source);
                    let Some(trim) = self.trim_of(root) else { return levels };
                    let base: Vec<(Vec<u8>, u32)> = if root == source { levels.iter().map(|l| (l.rgba.clone(), l.width)).collect() } else { self.levels(root).into_iter().map(|l| (l.rgba, l.width)).collect() };
                    let mut rgba: Vec<Vec<u8>> = levels.iter_mut().map(|l| std::mem::take(&mut l.rgba)).collect();
                    recolor::trim_levels(&mut rgba, &base, r, trim);
                    for (l, px) in levels.iter_mut().zip(rgba) {
                        l.rgba = px;
                    }
                    levels
                }
                Some((source, r)) => {
                    let mut levels = self.levels(source);
                    let mut rgba: Vec<Vec<u8>> = levels.iter_mut().map(|l| std::mem::take(&mut l.rgba)).collect();
                    recolor::recolor_levels(&mut rgba, r);
                    for (l, px) in levels.iter_mut().zip(rgba) {
                        l.rgba = px;
                    }
                    levels
                }
                // a decoration's texture no file holds (`DECOR_TEXTURES`)
                None => decor_texture(id).map(|l| l.as_ref().clone()).unwrap_or_default(),
            };
        };
        if let Some(l) = pack().and_then(|p| p.levels(c, &s, era)).filter(|l| !l.is_empty()) {
            return l;
        }
        s.decode_levels(&c.data)
    }
    fn key(&self, id: i32) -> u64 {
        if self.surface(id).is_none()
            && let Some((source, r)) = recolor::source_of(id)
        {
            return recolor::key_of(self.key(source), r);
        }
        if self.surface(id).is_none() && decor_texture(id).is_some() {
            return id as u32 as u64 | DECOR_KEY;
        }
        let key = self.0.texture_key(id);
        let Some(p) = pack() else { return key };
        match self.surface(id) {
            Some((c, s, era)) if p.replaces(key, c, &s, era) => key | p.generation() << 40,
            _ => key,
        }
    }
    fn native_size(&self, id: i32) -> Option<(u32, u32)> {
        if let Some((_, s)) = self.0.surface(id) {
            return Some((s.width.max(0) as u32, s.height.max(0) as u32));
        }
        if let Some((source, _)) = recolor::source_of(id) {
            return self.native_size(source);
        }
        decor_texture(id).and_then(|l| l.first().map(|i| (i.width, i.height)))
    }
    fn pending(&self, id: i32, key: u64, defer: bool) -> Option<(u32, u32, u32)> {
        let (c, s, era) = self.surface(id)?;
        pack()?.request(key, c, &s, era, defer)
    }
    fn packed(&self, id: i32) -> Option<std::sync::Arc<packed::Packed>> {
        let (c, s, era) = self.surface(id)?;
        pack()?.packed(c, &s, era)
    }
    fn own_levels(&self, id: i32) -> Vec<esf::Image> {
        match self.surface(id) {
            Some((c, s, _)) => s.decode_levels(&c.data),
            None => self.levels(id),
        }
    }
}

pub fn library_textures(lib: &esf::Library) -> LibraryTextures<'_> {
    LibraryTextures(lib)
}

#[cfg(test)]
mod tests {
    use super::level_drawn;

    /// A texture the device refuses (here one past its largest side; on
    /// Hoofbite's Iris Xe, one it had no memory for) is `None` from
    /// `made_safely`, the device's uncaptured-error handler (wgpu's default
    /// panic) never reached; one it takes is made. Needs a graphics adapter
    /// (any backend); without one it says so and passes.
    #[test]
    fn a_refused_texture_is_none_not_a_panic() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let raised = Arc::new(AtomicUsize::new(0));
        let r = raised.clone();
        device.on_uncaptured_error(Arc::new(move |_: wgpu::Error| {
            r.fetch_add(1, Ordering::SeqCst);
        }));
        let side = device.limits().max_texture_dimension_2d;
        let image = |w: u32| esf::Image { width: w, height: 1, rgba: vec![0; w as usize * 4], alpha: false };
        let big = [image(side * 2)];
        assert!(super::made_safely(&device, || super::upload_levels(&device, &queue, &big)).is_none());
        let small = [image(4)];
        assert!(super::made_safely(&device, || super::upload_levels(&device, &queue, &small)).is_some());
        assert_eq!(raised.load(Ordering::SeqCst), 0, "caught, not raised");
    }

    /// A pack's stand-ins (larger than their surfaces) within the pack's
    /// budget (the owner's report of 2026-11-18, "some textures are not
    /// being loaded from texture packs": past the budget of 2026-11-17 every
    /// new one was the game's own for the rest of the run). Needs a graphics
    /// adapter (any backend); without one it says so and passes.
    ///
    /// - All in use (none drawn since: this frame's): what fits is the
    ///   pack's at its size, the next a level smaller each way (128), the
    ///   last, with no room even at its surface's 64, the game's own; the
    ///   surfaces' own textures count nothing. With room for all four, all
    ///   four the pack's at their size.
    /// - A frame on, nothing holding them: a new one lets the least recently
    ///   drawn go and is the pack's at its size, and the one let go, asked
    ///   for again, is the pack's again, letting the next go.
    /// - One a model or group holds (`begin_holds`) is never let go: the
    ///   other goes.
    #[test]
    fn a_pack_s_textures_are_kept_within_its_budget_by_what_is_drawn() {
        struct Src;
        impl super::TextureSource for Src {
            fn image(&self, id: i32) -> Option<esf::Image> {
                self.levels(id).into_iter().next()
            }
            fn levels(&self, id: i32) -> Vec<esf::Image> {
                // ids 1-4 a pack's 256 (its levels to 64), 5 a surface's own 64
                let top = if id <= 4 { 256 } else { 64 };
                (0..).map(|k| top >> k).take_while(|&w| w >= 64).map(|w| esf::Image { width: w, height: w, rgba: vec![9; (w * w * 4) as usize], alpha: false }).collect()
            }
            fn native_size(&self, _id: i32) -> Option<(u32, u32)> {
                Some((64, 64))
            }
            fn own_levels(&self, _id: i32) -> Vec<esf::Image> {
                vec![esf::Image { width: 64, height: 64, rgba: vec![3; 64 * 64 * 4], alpha: false }]
            }
        }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let one: u64 = (256 * 256 + 128 * 128 + 64 * 64) * 4;
        let half: u64 = (128 * 128 + 64 * 64) * 4;
        let own: u64 = 64 * 64 * 4;
        let side = |r: &super::Renderer, id: i32| r.textures.get(&(id as u64)).cloned().flatten().map(|(v, _)| v.texture().width());
        // all in use
        for (budget, sides, pbytes) in [(2 * one + half, [256, 256, 128, 64], 2 * one + half), (4 * one, [256; 4], 4 * one)] {
            let mut r = super::Renderer::new(&device, &queue, super::TARGET_FORMAT);
            super::set_pack_limits(super::PackLimits { budget, max_side: 4096 });
            for id in 1..=5 {
                assert!(r.texture(&device, &queue, id, &Src).is_some());
            }
            assert_eq!([1, 2, 3, 4].map(|id| side(&r, id).unwrap()), sides, "budget {budget}");
            let (n, bytes, _, pb) = r.texture_memory();
            assert_eq!((n, pb), (5, pbytes), "budget {budget}");
            let owns = sides.iter().filter(|&&s| s == 64).count() as u64;
            assert_eq!(bytes, pbytes + (1 + owns) * own, "the rest at their own 64");
            let m = r.pack_memory();
            assert_eq!((m.used, m.evicted, m.held()), (pbytes, 0, budget != 4 * one));
            assert!(m.used <= budget);
        }
        // a frame on, nothing holding them
        let mut r = super::Renderer::new(&device, &queue, super::TARGET_FORMAT);
        super::set_pack_limits(super::PackLimits { budget: 2 * one, max_side: 4096 });
        r.texture(&device, &queue, 1, &Src);
        r.texture(&device, &queue, 2, &Src);
        r.pack.next_frame();
        r.texture(&device, &queue, 2, &Src);
        r.pack.next_frame();
        r.texture(&device, &queue, 3, &Src);
        assert_eq!([1, 2, 3].map(|id| side(&r, id)), [None, Some(256), Some(256)], "1, drawn longest ago, let go for 3");
        r.pack.next_frame();
        r.texture(&device, &queue, 1, &Src);
        assert_eq!([1, 2, 3].map(|id| side(&r, id)), [Some(256), None, Some(256)], "1 the pack's again, 2 let go");
        assert_eq!((r.pack_memory().used, r.pack_memory().evicted), (2 * one, 2));
        // held
        r.pack.next_frame();
        let outer = r.begin_holds();
        r.texture(&device, &queue, 3, &Src);
        let holds = r.end_holds(outer);
        assert_eq!(holds.len(), 1);
        for _ in 0..3 {
            r.pack.next_frame();
        }
        r.texture(&device, &queue, 4, &Src);
        assert_eq!([1, 3, 4].map(|id| side(&r, id)), [None, Some(256), Some(256)], "3 held though drawn before 1: 1 let go");
        // and with nothing to let go but what is held, smaller
        r.pack.next_frame();
        let outer = r.begin_holds();
        r.texture(&device, &queue, 4, &Src);
        let more = r.end_holds(outer);
        r.pack.next_frame();
        r.texture(&device, &queue, 2, &Src);
        assert_eq!(side(&r, 2), Some(64), "no room: the game's own");
        drop((holds, more));
        r.pack.next_frame();
        r.texture(&device, &queue, 2, &Src);
        assert_eq!(side(&r, 2), Some(256), "made again at its size once there is room");
        // the options' Texture Pack Memory over the device's
        super::set_pack_memory(512);
        assert_eq!(super::pack_limits().budget, 512 << 20);
        super::set_pack_memory(0);
        assert_eq!(super::pack_limits().budget, 2 * one, "Auto: the device's");
    }

    /// The card's memory as its Vulkan driver says it (`device_memory`),
    /// and Auto's budget from it: said where there is a Vulkan adapter
    /// (this machine's RX 580: 7,936 MB, a budget of 2,645 MB), `None` on any other
    /// backend. Needs a graphics adapter; without one it says so and passes.
    #[test]
    fn device_memory_where_vulkan_says_it() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let (device, _queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let info = adapter.get_info();
        let m = super::device_memory(&device);
        eprintln!("{} ({:?}, {:?}): device memory {:?} MB, Auto budget {} MB", info.name, info.backend, info.device_type, m.map(|m| m >> 20), super::PackLimits::for_adapter(info.device_type, m).budget >> 20);
        if info.backend == wgpu::Backend::Vulkan {
            assert!(m.is_some_and(|m| m > 0));
        } else {
            assert_eq!(m, None);
        }
    }

    /// Auto's budget: a discrete card's a third of its memory where the
    /// driver says it, 1 GB to 4 GB; 1 GB where it does not; an integrated
    /// chip's 256 MB whatever it says.
    #[test]
    fn auto_pack_budget_by_the_card() {
        use wgpu::DeviceType::*;
        let gb = 1u64 << 30;
        if std::env::var_os("EQOA_PACK_BUDGET_MB").is_some() {
            return;
        }
        let b = |kind, m| super::PackLimits::for_adapter(kind, m).budget;
        assert_eq!(b(DiscreteGpu, None), gb);
        assert_eq!(b(DiscreteGpu, Some(2 * gb)), gb, "at least 1 GB");
        assert_eq!(b(DiscreteGpu, Some(8 * gb)), 8 * gb / 3);
        assert_eq!(b(DiscreteGpu, Some(24 * gb)), 4 * gb, "at most 4 GB");
        assert_eq!(b(IntegratedGpu, Some(16 * gb)), 256 << 20);
        assert_eq!(b(Cpu, None), 256 << 20);
    }

    /// One level of an LOD body at a time, none past the last; a batch in no
    /// level whatever the model's level. The control: drawing every batch
    /// (the old rule) draws the far level with the near one.
    #[test]
    fn one_level_at_a_time() {
        for level in [None, Some(0), Some(1)] {
            assert!(level_drawn(None, level));
        }
        assert!(level_drawn(Some(0), Some(0)) && !level_drawn(Some(1), Some(0)));
        assert!(level_drawn(Some(1), Some(1)) && !level_drawn(Some(0), Some(1)));
        assert!(!level_drawn(Some(0), None) && !level_drawn(Some(1), None));
        let every = |_: Option<u8>, _: Option<u8>| true;
        assert_ne!(every(Some(1), Some(0)), level_drawn(Some(1), Some(0)));
    }

    /// What a page's WebGL2 refuses of a lighting feature is caught
    /// (`super::caught`): nothing made, the feature's name kept and what was
    /// said, the device's uncaught-error handler (a panic, by default) never
    /// reached; what it takes is made with nothing kept. The control: the
    /// same refusal made without the scopes reaches the handler. Needs a
    /// graphics adapter (any backend); without one it says so and passes.
    #[test]
    fn a_refused_feature_is_caught_not_raised() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let (device, _queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let raised = Arc::new(AtomicUsize::new(0));
        let r = raised.clone();
        device.on_uncaptured_error(Arc::new(move |_: wgpu::Error| {
            r.fetch_add(1, Ordering::SeqCst);
        }));
        // a buffer both mapped for reading and written to: refused without
        // the device feature that allows it
        let refused_buffer = || device.create_buffer(&wgpu::BufferDescriptor { label: Some("refused"), size: 16, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::MAP_WRITE, mapped_at_creation: false });
        let taken = || device.create_buffer(&wgpu::BufferDescriptor { label: Some("taken"), size: 16, usage: wgpu::BufferUsages::UNIFORM, mapped_at_creation: false });
        let (mut refused, mut said) = (Vec::new(), Vec::new());
        assert!(super::caught(&device, &mut refused, &mut said, super::POINTS, || Some(taken())).is_some(), "taken: made");
        assert!(refused.is_empty() && said.is_empty());
        assert!(super::caught(&device, &mut refused, &mut said, super::POINTS, || Some(refused_buffer())).is_none(), "refused: nothing made");
        assert_eq!(refused, [super::POINTS]);
        assert!(said.len() == 1 && said[0].starts_with("Light Shadows: ") && said[0].contains("MAP_WRITE"), "{said:?}");
        assert_eq!(raised.load(Ordering::SeqCst), 0, "caught, not raised");
        // natively `guarded` is `make` alone, its refusals as raised as ever
        let (mut r2, mut s2) = (Vec::new(), Vec::new());
        assert!(super::guarded(&device, &mut r2, &mut s2, super::SUN, || Some(refused_buffer())).is_some() && r2.is_empty());
        assert_eq!(raised.load(Ordering::SeqCst), 1, "the control: without the scopes the refusal is raised");
    }
}

#[cfg(test)]
mod sky_fog_tests {
    /// The shader's fog factor (1 none, 0 all fog) is 1 - s at any depth the
    /// sky's dome is drawn at; the control: the world's fog of a storm (near
    /// 3, far 40) leaves a dome 5 off nearly unfogged.
    #[test]
    fn the_sky_takes_its_share_of_the_fog_at_any_depth() {
        let factor = |f: [f32; 4], w: f32| ((f[1] - w) / (f[1] - f[0])).clamp(0.0, 1.0);
        for s in [0.0, 0.25, 0.5, 1.0] {
            for w in [0.5, 5.0, 50.0, 500.0] {
                assert!((factor(super::sky_fog_span(s), w) - (1.0 - s)).abs() < 1e-3, "s {s} at {w}");
            }
        }
        assert!(factor([3.2, 40.0, 0.0, 0.0], 5.0) > 0.9, "the control");
    }

    /// A storm's sky: all fog at the horizon and low (where a wall the far
    /// plane left out stood) whatever the share, the share overhead, between
    /// them rising smoothly; at a heavy storm's share (1) all fog everywhere.
    #[test]
    fn a_storms_sky_is_all_fog_low_and_its_share_overhead() {
        let share = super::storm_sky_share;
        for s in [0.0, 0.3, 1.0] {
            assert_eq!(share(s, -0.2), 1.0, "below the horizon at {s}");
            assert_eq!(share(s, 0.0), 1.0, "the horizon at {s}");
            assert_eq!(share(s, 0.3), 1.0, "a wall's height at {s}");
            assert!((share(s, 0.8) - s).abs() < 1e-6, "overhead at {s}");
            assert!((share(s, 1.0) - s).abs() < 1e-6, "the zenith at {s}");
        }
        let mut last = 1.0;
        for k in 0..=20 {
            let v = share(0.2, 0.3 + k as f32 * 0.015);
            assert!(v <= last + 1e-6, "rising smoothly: {v} after {last}");
            last = v;
        }
        assert!((0..=10).all(|k| share(1.0, k as f32 / 10.0) == 1.0), "a heavy storm: all fog");
    }
}
