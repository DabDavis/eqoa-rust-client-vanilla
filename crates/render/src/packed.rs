//! A texture pack's stand-in in the form the GPU takes it (the owner's of
//! 2026-11-18, "lets look at PCSX2 how they do texture packs like precaching
//! and we can make a better version"): its levels as RGBA8 or as BC1, BC2,
//! BC3 or BC7 blocks, the bytes either owned or a slice of a file mapped
//! read-only (`eqoa`'s `texcache`: one copy in the system's page cache for
//! every client open). A compressed one is made a texture of its own format
//! where the device has `TEXTURE_COMPRESSION_BC` (`Renderer`'s
//! `make_texture`): a quarter (BC3, BC7) or an eighth (BC1) of the RGBA's
//! memory, so four to eight times as much of a pack fits its budget
//! (`packmem`). A device without (a page's WebGL2, a phone's) is never given
//! one: `eqoa` decodes it to RGBA first.

use std::sync::Arc;

/// How a stand-in's texels are kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Rgba8,
    Bc1,
    Bc2,
    Bc3,
    Bc7,
}

impl Format {
    /// Its number in `eqoa`'s cache files.
    pub fn code(self) -> u8 {
        match self {
            Format::Rgba8 => 0,
            Format::Bc1 => 1,
            Format::Bc2 => 2,
            Format::Bc3 => 3,
            Format::Bc7 => 7,
        }
    }

    pub fn from_code(c: u8) -> Option<Format> {
        Some(match c {
            0 => Format::Rgba8,
            1 => Format::Bc1,
            2 => Format::Bc2,
            3 => Format::Bc3,
            7 => Format::Bc7,
            _ => return None,
        })
    }

    pub fn compressed(self) -> bool {
        self != Format::Rgba8
    }

    /// The bytes of a 4 x 4 block (a texel's for RGBA8).
    pub fn block_bytes(self) -> u32 {
        match self {
            Format::Rgba8 => 4,
            Format::Bc1 => 8,
            _ => 16,
        }
    }

    /// The bytes of a level `w` x `h`: a compressed one's in whole blocks.
    pub fn level_bytes(self, w: u32, h: u32) -> u64 {
        if self.compressed() { u64::from(w.div_ceil(4)) * u64::from(h.div_ceil(4)) * u64::from(self.block_bytes()) } else { u64::from(w) * u64::from(h) * 4 }
    }

    /// The bytes of a row (of blocks, for a compressed one) of a level `w` wide.
    pub fn row_bytes(self, w: u32) -> u32 {
        if self.compressed() { w.div_ceil(4) * self.block_bytes() } else { w * 4 }
    }

    /// The device's format for it (BCn unorm, as the game's own RGBA8 is
    /// unorm: the GS's bytes, never an sRGB decode).
    pub fn wgpu(self) -> wgpu::TextureFormat {
        match self {
            Format::Rgba8 => crate::TEXTURE_FORMAT,
            Format::Bc1 => wgpu::TextureFormat::Bc1RgbaUnorm,
            Format::Bc2 => wgpu::TextureFormat::Bc2RgbaUnorm,
            Format::Bc3 => wgpu::TextureFormat::Bc3RgbaUnorm,
            Format::Bc7 => wgpu::TextureFormat::Bc7RgbaUnorm,
        }
    }
}

/// Bytes owned or borrowed from something kept alive with them (a mapped
/// file): `len` bytes from `start` of `owner`.
#[derive(Clone)]
pub struct Bytes {
    owner: Arc<dyn AsRef<[u8]> + Send + Sync>,
    start: usize,
    len: usize,
    shared: bool,
}

impl Bytes {
    pub fn owned(v: Vec<u8>) -> Bytes {
        let len = v.len();
        Bytes { owner: Arc::new(v), start: 0, len, shared: false }
    }

    /// `len` bytes at `start` of `owner`, a mapped file's (`None` past its
    /// end).
    pub fn slice_of(owner: Arc<dyn AsRef<[u8]> + Send + Sync>, start: usize, len: usize) -> Option<Bytes> {
        let n = (*owner).as_ref().len();
        (start.checked_add(len)? <= n).then_some(Bytes { owner, start, len, shared: true })
    }

    /// Whether these bytes are a mapped file's, in the system's page cache
    /// that every client shares: what the page's RAM line counts apart.
    pub fn shared(&self) -> bool {
        self.shared
    }
}

impl std::ops::Deref for Bytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &(*self.owner).as_ref()[self.start..self.start + self.len]
    }
}

impl std::fmt::Debug for Bytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bytes({})", self.len)
    }
}

/// A level: its size and its texels or blocks, rows packed.
#[derive(Clone, Debug)]
pub struct Level {
    pub width: u32,
    pub height: u32,
    pub data: Bytes,
}

/// A stand-in's levels, each half the one before, the base first, in one
/// format, with its least texel alpha (what `Pass::alpha_min` wants of a
/// texture, found as it was made).
#[derive(Clone, Debug)]
pub struct Packed {
    pub format: Format,
    pub levels: Vec<Level>,
    pub alpha_min: u8,
}

impl Packed {
    /// Its base's size and level count.
    pub fn size(&self) -> (u32, u32, u32) {
        let b = &self.levels[0];
        (b.width, b.height, self.levels.len() as u32)
    }

    /// Its bytes over its levels.
    pub fn bytes(&self) -> u64 {
        self.levels.iter().map(|l| self.format.level_bytes(l.width, l.height)).sum()
    }

    /// Its bytes with its first `k` levels dropped.
    pub fn bytes_without(&self, k: u32) -> u64 {
        self.levels.iter().skip(k as usize).map(|l| self.format.level_bytes(l.width, l.height)).sum()
    }

    /// How many of its first levels may be dropped (`without`): one level
    /// kept at least, and a compressed one's base a whole number of blocks
    /// each way (a device's rule for a BCn texture's size).
    pub fn droppable(&self) -> u32 {
        let mut k = 0;
        while (k as usize) + 1 < self.levels.len() {
            let l = &self.levels[k as usize + 1];
            if self.format.compressed() && (l.width % 4 != 0 || l.height % 4 != 0) {
                break;
            }
            k += 1;
        }
        k
    }

    /// The same with its first `k` levels dropped (a level smaller each way
    /// for each: `packmem`'s step down, without decoding a block).
    pub fn without(&self, k: u32) -> Packed {
        let k = (k as usize).min(self.levels.len() - 1);
        Packed { format: self.format, levels: self.levels[k..].to_vec(), alpha_min: self.alpha_min }
    }

    /// Whether every level is the size and length it should be: each half
    /// the one before, its bytes whole, a compressed base in whole blocks.
    pub fn valid(&self) -> bool {
        let Some(b) = self.levels.first() else { return false };
        if b.width == 0 || b.height == 0 || (self.format.compressed() && (b.width % 4 != 0 || b.height % 4 != 0)) {
            return false;
        }
        self.levels.iter().enumerate().all(|(k, l)| {
            (l.width, l.height) == crate::standin::level_size(b.width, b.height, k as u32) && l.data.len() as u64 == self.format.level_bytes(l.width, l.height)
        })
    }
}

/// Whether `device` makes BCn textures (`Features::TEXTURE_COMPRESSION_BC`,
/// asked for by `device_descriptor` where the adapter has it).
pub fn bc_supported(device: &wgpu::Device) -> bool {
    device.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
}

/// What a game window's device is asked for: the defaults, and BCn
/// textures where the adapter has them (desktop GPUs; none in a page, where
/// WebGL2 never has them, nor on most phones).
pub fn device_descriptor(adapter: &wgpu::Adapter) -> wgpu::DeviceDescriptor<'static> {
    let mut d = wgpu::DeviceDescriptor::default();
    if cfg!(not(target_arch = "wasm32")) && adapter.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC) {
        d.required_features |= wgpu::Features::TEXTURE_COMPRESSION_BC;
    }
    d
}

static BC: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the game's renderer's device makes BCn textures (set by
/// `Renderer::new`): what `eqoa` keeps a pack's stand-ins compressed for.
pub fn compressed_textures() -> bool {
    BC.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn set_compressed_textures(on: bool) {
    BC.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// A texture of `p`'s levels in its own format (a compressed one only on a
/// device that has it: `bc_supported`).
pub fn upload(device: &wgpu::Device, queue: &wgpu::Queue, p: &Packed) -> wgpu::TextureView {
    let (w, h, n) = p.size();
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("stand-in"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: n,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: p.format.wgpu(),
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (k, l) in p.levels.iter().enumerate() {
        let (rows, extent) = if p.format.compressed() {
            // a level under a block a side is written as its whole block
            (l.height.div_ceil(4), wgpu::Extent3d { width: l.width.div_ceil(4) * 4, height: l.height.div_ceil(4) * 4, depth_or_array_layers: 1 })
        } else {
            (l.height, wgpu::Extent3d { width: l.width, height: l.height, depth_or_array_layers: 1 })
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: k as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &l.data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(p.format.row_bytes(l.width)), rows_per_image: Some(rows) },
            extent,
        );
    }
    tex.create_view(&Default::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(format: Format, w: u32, h: u32) -> Packed {
        let mut levels = Vec::new();
        let (mut lw, mut lh) = (w, h);
        loop {
            levels.push(Level { width: lw, height: lh, data: Bytes::owned(vec![0; format.level_bytes(lw, lh) as usize]) });
            if lw == 1 && lh == 1 {
                break;
            }
            (lw, lh) = ((lw / 2).max(1), (lh / 2).max(1));
        }
        Packed { format, levels, alpha_min: 128 }
    }

    /// A BC chain's bytes are a quarter (BC3, BC7) or an eighth (BC1) of the
    /// RGBA's, the small levels counted in whole blocks; dropping a level
    /// keeps a compressed base a whole number of blocks (the control: RGBA
    /// drops to its last level).
    #[test]
    fn compressed_bytes_and_what_may_be_dropped() {
        let rgba = chain(Format::Rgba8, 1024, 1024);
        let bc7 = chain(Format::Bc7, 1024, 1024);
        let bc1 = chain(Format::Bc1, 1024, 1024);
        assert!(rgba.valid() && bc7.valid() && bc1.valid());
        assert_eq!(bc7.levels[0].data.len() * 4, rgba.levels[0].data.len());
        assert_eq!(bc1.levels[0].data.len() * 8, rgba.levels[0].data.len());
        assert_eq!(Format::Bc7.level_bytes(2, 1), 16, "one whole block");
        assert_eq!(rgba.droppable(), 10);
        assert_eq!(bc7.droppable(), 8, "down to 4 x 4, not 2 x 2");
        let half = bc7.without(1);
        assert_eq!(half.size(), (512, 512, 10));
        assert_eq!(half.bytes(), bc7.bytes_without(1));
        assert!(!Packed { levels: vec![Level { width: 6, height: 4, data: Bytes::owned(vec![0; 32]) }], ..bc7.clone() }.valid(), "a base not in whole blocks");
    }

    /// A slice of a shared owner reads as its own bytes and is said to be
    /// shared; an owned `Vec` is not.
    #[test]
    fn bytes_borrowed_from_a_mapping() {
        let owner: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new((0u8..32).collect::<Vec<u8>>());
        let b = Bytes::slice_of(owner.clone(), 8, 4).unwrap();
        assert_eq!(&*b, &[8, 9, 10, 11]);
        assert!(b.shared());
        assert!(Bytes::slice_of(owner, 30, 4).is_none());
        assert!(!Bytes::owned(vec![1, 2, 3]).shared());
    }
}
