//! The map's pictures, made zone by zone while the game plays, a few
//! milliseconds a frame (`Builder::step`'s budget): each zone's record read
//! as the zone streaming reads it (`stream::ZoneSource`), parsed on its own
//! (every palette and texture its terrain names is in it), its terrain built
//! a few patches at a time (`esf::Container::build_terrain`) and uploaded to
//! a `Renderer` of the map's own (its textures go with it), then drawn from
//! straight above into a tile (`render::map::TileShot`) with the light
//! pinned and no fog, and read back. The zone's record is let go once its
//! terrain is built, so no more of the world than one zone is ever held.
//! The tile and its overview cell go to the cache (`store`), and to the map
//! for the screen. Zones the walk has read already come first (they are in
//! the library), then the rest nearest the place asked for (the player, or
//! the map's middle while it is open).

use super::cells::{Cells, OVERVIEW, TILE};
use super::picture;
use super::store::{Got, Store};
use crate::stream::ZoneSource;
use esf::{Container, Library, NodeId};
use glam::Vec3;
use render::map::TileShot;
use render::{GpuModel, Renderer, wgpu};
use std::collections::{BTreeSet, HashMap, HashSet};
use web_time::{Duration, Instant};

/// A zone's picture, as far as the map has it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zone {
    /// whether the cache has it is not known yet
    Unknown,
    /// the cache has it: its overview cell is to be read
    Kept,
    /// its overview cell asked of the cache
    Asked,
    /// in the overview
    Shown,
    /// to be drawn
    Missing,
    /// nothing to draw (no terrain, or its record would not read)
    Failed,
}

/// Terrain patches built and uploaded a step.
const PATCHES_A_STEP: usize = 8;

/// Overview cells asked of the cache at once.
const ASKED_AT_ONCE: usize = 8;

/// What the tile is drawn with: the light pinned, from the north-west and
/// above (the light travels south-east and down), so slopes read as a
/// shaded relief does; no fog; the clear colour where a cell has no terrain.
pub const SUN_DIR: [f32; 3] = [0.5, -1.0, 0.5];
pub const SUN: f32 = 0.55;
pub const AMBIENT: f32 = 0.35;
pub const VOID: [f32; 3] = [0.09, 0.11, 0.15];

/// The tile's view: the cell `rect` (x0, z0, x1, z1) from straight above,
/// north (-z) up, east (+x) right, everything from y -10,000 to 10,000.
pub fn tile_view(rect: [f32; 4]) -> render::View {
    let c = Vec3::new((rect[0] + rect[2]) / 2.0, 0.0, (rect[1] + rect[3]) / 2.0);
    let (hw, hh) = ((rect[2] - rect[0]) / 2.0, (rect[3] - rect[1]) / 2.0);
    let eye = c + Vec3::Y * 10000.0;
    let view = glam::camera::rh::view::look_at_mat4(eye, c, -Vec3::Z);
    let proj = glam::camera::rh::proj::directx::orthographic(-hw, hw, -hh, hh, 1.0, 20000.0);
    render::View {
        view_proj: proj * view,
        eye,
        sun_dir: Vec3::from(SUN_DIR),
        sun: Vec3::splat(SUN),
        ambient: Vec3::splat(AMBIENT),
        fog: None,
        point: None,
        clear: Vec3::from(VOID),
        sky_view_proj: None,
    }
}

/// The map's GPU, for a step.
pub struct Gpu<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
}

/// A zone's terrain being made: built a few patches at a time, each
/// piece's textures decoded one a step before it is uploaded.
struct Making {
    zi: usize,
    /// the zone's record (none: the library has the zone) and its node
    zone: Option<Container>,
    node: NodeId,
    patches: Vec<NodeId>,
    pre: Vec<[f32; 3]>,
    /// the patches built so far
    next: usize,
    /// the zone's placements by sprite, once its terrain is in, and how many
    /// are built
    sprites: Option<Vec<Placed>>,
    sprite_next: usize,
    /// a piece built and not uploaded yet, and its textures decoded so far
    piece: Option<Placed>,
    images: HashMap<i32, Option<esf::Image>>,
    parts: Vec<GpuModel>,
}

/// A model (a piece of terrain, a sprite) with where it stands: each
/// instance's matrix and static light.
struct Placed {
    sprite: i32,
    model: Option<esf::Model>,
    instances: Vec<[[f32; 4]; 4]>,
    statics: Vec<[f32; 4]>,
}

impl Making {
    fn new(zi: usize, zone: Option<Container>, node: NodeId, c: &Container) -> Box<Making> {
        Box::new(Making { zi, node, patches: c.terrain_patches(node), pre: c.pre_translations(node), zone, next: 0, sprites: None, sprite_next: 0, piece: None, images: HashMap::new(), parts: Vec::new() })
    }
}

/// A zone's placements by sprite, as the scene places them
/// (`VIScene::UpdateActor`'s `Transform(hpr, scale, location)`, the zone
/// actor's colour its static light), leaving out what is animated (an
/// HSprite: posed props) and what names no sprite in the zone.
fn placements(c: &Container, zone: NodeId) -> Vec<Placed> {
    let mut by: std::collections::BTreeMap<i32, Placed> = std::collections::BTreeMap::new();
    for a in c.actors(zone) {
        let Some(n) = c.lookup(a.sprite) else { continue };
        if c.node(n).ty == esf::types::HSPRITE {
            continue;
        }
        let f = vi::fpu::F::<vi::fpu::Ee>::new;
        let v = |x: [f32; 3]| vi::vect::Vect3::new(f(x[0]), f(x[1]), f(x[2]));
        let m = vi::geom::Matrix44::transform(&v(a.rot), f(a.scale), &v(a.pos)).to_cols();
        let p = by.entry(a.sprite).or_insert_with(|| Placed { sprite: a.sprite, model: None, instances: Vec::new(), statics: Vec::new() });
        p.instances.push(m);
        p.statics.push(a.color.map(|c| c as f32 / 255.0));
    }
    by.into_values().collect()
}

/// What the builder is doing.
enum Job {
    Idle,
    /// a zone's record asked of the source
    Read { zi: usize },
    /// a zone's record asked of the source for what it carries (`peek`), not
    /// to draw
    Peek { zi: usize },
    Make(Box<Making>),
    /// drawn, the picture coming back
    Shot { zi: usize },
    /// the picture back: its overview cell (`cell`: made), then its PNG for
    /// the cache a few rows a step, then the zone is on the map
    Keep { zi: usize, tile: Vec<u8>, cell: bool, png: Option<Box<picture::RowEncoder>> },
}

/// Rows of a tile's PNG made a step.
const ROWS_A_STEP: u32 = 32;

/// What a build has cost, for the log and the measurements.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    /// zones drawn here, and overview cells read from the cache
    pub drawn: usize,
    pub kept: usize,
    /// time spent in `step`, and the longest step
    pub busy: Duration,
    pub worst: Duration,
    /// wall time from the first step to the last zone shown
    pub took: Option<Duration>,
}

pub struct Builder {
    /// the world's file in the library
    pub world: usize,
    pub cells: Cells,
    pub zones: Vec<Zone>,
    store: Option<Box<dyn Store>>,
    listed: bool,
    source: Option<Box<dyn ZoneSource>>,
    job: Job,
    renderer: Option<Renderer>,
    /// the textures the builder's renderer has (decoded once)
    uploaded: HashSet<i32>,
    shot: Option<TileShot>,
    /// overview cells to put on the screen: (zone, `OVERVIEW` across, RGBA)
    pub cells_out: Vec<(usize, Vec<u8>)>,
    /// tiles to put on the screen: (zone, `TILE` across, RGBA)
    pub tiles_out: Vec<(usize, Vec<u8>)>,
    /// tiles asked of the cache and not come yet
    tiles_asked: BTreeSet<usize>,
    started: Option<Instant>,
    pub stats: Stats,
    /// `trace()`, asked once
    trace: bool,
    /// the stable sign's horseshoe (`markers::ICON`) out of a zone read here,
    /// while the map wants it (`want_icon`); `peek` a zone to read for it
    pub icon: Option<esf::Image>,
    want_icon: bool,
    peek: Option<usize>,
}

impl Builder {
    /// The builder for the world in file `world` of the library, its zones'
    /// bounds `bounds`, keeping its pictures in `store` (none: nothing kept,
    /// every zone drawn) and reading zones through `source`.
    pub fn new(world: usize, bounds: &[[f32; 4]], mut store: Option<Box<dyn Store>>, source: Option<Box<dyn ZoneSource>>) -> Self {
        let cells = Cells::new(bounds);
        let n = bounds.len();
        let listed = store.is_none();
        if let Some(s) = store.as_mut() {
            s.list();
        }
        let zones = (0..n).map(|z| if cells.cell_of[z].is_none() { Zone::Failed } else if listed { Zone::Missing } else { Zone::Unknown }).collect();
        Builder { world, cells, zones, store, listed, source, job: Job::Idle, renderer: None, uploaded: HashSet::new(), shot: None, cells_out: Vec::new(), tiles_out: Vec::new(), tiles_asked: BTreeSet::new(), started: None, stats: Stats::default(), trace: trace(), icon: None, want_icon: false, peek: None }
    }

    /// Whether the zones read from now on are looked through for the stable
    /// sign's horseshoe (`icon`).
    pub fn want_icon(&mut self, on: bool) {
        self.want_icon = on;
    }

    /// Zone `zi` read once more, for what it carries (the horseshoe), not
    /// drawn: once the build is idle. Whether it can be (a source to read it).
    pub fn peek(&mut self, zi: usize) -> bool {
        if self.source.is_none() || zi >= self.zones.len() {
            return false;
        }
        self.peek = Some(zi);
        true
    }

    /// The horseshoe out of `c`, a zone's records, while it is wanted.
    pub(super) fn look_for_icon(&mut self, c: &Container) {
        if !self.want_icon || self.icon.is_some() {
            return;
        }
        if let Some(im) = c.surfaces.iter().find(|s| s.dict_id == super::markers::ICON).and_then(|s| s.decode(&c.data)) {
            self.icon = Some(im);
        }
    }

    /// Whether every zone is on the map (or has nothing to draw).
    pub fn done(&self) -> bool {
        self.zones.iter().all(|z| matches!(z, Zone::Shown | Zone::Failed))
    }

    /// How many zones are on the map, of how many.
    pub fn progress(&self) -> (usize, usize) {
        (self.zones.iter().filter(|z| **z == Zone::Shown).count(), self.zones.iter().filter(|z| **z != Zone::Failed).count())
    }

    /// Asks the cache for zone `zi`'s tile (it comes out in `tiles_out`);
    /// false when it cannot be had there.
    pub fn want_tile(&mut self, zi: usize) -> bool {
        if self.zones.get(zi) != Some(&Zone::Shown) || self.tiles_asked.contains(&zi) {
            return self.tiles_asked.contains(&zi);
        }
        let Some(s) = self.store.as_mut() else { return false };
        s.get(&tile_name(zi));
        self.tiles_asked.insert(zi);
        true
    }

    /// One frame's work, for at most `budget` (a piece of it at least): the
    /// cache's answers, then the next zone's reading, building and drawing,
    /// as far as time allows.
    /// `focus` (x, z) is where the next zone is chosen nearest to.
    pub fn step(&mut self, lib: &Library, g: &Gpu, focus: [f32; 2], budget: Duration) {
        if self.done() && self.tiles_asked.is_empty() && matches!(self.job, Job::Idle) && self.peek.is_none() {
            return;
        }
        let t0 = Instant::now();
        self.started.get_or_insert(t0);
        self.take_answers(t0, budget);
        self.ask_cells();
        // a piece at least, then pieces while the budget lasts
        let trace = self.trace;
        loop {
            let (t, what) = (Instant::now(), self.job_name());
            let more = self.advance(lib, g, focus);
            if trace && t.elapsed() > Duration::from_millis(2) {
                crate::say(&format!("map: a {what} step took {:.1} ms", t.elapsed().as_secs_f64() * 1e3));
            }
            if !more || t0.elapsed() >= budget {
                break;
            }
        }
        let spent = t0.elapsed();
        self.stats.busy += spent;
        self.stats.worst = self.stats.worst.max(spent);
        if self.stats.took.is_none() && self.done() {
            self.stats.took = self.started.map(|s| s.elapsed());
        }
    }

    /// What the job is, for the trace (`trace`).
    fn job_name(&self) -> &'static str {
        match &self.job {
            Job::Idle => "choosing",
            Job::Read { .. } => "reading",
            Job::Peek { .. } => "peeking",
            Job::Make(m) if m.piece.is_none() => "building",
            Job::Make(_) => "texture or upload",
            Job::Shot { .. } => "picture",
            Job::Keep { .. } => "keeping",
        }
    }

    /// The cache's answers, decoded, while time is left.
    fn take_answers(&mut self, t0: Instant, budget: Duration) {
        while t0.elapsed() < budget {
            let t = Instant::now();
            let Some(got) = self.store.as_mut().and_then(|s| s.poll()) else { break };
            if self.trace && t.elapsed() > Duration::from_millis(2) {
                crate::say(&format!("map: the cache's answer took {:.1} ms", t.elapsed().as_secs_f64() * 1e3));
            }
            match got {
                Got::List(names) => {
                    self.listed = true;
                    let kept: BTreeSet<&str> = names.iter().map(String::as_str).collect();
                    for (zi, z) in self.zones.iter_mut().enumerate() {
                        if *z == Zone::Unknown {
                            // a tile is written after its cell: both, or drawn again
                            let both = kept.contains(tile_name(zi).as_str()) && kept.contains(cell_name(zi).as_str());
                            *z = if both { Zone::Kept } else { Zone::Missing };
                        }
                    }
                }
                Got::File(name, bytes) => {
                    let Some(zi) = name.strip_suffix(".png").and_then(|n| n.get(1..)).and_then(|n| n.parse::<usize>().ok()) else { continue };
                    if zi >= self.zones.len() {
                        continue;
                    }
                    if name.starts_with('c') {
                        match bytes.as_deref().and_then(|b| picture::decode(b, OVERVIEW, OVERVIEW)) {
                            Some(px) => {
                                self.cells_out.push((zi, px));
                                self.zones[zi] = Zone::Shown;
                                self.stats.kept += 1;
                            }
                            // gone or spoilt: drawn again
                            None => self.zones[zi] = Zone::Missing,
                        }
                    } else if name.starts_with('t') {
                        self.tiles_asked.remove(&zi);
                        if let Some(px) = bytes.as_deref().and_then(|b| picture::decode(b, TILE, TILE)) {
                            self.tiles_out.push((zi, px));
                        }
                    }
                }
            }
        }
    }

    /// The overview cells the cache has, asked for a few at a time.
    fn ask_cells(&mut self) {
        let out = self.zones.iter().filter(|z| **z == Zone::Asked).count();
        let Some(s) = self.store.as_mut() else { return };
        for (zi, z) in self.zones.iter_mut().enumerate().filter(|(_, z)| **z == Zone::Kept).take(ASKED_AT_ONCE.saturating_sub(out)) {
            s.get(&cell_name(zi));
            *z = Zone::Asked;
        }
    }

    /// The next zone to draw: one the library has first, then the nearest
    /// to `focus`.
    fn next_zone(&self, lib: &Library, focus: [f32; 2]) -> Option<usize> {
        let d = |zi: usize| {
            let r = self.cells.rect(self.cells.cell_of[zi].unwrap_or(0));
            let (dx, dz) = (focus[0] - (r[0] + r[2]) / 2.0, focus[1] - (r[1] + r[3]) / 2.0);
            dx * dx + dz * dz
        };
        (0..self.zones.len())
            .filter(|&zi| self.zones[zi] == Zone::Missing)
            .min_by(|&a, &b| (lib.zone(self.world, a).is_none(), d(a)).partial_cmp(&(lib.zone(self.world, b).is_none(), d(b))).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// One piece of the job: a zone chosen and asked for, its record parsed,
    /// a piece of its terrain built, one texture decoded, a piece uploaded,
    /// the tile drawn, the picture taken back with its cell made, a few rows
    /// of its PNG. Whether there is more to do this frame.
    fn advance(&mut self, lib: &Library, g: &Gpu, focus: [f32; 2]) -> bool {
        match std::mem::replace(&mut self.job, Job::Idle) {
            Job::Idle => {
                if let Some(zi) = self.peek.take() {
                    if let Some((zf, _)) = lib.zone(self.world, zi) {
                        self.look_for_icon(lib.file(zf));
                        return true;
                    }
                    if let (Some(src), Some((off, len))) = (self.source.as_mut(), lib.zone_range(self.world, zi)) {
                        src.request(zi, off, len);
                        self.job = Job::Peek { zi };
                    }
                    return true;
                }
                let Some(zi) = self.next_zone(lib, focus) else { return false };
                if let Some((zf, z)) = lib.zone(self.world, zi) {
                    self.look_for_icon(lib.file(zf));
                    self.job = Job::Make(Making::new(zi, None, z, lib.file(zf)));
                    return true;
                }
                match (self.source.as_mut(), lib.zone_range(self.world, zi)) {
                    (Some(src), Some((off, len))) => {
                        src.request(zi, off, len);
                        self.job = Job::Read { zi };
                    }
                    _ => self.zones[zi] = Zone::Failed,
                }
                true
            }
            Job::Read { zi } => {
                let Some(src) = self.source.as_mut() else { return false };
                let Some((got, r)) = src.poll() else {
                    self.job = Job::Read { zi };
                    return false;
                };
                if got != zi {
                    self.job = Job::Read { zi };
                    return true;
                }
                match r {
                    Ok(bytes) if bytes.len() >= 12 && u16::from_le_bytes([bytes[0], bytes[1]]) == esf::zone::ZONE => {
                        let c = Container::parse_records(bytes);
                        self.look_for_icon(&c);
                        match c.roots.first().copied() {
                            Some(z) => {
                                let m = Making::new(zi, None, z, &c);
                                self.job = Job::Make(Box::new(Making { zone: Some(c), ..*m }));
                            }
                            None => self.zones[zi] = Zone::Failed,
                        }
                    }
                    Ok(_) | Err(_) => {
                        crate::say(&format!("map: zone {zi} would not read"));
                        self.zones[zi] = Zone::Failed;
                    }
                }
                true
            }
            Job::Peek { zi } => {
                let Some(src) = self.source.as_mut() else { return false };
                let Some((got, r)) = src.poll() else {
                    self.job = Job::Peek { zi };
                    return false;
                };
                if got != zi {
                    self.job = Job::Peek { zi };
                    return true;
                }
                match r {
                    Ok(bytes) if bytes.len() >= 12 && u16::from_le_bytes([bytes[0], bytes[1]]) == esf::zone::ZONE => {
                        let c = Container::parse_records(bytes);
                        self.look_for_icon(&c);
                    }
                    _ => crate::say(&format!("map: zone {zi} would not read for what it carries")),
                }
                true
            }
            Job::Make(mut m) => {
                if self.make(lib, g, &mut m) {
                    self.job = Job::Make(m);
                    return true;
                }
                let zi = m.zi;
                // the record let go: no more of the world than a zone is held
                drop(m.zone.take());
                if m.parts.is_empty() {
                    self.zones[zi] = Zone::Failed;
                    return true;
                }
                let (Some(r), Some(cell)) = (self.renderer.as_mut(), self.cells.cell_of[zi]) else { return true };
                let shot = self.shot.get_or_insert_with(|| TileShot::new(g.device, TILE, TILE));
                let items: Vec<render::Draw> = m.parts.iter().map(render::Draw::Model).collect();
                shot.start(g.device, g.queue, r, &tile_view(self.cells.rect(cell)), &items);
                // a zone's textures are its own: let go with its models, not kept
                // for the rest of the world (the GPU's memory)
                r.forget_textures();
                self.uploaded.clear();
                self.job = Job::Shot { zi };
                true
            }
            Job::Shot { zi } => {
                let Some(shot) = self.shot.as_ref() else { return false };
                match shot.ready(g.device) {
                    None => {
                        self.job = Job::Shot { zi };
                        false
                    }
                    Some(false) => {
                        crate::say(&format!("map: zone {zi}'s picture did not come back"));
                        self.zones[zi] = Zone::Failed;
                        true
                    }
                    Some(true) => {
                        self.job = Job::Keep { zi, tile: shot.take(), cell: false, png: None };
                        true
                    }
                }
            }
            Job::Keep { zi, tile, cell: false, .. } => {
                // the overview's cell, and the cache's copy of it
                let (_, _, cell) = picture::quarter(TILE, TILE, &tile);
                if let Some(s) = self.store.as_mut() {
                    s.put(&cell_name(zi), picture::encode(OVERVIEW, OVERVIEW, &cell));
                }
                self.cells_out.push((zi, cell));
                let png = self.store.as_ref().and_then(|_| picture::RowEncoder::new(TILE, TILE)).map(Box::new);
                self.job = Job::Keep { zi, tile, cell: true, png };
                true
            }
            Job::Keep { zi, tile, cell: true, mut png } => {
                if let Some(e) = png.as_mut()
                    && !e.rows(&tile, ROWS_A_STEP)
                {
                    self.job = Job::Keep { zi, tile, cell: true, png };
                    return true;
                }
                // the tile kept after its cell: a tile kept means both are
                if let (Some(s), Some(e)) = (self.store.as_mut(), png) {
                    s.put(&tile_name(zi), e.finish());
                }
                self.tiles_out.push((zi, tile));
                self.zones[zi] = Zone::Shown;
                self.stats.drawn += 1;
                if self.done() {
                    // its textures go with it
                    self.renderer = None;
                    self.shot = None;
                    self.uploaded.clear();
                }
                true
            }
        }
    }

    /// A step of making zone `m`'s picture: a piece of its terrain built,
    /// its placements gathered, a sprite of them built, one texture decoded,
    /// or the piece uploaded. Whether there is more.
    fn make(&mut self, lib: &Library, g: &Gpu, m: &mut Making) -> bool {
        let c = match (&m.zone, lib.zone(self.world, m.zi)) {
            (Some(c), _) => c,
            (None, Some((zf, _))) => lib.file(zf),
            // the library let it go (a world change)
            (None, None) => return false,
        };
        let Some(piece) = m.piece.as_ref() else {
            if m.next < m.patches.len() {
                let end = (m.next + PATCHES_A_STEP).min(m.patches.len());
                let model = c.build_terrain(&m.patches[m.next..end], &m.pre);
                m.piece = Some(Placed { sprite: 0, model, instances: vec![render::IDENTITY], statics: Vec::new() });
                m.next = end;
                return true;
            }
            // then its placed actors (buildings, walls, piers, trees, rocks)
            let Some(sprites) = m.sprites.as_mut() else {
                m.sprites = Some(placements(c, m.node));
                return true;
            };
            // the next sprite with anything to draw
            while let Some(p) = sprites.get_mut(m.sprite_next) {
                m.sprite_next += 1;
                if let Some(model) = c.lookup(p.sprite).and_then(|n| c.build_model(n)) {
                    m.piece = Some(Placed { sprite: p.sprite, model: Some(model), instances: std::mem::take(&mut p.instances), statics: std::mem::take(&mut p.statics) });
                    return true;
                }
            }
            return false;
        };
        let Some(model) = piece.model.as_ref() else {
            m.piece = None;
            return true;
        };
        // its textures, one a step, ahead of the upload
        let wanted = model.batches.iter().flat_map(|b| b.layers.iter().map(|l| l.texture)).find(|&t| t != 0 && !self.uploaded.contains(&t) && !m.images.contains_key(&t));
        if let Some(t) = wanted {
            m.images.insert(t, c.surface(t).and_then(|s| s.decode(&c.data)));
            return true;
        }
        let r = self.renderer.get_or_insert_with(|| {
            let mut r = Renderer::new(g.device, g.queue, render::TARGET_FORMAT);
            // the layers' scrolls stopped: the same picture every time
            r.scroll_time = Some(0.0);
            r
        });
        self.uploaded.extend(m.images.keys());
        // each handed over once: the renderer keeps what it uploads by id
        let images = std::cell::RefCell::new(std::mem::take(&mut m.images));
        let textures = |id: i32| images.borrow_mut().remove(&id).flatten();
        if let Some(Placed { model: Some(model), instances, statics, .. }) = m.piece.take() {
            let gm = r.upload(g.device, g.queue, &model, &instances, &textures);
            if !statics.is_empty() {
                r.set_static_lights(g.queue, &gm, &statics);
            }
            m.parts.push(gm);
        }
        true
    }

    /// Where the pictures are kept, for the log.
    pub fn describe(&self) -> String {
        let (shown, of) = self.progress();
        let kept = self.store.as_ref().map_or("not kept".to_string(), |s| s.describe());
        format!("map: {shown} of {of} zones ({} drawn, {} from the cache: {kept})", self.stats.drawn, self.stats.kept)
    }
}

/// `EQOA_MAP_TRACE=1` (in a page `?maptrace`): each build step over 2 ms
/// said, with what it was.
fn trace() -> bool {
    #[cfg(target_arch = "wasm32")]
    return crate::web::page_flag("maptrace");
    #[cfg(not(target_arch = "wasm32"))]
    std::env::var_os("EQOA_MAP_TRACE").is_some()
}

/// The cache's name for zone `zi`'s tile.
pub fn tile_name(zi: usize) -> String {
    format!("t{zi}.png")
}

/// The cache's name for zone `zi`'s overview cell.
pub fn cell_name(zi: usize) -> String {
    format!("c{zi}.png")
}

/// Natively a zone's record is read on a thread of its own, so the frame
/// does not wait on the disc (a page's reads are asynchronous already).
#[cfg(not(target_arch = "wasm32"))]
pub struct ThreadZones {
    ask: std::sync::mpsc::Sender<(usize, u64, usize)>,
    got: std::sync::mpsc::Receiver<(usize, std::io::Result<Vec<u8>>)>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ThreadZones {
    /// Reads through `inner` on a thread.
    pub fn new(mut inner: Box<dyn ZoneSource + Send>) -> Self {
        let (ask, asked) = std::sync::mpsc::channel::<(usize, u64, usize)>();
        let (done, got) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            while let Ok((zi, off, len)) = asked.recv() {
                inner.request(zi, off, len);
                while let Some(r) = inner.poll() {
                    if done.send(r).is_err() {
                        return;
                    }
                }
            }
        });
        ThreadZones { ask, got }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ZoneSource for ThreadZones {
    fn request(&mut self, zi: usize, off: u64, len: usize) {
        let _ = self.ask.send((zi, off, len));
    }
    fn poll(&mut self) -> Option<(usize, std::io::Result<Vec<u8>>)> {
        self.got.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::store::Mem;

    /// A row of `n` zones, x 2,000 on, z 0..2,000.
    fn row(n: usize) -> Vec<[f32; 4]> {
        (0..n).map(|c| [2000.0 + c as f32 * 2000.0, 0.0, 4000.0 + c as f32 * 2000.0, 2000.0]).collect()
    }

    fn cell_png(shade: u8) -> Vec<u8> {
        picture::encode(OVERVIEW, OVERVIEW, &vec![shade; (OVERVIEW * OVERVIEW * 4) as usize])
    }

    /// The cache's listing decides what is drawn again: a zone whose tile
    /// and cell are both kept has its cell read and is on the map with no
    /// drawing; one with only its cell (a run cut off between the two
    /// writes, the cell going first) or neither is drawn; a kept cell that
    /// will not decode is drawn again. Nothing is chosen before the listing
    /// has come. The control: with no cache every zone is to be drawn at once.
    #[test]
    fn the_cache_says_what_is_drawn_already() {
        let mut mem = Mem::default();
        mem.files.insert(cell_name(0), cell_png(90));
        mem.files.insert(tile_name(0), vec![1]);
        mem.files.insert(cell_name(1), cell_png(90));
        mem.files.insert(cell_name(3), b"spoilt".to_vec());
        mem.files.insert(tile_name(3), vec![1]);
        let mut lib = Library::new();
        lib.add("W", Container::empty());
        let mut b = Builder::new(0, &row(4), Some(Box::new(mem)), None);
        assert!(b.zones.iter().all(|z| *z == Zone::Unknown));
        assert_eq!(b.next_zone(&lib, [0.0, 0.0]), None, "nothing before the listing");
        let t0 = Instant::now();
        b.take_answers(t0, Duration::from_secs(1));
        assert_eq!(b.zones, vec![Zone::Kept, Zone::Missing, Zone::Missing, Zone::Kept]);
        b.ask_cells();
        assert_eq!(b.zones[0], Zone::Asked);
        b.take_answers(t0, Duration::from_secs(1));
        assert_eq!(b.zones, vec![Zone::Shown, Zone::Missing, Zone::Missing, Zone::Missing]);
        assert_eq!(b.cells_out.len(), 1);
        assert_eq!(b.cells_out[0].0, 0);
        assert_eq!(b.cells_out[0].1[..4], [90, 90, 90, 255]);
        assert_eq!(b.progress(), (1, 4));
        let none = Builder::new(0, &row(4), None, None);
        assert!(none.zones.iter().all(|z| *z == Zone::Missing), "the control");
    }

    /// The next zone drawn is the nearest to where the player is (or the
    /// map's middle): from the east end, the east zone, then westward. The
    /// control: from the west end, the west zone.
    #[test]
    fn the_nearest_zone_is_drawn_first() {
        let mut lib = Library::new();
        lib.add("W", Container::empty());
        let mut b = Builder::new(0, &row(4), None, None);
        assert_eq!(b.next_zone(&lib, [9500.0, 1000.0]), Some(3));
        b.zones[3] = Zone::Shown;
        assert_eq!(b.next_zone(&lib, [9500.0, 1000.0]), Some(2));
        assert_eq!(b.next_zone(&lib, [0.0, 1000.0]), Some(0), "the control");
    }

    /// A zone is made a piece a frame: with no time to spare a frame does one
    /// piece (a few patches built, a texture decoded, a piece uploaded), so
    /// Freeport's zone is drawn after as many frames as it has pieces, and
    /// its picture comes out with the land on it. The control: with time to
    /// spare it is drawn in the first frame or two.
    /// Needs the disc and a graphics adapter; without them it says so and
    /// passes.
    #[test]
    fn a_zone_is_made_a_piece_a_frame() {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no disc at {}", iso.display());
            return;
        }
        let _gpu = crate::GPU_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let g = Gpu { device: &device, queue: &queue };
        let frames = |budget: Duration| {
            let mut lib = Library::new();
            lib.add("TUNARIA.ESF", crate::stream::open_file(&iso, "TUNARIA.ESF").unwrap());
            let c = lib.file(0);
            let base = c.child(c.roots[0], 0x8200).unwrap();
            let bounds: Vec<[f32; 4]> = vi::world::World::<vi::fpu::Ee>::proxies(c, base).iter().map(|b| [b.min.x.0, b.min.z.0, b.max.x.0, b.max.z.0]).collect();
            let src = crate::stream::FileZones::open(&iso, "TUNARIA.ESF").unwrap();
            let mut b = Builder::new(0, &bounds, Some(Box::new(Mem::default())), Some(Box::new(src)));
            // only Freeport's zone (84) to draw
            for (zi, z) in b.zones.iter_mut().enumerate() {
                *z = if zi == 84 { Zone::Missing } else { Zone::Shown };
            }
            // the frames until the tile is drawn (what follows waits on the
            // GPU, unless it answers at once)
            let mut n = 0;
            while b.job_name() != "picture" && b.zones[84] != Zone::Shown && n < 100_000 {
                b.step(&lib, &g, [25000.0, 15000.0], budget);
                n += 1;
            }
            let mut m = 0;
            while b.zones[84] != Zone::Shown && m < 1_000_000 {
                b.step(&lib, &g, [25000.0, 15000.0], budget);
                m += 1;
            }
            let tile = b.tiles_out.pop().map(|t| t.1).unwrap_or_default();
            (n, tile)
        };
        let (slow, tile) = frames(Duration::ZERO);
        // 292 patches, 8 a piece: 37 built and as many uploaded at least,
        // and the placements' sprites
        assert!(slow >= 37 * 2, "{slow} frames");
        assert_eq!(tile.len(), (TILE * TILE * 4) as usize);
        let lit = tile.as_chunks::<4>().0.iter().filter(|p| [p[0], p[1], p[2]] != VOID.map(|c| (c * 255.0).round() as u8)).count();
        assert!(lit > tile.len() / 8, "the land is drawn: {lit} of {} texels", tile.len() / 4);
        // (a debug build on a busy machine builds slowly: time enough for all)
        let (fast, _) = frames(Duration::from_secs(600));
        eprintln!("Freeport's zone: drawn after {slow} frames with no time to spare, {fast} with time");
        assert!(fast <= 2, "the control: {fast} frames with time to spare, {slow} with none");
    }

    /// The tile is drawn from straight above with north up: the cell's
    /// north-west corner (its least x and z) lands at the picture's top
    /// left, its south-east at the bottom right; a point high above the
    /// ground lands where its foot does. The control: a view with z up
    /// puts the north-west corner at the bottom.
    #[test]
    fn a_tile_is_drawn_north_up_from_straight_above() {
        let v = tile_view([24000.0, 14000.0, 26000.0, 16000.0]);
        let ndc = |p: Vec3| {
            let c = v.view_proj * p.extend(1.0);
            [c.x / c.w, c.y / c.w, c.z / c.w]
        };
        let nw = ndc(Vec3::new(24000.0, 50.0, 14000.0));
        let se = ndc(Vec3::new(26000.0, 50.0, 16000.0));
        assert!((nw[0] + 1.0).abs() < 1e-4 && (nw[1] - 1.0).abs() < 1e-4, "{nw:?}");
        assert!((se[0] - 1.0).abs() < 1e-4 && (se[1] + 1.0).abs() < 1e-4, "{se:?}");
        let high = ndc(Vec3::new(25000.0, 800.0, 15000.0));
        let foot = ndc(Vec3::new(25000.0, -200.0, 15000.0));
        assert!((high[0] - foot[0]).abs() < 1e-5 && (high[1] - foot[1]).abs() < 1e-5);
        assert!((0.0..1.0).contains(&high[2]) && (0.0..1.0).contains(&foot[2]) && high[2] < foot[2], "nearer is higher");
        let up = glam::camera::rh::view::look_at_mat4(Vec3::new(25000.0, 10000.0, 15000.0), Vec3::new(25000.0, 0.0, 15000.0), Vec3::Z);
        let c = glam::camera::rh::proj::directx::orthographic(-1000.0, 1000.0, -1000.0, 1000.0, 1.0, 20000.0) * up * Vec3::new(24000.0, 50.0, 14000.0).extend(1.0);
        assert!(c.y / c.w < 0.0, "the control: z up puts the north-west at the bottom");
    }
}
