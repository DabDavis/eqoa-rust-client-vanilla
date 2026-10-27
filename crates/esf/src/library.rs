//! The dictionary across files.
//!
//! The game keeps ONE dictionary per stage (title, character select, in-game),
//! and every container it loads registers into it; about 30% of in-game
//! texture lookups resolve to a surface another, earlier file registered
//! (`claude-notes/dictid-cross-file-resolution.md`, measured). A DictID that is
//! already present is not registered again: `VIESFParse::ParseSurfaceObj` finds
//! the id first and skips the record, and `VIMap::Insert` hands back the
//! existing node (decomp `viesfparse.cpp`, `vimap_t.h`). So the FIRST file
//! loaded wins, and load order is part of what a scene looks like.
//!
//! A library also answers as the 2002 disc would (`Era::Y2002`, the client's
//! own: F9 swaps the art): a file of that disc stands in for its namesake
//! ([`Library::stand_in`]) and registers just ahead of it in a second
//! dictionary, which lookups answer from while the library's view is 2002's
//! ([`Library::set_view`]). The two discs name their art alike (a DictID is
//! the hash of the art's name), so a 2002 record takes the place of the
//! Frontiers record of the same name, and what the 2002 disc does not have
//! (Frontiers' new creatures, worlds, icons) is still found in Frontiers'
//! files. A 2002 record that is byte for byte its Frontiers namesake is
//! registered as that Frontiers record: what did not change is the same
//! texture and the same model in both views.

use crate::{Container, NodeId, Surface};
use std::cell::Cell;
use std::collections::HashMap;

/// Which art: the disc the game runs off (Frontiers), or the 2002 disc's,
/// EverQuest Online Adventures as it was before Frontiers. A file comes off
/// one of the two ([`Library::era_of`]); a library answers lookups as one of
/// the two ([`Library::view`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Era {
    #[default]
    Frontiers,
    Y2002,
}

/// Which container, and which record in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ref {
    pub file: usize,
    pub node: NodeId,
}

/// A registration's place in the 2002 dictionary's order: the file it ranks
/// as and the offset in that file (as `Library::rank`), then 0 for a 2002 file
/// standing in for that file and 1 for the file itself.
type Rank = (usize, u64, u8);

/// An entry of the 2002 dictionary: the record a lookup answers, where the
/// registration ranks, and the file that registered it (a stand-in may
/// register its Frontiers namesake's record).
#[derive(Clone, Copy, Debug)]
struct Entry2002 {
    r: Ref,
    rank: Rank,
    by: usize,
}

/// A world opened with its zones on the disc (`Container::open_deferred`)
/// loads them one at a time, each as a container of its own appended to
/// `files`; a zone's DictIDs register **as if at its world's place**, ranked
/// by the zone record's offset in the world file, so a lookup answers what it
/// answered with the whole file loaded: an earlier zone over a later one, the
/// world over every file added after it.
#[derive(Default)]
pub struct Library {
    pub files: Vec<(String, Container)>,
    dict: HashMap<i32, Ref>,
    /// Each file's place in the registration order: (the file it ranks as,
    /// the offset in that file). A file ranks as itself at 0; a zone as its
    /// world at its record's offset; a stand-in as the file it stands in for.
    rank: Vec<(usize, u64)>,
    /// (world file, zone index) -> the file holding that zone, once loaded.
    zone_files: HashMap<(usize, usize), usize>,
    /// DictIDs given to objects already loaded (`alias`), kept through a
    /// `replace`.
    aliases: Vec<(i32, Ref)>,
    /// Each file's disc.
    eras: Vec<Era>,
    /// A 2002 file standing in for another (`stand_in`): that file. A
    /// stand-in registers only in the 2002 dictionary.
    stands_for: Vec<Option<usize>>,
    /// The 2002 dictionary, kept once a stand-in is added (`stand_in`); until
    /// then the 2002 view answers as `dict`.
    dict_2002: Option<HashMap<i32, Entry2002>>,
    /// DictIDs a stand-in does not take: Frontiers' record answers in both
    /// views (`keep_frontiers`).
    kept: std::collections::HashSet<i32>,
    /// For each stand-in, the records that are its namesake's bytes: DictID ->
    /// the namesake's node (`same_as_namesake`, worked out once per pair, not
    /// at every registration: a world change registers every id again).
    same: HashMap<usize, HashMap<i32, NodeId>>,
    /// A world off one disc -> the same world off the other, opened with its
    /// zones on the disc and never filled (`set_twin`): where the zones'
    /// additions are read from (`add_zone_extras`).
    twins: HashMap<usize, usize>,
    /// (world file, zone index) -> what another disc's zone of the same name
    /// adds to it: the file holding the records, and the actors placing them
    /// (`add_zone_extras`).
    extras: HashMap<(usize, usize), (usize, Vec<crate::zone::Actor>)>,
    /// The dictionary lookups answer from.
    view: Cell<Era>,
}

/// Whether record `n` of `a` and record `m` of `b` are the same bytes: type,
/// version, children and body.
fn same_record(a: &Container, n: NodeId, b: &Container, m: NodeId) -> bool {
    let (x, y) = (a.node(n), b.node(m));
    x.ty == y.ty && x.version == y.version && x.children.len() == y.children.len() && a.body(n) == b.body(m)
}

impl Library {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a container after every one already loaded; its DictIDs register
    /// only where no earlier file has them. Returns its index.
    pub fn add(&mut self, name: impl Into<String>, c: Container) -> usize {
        self.add_from(name, c, Era::Frontiers)
    }

    /// `add` for a file off the disc of `era` (a world off the 2002 disc
    /// takes the place of Frontiers' whole, not as a stand-in).
    pub fn add_from(&mut self, name: impl Into<String>, c: Container, era: Era) -> usize {
        let file = self.push(name.into(), c, (self.files.len(), 0), era, None);
        self.register(file);
        file
    }

    fn push(&mut self, name: String, c: Container, rank: (usize, u64), era: Era, stands_for: Option<usize>) -> usize {
        let file = self.files.len();
        self.files.push((name, c));
        self.rank.push(rank);
        self.eras.push(era);
        self.stands_for.push(stands_for);
        file
    }

    /// Adds `c`, the 2002 disc's copy of file `file`, standing in for it in
    /// the 2002 view: its DictIDs register there just ahead of `file`'s, and
    /// the Frontiers view does not see it. A second stand-in for the same file
    /// takes the first's place. Returns its index.
    pub fn stand_in(&mut self, file: usize, c: Container) -> usize {
        if let Some(s) = self.stand_in_of(file) {
            self.files[s].1 = c;
            self.same_as_namesake(s);
            self.reregister();
            return s;
        }
        let name = self.files[file].0.clone();
        let rank = self.rank[file];
        let s = self.push(name, c, rank, Era::Y2002, Some(file));
        self.same_as_namesake(s);
        if self.dict_2002.is_none() {
            // the 2002 dictionary begins: every file so far, this one with them
            self.dict_2002 = Some(HashMap::new());
            for f in 0..self.files.len() {
                self.register_2002(f);
            }
            self.alias_2002();
        } else {
            self.register_2002(s);
        }
        s
    }

    /// The records of stand-in `s` that are its namesake's bytes, kept.
    fn same_as_namesake(&mut self, s: usize) {
        let Some(f) = self.stands_for[s] else { return };
        let (a, b) = (&self.files[f].1, &self.files[s].1);
        let same = b.dict.iter().filter_map(|(&id, &m)| a.dict.get(&id).filter(|&&n| same_record(a, n, b, m)).map(|&n| (id, n))).collect();
        self.same.insert(s, same);
    }

    /// The 2002 file standing in for `file`, if one does.
    pub fn stand_in_of(&self, file: usize) -> Option<usize> {
        self.stands_for.iter().position(|&f| f == Some(file))
    }

    /// `ids` keep their Frontiers records in the 2002 view: no stand-in takes
    /// them (art whose layout Frontiers' code reads, such as the HUD atlas's
    /// sheet, whose rects SUPPORT's table names).
    pub fn keep_frontiers(&mut self, ids: &[i32]) {
        let new = ids.iter().fold(false, |n, &id| self.kept.insert(id) || n);
        if new && self.dict_2002.is_some() {
            self.reregister();
        }
    }

    /// The file `file` stands in for, if it is a stand-in.
    pub fn stands_for(&self, file: usize) -> Option<usize> {
        self.stands_for.get(file).copied().flatten()
    }

    /// The disc file `file` came off.
    pub fn era_of(&self, file: usize) -> Era {
        self.eras.get(file).copied().unwrap_or_default()
    }

    /// Whether any file stands in for another: whether the 2002 view differs
    /// from Frontiers' at all.
    pub fn has_2002(&self) -> bool {
        self.dict_2002.is_some()
    }

    /// The dictionary lookups answer from now.
    pub fn view(&self) -> Era {
        self.view.get()
    }

    /// Lookups answer from `era`'s dictionary from now on.
    pub fn set_view(&self, era: Era) {
        self.view.set(era);
    }

    /// `f` with lookups answering from `era`'s dictionary, the view as it was
    /// after: a model built whole in one view.
    pub fn with_view<R>(&self, era: Era, f: impl FnOnce() -> R) -> R {
        let was = self.view.replace(era);
        let r = f();
        self.view.set(was);
        r
    }

    /// The view a model whose root is `dict_id` is built in: 2002's while the
    /// library's view is 2002's and a 2002 file registered the id (the 2002
    /// disc has that model), else Frontiers'. A model the 2002 disc does not
    /// have is built from Frontiers' records alone, whatever 2002 records
    /// share its parts' names (eight of Frontiers' 171 new creatures name an
    /// animation or a texture whose 2002 namesake differs).
    pub fn view_for(&self, dict_id: i32) -> Era {
        match self.view() {
            Era::Y2002 if self.in_2002(dict_id) => Era::Y2002,
            _ => Era::Frontiers,
        }
    }

    /// Whether a file off the 2002 disc registered `dict_id` in the 2002
    /// dictionary.
    pub fn in_2002(&self, dict_id: i32) -> bool {
        match &self.dict_2002 {
            Some(d) => d.get(&dict_id).is_some_and(|e| self.era_of(e.by) == Era::Y2002),
            None => self.dict.get(&dict_id).is_some_and(|r| self.era_of(r.file) == Era::Y2002),
        }
    }

    /// The rank a file registers at in the 2002 dictionary.
    fn rank_2002(&self, file: usize) -> Rank {
        let (r, sub) = match self.stands_for[file] {
            Some(f) => (self.rank[f], 0),
            None => (self.rank[file], 1),
        };
        (r.0, r.1, sub)
    }

    /// Registers file `file`'s DictIDs: where an id is absent, or held by a
    /// file that ranks after this one.
    fn register(&mut self, file: usize) {
        if self.stands_for[file].is_none() {
            let rank = self.rank[file];
            for (&id, &node) in &self.files[file].1.dict {
                match self.dict.get(&id) {
                    Some(old) if self.rank[old.file] <= rank => {}
                    _ => {
                        self.dict.insert(id, Ref { file, node });
                    }
                }
            }
        }
        if self.dict_2002.is_some() {
            self.register_2002(file);
        }
    }

    /// `register` in the 2002 dictionary: a stand-in's record that equals its
    /// namesake's is registered as the namesake's.
    fn register_2002(&mut self, file: usize) {
        let rank = self.rank_2002(file);
        let stands_for = self.stands_for[file];
        let mut new = Vec::new();
        {
            let Some(d) = &self.dict_2002 else { return };
            let c = &self.files[file].1;
            for (&id, &node) in &c.dict {
                if d.get(&id).is_some_and(|e| e.rank <= rank) || (stands_for.is_some() && self.kept.contains(&id)) {
                    continue;
                }
                let r = match (stands_for, self.same.get(&file).and_then(|m| m.get(&id))) {
                    (Some(f), Some(&n)) => Ref { file: f, node: n },
                    _ => Ref { file, node },
                };
                new.push((id, Entry2002 { r, rank, by: file }));
            }
        }
        if let Some(d) = &mut self.dict_2002 {
            d.extend(new);
        }
    }

    /// The aliases in the 2002 dictionary, where nothing has their ids.
    fn alias_2002(&mut self) {
        let aliases: Vec<(i32, Ref, Rank)> = self.aliases.iter().map(|&(id, r)| (id, r, self.rank_2002(r.file))).collect();
        if let Some(d) = &mut self.dict_2002 {
            for (id, r, rank) in aliases {
                d.entry(id).or_insert(Entry2002 { r, rank, by: r.file });
            }
        }
    }

    /// Every DictID registered again in rank order, so the first registration
    /// still wins, then the aliases where nothing has their ids.
    fn reregister(&mut self) {
        self.dict.clear();
        if let Some(d) = &mut self.dict_2002 {
            d.clear();
        }
        for file in 0..self.files.len() {
            self.register(file);
        }
        for &(id, r) in &self.aliases {
            self.dict.entry(id).or_insert(r);
        }
        self.alias_2002();
    }

    /// Puts `c` in place of file `i` (a world change) and registers every
    /// DictID again in rank order, so the first registration still wins. The
    /// old world's loaded zones are dropped (their slots left empty, so every
    /// other file keeps its index).
    pub fn replace(&mut self, i: usize, name: impl Into<String>, c: Container) {
        self.replace_from(i, name, c, Era::Frontiers)
    }

    /// `replace` with a file off the disc of `era`: a world off the 2002 disc
    /// in place of Frontiers' (F9), or back.
    pub fn replace_from(&mut self, i: usize, name: impl Into<String>, c: Container, era: Era) {
        self.files[i] = (name.into(), c);
        self.eras[i] = era;
        if let Some(s) = self.stand_in_of(i) {
            self.same_as_namesake(s);
        }
        let gone: Vec<(usize, usize)> = self.zone_files.keys().filter(|k| k.0 == i).copied().collect();
        for k in gone {
            if let Some(f) = self.zone_files.remove(&k) {
                self.files[f].1 = Container::empty();
            }
        }
        // the old world's twin and its zones' additions go with it
        let gone: Vec<(usize, usize)> = self.extras.keys().filter(|k| k.0 == i).copied().collect();
        for k in gone {
            if let Some((f, _)) = self.extras.remove(&k) {
                self.files[f].1 = Container::empty();
            }
        }
        if let Some(t) = self.twins.remove(&i) {
            self.files[t].1 = Container::empty();
        }
        self.reregister();
    }

    /// `c` as the twin of the world in file `world`: the same world off the
    /// other disc, its zones left there, read only for what they add to
    /// `world`'s (`add_zone_extras`); None lets the twin go. A twin registers
    /// nothing (a world's own records are its zones'), and is replaced in its
    /// place, so file indices stay put. Returns its file.
    pub fn set_twin(&mut self, world: usize, c: Option<Container>, era: Era) -> Option<usize> {
        let Some(c) = c else {
            if let Some(t) = self.twins.remove(&world) {
                self.files[t].1 = Container::empty();
            }
            return None;
        };
        if let Some(&t) = self.twins.get(&world) {
            self.files[t].1 = c;
            self.eras[t] = era;
            return Some(t);
        }
        let name = self.files[world].0.clone();
        let t = self.push(name, c, (self.files.len(), 0), era, None);
        self.twins.insert(world, t);
        Some(t)
    }

    /// The twin of the world in file `world` (`set_twin`).
    pub fn twin_of(&self, world: usize) -> Option<usize> {
        self.twins.get(&world).copied()
    }

    /// Adds what zone `zi` of the world in file `world` gets from its twin's
    /// zone of the same index: the records `c` holds (registered as if at the
    /// zone's place, after its own) and the actors placing them, which the
    /// collision world puts in the zone's rooms with its own
    /// (`vi::world::World::add_zone`). Their disc is the twin's. Once.
    pub fn add_zone_extras(&mut self, world: usize, zi: usize, c: Container, actors: Vec<crate::zone::Actor>) -> Option<usize> {
        if let Some(&(f, _)) = self.extras.get(&(world, zi)) {
            return Some(f);
        }
        let (off, _) = self.zone_range(world, zi)?;
        let era = self.twin_of(world).map_or(Era::Frontiers, |t| self.era_of(t));
        let name = format!("{}#{zi}+", self.files[world].0);
        let file = self.push(name, c, (world, off), era, None);
        self.extras.insert((world, zi), (file, actors));
        self.register(file);
        Some(file)
    }

    /// What zone `zi` of the world in file `world` was given by its twin's
    /// (`add_zone_extras`): the records' file and the actors.
    pub fn zone_extras(&self, world: usize, zi: usize) -> Option<(usize, &[crate::zone::Actor])> {
        self.extras.get(&(world, zi)).map(|(f, a)| (*f, a.as_slice()))
    }

    /// Registers `r` under `id` too, unless something already has `id` (the
    /// dictionary's first registration wins): an object already loaded given
    /// another DictID, as a server's patch does (Sandstorm's S2 registers
    /// five CHAR.ESF surfaces under DictIDs of its own for its armour set 9).
    /// The 2002 view takes it as it is: its record is Frontiers'.
    pub fn alias(&mut self, id: i32, r: Ref) -> bool {
        if self.dict.contains_key(&id) {
            return false;
        }
        self.dict.insert(id, r);
        self.aliases.push((id, r));
        self.alias_2002();
        true
    }

    pub fn file(&self, i: usize) -> &Container {
        &self.files[i].1
    }

    /// The file named `name` (any case) as the view sees it: in 2002's, the
    /// file standing in for it if one does. A stand-in is never found by name
    /// in Frontiers' view.
    pub fn file_named(&self, name: &str) -> Option<usize> {
        let f = (0..self.files.len()).find(|&i| self.stands_for[i].is_none() && self.files[i].0.eq_ignore_ascii_case(name))?;
        match self.view() {
            Era::Y2002 => Some(self.stand_in_of(f).unwrap_or(f)),
            Era::Frontiers => Some(f),
        }
    }

    /// Every file in the order the view registers them: a file's stand-in
    /// just ahead of it in 2002's view, and none in Frontiers' (what walks
    /// the files by name for their surfaces, the HUD's and the pregame's 2D
    /// layers, sees what the view does: a surface it already took wins).
    pub fn in_view(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.files.len());
        for f in (0..self.files.len()).filter(|&f| self.stands_for[f].is_none()) {
            if self.view() == Era::Y2002
                && let Some(s) = self.stand_in_of(f)
            {
                out.push(s);
            }
            out.push(f);
        }
        out
    }

    pub fn lookup(&self, dict_id: i32) -> Option<Ref> {
        self.lookup_in(self.view(), dict_id)
    }

    /// `lookup` in `era`'s dictionary, whatever the view.
    pub fn lookup_in(&self, era: Era, dict_id: i32) -> Option<Ref> {
        match (era, &self.dict_2002) {
            (Era::Y2002, Some(d)) => d.get(&dict_id).map(|e| e.r),
            _ => self.dict.get(&dict_id).copied(),
        }
    }

    /// What a texture cache keys the surface `dict_id` names now by: the id,
    /// and whether its record is off the 2002 disc. A DictID names one record
    /// in each view, and a 2002 record that is its Frontiers namesake's bytes
    /// is that record, so the two views share every texture that did not
    /// change and keep apart the ones that did.
    pub fn texture_key(&self, dict_id: i32) -> u64 {
        let y2002 = self.lookup(dict_id).is_some_and(|r| self.era_of(r.file) == Era::Y2002);
        dict_id as u32 as u64 | (y2002 as u64) << 32
    }

    /// The surface a DictID names, wherever it was registered, with the
    /// container that holds its bytes.
    pub fn surface(&self, dict_id: i32) -> Option<(&Container, Surface)> {
        let r = self.lookup(dict_id)?;
        let c = self.file(r.file);
        (c.node(r.node).ty == crate::types::SURFACE).then(|| (c, Surface::read(&c.data, c.node(r.node).body)))
    }

    /// How many zones the world in file `world` has, loaded or not.
    pub fn zone_count(&self, world: usize) -> usize {
        self.file(world).zones().len()
    }

    /// Zone `zi` of the world in file `world`: the file holding its records
    /// and its node there; `None` while it is still on the disc.
    pub fn zone(&self, world: usize, zi: usize) -> Option<(usize, NodeId)> {
        let c = self.file(world);
        let z = *c.zones().get(zi)?;
        if c.node(z).deferred.is_none() {
            return Some((world, z));
        }
        let f = *self.zone_files.get(&(world, zi))?;
        Some((f, *self.file(f).roots.first()?))
    }

    /// Where zone `zi` of a deferred world is in its file: the offset of the
    /// record's header and the header and body's length (`None` if the world
    /// was read whole, or there is no such zone).
    pub fn zone_range(&self, world: usize, zi: usize) -> Option<(u64, usize)> {
        let c = self.file(world);
        c.node(*c.zones().get(zi)?).deferred
    }

    /// Adds zone `zi` of the world in file `world`, as
    /// [`Container::parse_records`] made it out of [`Library::zone_range`]'s
    /// bytes. Returns its file index; a zone already loaded is not added again.
    /// It comes off its world's disc.
    pub fn add_zone(&mut self, world: usize, zi: usize, c: Container) -> Option<usize> {
        if let Some(&f) = self.zone_files.get(&(world, zi)) {
            return Some(f);
        }
        let (off, _) = self.zone_range(world, zi)?;
        let name = format!("{}#{zi}", self.files[world].0);
        let file = self.push(name, c, (world, off), self.era_of(world), None);
        self.zone_files.insert((world, zi), file);
        self.register(file);
        Some(file)
    }

    /// Whether the world in file `world` has zones still on the disc (it was
    /// opened deferred).
    pub fn is_deferred(&self, world: usize) -> bool {
        self.file(world).is_deferred()
    }
}

impl Library {
    /// Builds the sprite at `r`, resolving members and palettes across files.
    pub fn build_model(&self, r: Ref) -> Option<crate::Model> {
        crate::mesh::build(self.file(r.file), r.node, &|d| self.lookup(d).map(|r| (self.file(r.file), r.node)))
    }

    /// Builds a zone's terrain into one model.
    pub fn build_terrain(&self, file: usize, zone: NodeId) -> Option<crate::Model> {
        self.build_terrain_where(file, zone, |_| true)
    }

    /// Builds the zone's terrain patches whose DictID `keep` takes into one
    /// model, None when it takes none. A room's own sprite (`VIZoneRoom`'s
    /// `m_p28`) is a patch of its zone's: what `VIZone::RenderRoom` draws of
    /// the room before its actors.
    pub fn build_terrain_where(&self, file: usize, zone: NodeId, keep: impl Fn(i32) -> bool) -> Option<crate::Model> {
        let c = self.file(file);
        let id = |p: NodeId| c.child(p, crate::types::SUBSPRITE_HDR).and_then(|h| c.body(h).get(..4)).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let patches: Vec<NodeId> = c.terrain_patches(zone).into_iter().filter(|&p| keep(id(p))).collect();
        if patches.is_empty() {
            return None;
        }
        crate::mesh::build_terrain(c, &patches, &c.pre_translations(zone), &|d| self.lookup(d).map(|r| (self.file(r.file), r.node)))
    }
}
