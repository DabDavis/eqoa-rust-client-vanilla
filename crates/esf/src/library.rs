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

use crate::{Container, NodeId, Surface};
use std::collections::HashMap;

/// Which container, and which record in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ref {
    pub file: usize,
    pub node: NodeId,
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
    /// world at its record's offset.
    rank: Vec<(usize, u64)>,
    /// (world file, zone index) -> the file holding that zone, once loaded.
    zone_files: HashMap<(usize, usize), usize>,
}

impl Library {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a container after every one already loaded; its DictIDs register
    /// only where no earlier file has them. Returns its index.
    pub fn add(&mut self, name: impl Into<String>, c: Container) -> usize {
        let file = self.files.len();
        self.files.push((name.into(), c));
        self.rank.push((file, 0));
        self.register(file);
        file
    }

    /// Registers file `file`'s DictIDs: where an id is absent, or held by a
    /// file that ranks after this one.
    fn register(&mut self, file: usize) {
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

    /// Puts `c` in place of file `i` (a world change) and registers every
    /// DictID again in rank order, so the first registration still wins. The
    /// old world's loaded zones are dropped (their slots left empty, so every
    /// other file keeps its index).
    pub fn replace(&mut self, i: usize, name: impl Into<String>, c: Container) {
        self.files[i] = (name.into(), c);
        let gone: Vec<(usize, usize)> = self.zone_files.keys().filter(|k| k.0 == i).copied().collect();
        for k in gone {
            if let Some(f) = self.zone_files.remove(&k) {
                self.files[f].1 = Container::empty();
            }
        }
        self.dict.clear();
        for file in 0..self.files.len() {
            self.register(file);
        }
    }

    pub fn file(&self, i: usize) -> &Container {
        &self.files[i].1
    }

    pub fn lookup(&self, dict_id: i32) -> Option<Ref> {
        self.dict.get(&dict_id).copied()
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
    pub fn add_zone(&mut self, world: usize, zi: usize, c: Container) -> Option<usize> {
        if let Some(&f) = self.zone_files.get(&(world, zi)) {
            return Some(f);
        }
        let (off, _) = self.zone_range(world, zi)?;
        let file = self.files.len();
        let name = format!("{}#{zi}", self.files[world].0);
        self.files.push((name, c));
        self.rank.push((world, off));
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
        let c = self.file(file);
        crate::mesh::build_terrain(c, &c.terrain_patches(zone), &c.pre_translations(zone), &|d| {
            self.lookup(d).map(|r| (self.file(r.file), r.node))
        })
    }
}
