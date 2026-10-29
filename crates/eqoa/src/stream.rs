//! Zone streaming for the walk, at two levels, as the PS2 streams them.
//!
//! - **The disc.** A world file opens with its zones left on the disc
//!   (`esf::Container::open_deferred`: TUNARIA's 997 MB become 20 KB read).
//!   A zone's bytes are asked of a [`ZoneSource`] once it comes within
//!   [`PREFETCH`] of the player, and when they arrive (at once natively, a
//!   frame or more later in a browser) the zone goes into the library and the
//!   collision world (`Streamer::install`). Before a walk starts, the zones
//!   within the inner radius of the spawn are read first ([`fill`],
//!   `VIScene::StreamFillCache`). Zones are only added, never let go.
//! - **The GPU.** The zones within the inner radius come and go as
//!   `VIWorld::StreamLeaf` decides (`vi::world::World::stream`), each with its
//!   terrain, its placed actors and its animated props.
//!
//! `EQOA_WHOLE_WORLD=1` reads a world file whole, as before.

use crate::scene;
use crate::walker::AnimatedProps;
use esf::{Container, Library};
use glam::Vec3;
use render::{GpuGroup, GroupPart, Renderer, wgpu};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use vi::fpu::{Ee, F};
use vi::vect::Vect3;
use vi::world::{STREAM_INNER, World};

/// How far ahead of the player a zone is read: the radius the game's BSP
/// walk measures zones within, past the inner one (900) a zone is drawn at.
pub const PREFETCH: f32 = 1050.0;

/// Where a deferred world's zones come from: each zone's bytes (its record,
/// header and body, at `esf::Library::zone_range`) asked for once and handed
/// back now or on a later frame.
pub trait ZoneSource {
    /// Starts reading `len` bytes at `off` of the world file: zone `zi`.
    fn request(&mut self, zi: usize, off: u64, len: usize);
    /// A zone whose read has finished, each once.
    fn poll(&mut self) -> Option<(usize, std::io::Result<Vec<u8>>)>;
    /// The twin world's zone `zi`, read with it, once (`Twinned`): what
    /// [`install_with`] takes the zone's additions from.
    fn twin_zone(&mut self, _zi: usize) -> Option<Vec<u8>> {
        None
    }
}

/// A world's zones read with its twin's (`esf::Library::set_twin`: the 2002
/// disc's Tunaria with Frontiers'), zone for zone: a zone is handed on once
/// both reads are in (a twin's that fails, or a zone the twin lacks, does not
/// hold it back), the twin's kept for [`ZoneSource::twin_zone`].
pub struct Twinned {
    main: Box<dyn ZoneSource>,
    twin: Box<dyn ZoneSource>,
    /// each of the twin's zones: where its record is in the twin's file
    ranges: Vec<Option<(u64, usize)>>,
    /// reads in: the zone's, and the twin's (`None`: none coming)
    got: std::collections::HashMap<usize, (Option<std::io::Result<Vec<u8>>>, Option<Option<Vec<u8>>>)>,
    twins: std::collections::HashMap<usize, Vec<u8>>,
}

impl Twinned {
    pub fn new(main: Box<dyn ZoneSource>, twin: Box<dyn ZoneSource>, ranges: Vec<Option<(u64, usize)>>) -> Self {
        Twinned { main, twin, ranges, got: Default::default(), twins: Default::default() }
    }
}

impl ZoneSource for Twinned {
    fn request(&mut self, zi: usize, off: u64, len: usize) {
        self.main.request(zi, off, len);
        let twin = match self.ranges.get(zi).copied().flatten() {
            Some((o, n)) => {
                self.twin.request(zi, o, n);
                None
            }
            None => Some(None),
        };
        self.got.insert(zi, (None, twin));
    }

    fn poll(&mut self) -> Option<(usize, std::io::Result<Vec<u8>>)> {
        while let Some((zi, r)) = self.main.poll() {
            self.got.entry(zi).or_insert((None, Some(None))).0 = Some(r);
        }
        while let Some((zi, r)) = self.twin.poll() {
            if let Err(e) = &r {
                eprintln!("zone {zi}'s twin: {e}");
            }
            self.got.entry(zi).or_insert((None, None)).1 = Some(r.ok());
        }
        let zi = *self.got.iter().find(|(_, (m, t))| m.is_some() && t.is_some())?.0;
        let (m, t) = self.got.remove(&zi)?;
        if let Some(Some(b)) = t {
            self.twins.insert(zi, b);
        }
        Some((zi, m?))
    }

    fn twin_zone(&mut self, zi: usize) -> Option<Vec<u8>> {
        self.twins.remove(&zi)
    }
}

/// What a world's twin adds to its zones (`esf::Library::set_twin`): the
/// placements of the record types the 2002 disc's Tunaria has none of,
/// Frontiers' effect volumes (fog, dust, leaves, shafts, flocks: `fx`) and
/// streamed audio (the zones' ambient music and sound,
/// `vi::world::SpriteKind::Stream`).
pub const TWIN_ADDS: &[u16] = &[crate::fx::RECORD, 0x2E00];

/// The records a placed addition names besides its own: an effect volume's
/// surface and flock sprite (another volume of the zone may hold them).
fn addition_needs(c: &Container, n: esf::NodeId) -> Vec<i32> {
    crate::fx::Record::read(c, n).and_then(|r| r.params).map_or_else(Vec::new, |p| vec![p.surface, p.sprite])
}

/// Zone `zi` of the twin of the world in file `world`, its record's bytes:
/// what it adds (`TWIN_ADDS`) put in the library for the zone
/// (`esf::Library::add_zone_extras`), before the collision world takes the
/// zone. Nothing when it adds nothing.
pub fn add_twin_zone(lib: &mut Library, world: usize, zi: usize, bytes: Vec<u8>) {
    let c = Container::parse_records(bytes);
    let Some(&zone) = c.roots.first() else { return };
    if let Some((extra, actors)) = c.additions(zone, TWIN_ADDS, addition_needs) {
        lib.add_zone_extras(world, zi, extra, actors);
    }
}

/// [`install`], then what the zone's twin adds (`ZoneSource::twin_zone`).
pub fn install_with(lib: &mut Library, file: usize, zi: usize, bytes: Vec<u8>, src: &mut dyn ZoneSource) -> Result<(), String> {
    install(lib, file, zi, bytes)?;
    if let Some(t) = src.twin_zone(zi) {
        add_twin_zone(lib, file, zi, t);
    }
    Ok(())
}

/// The native source: the world file's extent in the disc image, read at once
/// (on the game's thread: each read timed, `stall::disc`).
pub struct FileZones {
    src: esf::Window<std::fs::File>,
    done: VecDeque<(usize, std::io::Result<Vec<u8>>)>,
}

impl FileZones {
    /// The file `name` inside the image at `iso_path`.
    pub fn open(iso_path: &std::path::Path, name: &str) -> Option<Self> {
        let e = iso::Iso::open(iso_path).ok()?.find(name)?.clone();
        let f = std::fs::File::open(iso_path).ok()?;
        Some(FileZones { src: esf::Window::new(f, e.lba as u64 * iso::SECTOR, e.size as u64), done: VecDeque::new() })
    }
}

impl ZoneSource for FileZones {
    fn request(&mut self, zi: usize, off: u64, len: usize) {
        let mut b = vec![0u8; len];
        let r = crate::stall::disc::timed(crate::stall::disc::Read::Zone, || esf::ReadAt::read_at(&mut self.src, off, &mut b)).map(|_| b);
        self.done.push_back((zi, r));
    }
    fn poll(&mut self) -> Option<(usize, std::io::Result<Vec<u8>>)> {
        self.done.pop_front()
    }
}

/// Makes the zone source for a deferred world (file index in the library).
pub type Factory = Box<dyn Fn(&Library, usize) -> Option<Box<dyn ZoneSource>>>;

thread_local! {
    static FACTORY: RefCell<Option<Factory>> = const { RefCell::new(None) };
}

/// Makes `f` the way a deferred world's zones are read (the browser's
/// fetches); without one they come off the disc image ([`FileZones`]).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn set_source_factory(f: Factory) {
    FACTORY.with(|c| *c.borrow_mut() = Some(f));
}

/// The source for the world in file `file`, if it was opened deferred; with
/// a twin (`esf::Library::set_twin`), its zones read with the twin's
/// (`Twinned`).
pub fn source_for(lib: &Library, file: usize) -> Option<Box<dyn ZoneSource>> {
    let main = own_source(lib, file)?;
    let Some(t) = lib.twin_of(file) else { return Some(main) };
    let Some(twin) = own_source(lib, t) else { return Some(main) };
    let ranges = (0..lib.zone_count(t)).map(|zi| lib.zone_range(t, zi)).collect();
    Some(Box::new(Twinned::new(main, twin, ranges)))
}

/// `source_for` without the twin.
fn own_source(lib: &Library, file: usize) -> Option<Box<dyn ZoneSource>> {
    if !lib.is_deferred(file) {
        return None;
    }
    if let Some(s) = FACTORY.with(|c| c.borrow().as_ref().map(|f| f(lib, file))) {
        return s;
    }
    // off the disc the world came off (`era`: Tunaria off the 2002 disc)
    let name = lib.files[file].0.clone();
    #[cfg(not(target_arch = "wasm32"))]
    let image = crate::era::image_of(lib.era_of(file))?;
    #[cfg(target_arch = "wasm32")]
    let image = crate::disc_path(None);
    FileZones::open(std::path::Path::new(&image), &name).map(|z| Box::new(z) as Box<dyn ZoneSource>)
}

/// A world file opened for a world change, with the zones the walk needs
/// first at the arrival read (each zone's index and its record's bytes, for
/// [`install`]), and the disc it came off (`era::world_era`).
pub struct Opened {
    pub container: Container,
    pub zones: Vec<(usize, Vec<u8>)>,
    pub era: esf::Era,
    /// The same world off the other disc, its zones left there, for what
    /// they add (`esf::Library::set_twin`): Frontiers' Tunaria under the
    /// 2002 disc's. Its era is the other one.
    pub twin: Option<Container>,
    /// The twin's zones of the same indices as `zones`, read with them (a
    /// page's; natively none: they come with the zones' own reads).
    pub twin_zones: Vec<(usize, Vec<u8>)>,
}

/// The twin a world off the disc of `era` is given (`Opened::twin`): the 2002
/// disc's Tunaria gets Frontiers', for its effect volumes and streamed audio.
pub fn wants_twin(era: esf::Era) -> bool {
    era == esf::Era::Y2002
}

/// A world change's reads: here at once natively, a frame or more later in a
/// page.
pub type Arrival = std::rc::Rc<RefCell<Option<Result<Opened, String>>>>;

/// How a page opens a world for a world change: the file's name and the
/// arrival's x and z.
pub type Opener = Box<dyn Fn(&str, f32, f32) -> Arrival>;

thread_local! {
    static OPENER: RefCell<Option<Opener>> = const { RefCell::new(None) };
}

/// Makes `f` the way a world change opens its world (a page's reads).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn set_world_opener(f: Opener) {
    OPENER.with(|c| *c.borrow_mut() = Some(f));
}

/// Begins a world change's reads (`VIClient::EnterWorld`'s `LoadWorld`): the
/// world file `name`, off the disc of the art drawn when that disc has it
/// (`era::world_era`: Tunaria off the 2002 disc), and, in a page, the zones
/// round the arrival at (x, z) ([`fill_set`]). Natively it is all here at
/// once, the arrival's zone read with the walk ([`load_zone_now`]).
pub fn open_world(name: &str, x: f32, z: f32) -> Arrival {
    if let Some(a) = OPENER.with(|c| c.borrow().as_ref().map(|f| f(name, x, z))) {
        return a;
    }
    let era = crate::era::world_era(name);
    #[cfg(not(target_arch = "wasm32"))]
    let image = crate::era::image_of(era).unwrap_or_else(|| crate::disc_path(None));
    #[cfg(target_arch = "wasm32")]
    let image = crate::disc_path(None);
    let frontiers = std::path::PathBuf::from(crate::disc_path(None));
    // natively read here, on the game's thread: timed (`stall::disc`)
    let open = |p: &std::path::Path| crate::stall::disc::timed(crate::stall::disc::Read::World, || open_file(p, name));
    // the 2002 disc's world that does not open is Frontiers' (the game plays on)
    let (era, opened) = match open(std::path::Path::new(&image)) {
        Err(e) if era == esf::Era::Y2002 => {
            crate::say(&format!("art: the 2002 disc's {name} did not open ({e}): Frontiers' world"));
            (esf::Era::Frontiers, open(&frontiers))
        }
        r => (era, r),
    };
    let twin = wants_twin(era).then(|| open(&frontiers).ok()).flatten();
    let r = opened.map(|container| Opened { container, zones: Vec::new(), era, twin, twin_zones: Vec::new() }).map_err(|e| e.to_string());
    std::rc::Rc::new(RefCell::new(Some(r)))
}

/// Opens `name` off the disc image: a world (its first record 0x8000) with
/// its zones left on the disc, anything else whole. `EQOA_WHOLE_WORLD=1`
/// reads every file whole.
pub fn open_file(iso_path: &std::path::Path, name: &str) -> Result<Container, Box<dyn std::error::Error>> {
    let Some(e) = iso::Iso::open(iso_path)?.find(name).cloned() else {
        // a world a server's patch adds that no disc carries (`s2`:
        // Sandstorm's world 6), out of the server's folder, whole
        if let Some(path) = crate::s2::get().folder_file(name) {
            return Ok(Container::open(std::fs::read(path)?)?);
        }
        return Err(format!("{name}: not on the disc").into());
    };
    let mut src = esf::Window::new(std::fs::File::open(iso_path)?, e.lba as u64 * iso::SECTOR, e.size as u64);
    let whole = std::env::var_os("EQOA_WHOLE_WORLD").is_some_and(|v| v != "0");
    if !whole && is_world(&mut src) {
        return Ok(Container::open_deferred(&mut src)?);
    }
    Ok(Container::open(esf::read_all(&mut src)?)?)
}

/// Whether the file is a world: an OBJF whose first record is 0x8000.
pub fn is_world(src: &mut dyn esf::ReadAt) -> bool {
    let mut h = [0u8; 24];
    if src.len() < 36 || src.read_at(0, &mut h).is_err() || &h[..4] != b"FJBO" {
        return false;
    }
    let start = i64::from_le_bytes(h[16..24].try_into().unwrap());
    let mut r = [0u8; 2];
    start >= 0 && src.read_at(start as u64, &mut r).is_ok() && u16::from_le_bytes(r) == 0x8000
}

/// Puts zone `zi`'s bytes (its record, as `esf::Library::zone_range` names
/// it) into the library.
pub fn install(lib: &mut Library, file: usize, zi: usize, bytes: Vec<u8>) -> Result<(), String> {
    let (_, len) = lib.zone_range(file, zi).ok_or_else(|| format!("zone {zi}: not on the disc"))?;
    if bytes.len() != len || bytes.len() < 12 || u16::from_le_bytes([bytes[0], bytes[1]]) != esf::zone::ZONE {
        return Err(format!("zone {zi}: {} bytes that are not its record", bytes.len()));
    }
    lib.add_zone(file, zi, Container::parse_records(bytes)).map(|_| ()).ok_or_else(|| format!("zone {zi}: not added"))
}

/// Each zone's bounds, out of the world base's proxies.
fn zone_bounds(lib: &Library, file: usize) -> Vec<vi::geom::BBox<Ee>> {
    let c = lib.file(file);
    c.roots.first().and_then(|&r| c.child(r, 0x8200)).map_or_else(Vec::new, |base| World::<Ee>::proxies(c, base))
}

/// What a walk at (x, z) needs read before it starts
/// (`VIScene::StreamFillCache`): the zone there and every zone within the
/// inner radius of it (measured flat, the height not known yet), nearest
/// first, that is not in yet: (zone, offset, length) in the world file.
pub fn fill_set(lib: &Library, file: usize, x: f32, z: f32) -> Vec<(usize, u64, usize)> {
    let at = Vect3::<Ee>::of(x, 0.0, z);
    let flat = |b: &vi::geom::BBox<Ee>| {
        let mut b = *b;
        b.min.y = F::new(-1e9);
        b.max.y = F::new(1e9);
        b.distance_squared(&at).0
    };
    let bounds = zone_bounds(lib, file);
    let here = World::<Ee>::zone_at(lib.file(file), x, z);
    let mut want: Vec<(f32, usize)> = (0..bounds.len()).filter(|&k| Some(k) == here || flat(&bounds[k]) < STREAM_INNER * STREAM_INNER).map(|k| (flat(&bounds[k]), k)).collect();
    want.sort_by(|a, b| a.0.total_cmp(&b.0));
    want.into_iter().filter(|&(_, k)| lib.zone(file, k).is_none()).filter_map(|(_, k)| lib.zone_range(file, k).map(|(o, n)| (k, o, n))).collect()
}

/// Whether the zones round `at` are in and drawn, by each zone's bounds and
/// what is known of it: every zone the world file has within the inner
/// radius of `at` measured flat (what [`fill_set`] reads) is `installed` in
/// the collision world, and every installed one within it measured whole
/// (what `World::stream` hands the GPU) is `uploaded`.
pub fn settled_round(at: Vec3, bounds: &[vi::geom::BBox<Ee>], installed: impl Fn(usize) -> bool, uploaded: impl Fn(usize) -> bool, on_disc: impl Fn(usize) -> bool) -> bool {
    let p = Vect3::<Ee>::of(at.x, at.y, at.z);
    let r = STREAM_INNER * STREAM_INNER;
    bounds.iter().enumerate().all(|(k, b)| {
        let mut flat = *b;
        flat.min.y = F::new(-1e9);
        flat.max.y = F::new(1e9);
        let read = flat.distance_squared(&p).0 >= r || installed(k) || !on_disc(k);
        let drawn = b.distance_squared(&p).0 >= r || !installed(k) || uploaded(k);
        read && drawn
    })
}

/// [`fill_set`] read through `src` and installed; the zones asked for that
/// have not come yet (natively none: the disc answers at once), for the
/// streamer to expect ([`Streamer::new`]).
pub fn fill(lib: &mut Library, file: usize, x: f32, z: f32, src: &mut dyn ZoneSource) -> Vec<usize> {
    let want = fill_set(lib, file, x, z);
    for &(zi, off, len) in &want {
        src.request(zi, off, len);
    }
    while let Some((zi, r)) = src.poll() {
        match r.map_err(|e| e.to_string()).and_then(|b| install_with(lib, file, zi, b, src)) {
            Ok(()) => {}
            Err(e) => eprintln!("zone {zi}: {e}"),
        }
    }
    want.iter().filter(|w| lib.zone(file, w.0).is_none()).map(|w| w.0).collect()
}

/// One zone read and installed now, through the world's source (natively:
/// the disc); a world read whole has it already.
pub fn load_zone_now(lib: &mut Library, file: usize, zi: usize) -> Result<(), String> {
    if lib.zone(file, zi).is_some() {
        return Ok(());
    }
    let (off, len) = lib.zone_range(file, zi).ok_or_else(|| format!("no zone {zi}"))?;
    let mut src = source_for(lib, file).ok_or("no source for the world's zones")?;
    src.request(zi, off, len);
    match src.poll() {
        Some((_, Ok(b))) => install_with(lib, file, zi, b, src.as_mut()),
        Some((_, Err(e))) => Err(format!("zone {zi}: {e}")),
        None => Err(format!("zone {zi}: not read yet")),
    }
}

/// `o`'s world in place of the one in file `file`: the file, its twin and
/// what the twin's zones read with it add (`add_twin_zone`). The zones read
/// with it, for the caller to install.
pub fn put_world(lib: &mut Library, file: usize, name: &str, o: Opened) -> Vec<(usize, Vec<u8>)> {
    let Opened { container, zones, era, twin, twin_zones } = o;
    lib.replace_from(file, name, container, era);
    lib.set_twin(file, twin, crate::era::other(era));
    for (zi, b) in twin_zones {
        add_twin_zone(lib, file, zi, b);
    }
    zones
}

/// A world put in place of the one in file `file` under a walk at `at` (F9's
/// world, the viewer's `take_world_art`): the library's file replaced with
/// `o`'s off its disc, the zones its reads brought put in, those round `at`
/// read through its own source (`fill`), and the collision world made of
/// them. With the source and the zones still coming, for its streamer.
pub fn reopen(lib: &mut Library, file: usize, name: &str, o: Opened, at: Vec3) -> Result<(World<Ee>, Option<Box<dyn ZoneSource>>, Vec<usize>), String> {
    let zones = put_world(lib, file, name, o);
    for (zi, b) in zones {
        install(lib, file, zi, b)?;
    }
    let mut source = source_for(lib, file);
    let mut pending = Vec::new();
    if let Some(src) = source.as_mut() {
        pending = fill(lib, file, at.x, at.z, src.as_mut());
    }
    let world = World::load(lib, file).ok_or("no world in the file")?;
    Ok((world, source, pending))
}

struct Loaded {
    /// the zone's parts, then its animated props' models (from `props_at`),
    /// uploaded and drawn as one group
    group: GpuGroup,
    props: AnimatedProps,
    props_at: usize,
}

pub struct Streamer {
    file: usize,
    zones: BTreeMap<usize, Loaded>,
    /// The deferred world's zones' source (`None`: the world was read whole).
    source: Option<Box<dyn ZoneSource>>,
    /// Zones asked for whose bytes have not come yet.
    requested: BTreeSet<usize>,
}

pub struct Gpu<'a> {
    pub renderer: &'a mut Renderer,
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
}

impl Streamer {
    /// A streamer for the world in file `file`, reading its zones through
    /// `source` (`None`: read whole); `pending` are zones already asked of
    /// it (what [`fill`] left in flight).
    pub fn new(file: usize, source: Option<Box<dyn ZoneSource>>, pending: Vec<usize>) -> Self {
        Streamer { file, zones: BTreeMap::new(), source, requested: pending.into_iter().collect() }
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn set_source(&mut self, s: Box<dyn ZoneSource>) {
        self.source = Some(s);
    }

    /// Asks for the zones within [`PREFETCH`] of `at` that are neither in
    /// nor asked for, nearest first.
    pub fn fetch(&mut self, at: Vec3, world: &World<Ee>, lib: &Library) {
        let Some(src) = self.source.as_mut() else { return };
        let p = Vect3::<Ee>::of(at.x, at.y, at.z);
        let mut want: Vec<(f32, usize)> = world
            .zone_bounds
            .iter()
            .enumerate()
            .filter(|&(k, _)| !world.loaded.get(k).copied().unwrap_or(true) && !self.requested.contains(&k))
            .map(|(k, b)| (b.distance_squared(&p).0, k))
            .filter(|&(d, _)| d < PREFETCH * PREFETCH)
            .collect();
        want.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, k) in want {
            if let Some((off, len)) = lib.zone_range(self.file, k) {
                src.request(k, off, len);
                self.requested.insert(k);
            }
        }
    }

    /// Puts the zones whose bytes have come into the library and the
    /// collision world. Returns the first actor they added, if any (for what
    /// keeps state per actor: the particles).
    pub fn install(&mut self, lib: &mut Library, world: &mut World<Ee>) -> Option<usize> {
        let src = self.source.as_mut()?;
        let first = world.actors.len();
        while let Some((zi, r)) = src.poll() {
            self.requested.remove(&zi);
            match r.map_err(|e| e.to_string()).and_then(|b| install_with(lib, self.file, zi, b, src.as_mut())) {
                Ok(()) => {
                    world.add_zone(lib, self.file, zi);
                }
                Err(e) => eprintln!("zone {zi}: {e}"),
            }
        }
        (world.actors.len() > first).then_some(first)
    }

    /// Asks for the zones ahead ([`Streamer::fetch`]), lets go of the zones
    /// past the outer radius of `at` and uploads up to `budget` of those in
    /// and inside the inner one (all, if `None`), nearest first. Returns
    /// whether anything changed.
    pub fn update(&mut self, at: Vec3, world: &mut World<Ee>, lib: &Library, g: Gpu, budget: Option<usize>) -> bool {
        self.fetch(at, world, lib);
        let loaded: BTreeSet<usize> = self.zones.keys().copied().collect();
        let (load, release) = world.stream(&vi::vect::Vect3::of(at.x, at.y, at.z), &loaded);
        for z in &release {
            self.zones.remove(z);
        }
        let load: Vec<usize> = load.into_iter().filter(|&z| world.loaded.get(z).copied().unwrap_or(false)).collect();
        let textures = render::library_textures(lib);
        let n = budget.unwrap_or(usize::MAX).min(load.len());
        for &z in &load[..n] {
            let Some((zf, zn)) = lib.zone(self.file, z) else { continue };
            let s = scene::zones(lib, zf, &[zn], &format!("zone {z}"), true);
            let props = AnimatedProps::new(world, lib, &s.hsprites);
            let prop_mats = props.matrices(world);
            let mut parts: Vec<GroupPart> =
                s.parts.iter().zip(&s.statics).map(|((m, mats), st)| GroupPart { model: m, instances: mats, statics: st, moves: false }).collect();
            let props_at = parts.len();
            parts.extend(
                props.groups.iter().zip(&prop_mats).map(|(group, mats)| GroupPart { model: &group.model, instances: mats, statics: &[], moves: true }),
            );
            let group = g.renderer.upload_group(g.device, g.queue, &parts, &textures);
            self.zones.insert(z, Loaded { group, props, props_at });
        }
        !release.is_empty() || n > 0
    }

    /// Whether the zones round `at` are in and drawn: what the loading screen
    /// waits for (`loading.rs`; `VIScene::StreamFillCache` returns once its
    /// cache is filled there). See [`settled_round`].
    pub fn settled(&self, at: Vec3, world: &World<Ee>, lib: &Library) -> bool {
        let installed = |k: usize| world.loaded.get(k).copied().unwrap_or(false);
        settled_round(at, &world.zone_bounds, installed, |k| self.zones.contains_key(&k), |k| lib.zone_range(self.file, k).is_some())
    }

    /// Poses every loaded zone's props for this frame.
    pub fn animate(&self, world: &mut World<Ee>, r: &Renderer, queue: &wgpu::Queue) {
        for l in self.zones.values() {
            for (k, mats) in l.props.matrices(world).iter().enumerate() {
                r.set_group_instances(queue, &l.group, l.props_at + k, mats);
            }
        }
    }

    /// Each loaded zone's models, as `Renderer::draw` takes them.
    pub fn draws(&self) -> impl Iterator<Item = render::Draw<'_>> {
        self.zones.values().map(|l| render::Draw::Group(&l.group))
    }

    pub fn describe(&self) -> String {
        format!("{} zones streamed in", self.zones.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vi::geom::BBox;

    fn bbox(min: [f32; 3], max: [f32; 3]) -> BBox<Ee> {
        BBox { min: Vect3::of(min[0], min[1], min[2]), max: Vect3::of(max[0], max[1], max[2]) }
    }

    /// The loading screen's test: the zone at the arrival and its
    /// neighbours within 900 (measured flat) in, those within 900 measured
    /// whole drawn too; what is past them, or not in the file, does not
    /// count. A zone near on the map but far below or above must be in,
    /// though it need not be drawn yet.
    #[test]
    fn settled_is_every_zone_round_the_arrival_in_and_drawn() {
        let at = Vec3::new(500.0, 0.0, 500.0);
        let zones = [
            bbox([0.0, -50.0, 0.0], [1000.0, 50.0, 1000.0]),       // 0: under the arrival
            bbox([1000.0, -50.0, 0.0], [2000.0, 50.0, 1000.0]),    // 1: 500 off
            bbox([5000.0, -50.0, 0.0], [6000.0, 50.0, 1000.0]),    // 2: far off
            bbox([0.0, 2000.0, 1000.0], [1000.0, 2100.0, 2000.0]), // 3: 500 off on the map, 2000 above
        ];
        let all = |_: usize| true;
        let none = |_: usize| false;
        assert!(settled_round(at, &zones, all, all, all));
        let not = |n: usize| move |k: usize| k != n;
        assert!(!settled_round(at, &zones, not(1), all, all), "a neighbour not read");
        assert!(!settled_round(at, &zones, all, not(1), all), "a neighbour read, not drawn");
        assert!(settled_round(at, &zones, not(2), not(2), all), "a far zone does not count");
        assert!(!settled_round(at, &zones, not(3), all, all), "near on the map: read first");
        assert!(settled_round(at, &zones, all, not(3), all), "but far for the GPU");
        assert!(settled_round(at, &zones, not(1), all, not(1)), "a zone the file lacks never comes");
        assert!(!settled_round(at, &zones, none, none, all), "nothing in");
        // control: measured whole, zone 3 (2000 above) would not be waited for
        let p = Vect3::<Ee>::of(at.x, at.y, at.z);
        let whole = |inst: &dyn Fn(usize) -> bool| zones.iter().enumerate().all(|(k, b)| b.distance_squared(&p).0 >= STREAM_INNER * STREAM_INNER || inst(k));
        assert!(whole(&not(3)));
    }

    /// F9's world (`reopen`): Tunaria off the 2002 disc put under a walk
    /// stood at the Freeport spawn on Frontiers', the walk control kept. It
    /// stands on through a second of frames at the same place, its zones
    /// read off the 2002 disc; the control, the same world with no zone read
    /// round the walk, gives it nothing to stand on and it falls.
    #[test]
    fn a_world_swapped_under_the_walk_keeps_it_standing() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let frontiers = root.join("run/eqoa.iso");
        let y2002 = std::env::var_os("EQOA_2002_ISO").map(std::path::PathBuf::from).or_else(|| {
            std::fs::read_dir(root.join("../decomp-builds/discs/eqoa-usa")).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("iso")))
        });
        let (true, Some(y2002)) = (frontiers.exists(), y2002) else {
            eprintln!("SKIP: no Frontiers disc or no 2002 disc (set EQOA_2002_ISO)");
            return;
        };
        crate::settings::set_disc_2002(y2002.to_str().unwrap()).unwrap();
        let (x, z) = (25393.7f32, 15749.5f32);
        let walked = |zones: bool| {
            let mut lib = Library::new();
            let f = lib.add("TUNARIA.ESF", open_file(&frontiers, "TUNARIA.ESF").unwrap());
            fill(&mut lib, f, x, z, &mut FileZones::open(&frontiers, "TUNARIA.ESF").unwrap());
            let mut w = crate::walker::Walker::arrive(World::load(&lib, f).unwrap(), crate::walker::Spawn::ground(x, z, 0.0));
            for k in 1..=10 {
                w.frame(Default::default(), k * 33);
            }
            let before = w.cam.walk.phys.position;
            let o = Opened { container: open_file(&y2002, "TUNARIA.ESF").unwrap(), zones: Vec::new(), era: esf::Era::Y2002, twin: None, twin_zones: Vec::new() };
            let at = Vec3::from(before.to_array());
            let world = if zones {
                let (world, source, pending) = reopen(&mut lib, f, "TUNARIA.ESF", o, at).unwrap();
                assert!(source.is_some() && pending.is_empty(), "read off the 2002 disc at once");
                world
            } else {
                lib.replace_from(f, "TUNARIA.ESF", o.container, o.era);
                World::load(&lib, f).unwrap()
            };
            assert_eq!(lib.era_of(f), esf::Era::Y2002);
            let time = w.scene.time;
            w.scene = world;
            w.scene.time = time;
            for k in 11..=40 {
                w.frame(Default::default(), k * 33);
            }
            (before, w.cam.walk.phys.position, w.scene.loaded.iter().filter(|&&l| l).count())
        };
        let (before, after, n) = walked(true);
        eprintln!("before {:?}, after {:?}, {n} zones in off the 2002 disc", before.to_array(), after.to_array());
        assert!(n >= 4, "the zones round the walk: {n}");
        assert_eq!(before.to_array(), after.to_array(), "stood at the same place");
        let (before, after, n) = walked(false);
        assert_eq!(n, 0);
        assert!(after.y.0 < before.y.0 - 0.1, "the control falls: {:?} to {:?}", before.to_array(), after.to_array());
    }

    /// The 2002 disc's Tunaria with Frontiers' as its twin: the zones round
    /// the Freeport spawn place every effect volume and streamed-audio region
    /// Frontiers' own zones do, at the same places; the control, the 2002
    /// Tunaria with no twin, places none.
    #[test]
    fn the_2002_tunaria_keeps_frontiers_volumes_and_streams() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let frontiers = root.join("run/eqoa.iso");
        let y2002 = std::env::var_os("EQOA_2002_ISO").map(std::path::PathBuf::from).or_else(|| {
            std::fs::read_dir(root.join("../decomp-builds/discs/eqoa-usa")).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("iso")))
        });
        let (true, Some(y2002)) = (frontiers.exists(), y2002) else {
            eprintln!("SKIP: no Frontiers disc or no 2002 disc (set EQOA_2002_ISO)");
            return;
        };
        crate::settings::set_disc_2002(y2002.to_str().unwrap()).unwrap();
        crate::discs::take_for_tests(&frontiers);
        let (x, z) = (25393.7f32, 15749.5f32);
        // each volume's and stream's place, sorted
        let placed = |lib: &Library, w: &World<Ee>| {
            let mut out: Vec<(u16, [u32; 3])> = w
                .actors
                .iter()
                .filter_map(|a| {
                    let r = w.sprite_refs[a.sprite?];
                    let ty = lib.file(r.file).node(r.node).ty;
                    TWIN_ADDS.contains(&ty).then(|| (ty, [a.location.x.0.to_bits(), a.location.y.0.to_bits(), a.location.z.0.to_bits()]))
                })
                .collect();
            out.sort_unstable();
            out
        };
        let world = |era: esf::Era, twin: bool| {
            let image = if era == esf::Era::Y2002 { &y2002 } else { &frontiers };
            let mut lib = Library::new();
            let f = lib.add_from("TUNARIA.ESF", open_file(image, "TUNARIA.ESF").unwrap(), era);
            if twin {
                lib.set_twin(f, Some(open_file(&frontiers, "TUNARIA.ESF").unwrap()), esf::Era::Frontiers);
            }
            let mut src = source_for(&lib, f).unwrap();
            let pending = fill(&mut lib, f, x, z, src.as_mut());
            assert!(pending.is_empty(), "read at once");
            let w = World::load(&lib, f).unwrap();
            (placed(&lib, &w), w.loaded.iter().filter(|&&l| l).count())
        };
        let (theirs, n) = world(esf::Era::Frontiers, false);
        let (ours, m) = world(esf::Era::Y2002, true);
        eprintln!("{} volumes and streams in Frontiers' {n} zones, {} in the 2002 disc's {m} with the twin", theirs.len(), ours.len());
        assert!(!theirs.is_empty() && theirs.iter().any(|p| p.0 == 0x2E00) && theirs.iter().any(|p| p.0 == crate::fx::RECORD));
        assert_eq!(n, m);
        assert_eq!(ours, theirs, "every one, at its place");
        let (none, _) = world(esf::Era::Y2002, false);
        assert!(none.is_empty(), "the control: the 2002 disc's own zones place none");
        // F9 under a walk (`reopen`): the same, the twin put with the world
        let mut lib = Library::new();
        let f = lib.add("TUNARIA.ESF", open_file(&frontiers, "TUNARIA.ESF").unwrap());
        let o = Opened { container: open_file(&y2002, "TUNARIA.ESF").unwrap(), zones: Vec::new(), era: esf::Era::Y2002, twin: Some(open_file(&frontiers, "TUNARIA.ESF").unwrap()), twin_zones: Vec::new() };
        let (w, _, _) = reopen(&mut lib, f, "TUNARIA.ESF", o, Vec3::new(x, 54.0, z)).unwrap();
        assert_eq!(placed(&lib, &w), theirs, "swapped in, every one at its place");
        // and back: Frontiers' own, the twin gone
        let o = Opened { container: open_file(&frontiers, "TUNARIA.ESF").unwrap(), zones: Vec::new(), era: esf::Era::Frontiers, twin: None, twin_zones: Vec::new() };
        let (w, _, _) = reopen(&mut lib, f, "TUNARIA.ESF", o, Vec3::new(x, 54.0, z)).unwrap();
        assert_eq!((placed(&lib, &w), lib.twin_of(f)), (theirs, None));
    }

    /// A 2002 disc kept but gone (moved, its drive unplugged) is none: the
    /// art in effect is Frontiers' and so is the world; one that is there but
    /// does not open as a disc gives Frontiers' world too. The controls: a
    /// disc that is there is at hand, and the art chosen stays 2002's.
    #[test]
    fn a_2002_disc_gone_plays_frontiers() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let frontiers = root.join("run/eqoa.iso");
        if !frontiers.exists() || std::env::var_os("EQOA_2002_ISO").is_some() {
            eprintln!("SKIP: no Frontiers disc, or EQOA_2002_ISO names a disc");
            return;
        }
        crate::discs::take_for_tests(&frontiers);
        crate::settings::set_art(esf::Era::Y2002).unwrap();
        crate::settings::set_disc_2002("/nowhere/EverQuest - Online Adventures (USA).iso").unwrap();
        assert_eq!(crate::era::disc(), None, "a kept image that is not there");
        assert!(!crate::era::available());
        assert_eq!((crate::settings::art(), crate::era::current()), (esf::Era::Y2002, esf::Era::Frontiers));
        assert_eq!(crate::era::world_era("TUNARIA.ESF"), esf::Era::Frontiers);
        // there, but not a disc image: chosen and at hand, its world Frontiers'
        let not_a_disc = root.join("Cargo.toml");
        crate::settings::set_disc_2002(not_a_disc.to_str().unwrap()).unwrap();
        assert!(crate::era::available(), "the control: a file that is there is at hand");
        assert_eq!(crate::era::world_era("TUNARIA.ESF"), esf::Era::Y2002);
        let o = open_world("TUNARIA.ESF", 25393.7, 15749.5).borrow_mut().take().unwrap().unwrap();
        assert_eq!((o.era, o.twin.is_some()), (esf::Era::Frontiers, false), "Frontiers' world, no twin");
        assert_eq!(o.container.zones().len(), 175);
    }
}
