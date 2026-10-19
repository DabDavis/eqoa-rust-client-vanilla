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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ref {
    pub file: usize,
    pub node: NodeId,
}

#[derive(Default)]
pub struct Library {
    pub files: Vec<(String, Container)>,
    dict: HashMap<i32, Ref>,
}

impl Library {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a container after every one already loaded; its DictIDs register
    /// only where no earlier file has them. Returns its index.
    pub fn add(&mut self, name: impl Into<String>, c: Container) -> usize {
        let file = self.files.len();
        for (&id, &node) in &c.dict {
            self.dict.entry(id).or_insert(Ref { file, node });
        }
        self.files.push((name.into(), c));
        file
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
