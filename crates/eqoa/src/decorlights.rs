//! A server's decoration lights, as this client draws them (go-eqoa-server's
//! own; the server owner, 2026-11-16: Freeport's Christmas tree, "needs lights
//! on trees in different colors. the lights can be a template so we can put
//! christmas lights on fences buildings etc"; `net::decor` has the message, the
//! server's `entity/decorlights.go` the tables).
//!
//! The server sends strings of bulbs, each hung on every NPC of a name or placed
//! in a world, laid out by a template of its own (colours, spacing, how they
//! light). Each bulb is drawn as a glow, additive, its colour the string's
//! colour for it times how lit it is now ([`net::decor::lit`]: steady, a
//! twinkle, alternating, a chase), a soft round flare off SPELLFX.CSF
//! ([`BULB`]); a string's top bulb (a tree's star) as a sparkle ([`STAR`]). One
//! batch a colour a string, drawn with the particles after the world's: so
//! Dynamic Lighting takes each as a light of the moment (`lights::from_batches`)
//! and the tree casts its colours on the ground round it at night.
//!
//! The bulbs hang on a cord (the owner: "for the lights let there look like a
//! cord connecting them"): a thin wire in the string's cord colour from bulb to
//! bulb in order, hanging a little between each two ([`cords`]), a ribbon turned
//! to the eye drawn opaque with the world (`render::billboard::Strips`), lit as
//! the ground is by the sun and the ambient ([`light_cords`]), never a light of
//! its own. A ring's first bulb and a box's edge apart from the last break it
//! (`net::decor::BREAK`); a run that comes back round to its first bulb (a
//! ring) is closed. A string of single points has none.
//!
//! The art a decoration's NPC wears may be a sprite of a zone the player is not
//! near (Freeport's tree is a fir of Tunaria's north): the server names the
//! zone, and the walk's streamer reads it ([`art_zones`],
//! `stream::Streamer::want`), as it would were the player walking there, so
//! the sprite is in the library when the NPC is built.
//!
//! A string may look like more than glows (`net::decor::Look`; the owner,
//! 2026-11-16: "the magic globe can be used for bulbs and made different
//! colors", "the rope hanging also can be used to decorate the tree", "these
//! tents as wrapping paper for presents"): a model at each bulb, tinted its
//! colour ([`globes`]), a cord drawn with a texture, a span a repeat (a rope,
//! tinsel), and a present's paper a texture tinted its colour. That art may be
//! of a zone the player is nowhere near, or of another world's file (Odus's
//! tents for Freeport's presents): the server names it ([`net::decor::Asset`]),
//! and each zone named is read off the player's own disc once ([`poll_assets`]:
//! natively at once, in a page through `set_asset_reader`), its textures kept
//! for the decorations alone (`render::set_decor_texture`: never the library's,
//! so nothing of another world's dictionary stands in for this world's), its
//! models here.
//!
//! A server that sends none, and a PS2, draw nothing of it.

use net::decor::{Anchor, Decor};
use render::billboard::{Batch, Quad, Strips};
use std::cell::RefCell;

/// The bulb's glow: SPELLFX.CSF's soft round flare, white at its heart.
pub const BULB: i32 = 0x0BE1_EF02;
/// A string's top bulb (a tree's star): SPELLFX.CSF's six-pointed sparkle.
pub const STAR: i32 = 0xEDC2_9ADDu32 as i32;

pub use net::decor::TOP;

/// How far from the eye a bulb is drawn.
pub const REACH: f32 = 300.0;

thread_local! {
    static DECOR: RefCell<Decor> = RefCell::default();
    static ASSETS: RefCell<Assets> = RefCell::default();
    static READER: RefCell<Option<AssetReader>> = const { RefCell::new(None) };
}

/// A zone's bytes, read (or failed) at some frame: what an [`AssetReader`]
/// hands back at once and fills when the read is done.
pub type ZoneBytes = std::rc::Rc<RefCell<Option<Result<Vec<u8>, String>>>>;

/// How a page reads zone `zone` of the disc's world file `name`
/// (`set_asset_reader`); natively it is read at once ([`read_zone_now`]).
pub type AssetReader = Box<dyn Fn(&str, u16) -> ZoneBytes>;

/// Makes `f` the way the decorations' assets are read (a page's disc).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn set_asset_reader(f: AssetReader) {
    READER.with_borrow_mut(|r| *r = Some(f));
}

/// A model the bulbs wear: its middle and size across, and the batch that
/// takes the bulb's colour (its largest: the globe's glass, not its stand),
/// drawn with its texture made grey ([`grey_id`]) so any colour shows through
/// its pattern (the globe's glass is all blue: red over blue is black).
pub struct Globe {
    pub model: esf::Model,
    pub mid: glam::Vec3,
    pub across: f32,
    pub tinted: usize,
}

/// How light a globe's tinted glass is at its darkest (`greyed`): bright
/// baubles by day, which at night glow (they are drawn unlit).
const GLOBE_FLOOR: f32 = 0.6;
/// A cord's texture's (tinsel, a ribbon) darkest: its pattern kept.
const CORD_FLOOR: f32 = 0.15;

/// The id a texture's grey copy is kept under (`render::set_decor_texture`).
pub fn grey_id(id: i32) -> i32 {
    id ^ 0x4752_4559
}

/// `levels` made grey by each texel's brightest channel, stretched so the
/// brightest tenth is white and lifted so its darkest is `floor` (0 to 1): a
/// pattern that takes any tint (a globe's glass, mostly black, lifted to show
/// its colour bright: the owner, 2026-11-17, "make the globes brighter").
fn greyed(mut levels: Vec<esf::Image>, floor: f32) -> Vec<esf::Image> {
    for l in &mut levels {
        let mut v: Vec<u8> = l.rgba.chunks(4).map(|p| p[0].max(p[1]).max(p[2])).collect();
        let mut sorted = v.clone();
        sorted.sort_unstable();
        let top = sorted.get(sorted.len() * 9 / 10).copied().unwrap_or(255).max(16) as f32;
        for x in &mut v {
            *x = ((floor + (1.0 - floor) * (*x as f32 / top).min(1.0)) * 255.0) as u8;
        }
        for (p, g) in l.rgba.chunks_mut(4).zip(v) {
            p[0] = g;
            p[1] = g;
            p[2] = g;
        }
    }
    levels
}

/// The assets read and being read.
#[derive(Default)]
struct Assets {
    /// (world, zone) -> its read, while under way
    reading: Vec<((u8, u16), ZoneBytes)>,
    /// (world, zone) read (or failed), never read again
    read: std::collections::HashSet<(u8, u16)>,
    /// the models read, by DictID, with their middle and size across
    models: std::collections::HashMap<u32, std::rc::Rc<Globe>>,
}

/// Zone `zone` of world file `name` off the player's disc, read now.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_zone_now(name: &str, zone: u16) -> Result<Vec<u8>, String> {
    let iso_path = crate::disc_path(None);
    let disc = iso::Iso::open(std::path::Path::new(&iso_path)).map_err(|e| e.to_string())?;
    let e = disc.find(name).ok_or_else(|| format!("{name}: not on the disc"))?.clone();
    let f = iso::open_file(std::path::Path::new(&iso_path)).map_err(|e| e.to_string())?;
    let mut w = esf::Window::new(f, e.lba as u64 * iso::SECTOR, e.size as u64);
    let def = esf::Container::open_deferred(&mut w).map_err(|e| e.to_string())?;
    let zn = *def.zones().get(zone as usize).ok_or_else(|| format!("{name}: no zone {zone}"))?;
    let (off, len) = def.node(zn).deferred.ok_or_else(|| format!("{name}: zone {zone} is not deferred"))?;
    let mut b = vec![0u8; len];
    esf::ReadAt::read_at(&mut w, off, &mut b).map_err(|e| e.to_string())?;
    Ok(b)
}

/// Begins reading every asset zone of the set not yet read, and keeps what
/// has come of each: its textures for the decorations (`render`), its models
/// here. Called each frame the decorations are drawn.
pub fn poll_assets() {
    let wanted: Vec<(u8, u16, Vec<u32>)> = DECOR.with_borrow(|d| d.assets.iter().map(|a| (a.world, a.zone, a.ids.clone())).collect());
    if wanted.is_empty() {
        return;
    }
    ASSETS.with_borrow_mut(|st| {
        for (world, zone, _) in &wanted {
            let k = (*world, *zone);
            if st.read.contains(&k) || st.reading.iter().any(|(r, _)| *r == k) {
                continue;
            }
            let Some(name) = crate::play::WORLD_FILES.get(*world as usize) else {
                st.read.insert(k);
                continue;
            };
            let bytes = READER.with_borrow(|r| r.as_ref().map(|f| f(name, *zone)));
            #[cfg(not(target_arch = "wasm32"))]
            let bytes = bytes.unwrap_or_else(|| std::rc::Rc::new(RefCell::new(Some(read_zone_now(name, *zone)))));
            #[cfg(target_arch = "wasm32")]
            let Some(bytes) = bytes else { continue };
            st.reading.push((k, bytes));
        }
        let mut i = 0;
        while i < st.reading.len() {
            let Some(r) = st.reading[i].1.borrow_mut().take() else {
                i += 1;
                continue;
            };
            let (k, _) = st.reading.swap_remove(i);
            st.read.insert(k);
            let ids: Vec<u32> = wanted.iter().filter(|w| (w.0, w.1) == k).flat_map(|w| w.2.iter().copied()).collect();
            match r {
                Ok(b) => {
                    let n = keep_assets(&mut st.models, esf::Container::parse_records(b), &ids);
                    eprintln!("decorations: world {} zone {}: {n} of {} assets kept", k.0, k.1, ids.len());
                }
                Err(e) => eprintln!("decorations: world {} zone {}: {e}", k.0, k.1),
            }
        }
    });
}

/// The textures and models `ids` of the zone `c`: each texture's levels for
/// the decorations, each model with the textures it is drawn with.
fn keep_assets(models: &mut std::collections::HashMap<u32, std::rc::Rc<Globe>>, c: esf::Container, ids: &[u32]) -> usize {
    let mut kept = 0;
    // each texture as it is and grey (a cord's: tinsel, a rope, tinted its colour)
    let texture = |id: i32| {
        if let Some(s) = c.surface(id) {
            let levels = s.decode_levels(&c.data);
            render::set_decor_texture(grey_id(id), greyed(levels.clone(), CORD_FLOOR));
            render::set_decor_texture(id, levels);
        }
    };
    for &id in ids {
        let id32 = id as i32;
        if c.surface(id32).is_some() {
            texture(id32);
            kept += 1;
            continue;
        }
        let Some(&n) = c.dict.get(&id32) else {
            eprintln!("decorations: {id:#010X} is not in its zone");
            continue;
        };
        let Some(m) = c.build_model(n) else { continue };
        for b in &m.batches {
            texture(b.texture());
        }
        let tinted = (0..m.batches.len()).max_by_key(|&i| m.batches[i].indices.len()).unwrap_or(0);
        if let Some(s) = m.batches.get(tinted).and_then(|b| c.surface(b.texture())) {
            render::set_decor_texture(grey_id(m.batches[tinted].texture()), greyed(s.decode_levels(&c.data), GLOBE_FLOOR));
        }
        let (mut lo, mut hi) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
        for p in &m.pos {
            lo = lo.min(glam::Vec3::from(*p));
            hi = hi.max(glam::Vec3::from(*p));
        }
        let across = (hi - lo).max_element().max(1e-3);
        models.insert(id, std::rc::Rc::new(Globe { model: m, mid: (lo + hi) / 2.0, across, tinted }));
        kept += 1;
    }
    kept
}

/// A pet's look's model off the assets (`net::decor::Asset`: its zone read
/// off the disc whatever world the player is in), once read: what a pet in
/// a prop is built from where the world's library has no such model (Odus
/// has no catapult); its textures are the decorations' (`render::
/// decor_texture`), which the library's textures fall back on.
pub fn pet_asset_model(id: u32) -> Option<esf::Model> {
    pet_art(id)?;
    poll_assets();
    asset_model(id).map(|g| g.model.clone())
}

/// Whether `id` is a pet's look's shot (`net::decor::PetArt::shot`).
pub fn pet_art_shot(id: u32) -> bool {
    id != 0 && DECOR.with_borrow(|d| d.pets.iter().any(|p| p.shot == id))
}

/// Model `id` of the assets, once read, built.
pub fn asset_built(id: u32) -> Option<esf::Model> {
    asset_model(id).map(|g| g.model.clone())
}

/// Model `id` of the assets, once read.
fn asset_model(id: u32) -> Option<std::rc::Rc<Globe>> {
    ASSETS.with_borrow(|st| st.models.get(&id).cloned())
}

/// The set the server sent (`net::session::OP_DECOR_LIGHTS`), the last one
/// standing.
pub fn set(d: Decor) {
    DECOR.with_borrow_mut(|x| *x = d);
}

/// How a walk meets an NPC of `name` (`net::decor::WalkOn`'s mode: a deck
/// walked onto, or passed through), if the server named it.
pub fn walk_on(name: &str) -> Option<u8> {
    DECOR.with_borrow(|d| {
        if d.walk_ons.is_empty() {
            return None;
        }
        let k = net::npctint::key(name);
        d.walk_ons.iter().find(|w| w.key == k).map(|w| w.mode)
    })
}

/// Whether any string is lit.
pub fn any() -> bool {
    DECOR.with_borrow(|d| !d.strings.is_empty() || !d.presents.is_empty())
}

/// The zones of world `world` whose art a decoration wears: for the walk's
/// streamer to read.
pub fn art_zones(world: i64) -> Vec<usize> {
    DECOR.with_borrow(|d| d.art.iter().filter(|a| a.world as i64 == world).map(|a| a.zone as usize).collect())
}

/// How much lower a decoration's art is drawn than its NPC stands, in the
/// art's own units, if `model` is one (a fir's bare stem, so its boughs rest
/// on the ground): `play`'s puppets are built lowered by it.
pub fn sink(model: u32) -> Option<f32> {
    DECOR.with_borrow(|d| d.art.iter().find(|a| a.model == model && a.sink != 0.0).map(|a| a.sink))
}

/// Whether `model` is a decoration's art (`net::decor::Art`): scenery the
/// server places, which the client never moves. A pet's art ([`pet_art`])
/// is not: it walks as a creature.
pub fn is_art(model: u32) -> bool {
    model != 0 && DECOR.with_borrow(|d| d.art.iter().any(|a| a.model == model) && !d.pets.iter().any(|p| p.model == model))
}

/// The pet's look `model` is, if the server named it one (`net::decor::
/// PetArt`: a player's catapult): a prop worn by a pet, which rams at its
/// swings and fires its shot at its casts (`petart`).
pub fn pet_art(model: u32) -> Option<net::decor::PetArt> {
    if model == 0 {
        return None;
    }
    DECOR.with_borrow(|d| d.pets.iter().find(|p| p.model == model).copied())
}

/// `model` lowered by `sink(id)`, if the art is a decoration's that is.
pub fn lower(id: u32, model: &mut esf::Model) {
    if let Some(s) = sink(id) {
        for p in &mut model.pos {
            p[1] -= s;
        }
    }
}

/// An NPC a string may hang on: its name as its channel carries it, where it
/// is drawn, its heading and its scale.
pub struct Holder<'a> {
    pub name: &'a str,
    pub at: [f32; 3],
    pub heading: f32,
    pub scale: f32,
    /// Its body's bones as drawn this frame, for a string that rides them
    /// ([`rides_bones`]); none for a holder no such string hangs on.
    pub bones: Option<Bones>,
    /// It wears a decoration's art ([`is_art`]: the Christmas fir): its
    /// cords are laid on it from its axis, never moved by the eye
    /// ([`OUTWARD`]).
    pub art: bool,
}

/// A holder's body as drawn: each bone's transform this frame (its node's
/// world times the body's centre), and whether it stands still now.
pub struct Bones {
    pub drawn: Vec<glam::Mat4>,
    pub still: bool,
}

thread_local! {
    /// Each bone-riding bulb's bone and its place in that bone's frame, as
    /// taken the first time its holder was seen standing (`Bones::follow`),
    /// by the holder's name, the string, the bulb and where it was laid (a
    /// GM's move of the point lays it anew).
    static RIDES: RefCell<std::collections::HashMap<(String, usize, usize, [u32; 3]), (usize, glam::Vec3)>> = RefCell::new(Default::default());
}

impl Bones {
    /// `p` (where the bulb hangs off the holder's frame) carried by the bone
    /// nearest it as the body is drawn: the first time the holder stands
    /// still, the bulb's place on that bone is taken (the pose it was laid
    /// on, not the bind pose, which put Rudolph's nose over his head); from
    /// then on it goes where that bone goes: Rudolph's nose on his nose as
    /// his head moves (the owner, 2026-11-17: "rudolphs nose glow needs
    /// attached to his nose skeleton"). Before that, where it was laid.
    pub fn follow(&self, key: (&str, usize, usize, [f32; 3]), p: glam::Vec3) -> glam::Vec3 {
        let k = (key.0.to_string(), key.1, key.2, key.3.map(f32::to_bits));
        if let Some((n, off)) = RIDES.with_borrow(|r| r.get(&k).copied())
            && let Some(m) = self.drawn.get(n)
        {
            return m.transform_point3(off);
        }
        if self.still
            && let Some((n, m)) = self.drawn.iter().enumerate().min_by(|a, b| a.1.w_axis.truncate().distance_squared(p).total_cmp(&b.1.w_axis.truncate().distance_squared(p)))
            && m.determinant().abs() > 1e-12
        {
            RIDES.with_borrow_mut(|r| r.insert(k, (n, m.inverse().transform_point3(p))));
        }
        p
    }
}

/// A string that rides its holder's bones: a few bulbs and no cord (a red
/// nose, a lantern held), never a tree's strings, whose cords are laid
/// between bulbs at rest.
pub fn rides_bones(s: &net::decor::LightString) -> bool {
    matches!(s.anchor, Anchor::Npc(_)) && s.bulbs.len() <= 4 && (s.cord & 0xFF == 0 || s.cord_width <= 0.0)
}

/// The names (`npctint::key`) a string riding bones hangs on: the holders to
/// give [`Holder::bones`].
pub fn boned_keys() -> Vec<String> {
    DECOR.with_borrow(|d| d.strings.iter().filter(|s| rides_bones(s)).filter_map(|s| match &s.anchor { Anchor::Npc(k) => Some(k.clone()), _ => None }).collect())
}

/// The bulbs of every string, as particle batches: those on the NPCs in
/// `holders`, and those placed in world `world`, within [`REACH`] of `eye`, lit
/// at `ms` (the game's time).
pub fn batches(holders: &[Holder], world: i64, eye: [f32; 3], ms: i64) -> Vec<Batch> {
    DECOR.with_borrow(|d| batches_of(d, holders, world, eye, ms))
}

/// The bulbs of `d`'s strings placed in world `world` (a worn String of
/// Lights', `toys::placed`), as [`batches`] draws a server's.
pub fn batches_in(d: &Decor, world: i64, eye: [f32; 3], ms: i64) -> Vec<Batch> {
    batches_of(d, &[], world, eye, ms)
}

/// The cords of `d`'s strings placed in world `world`, as [`cords`] draws a
/// server's.
pub fn cords_in(d: &Decor, world: i64, eye: [f32; 3]) -> Vec<Strips> {
    cords_of(d, &[], world, eye)
}

fn batches_of(d: &Decor, holders: &[Holder], world: i64, eye: [f32; 3], ms: i64) -> Vec<Batch> {
    let t = ms as f64 / 1000.0;
    let mut out: Vec<Batch> = Vec::new();
    // each batch's colour key (usize::MAX the star's), beside it
    let mut keys: Vec<usize> = Vec::new();
    let near = |p: [f32; 3]| {
        let (dx, dy, dz) = (p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]);
        dx * dx + dy * dy + dz * dz < REACH * REACH
    };
    for (si, s) in d.strings.iter().enumerate() {
        // where it hangs: each NPC of its name's frame, or the world's own
        let boned = rides_bones(s);
        let frames: Vec<([f32; 3], f32, f32, Option<&Bones>)> = match &s.anchor {
            Anchor::Npc(key) => holders.iter().filter(|h| net::npctint::key(h.name) == *key).map(|h| (h.at, h.heading, h.scale, h.bones.as_ref().filter(|_| boned))).collect(),
            Anchor::World(w) if *w as i64 == world => vec![([0.0; 3], 0.0, 1.0, None)],
            Anchor::World(_) => Vec::new(),
        };
        if frames.is_empty() {
            continue;
        }
        let first = out.len();
        for (at, heading, scale, bones) in frames {
            if matches!(s.anchor, Anchor::Npc(_)) && !near(at) {
                continue;
            }
            let (sin, cos) = heading.sin_cos();
            for (bi, b) in s.bulbs.iter().enumerate() {
                // the NPC's frame as a zone actor's (`esf::zone::Actor::matrix`)
                let [x, y, z] = b.at;
                let mut p = [at[0] + scale * (z * sin + x * cos), at[1] + scale * y, at[2] + scale * (z * cos - x * sin)];
                if let (Some(bn), Anchor::Npc(key)) = (bones, &s.anchor) {
                    p = bn.follow((key, si, bi, b.at), glam::Vec3::from(p)).to_array();
                }
                if matches!(s.anchor, Anchor::World(_)) && !near(p) {
                    continue;
                }
                let top = b.color & TOP != 0;
                // a string of no glow (a garland: its cord alone)
                if s.size <= 0.0 && !top {
                    continue;
                }
                let ci = (b.color & net::decor::COLOR) as usize;
                let c = s.colors.get(ci).copied().unwrap_or(0xFFFF_FFFF);
                let rgb = [(c >> 24) as u8, (c >> 16) as u8, (c >> 8) as u8];
                let lit = if top { 0.85 + 0.15 * net::decor::lit(net::decor::Effect::Twinkle, 3.0, 0, t) } else { net::decor::lit(s.effect, s.period, b.phase, t) };
                let k = (lit * s.bright).clamp(0.0, 2.0);
                let half = 0.5 * s.size * scale.max(0.1) * if top { 4.0 } else { 1.0 } * (0.85 + 0.15 * lit);
                let color = [rgb[0] as f32 / 255.0 * k, rgb[1] as f32 / 255.0 * k, rgb[2] as f32 / 255.0 * k, 1.0];
                let texture = if top { STAR } else { BULB };
                // a batch a colour (and the star its own), so Dynamic Lighting
                // casts each colour from where its bulbs are
                let key = if top { usize::MAX } else { ci };
                let i = match keys[first..].iter().position(|&k| k == key) {
                    Some(i) => first + i,
                    None => {
                        out.push(Batch { texture, blend: 1, ztest: true, zwrite: false, quads: Vec::new() });
                        keys.push(key);
                        out.len() - 1
                    }
                };
                out[i].quads.push(Quad { center: p, half: [half, half], frame: 0, color });
            }
        }
    }
    out
}

/// How far a cord is drawn toward the eye from where it is laid, and that
/// more a unit of the eye's distance (`cords`).
pub const NUDGE: f32 = 0.05;
pub const NUDGE_PER: f32 = 0.004;

/// How far a cord on a decoration's art ([`Holder::art`]: the Christmas fir)
/// is laid out from the art's axis, in the art's units, its tinsel pieces
/// every other one [`OUTWARD_ALT`] more: the boughs' surface wanders round the
/// fitted paths (the ribbon's middle a median 0.47 out, the gold tinsel's
/// 0.23), and a band the eye turned and nudged cut into them along a line
/// that crawled as the eye moved (the owner, 2026-11-19: "lots of zfighting
/// on xmas tree when player is moving"). Laid from the axis, the cut is
/// where it is whatever the eye does.
pub const OUTWARD: f32 = 0.2;
pub const OUTWARD_ALT: f32 = 0.06;

/// A cord on a decoration's art wider than this is laid flat on it (the
/// tree's ribbon, 1.5) rather than turned to the eye (its lights' cord, 0.07).
pub const FLAT_WIDTH: f32 = 0.3;

/// A tinsel piece's length, of the cord's width (`tinsel_pieces`).
pub const TINSEL_PIECE: f32 = 0.7;

/// A tinsel cord as pieces (the owner, 2026-11-17: "the tinsel should be
/// rotated per piece"): along `line`, a piece every `TINSEL_PIECE` of `width`,
/// each two quads `width` across crossed about the line, turned by a twist of
/// its own and its texture in one of four turns, both from `seed` and the
/// piece (the same every frame): a bushy, twisted garland from any side, where
/// one band turned to the eye read as a flat strip.
fn tinsel_pieces(line: &[glam::Vec3], width: f32, seed: u32, eye: glam::Vec3, axis: Option<glam::Vec3>, strips: &mut Vec<Vec<([f32; 2], [f32; 3])>>) {
    use glam::Vec3;
    let len = (width * TINSEL_PIECE).max(0.05);
    // the line's length to each point
    let mut at = vec![0.0f32];
    for w in line.windows(2) {
        at.push(at[at.len() - 1] + (w[1] - w[0]).length());
    }
    let total = *at.last().unwrap_or(&0.0);
    let point = |d: f32| -> (Vec3, Vec3) {
        let i = at.partition_point(|&x| x <= d).clamp(1, line.len().max(2) - 1);
        let (a, b) = (line[i - 1], line[i]);
        let f = if at[i] > at[i - 1] { (d - at[i - 1]) / (at[i] - at[i - 1]) } else { 0.0 };
        (a.lerp(b, f.clamp(0.0, 1.0)), (b - a).normalize_or_zero())
    };
    let hash = |k: u32| {
        let mut x = seed.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B);
        x ^= x >> 15;
        x = x.wrapping_mul(0x2C1B_3C6D);
        x ^= x >> 12;
        x
    };
    let uvs = [[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]], [[1.0, 0.0], [1.0, 1.0], [0.0, 0.0], [0.0, 1.0]],
        [[1.0, 1.0], [0.0, 1.0], [1.0, 0.0], [0.0, 0.0]], [[0.0, 1.0], [0.0, 0.0], [1.0, 1.0], [1.0, 0.0]]];
    let n = (total / len).ceil().max(1.0) as u32;
    for k in 0..n {
        let (c, dir) = point((k as f32 + 0.5) * total / n as f32);
        if dir == Vec3::ZERO {
            continue;
        }
        // a frame about the line: any side, then the twist
        let side = if dir.y.abs() < 0.9 { dir.cross(Vec3::Y) } else { dir.cross(Vec3::X) }.normalize();
        let up = dir.cross(side);
        let h = hash(k);
        // each piece an eighth of a turn on from the last, give or take a
        // twenty-fourth (the owner, 2026-11-17: "the christmas tree ribbon and
        // tinsel ... are Z fighting"): two neighbours' overlapping ends are
        // never one plane, which a free twist made them now and then
        let jitter = ((h & 0xFFFF) as f32 / 65536.0 - 0.5) * (std::f32::consts::PI / 6.0);
        let twist = k as f32 * std::f32::consts::FRAC_PI_4 + jitter;
        // and the piece a little toward the eye, every other one a little
        // more, so where two overlap one is always in front, and the boughs
        // under the garland never fight it for the depth
        // on a decoration's art, out from its axis instead, the same whatever
        // the eye does ([`OUTWARD`])
        let c = match axis {
            Some(a) => c + outward(c, a) * (OUTWARD + if k % 2 == 0 { 0.0 } else { OUTWARD_ALT }),
            None => {
                let d = eye - c;
                c + d.normalize_or_zero() * ((NUDGE + NUDGE_PER * d.length()) * if k % 2 == 0 { 1.0 } else { 1.25 })
            }
        };
        // pieces overlap a little, so the garland has no gaps
        let along = dir * (0.6 * total / n as f32);
        for q in 0..2 {
            let t = twist + q as f32 * std::f32::consts::FRAC_PI_2;
            let across = (side * t.cos() + up * t.sin()) * (0.5 * width);
            let uv = uvs[((h >> (16 + 2 * q)) & 3) as usize];
            strips.push(vec![
                (uv[0], (c - along - across).to_array()),
                (uv[1], (c + along - across).to_array()),
                (uv[2], (c - along + across).to_array()),
                (uv[3], (c + along + across).to_array()),
            ]);
        }
    }
}

/// The way out from `axis` (a vertical line through it) at `p`, level.
fn outward(p: glam::Vec3, axis: glam::Vec3) -> glam::Vec3 {
    glam::Vec3::new(p.x - axis.x, 0.0, p.z - axis.z).normalize_or_zero()
}

/// How many pieces a cord's span between two bulbs is drawn in (its sag).
pub const CORD_PIECES: usize = 4;

/// The cords of every string with one, as world-space strips: the same strings
/// and frames as [`batches`], each run of bulbs (to a break) one strip, a
/// ribbon `cord_width` across turned to `eye`, sagging `cord_sag` of each span.
/// Unlit: [`light_cords`] lights them.
pub fn cords(holders: &[Holder], world: i64, eye: [f32; 3]) -> Vec<Strips> {
    DECOR.with_borrow(|d| cords_of(d, holders, world, eye))
}

fn cords_of(d: &Decor, holders: &[Holder], world: i64, eye: [f32; 3]) -> Vec<Strips> {
    use glam::Vec3;
    let e = Vec3::from(eye);
    let mut out = Vec::new();
    for (si, s) in d.strings.iter().enumerate() {
        if s.cord & 0xFF == 0 || s.cord_width <= 0.0 {
            continue;
        }
        let frames: Vec<([f32; 3], f32, f32, bool)> = match &s.anchor {
            Anchor::Npc(key) => holders.iter().filter(|h| net::npctint::key(h.name) == *key).map(|h| (h.at, h.heading, h.scale, h.art)).collect(),
            Anchor::World(w) if *w as i64 == world => vec![([0.0; 3], 0.0, 1.0, false)],
            Anchor::World(_) => Vec::new(),
        };
        let color = [(s.cord >> 24) as f32 / 255.0, ((s.cord >> 16) & 0xFF) as f32 / 255.0, ((s.cord >> 8) & 0xFF) as f32 / 255.0, 1.0];
        let mut strips = Vec::new();
        for (at, heading, scale, art) in frames {
            // a decoration's art's axis: its cords laid out from it ([`OUTWARD`])
            let axis = art.then(|| Vec3::from(at));
            let (sin, cos) = heading.sin_cos();
            let place = |b: &net::decor::Bulb| {
                let [x, y, z] = b.at;
                Vec3::new(at[0] + scale * (z * sin + x * cos), at[1] + scale * y, at[2] + scale * (z * cos - x * sin))
            };
            // the runs: bulbs in order, broken where a bulb says, the star never on one
            let mut runs: Vec<Vec<Vec3>> = Vec::new();
            for b in s.bulbs.iter().filter(|b| b.color & TOP == 0) {
                let p = place(b);
                match runs.last_mut() {
                    Some(r) if b.color & net::decor::BREAK == 0 => r.push(p),
                    _ => runs.push(vec![p]),
                }
            }
            let half = 0.5 * s.cord_width * scale.max(0.1);
            let textured = s.look.cord_texture != 0;
            for mut run in runs {
                if run.len() < 2 || (run[0] - e).length() > REACH {
                    continue;
                }
                // a ring that comes back round: closed
                if run.len() >= 3 {
                    let step = (run[1] - run[0]).length();
                    if (run[run.len() - 1] - run[0]).length() < 1.6 * step {
                        run.push(run[0]);
                    }
                }
                // the wire: each span in CORD_PIECES, hanging cord_sag of its length at its middle
                let mut line = vec![run[0]];
                for w in run.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    let sag = s.cord_sag * (b - a).length();
                    for k in 1..=CORD_PIECES {
                        let f = k as f32 / CORD_PIECES as f32;
                        line.push(a.lerp(b, f) - Vec3::Y * (sag * 4.0 * f * (1.0 - f)));
                    }
                }
                // tinsel (`Look::tinsel`): pieces along the wire, each its own
                // twist, crossed, bushy from any side; no band, no nudge
                if s.look.tinsel {
                    tinsel_pieces(&line, 2.0 * half, si as u32, e, axis, &mut strips);
                    continue;
                }
                // a little toward the eye, more the farther (the depth's own
                // steps grow with it): a band laid on the boughs never fights
                // them for the depth (the owner: "the tree red cloth wrap is z
                // fighting")
                // and more along the run, so where a wide ribbon's coils
                // overlap round the tree the later is always in front (at one
                // nudge the coils fought each other)
                let n = line.len().max(1) as f32;
                for (i, p) in line.iter_mut().enumerate() {
                    if let Some(a) = axis {
                        // on a decoration's art: out from its axis, the eye no part of it
                        *p += outward(*p, a) * (OUTWARD * scale.max(0.1));
                        continue;
                    }
                    let d = e - *p;
                    *p += d.normalize_or_zero() * ((NUDGE + NUDGE_PER * d.length()) * (1.0 + 0.5 * i as f32 / n));
                }
                // a ribbon turned to the eye; a textured one a strip a span,
                // the texture once across it (the sampler clamps)
                let mut strip = Vec::with_capacity(line.len() * 2);
                for (i, &p) in line.iter().enumerate() {
                    let dir = if i + 1 < line.len() { line[i + 1] - p } else { p - line[i - 1] };
                    // a band turned to the eye; a wide one on a decoration's
                    // art (the tree's ribbon) laid flat on it, across the way
                    // out from its axis, so it never turns into the boughs
                    let flat = axis.filter(|_| 2.0 * half > FLAT_WIDTH);
                    let side = match flat {
                        Some(a) => dir.cross(outward(p, a)).normalize_or_zero() * half,
                        None => dir.cross(e - p).normalize_or_zero() * half,
                    };
                    if textured {
                        let k = i % CORD_PIECES;
                        let u = k as f32 / CORD_PIECES as f32;
                        // a span's last point closes it and opens the next
                        if k == 0 && i > 0 {
                            strip.push(([1.0, 0.0], (p - side).to_array()));
                            strip.push(([1.0, 1.0], (p + side).to_array()));
                            strips.push(std::mem::take(&mut strip));
                        }
                        strip.push(([u, 0.0], (p - side).to_array()));
                        strip.push(([u, 1.0], (p + side).to_array()));
                        continue;
                    }
                    strip.push(([0.0, 0.0], (p - side).to_array()));
                    strip.push(([0.0, 1.0], (p + side).to_array()));
                }
                if strip.len() >= 4 {
                    strips.push(strip);
                }
            }
        }
        if !strips.is_empty() {
            // a textured cord drawn grey, tinted its colour (gold or silver tinsel, a red rope)
            let texture = if s.look.cord_texture != 0 { grey_id(s.look.cord_texture as i32) } else { 0 };
            out.push(Strips { texture, blend: 0, ztest: true, zwrite: true, color, strips, colors: Vec::new() });
        }
    }
    out
}

/// The model bulbs of every string with one (`Look::bulb_model`, once read:
/// [`poll_assets`]), as world-space strips: at each bulb but the star, the
/// model `bulb_scale` across, its middle on the bulb, tinted the bulb's colour
/// as lit now, a strip a triangle, a Strips a texture. Drawn as they are, not
/// lit by the scene: a bulb is its own light.
pub fn globes(holders: &[Holder], world: i64, eye: [f32; 3], ms: i64) -> Vec<Strips> {
    DECOR.with_borrow(|d| globes_of(d, holders, world, eye, ms, &asset_model))
}

#[allow(clippy::type_complexity)]
fn globes_of(d: &Decor, holders: &[Holder], world: i64, eye: [f32; 3], ms: i64, model: &dyn Fn(u32) -> Option<std::rc::Rc<Globe>>) -> Vec<Strips> {
    use glam::Vec3;
    let t = ms as f64 / 1000.0;
    let e = Vec3::from(eye);
    // by texture: its strips and their colours
    let mut by: Vec<(i32, Vec<Vec<([f32; 2], [f32; 3])>>, Vec<Vec<[f32; 4]>>)> = Vec::new();
    for (si, s) in d.strings.iter().enumerate() {
        if s.look.bulb_model == 0 {
            continue;
        }
        let Some(m) = model(s.look.bulb_model) else { continue };
        let (g, m, mid, across) = (&*m, &m.model, m.mid, m.across);
        let boned = rides_bones(s);
        let frames: Vec<([f32; 3], f32, f32, Option<&Bones>)> = match &s.anchor {
            Anchor::Npc(key) => holders.iter().filter(|h| net::npctint::key(h.name) == *key).map(|h| (h.at, h.heading, h.scale, h.bones.as_ref().filter(|_| boned))).collect(),
            Anchor::World(w) if *w as i64 == world => vec![([0.0; 3], 0.0, 1.0, None)],
            Anchor::World(_) => Vec::new(),
        };
        for (at, heading, scale, bones) in frames {
            let (sin, cos) = heading.sin_cos();
            for (bi, b) in s.bulbs.iter().enumerate().filter(|(_, b)| b.color & TOP == 0) {
                let [x, y, z] = b.at;
                let mut p = Vec3::new(at[0] + scale * (z * sin + x * cos), at[1] + scale * y, at[2] + scale * (z * cos - x * sin));
                if let (Some(bn), Anchor::Npc(key)) = (bones, &s.anchor) {
                    p = bn.follow((key, si, bi, b.at), p);
                }
                if (p - e).length() > REACH {
                    continue;
                }
                let c = s.colors.get((b.color & net::decor::COLOR) as usize).copied().unwrap_or(0xFFFF_FFFF);
                let lit = net::decor::lit(s.effect, s.period, b.phase, t);
                // a tint over the model's own texture, lifted so a dark glass
                // shows the colour (the shader keeps it to 1)
                let k = 1.5 * (0.75 + 0.25 * lit);
                let color = [((c >> 24) & 0xFF) as f32 / 255.0 * k, ((c >> 16) & 0xFF) as f32 / 255.0 * k, ((c >> 8) & 0xFF) as f32 / 255.0 * k, 1.0];
                // the rest (a stand) as it is, lit a little with it
                let plain = [0.75 + 0.25 * lit, 0.75 + 0.25 * lit, 0.75 + 0.25 * lit, 1.0];
                let f = s.look.bulb_scale.max(0.01) * scale.max(0.1) / across;
                let place = |v: [f32; 3]| (p + (Vec3::from(v) - mid) * f).to_array();
                for (bi, bt) in m.batches.iter().enumerate() {
                    let (tex, color) = if bi == g.tinted { (grey_id(bt.texture()), color) } else { (bt.texture(), plain) };
                    let i = match by.iter().position(|x| x.0 == tex) {
                        Some(i) => i,
                        None => {
                            by.push((tex, Vec::new(), Vec::new()));
                            by.len() - 1
                        }
                    };
                    for tri in bt.indices.chunks(3).filter(|t| t.len() == 3) {
                        let v = |k: usize| (m.uv.get(tri[k] as usize).copied().unwrap_or([0.0; 2]), place(m.pos[tri[k] as usize]));
                        by[i].1.push(vec![v(0), v(1), v(2)]);
                        by[i].2.push(vec![color; 3]);
                    }
                }
            }
        }
    }
    by.into_iter().map(|(texture, strips, colors)| Strips { texture, blend: 0, ztest: true, zwrite: true, color: [1.0; 4], strips, colors }).collect()
}

/// The cords lit as the world's own matte geometry is: their colour times the
/// ambient and the sun on a wire facing up, never brighter than it, never a
/// glow (a bolt's light, `spells::light_bolts`, adds the light to its colour).
pub fn light_cords(strips: &mut [Strips], view: &render::View) {
    matte(strips, view.ambient, view.sun, view.sun_dir);
}

fn matte(strips: &mut [Strips], ambient: glam::Vec3, sun: glam::Vec3, sun_dir: glam::Vec3) {
    let light = (ambient + sun * (-glam::Vec3::Y.dot(sun_dir)).max(0.0)).clamp(glam::Vec3::ZERO, glam::Vec3::ONE);
    for s in strips {
        let c = s.color;
        s.color = [c[0] * light.x, c[1] * light.y, c[2] * light.z, c[3]];
        for strip in &mut s.colors {
            for c in strip {
                *c = [c[0] * light.x, c[1] * light.y, c[2] * light.z, c[3]];
            }
        }
    }
}

/// The presents of every set (`net::decor::Presents`) on the NPCs in
/// `holders` and in world `world`, within [`REACH`] of `eye`: each box its
/// paper's five faces (the bottom on the ground unseen), shaded by the face's
/// facing, its ribbon a cross of bands over the top and down the sides, and a
/// bow of two crossed loops on top; opaque world-space strips, a colour a
/// vertex, unlit (lit as the cords are: [`light_cords`]).
pub fn presents(holders: &[Holder], world: i64, eye: [f32; 3]) -> Vec<Strips> {
    DECOR.with_borrow(|d| presents_of(d, holders, world, eye))
}

fn presents_of(d: &Decor, holders: &[Holder], world: i64, eye: [f32; 3]) -> Vec<Strips> {
    use glam::Vec3;
    let e = Vec3::from(eye);
    let mut out = Vec::new();
    for set in &d.presents {
        let frames: Vec<([f32; 3], f32, f32)> = match &set.anchor {
            Anchor::Npc(key) => holders.iter().filter(|h| net::npctint::key(h.name) == *key).map(|h| (h.at, h.heading, h.scale)).collect(),
            Anchor::World(w) if *w as i64 == world => vec![([0.0; 3], 0.0, 1.0)],
            Anchor::World(_) => Vec::new(),
        };
        let mut strips: Vec<Vec<([f32; 2], [f32; 3])>> = Vec::new();
        let mut colors: Vec<Vec<[f32; 4]>> = Vec::new();
        // the papers' faces, by texture
        let mut papers: Vec<(i32, Vec<Vec<([f32; 2], [f32; 3])>>, Vec<Vec<[f32; 4]>>)> = Vec::new();
        for (at, heading, scale) in frames {
            let (sin, cos) = heading.sin_cos();
            let frame = |p: Vec3| Vec3::new(at[0] + scale * (p.z * sin + p.x * cos), at[1] + scale * p.y, at[2] + scale * (p.z * cos - p.x * sin));
            for b in &set.boxes {
                let centre = frame(Vec3::from(b.at));
                if (centre - e).length() > REACH {
                    continue;
                }
                let (ys, yc) = b.yaw.sin_cos();
                // the box's own corner (x, y, z) in its anchor's frame, then the world's
                let place = |x: f32, y: f32, z: f32| frame(Vec3::new(b.at[0] + z * ys + x * yc, b.at[1] + y, b.at[2] + z * yc - x * ys));
                let rgba = |c: u32, k: f32| [((c >> 24) & 0xFF) as f32 / 255.0 * k, ((c >> 16) & 0xFF) as f32 / 255.0 * k, ((c >> 8) & 0xFF) as f32 / 255.0 * k, 1.0];
                let (w, h, dp) = (b.size[0] / 2.0, b.size[1], b.size[2] / 2.0);
                // toward the eye by `lift` nudges (`NUDGE`, more the farther):
                // the ribbon over the paper and the second band over the first
                // at any distance, where 0.002 apart fought far off
                let toward = |p: Vec3, lift: f32| {
                    let d = e - p;
                    p + d.normalize_or_zero() * (lift * (NUDGE + NUDGE_PER * d.length()))
                };
                let mut quad = |q: [Vec3; 4], c: [f32; 4]| {
                    // a quad as a strip: its corners in order round it, the strip 0 1 3 2
                    strips.push(vec![([0.0, 0.0], q[0].to_array()), ([0.0, 1.0], q[1].to_array()), ([1.0, 0.0], q[3].to_array()), ([1.0, 1.0], q[2].to_array())]);
                    colors.push(vec![c; 4]);
                };
                // the paper: the top, then the four sides, each shaded by its facing
                let shade = [1.0, 0.82, 0.68, 0.74, 0.88];
                let faces: [[(f32, f32, f32); 4]; 5] = [
                    [(-w, h, -dp), (w, h, -dp), (w, h, dp), (-w, h, dp)],
                    [(-w, 0.0, dp), (w, 0.0, dp), (w, h, dp), (-w, h, dp)],
                    [(w, 0.0, -dp), (w, 0.0, dp), (w, h, dp), (w, h, -dp)],
                    [(-w, 0.0, -dp), (w, 0.0, -dp), (w, h, -dp), (-w, h, -dp)],
                    [(-w, 0.0, -dp), (-w, 0.0, dp), (-w, h, dp), (-w, h, -dp)],
                ];
                if b.paper != 0 {
                    // the paper a texture, tinted its colour: each face the
                    // texture once over it, the long way along its width
                    let i = match papers.iter().position(|x| x.0 == b.paper as i32) {
                        Some(i) => i,
                        None => {
                            papers.push((b.paper as i32, Vec::new(), Vec::new()));
                            papers.len() - 1
                        }
                    };
                    for (f, k) in faces.iter().zip(shade) {
                        let q = f.map(|(x, y, z)| place(x, y, z));
                        papers[i].1.push(vec![([0.0, 0.0], q[0].to_array()), ([0.0, 1.0], q[1].to_array()), ([1.0, 0.0], q[3].to_array()), ([1.0, 1.0], q[2].to_array())]);
                        papers[i].2.push(vec![rgba(b.color, k * b.paper_gain.max(0.0)); 4]);
                    }
                } else {
                    for (f, k) in faces.iter().zip(shade) {
                        quad(f.map(|(x, y, z)| place(x, y, z)), rgba(b.color, k));
                    }
                }
                if b.ribbon & 0xFF == 0 {
                    continue;
                }
                // the ribbon: a band across the top and down two sides each way, a little proud of the paper
                let rw = 0.09 * b.size[0].min(b.size[2]);
                let o = 0.012;
                let (wo, ho, dpo) = (w + o, h + o, dp + o);
                for (n, (f, k)) in [
                    // along x: over the top and down the +x and -x sides
                    ([(-wo, ho, -rw), (wo, ho, -rw), (wo, ho, rw), (-wo, ho, rw)], 1.0),
                    ([(wo, 0.0, -rw), (wo, 0.0, rw), (wo, ho, rw), (wo, ho, -rw)], 0.68),
                    ([(-wo, 0.0, -rw), (-wo, 0.0, rw), (-wo, ho, rw), (-wo, ho, -rw)], 0.88),
                    // along z: over the top and down the +z and -z sides
                    ([(-rw, ho + 0.002, -dpo), (rw, ho + 0.002, -dpo), (rw, ho + 0.002, dpo), (-rw, ho + 0.002, dpo)], 1.0),
                    ([(-rw, 0.0, dpo), (rw, 0.0, dpo), (rw, ho, dpo), (-rw, ho, dpo)], 0.82),
                    ([(-rw, 0.0, -dpo), (rw, 0.0, -dpo), (rw, ho, -dpo), (-rw, ho, -dpo)], 0.74),
                ]
                .into_iter()
                .enumerate()
                {
                    let lift = if n < 3 { 0.5 } else { 1.0 };
                    quad(f.map(|(x, y, z)| toward(place(x, y, z), lift)), rgba(b.ribbon, k));
                }
                if !b.bow {
                    continue;
                }
                // the bow: two loops crossed on top, each a pair of tilted petals
                let bw = 0.32 * b.size[0].min(b.size[2]);
                let bh = 0.55 * bw;
                for (dx, dz) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                    let tip = (dx * bw, ho + bh, dz * bw);
                    let side = (dz * bw * 0.35, 0.0, dx * bw * 0.35);
                    let q = [(0.0, ho, 0.0), (tip.0 + side.0, tip.1, tip.2 + side.2), (tip.0 * 1.15, ho + bh * 0.4, tip.2 * 1.15), (tip.0 - side.0, tip.1, tip.2 - side.2)];
                    quad(q.map(|(x, y, z)| toward(place(x, y, z), 1.0)), rgba(b.ribbon, 0.95));
                }
            }
        }
        if !strips.is_empty() {
            out.push(Strips { texture: 0, blend: 0, ztest: true, zwrite: true, color: [1.0; 4], strips, colors });
        }
        for (texture, strips, colors) in papers {
            out.push(Strips { texture, blend: 0, ztest: true, zwrite: true, color: [1.0; 4], strips, colors });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::decor::{Bulb, Effect, LightString};

    fn tree(effect: Effect) -> Decor {
        Decor {
            strings: vec![LightString {
                anchor: Anchor::Npc("christmas tree".into()),
                effect,
                period: 1.5,
                size: 0.4,
                bright: 1.0,
                cord: 0x1E3F_1EFF,
                cord_width: 0.1,
                cord_sag: 0.06,
                colors: vec![0xFF20_20FF, 0x20D0_40FF, 0xFFD0_40FF],
                bulbs: vec![
                    Bulb { at: [1.0, 2.0, 0.0], color: 0, phase: 0 },
                    Bulb { at: [0.0, 4.0, 1.0], color: 1, phase: 64 },
                    Bulb { at: [-1.0, 6.0, 0.0], color: 0, phase: 128 },
                    Bulb { at: [0.0, 9.0, 0.0], color: 2 | TOP, phase: 0 },
                ],
                look: Default::default(),
            }],
            art: Vec::new(),
            presents: Vec::new(),
            assets: Vec::new(),
            walk_ons: Vec::new(),
            pets: Vec::new(),
        }
    }

    /// Hung on the tree's frame: placed by its place, heading and scale, a
    /// batch a colour and the star its own, additive, none for another name or
    /// out of reach.
    #[test]
    fn the_bulbs_hang_on_the_tree() {
        let d = tree(Effect::Steady);
        let h = [Holder { name: "Christmas Tree", at: [100.0, 50.0, 200.0], heading: 0.0, scale: 2.0, bones: None, art: false }];
        let b = batches_of(&d, &h, 0, [100.0, 55.0, 190.0], 0);
        let quads: usize = b.iter().map(|b| b.quads.len()).sum();
        assert_eq!(quads, 4);
        assert!(b.iter().all(|b| b.blend == 1 && !b.zwrite && b.ztest));
        let mut centres: Vec<[f32; 3]> = b.iter().flat_map(|b| b.quads.iter().map(|q| q.center)).collect();
        centres.sort_by(|a, b| a[1].total_cmp(&b[1]));
        assert_eq!(centres[0], [102.0, 54.0, 200.0], "x 1 at scale 2, y 2 up");
        assert_eq!(centres[3], [100.0, 68.0, 200.0], "the star at the top");
        let star: Vec<&Batch> = b.iter().filter(|b| b.texture == STAR).collect();
        assert_eq!(star.len(), 1);
        assert!(star[0].quads[0].half[0] > b.iter().find(|b| b.texture == BULB).unwrap().quads[0].half[0], "the star is the larger");
        // the red bulbs share a batch, the green its own
        let red = b.iter().find(|b| b.texture == BULB && b.quads[0].color[0] > 0.9).unwrap();
        assert_eq!(red.quads.len(), 2);
        // turned half round: x 1 goes to x -1
        let h2 = [Holder { name: "christmas tree", at: [0.0; 3], heading: std::f32::consts::PI, scale: 1.0, bones: None, art: false }];
        let b2 = batches_of(&d, &h2, 0, [0.0; 3], 0);
        let low = b2.iter().flat_map(|b| b.quads.iter()).find(|q| (q.center[1] - 2.0).abs() < 1e-4).unwrap();
        assert!((low.center[0] + 1.0).abs() < 1e-4, "{:?}", low.center);
        // another name, or past the reach: nothing
        assert!(batches_of(&d, &[Holder { name: "Christmas Bush", at: [0.0; 3], heading: 0.0, scale: 1.0, bones: None, art: false }], 0, [0.0; 3], 0).is_empty());
        assert!(batches_of(&d, &h, 0, [100.0 + REACH + 10.0, 50.0, 200.0], 0).is_empty());
    }

    /// The cord runs bulb to bulb, the star off it, a break starting a new
    /// run, each span sagging below the straight line between its bulbs; a
    /// string with no cord has none.
    #[test]
    fn the_cord_hangs_from_bulb_to_bulb() {
        let mut d = tree(Effect::Steady);
        let h = [Holder { name: "christmas tree", at: [0.0; 3], heading: 0.0, scale: 1.0, bones: None, art: false }];
        let c = cords_of(&d, &h, 0, [0.0, 3.0, -20.0]);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].strips.len(), 1, "three bulbs, the star off it: one run");
        let s = &c[0].strips[0];
        assert_eq!(s.len(), 2 * (1 + 2 * CORD_PIECES), "two spans of {CORD_PIECES} pieces, two sides a point");
        // the middle of the first span hangs below the straight line
        let mid = |i: usize| (glam::Vec3::from(s[2 * i].1) + glam::Vec3::from(s[2 * i + 1].1)) / 2.0;
        let straight = (glam::Vec3::new(1.0, 2.0, 0.0) + glam::Vec3::new(0.0, 4.0, 1.0)) / 2.0;
        assert!(mid(CORD_PIECES / 2).y < straight.y - 0.01, "{:?} vs {straight:?}", mid(CORD_PIECES / 2));
        // at the first bulb, drawn a little toward the eye (NUDGE)
        let eye_d = (glam::Vec3::new(0.0, 3.0, -20.0) - glam::Vec3::new(1.0, 2.0, 0.0)).length();
        assert!((mid(0) - glam::Vec3::new(1.0, 2.0, 0.0)).length() < NUDGE + NUDGE_PER * eye_d + 1e-3, "it starts at the first bulb");
        assert!(mid(0).z < -0.05, "drawn toward the eye: {:?}", mid(0));
        // a break: two runs, the second too short for a wire
        d.strings[0].bulbs[1].color |= net::decor::BREAK;
        assert_eq!(cords_of(&d, &h, 0, [0.0; 3])[0].strips.len(), 1, "a run of one bulb has no wire");
        d.strings[0].bulbs[2].color |= net::decor::BREAK;
        assert!(cords_of(&d, &h, 0, [0.0; 3]).is_empty(), "every bulb alone: no wire");
        // no cord
        let mut none = tree(Effect::Steady);
        none.strings[0].cord = 0;
        assert!(cords_of(&none, &h, 0, [0.0; 3]).is_empty());
        // lit: darker than its colour at night, never brighter
        let mut lit = cords_of(&tree(Effect::Steady), &h, 0, [0.0; 3]);
        matte(&mut lit, glam::Vec3::splat(0.3), glam::Vec3::ZERO, glam::Vec3::NEG_Y);
        assert!(lit[0].color[1] < 0x3F as f32 / 255.0 && lit[0].color[1] > 0.0);
    }

    /// Presents: five faces a box, six ribbon bands with a ribbon and four bow
    /// petals with a bow, placed in the holder's frame; none past the reach.
    #[test]
    fn the_presents_are_wrapped() {
        let mut d = tree(Effect::Steady);
        d.presents.push(net::decor::Presents {
            anchor: Anchor::Npc("christmas tree".into()),
            boxes: vec![
                net::decor::GiftBox { at: [2.0, 0.0, 0.0], size: [1.0, 0.8, 1.0], yaw: 0.0, color: 0xC020_20FF, ribbon: 0xFFD0_40FF, bow: true, paper: 0, paper_gain: 1.0 },
                net::decor::GiftBox { at: [-2.0, 0.0, 0.0], size: [0.6, 0.6, 0.6], yaw: 0.3, color: 0x2050_C0FF, ribbon: 0, bow: false, paper: 0, paper_gain: 1.0 },
            ],
        });
        let h = [Holder { name: "Christmas Tree", at: [100.0, 50.0, 200.0], heading: 0.0, scale: 2.0, bones: None, art: false }];
        let p = presents_of(&d, &h, 0, [100.0, 52.0, 190.0]);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].strips.len(), (5 + 6 + 4) + 5, "the red box wrapped and bowed, the blue plain");
        assert_eq!(p[0].colors.len(), p[0].strips.len());
        // the red box's top at 50 + 2 x 0.8, its middle at x 100 + 2 x 2
        let top = &p[0].strips[0];
        assert!(top.iter().all(|(_, v)| (v[1] - 51.6).abs() < 1e-4), "{top:?}");
        let mid_x = top.iter().map(|(_, v)| v[0]).sum::<f32>() / 4.0;
        assert!((mid_x - 104.0).abs() < 1e-4);
        assert!(p[0].colors[0][0][0] > 0.7, "red paper");
        assert!(presents_of(&d, &h, 0, [100.0 + REACH + 10.0, 50.0, 200.0]).is_empty());
    }

    /// A string's look: a model at each bulb but the star, its middle on the
    /// bulb and `bulb_scale` across, tinted the bulb's colour; a textured cord
    /// a strip a span, the texture once across each; no glow with a size of
    /// 0 but the star's.
    #[test]
    fn the_looks_draw_models_ropes_and_garlands() {
        let mut d = tree(Effect::Steady);
        d.strings[0].look = net::decor::Look { bulb_model: 0xF8E6_C045, bulb_scale: 0.5, cord_texture: 0x7B3D_8677, cord_repeat: 1.0, tinsel: false };
        // a model of one triangle, 2 across, its middle at (1, 1, 0)
        let mut m = esf::Model::default();
        m.pos = vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [1.0, 2.0, 0.0]];
        m.uv = vec![[0.0; 2]; 3];
        m.batches.push(esf::Batch { material: 0, layers: Vec::new(), indices: vec![0, 1, 2], lod: None });
        let rc = std::rc::Rc::new(Globe { model: m, mid: glam::Vec3::new(1.0, 1.0, 0.0), across: 2.0, tinted: 0 });
        let h = [Holder { name: "christmas tree", at: [0.0; 3], heading: 0.0, scale: 1.0, bones: None, art: false }];
        let g = globes_of(&d, &h, 0, [0.0, 3.0, -20.0], 0, &|id| (id == 0xF8E6_C045).then(|| rc.clone()));
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].strips.len(), 3, "a triangle a bulb, the star none");
        // the first bulb at (1, 2, 0): the corners a quarter of their way from the middle
        let first = &g[0].strips[0];
        assert!((glam::Vec3::from(first[0].1) - glam::Vec3::new(0.75, 1.75, 0.0)).length() < 1e-4, "{first:?}");
        assert!(g[0].colors[0][0][0] > 1.0 && g[0].colors[0][0][1] < 0.3, "red, lifted");
        // no model read: none
        assert!(globes_of(&d, &h, 0, [0.0; 3], 0, &|_| None).is_empty());
        // the textured cord: two spans, a strip each, u 0 to 1 along it
        let c = cords_of(&d, &h, 0, [0.0, 3.0, -20.0]);
        assert_eq!(c[0].texture, grey_id(0x7B3D_8677u32 as i32));
        assert_eq!(c[0].strips.len(), 2);
        assert!(c[0].strips.iter().all(|s| s.len() == 2 * (CORD_PIECES + 1) && s[0].0[0] == 0.0 && s[s.len() - 1].0[0] == 1.0));
        // a garland: no glow but the star
        d.strings[0].size = 0.0;
        let b = batches_of(&d, &h, 0, [0.0; 3], 0);
        assert_eq!(b.iter().map(|b| b.quads.len()).sum::<usize>(), 1);
        assert_eq!(b[0].texture, STAR);
        // a present of paper: its faces a Strips of the paper's texture, tinted
        d.presents.push(net::decor::Presents {
            anchor: Anchor::Npc("christmas tree".into()),
            boxes: vec![net::decor::GiftBox { at: [2.0, 0.0, 0.0], size: [1.0, 0.8, 1.0], yaw: 0.0, color: 0x8020_20FF, ribbon: 0xFFD0_40FF, bow: false, paper: 0x58B4_1D37, paper_gain: 2.0 }],
        });
        let p = presents_of(&d, &h, 0, [0.0, 3.0, -20.0]);
        assert_eq!(p.len(), 2, "the ribbons plain, the paper its own");
        assert_eq!((p[0].texture, p[0].strips.len()), (0, 6));
        assert_eq!((p[1].texture, p[1].strips.len()), (0x58B4_1D37, 5));
        assert!((p[1].colors[0][0][0] - 2.0 * 0x80 as f32 / 255.0).abs() < 1e-4, "its colour times its gain");
    }

    /// A decoration's art is known by its model, nothing else is.
    #[test]
    fn the_art_is_scenery() {
        let mut d = tree(Effect::Steady);
        d.art.push(net::decor::Art { model: 0x0B3B_A520, world: 0, zone: 22, sink: 0.0 });
        set(d);
        assert!(is_art(0x0B3B_A520));
        assert!(!is_art(0x84807490) && !is_art(0));
        set(Decor::default());
        assert!(!is_art(0x0B3B_A520));
    }

    /// A red nose rides the nearest bone: where it was laid until its holder
    /// is first seen standing, then on that bone as the bone moves; a tree's
    /// corded strings do not ride.
    #[test]
    fn a_nose_rides_the_head() {
        use glam::{Mat4, Vec3};
        let at = Vec3::new(0.0, 2.85, 1.1);
        let head = Mat4::from_translation(Vec3::new(0.0, 2.6, 0.8));
        let key = ("rudolph-test", 0, 0, at.to_array());
        let running = Bones { drawn: vec![Mat4::IDENTITY, head], still: false };
        assert_eq!(running.follow(key, at), at, "not yet seen standing: where it was laid");
        let standing = Bones { drawn: vec![Mat4::IDENTITY, head], still: true };
        assert!(standing.follow(key, at).distance(at) < 1e-5, "standing: where it was laid");
        // the head dropped half a unit and turned a quarter: the nose with it
        let moved = head * Mat4::from_translation(-Vec3::Y * 0.5) * Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let q = Bones { drawn: vec![Mat4::IDENTITY, moved], still: false }.follow(key, at);
        let want = moved.transform_point3(head.inverse().transform_point3(at));
        assert!(q.distance(want) < 1e-5 && q.distance(at) > 0.4, "with the head: {q}");
        // a GM moves the point: laid anew where it now is, the next time he stands
        let higher = at + Vec3::Y * 0.15;
        let key2 = ("rudolph-test", 0, 0, higher.to_array());
        assert_eq!(Bones { drawn: vec![Mat4::IDENTITY, moved], still: true }.follow(key2, higher), higher);
        let mut nose = tree(Effect::Steady).strings[0].clone();
        nose.anchor = Anchor::Npc("rudolph".into());
        nose.bulbs = vec![Bulb { at: at.to_array(), color: 0, phase: 0 }];
        nose.cord = 0;
        assert!(rides_bones(&nose));
        nose.cord = 0x1E3F_1EFF;
        assert!(!rides_bones(&nose), "a corded string is laid at rest");
    }

    /// Tinsel: crossed pairs of quads along the cord, each pair at its own
    /// twist (the same each time), none a band; the control, a plain textured
    /// cord, is strips of the line's length.
    #[test]
    fn tinsel_is_twisted_pieces() {
        let mut d = tree(Effect::Steady);
        d.strings[0].cord_width = 0.5;
        d.strings[0].look = net::decor::Look { cord_texture: 0x9196_DAC4, tinsel: true, ..Default::default() };
        let h = [Holder { name: "christmas tree", at: [0.0; 3], heading: 0.0, scale: 1.0, bones: None, art: false }];
        let c = cords_of(&d, &h, 0, [0.0, 3.0, -20.0]);
        let s = &c[0].strips;
        assert!(s.len() >= 8 && s.len() % 2 == 0, "{} quads", s.len());
        assert!(s.iter().all(|q| q.len() == 4));
        // two pieces' first quads: not the same turn
        let across = |q: &Vec<([f32; 2], [f32; 3])>| (glam::Vec3::from(q[2].1) - glam::Vec3::from(q[0].1)).normalize();
        assert!(across(&s[0]).dot(across(&s[2])).abs() < 0.999, "each piece its own twist");
        // a pair crossed: square to each other
        assert!(across(&s[0]).dot(across(&s[1])).abs() < 1e-3);
        assert_eq!(cords_of(&d, &h, 0, [0.0, 3.0, -20.0])[0].strips[0], s[0], "the same each time");
        d.strings[0].look.tinsel = false;
        assert_eq!(cords_of(&d, &h, 0, [0.0, 3.0, -20.0])[0].strips.len(), 2, "a plain textured cord: a strip a span");
    }

    /// On a decoration's art (the Christmas fir) a cord is laid from the
    /// art's axis, the eye no part of it (the owner, 2026-11-19: "lots of
    /// zfighting on xmas tree when player is moving"): a wide ribbon and
    /// tinsel the same seen from anywhere, the ribbon flat on the tree
    /// (across square to the way out from its axis). The control: the same strings on a holder that is not art
    /// still turn and nudge to the eye.
    #[test]
    fn a_cord_on_art_is_the_same_from_any_eye() {
        let (a, b) = ([0.0, 3.0, -20.0], [15.0, 8.0, 6.0]);
        let mut ribbon = tree(Effect::Steady);
        ribbon.strings[0].cord_width = 1.5;
        ribbon.strings[0].look = net::decor::Look { cord_texture: 0x6A41_D388, ..Default::default() };
        let mut tinsel = ribbon.clone();
        tinsel.strings[0].look.tinsel = true;
        let art = [Holder { name: "christmas tree", at: [0.0; 3], heading: 0.0, scale: 1.0, bones: None, art: true }];
        let plain = [Holder { name: "christmas tree", at: [0.0; 3], heading: 0.0, scale: 1.0, bones: None, art: false }];
        for d in [&ribbon, &tinsel] {
            assert_eq!(cords_of(d, &art, 0, a)[0].strips, cords_of(d, &art, 0, b)[0].strips, "the same from any eye");
            assert_ne!(cords_of(d, &plain, 0, a)[0].strips, cords_of(d, &plain, 0, b)[0].strips, "the control moves with the eye");
        }
        // the ribbon flat on the tree: each pair across the way out from its axis
        let c = cords_of(&ribbon, &art, 0, a);
        for strip in &c[0].strips {
            for pair in strip.chunks(2) {
                let (l, r) = (glam::Vec3::from(pair[0].1), glam::Vec3::from(pair[1].1));
                let mid = (l + r) * 0.5;
                let out = mid.with_y(0.0).normalize();
                assert!((r - l).normalize().dot(out).abs() < 1e-3, "across the way out, not into the tree");
            }
        }
    }

    /// A twinkle changes each bulb's light over time, out of step.
    #[test]
    fn the_bulbs_twinkle() {
        let d = tree(Effect::Twinkle);
        let h = [Holder { name: "christmas tree", at: [0.0; 3], heading: 0.0, scale: 1.0, bones: None, art: false }];
        let light = |ms| -> Vec<f32> {
            let mut v: Vec<(f32, f32)> = batches_of(&d, &h, 0, [0.0; 3], ms).iter().filter(|b| b.texture == BULB).flat_map(|b| b.quads.iter().map(|q| (q.center[1], q.color[0] + q.color[1] + q.color[2]))).collect();
            v.sort_by(|a, b| a.0.total_cmp(&b.0));
            v.into_iter().map(|x| x.1).collect()
        };
        let (a, b) = (light(300), light(700));
        assert_ne!(a, b, "lit differently 0.4 s on");
        assert_ne!(a[0], a[2], "two red bulbs out of step");
    }

    /// The server sends its decorations again at every touch of the panel
    /// (Hoofbite's crashes of 2026-11-17 came on a night of many): 50 sets,
    /// each drawn, keep the renderer's GPU memory where the first left it
    /// (no texture, no buffer more): a set is data, the strips' buffer the
    /// renderer's one kept slot, the textures by key. A scene of Freeport's
    /// size (32 strings of 32 bulbs, a cord each, 40 presents). Needs a
    /// graphics adapter (any backend); without one it says so and passes.
    #[test]
    fn fifty_reloads_keep_the_gpu_memory_flat() {
        use render::wgpu;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let mut r = render::Renderer::new(&device, &queue, render::TARGET_FORMAT);
        let (w, h) = (320, 180);
        let target = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: render::TARGET_FORMAT, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[] });
        let depth = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: render::DEPTH_FORMAT, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[] });
        let (tv, dv) = (target.create_view(&Default::default()), depth.create_view(&Default::default()));
        // a paper's texture and a tinsel's, as `poll_assets` keeps them
        let pic = |c: u8| esf::Image { width: 64, height: 64, rgba: vec![c; 64 * 64 * 4], alpha: false };
        render::set_decor_texture(0x1234_5678, vec![pic(200)]);
        render::set_decor_texture(grey_id(0x1234_5679), vec![pic(128)]);
        let textures = |id: i32| render::decor_texture(id).and_then(|l| l.first().cloned());
        let scene = |k: usize| {
            let mut d = Decor::default();
            for i in 0..32 {
                let mut st = tree(Effect::Twinkle).strings.remove(0);
                st.anchor = Anchor::World(0);
                st.look.cord_texture = if i % 2 == 0 { 0x1234_5679 } else { 0 };
                st.bulbs = (0..32).map(|b| net::decor::Bulb { at: [i as f32 * 2.0 + b as f32 * 0.3, 2.0 + (b % 5) as f32 + k as f32 * 0.01, 10.0], color: (b % 3) as u8, phase: b as u8 }).collect();
                d.strings.push(st);
            }
            d.presents.push(net::decor::Presents {
                anchor: Anchor::World(0),
                boxes: (0..40).map(|b| net::decor::GiftBox { at: [b as f32, 0.0, 5.0], size: [0.8, 0.6, 0.8], yaw: 0.1 * b as f32, color: 0xC020_20FF, ribbon: 0xFFD0_40FF, bow: true, paper: if b % 2 == 0 { 0x1234_5678 } else { 0 }, paper_gain: 1.0 }).collect(),
            });
            d
        };
        let view = glam::camera::rh::view::look_at_mat4(glam::Vec3::new(30.0, 5.0, -20.0), glam::Vec3::new(30.0, 3.0, 10.0), glam::Vec3::Y);
        let proj = glam::camera::rh::proj::directx::perspective(1.0, w as f32 / h as f32, 0.5, 500.0);
        let mut first = None;
        for reload in 0..50 {
            set(scene(reload % 2));
            let mut strips = cords(&[], 0, [30.0, 5.0, -20.0]);
            strips.extend(presents(&[], 0, [30.0, 5.0, -20.0]));
            assert!(!strips.is_empty());
            r.decor = strips;
            let mut enc = device.create_command_encoder(&Default::default());
            r.draw_decor(&device, &queue, &mut enc, &tv, &dv, view, proj, &textures);
            queue.submit([enc.finish()]);
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            let now = (r.texture_memory(), r.billboard_memory());
            match first {
                None => first = Some(now),
                Some(f) => assert_eq!(now, f, "reload {reload}: the GPU memory grew"),
            }
        }
        let ((n, bytes, _, _), buffers) = first.unwrap();
        eprintln!("50 reloads: {n} textures {bytes} bytes, strips' buffers {buffers} bytes, flat");
    }
}
