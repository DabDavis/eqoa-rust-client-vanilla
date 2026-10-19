//! A world opened with its zones on the disc (`Container::open_deferred`)
//! against the same file read and parsed whole: the world-level records are
//! the same records (types, versions, sizes, bytes, DictIDs), every zone is a
//! stub that names its own header and length, only a few kilobytes are read,
//! and a zone parsed on its own (`Container::parse_records`) is the same
//! subtree the whole parse made. Each check has a control that must fail.

use esf::{Container, NodeId, ReadAt, Window};
use std::collections::HashMap;
use std::fs::File;

/// A source that counts what is read through it.
struct Counting<R> {
    inner: R,
    bytes: u64,
    reads: u64,
}

impl<R: ReadAt> ReadAt for Counting<R> {
    fn len(&self) -> u64 {
        self.inner.len()
    }
    fn read_at(&mut self, off: u64, buf: &mut [u8]) -> std::io::Result<()> {
        self.bytes += buf.len() as u64;
        self.reads += 1;
        self.inner.read_at(off, buf)
    }
}

fn disc() -> Option<(iso::Iso, std::path::PathBuf)> {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    if !p.exists() {
        eprintln!("SKIP: no disc at {}", p.display());
        return None;
    }
    Some((iso::Iso::open(&p).unwrap(), p))
}

/// The file inside the image, as a source.
fn window(disc: &iso::Iso, path: &std::path::Path, name: &str) -> Window<File> {
    let e = disc.find(name).unwrap_or_else(|| panic!("{name}: not on the disc"));
    Window::new(File::open(path).unwrap(), e.lba as u64 * iso::SECTOR, e.size as u64)
}

/// Walks the two trees side by side from the roots and maps the deferred
/// container's nodes to the whole one's. Kept records must match exactly;
/// walked-through ones (no bytes) by type and child count; a stub must name
/// the whole parse's zone record (header offset, length).
fn compare(def: &Container, full: &Container) -> Result<HashMap<NodeId, NodeId>, String> {
    let mut map = HashMap::new();
    if def.roots.len() != full.roots.len() {
        return Err(format!("{} roots against {}", def.roots.len(), full.roots.len()));
    }
    let mut stack: Vec<(NodeId, NodeId)> = def.roots.iter().copied().zip(full.roots.iter().copied()).collect();
    while let Some((d, f)) = stack.pop() {
        let (dn, fn_) = (def.node(d), full.node(f));
        if dn.ty != fn_.ty || dn.version != fn_.version {
            return Err(format!("node {d}: type {:04X} v{} against {:04X} v{}", dn.ty, dn.version, fn_.ty, fn_.version));
        }
        map.insert(d, f);
        if let Some((off, len)) = dn.deferred {
            if off != (fn_.body - 12) as u64 || len != fn_.size + 12 {
                return Err(format!("zone stub {d}: ({off}, {len}) against ({}, {})", fn_.body - 12, fn_.size + 12));
            }
            continue;
        }
        if dn.size == 0 && fn_.size != 0 {
            // walked through: its children must line up
        } else if def.body(d) != full.body(f) {
            return Err(format!("node {d} ({:04X}): body differs", dn.ty));
        }
        if dn.children.len() != fn_.children.len() {
            return Err(format!("node {d} ({:04X}): {} children against {}", dn.ty, dn.children.len(), fn_.children.len()));
        }
        stack.extend(dn.children.iter().copied().zip(fn_.children.iter().copied()));
    }
    Ok(map)
}

#[test]
fn tunaria_opens_with_its_zones_on_the_disc() {
    let Some((disc, path)) = disc() else { return };
    let mut src = Counting { inner: window(&disc, &path, "TUNARIA.ESF"), bytes: 0, reads: 0 };
    let def = Container::open_deferred(&mut src).unwrap();
    eprintln!("deferred open: {} bytes in {} reads, {} records kept, {} bytes held", src.bytes, src.reads, def.nodes.len(), def.data.len());
    assert!(src.bytes < 1 << 20, "{} bytes read", src.bytes);
    let full = Container::open(esf::read_all(&mut window(&disc, &path, "TUNARIA.ESF")).unwrap()).unwrap();
    let map = compare(&def, &full).unwrap();
    assert_eq!(def.zones().len(), 175);
    assert_eq!(def.zones().len(), full.zones().len());
    assert!(def.zones().iter().all(|&z| def.node(z).deferred.is_some()));
    // The DictIDs of the world-level records: the whole parse's, minus those
    // inside zones (and there are none outside them in a world file).
    let inside: std::collections::HashSet<NodeId> = {
        let mut s = std::collections::HashSet::new();
        let mut st: Vec<NodeId> = full.zones();
        while let Some(n) = st.pop() {
            s.insert(n);
            st.extend(full.node(n).children.iter().copied());
        }
        s
    };
    let want: HashMap<i32, NodeId> = full.dict.iter().filter(|(_, n)| !inside.contains(n)).map(|(&k, &v)| (k, v)).collect();
    let got: HashMap<i32, NodeId> = def.dict.iter().map(|(&k, &v)| (k, map[&v])).collect();
    assert_eq!(got, want);
    eprintln!("world-level DictIDs: {}", got.len());

    // Control: a container whose kept record is changed by one byte differs.
    let mut bad = Container::open_deferred(&mut window(&disc, &path, "TUNARIA.ESF")).unwrap();
    let k = bad.nodes.iter().position(|n| n.deferred.is_none() && n.size > 0).unwrap();
    let at = bad.nodes[k].body;
    bad.data[at] ^= 1;
    assert!(compare(&bad, &full).is_err(), "a changed byte must show");
}

#[test]
fn a_zone_on_its_own_is_the_whole_parse_subtree() {
    let Some((disc, path)) = disc() else { return };
    let mut w = window(&disc, &path, "TUNARIA.ESF");
    let def = Container::open_deferred(&mut w).unwrap();
    let full = Container::open(esf::read_all(&mut w).unwrap()).unwrap();
    // Freeport's river (84), the coast north of it (97) and the zones round them.
    for zi in [83, 84, 85, 96, 97, 98, 70, 71, 72] {
        let (off, len) = def.node(def.zones()[zi]).deferred.unwrap();
        let mut bytes = vec![0u8; len];
        w.read_at(off, &mut bytes).unwrap();
        let zc = Container::parse_records(bytes);
        let fz = full.zones()[zi];
        assert_eq!(zc.roots.len(), 1, "zone {zi}");
        let map = same_subtree(&zc, zc.roots[0], &full, fz).unwrap_or_else(|e| panic!("zone {zi}: {e}"));
        // its DictIDs are the whole parse's that point into this zone, except
        // where an earlier record of the file had the id first
        for (&id, &n) in &zc.dict {
            let f = map[&n];
            match full.dict.get(&id) {
                Some(&g) if g == f => {}
                Some(&g) => assert!(full.node(g).body < full.node(fz).body, "zone {zi}: {id:#x} held by a later record"),
                None => panic!("zone {zi}: {id:#x} not in the whole parse"),
            }
        }
        let surfaces_in_zone = {
            let mut n = 0;
            let mut st = vec![fz];
            while let Some(x) = st.pop() {
                n += (full.node(x).ty == esf::types::SURFACE) as usize;
                st.extend(full.node(x).children.iter().copied());
            }
            n
        };
        assert_eq!(zc.surfaces.len(), surfaces_in_zone, "zone {zi}");
        // Control: the range one byte late is not the zone.
        let mut late = vec![0u8; len];
        w.read_at(off + 1, &mut late).unwrap();
        let lc = Container::parse_records(late);
        assert!(lc.roots.first().is_none_or(|&r| same_subtree(&lc, r, &full, fz).is_err()), "zone {zi}: a shifted range must not match");
    }
}

/// The subtree at `a` in `ca` against the one at `b` in `cb`: types,
/// versions, sizes and bodies, child by child.
fn same_subtree(ca: &Container, a: NodeId, cb: &Container, b: NodeId) -> Result<HashMap<NodeId, NodeId>, String> {
    let mut map = HashMap::new();
    let mut st = vec![(a, b)];
    while let Some((x, y)) = st.pop() {
        let (nx, ny) = (ca.node(x), cb.node(y));
        if nx.ty != ny.ty || nx.version != ny.version || nx.size != ny.size || ca.body(x) != cb.body(y) || nx.children.len() != ny.children.len() {
            return Err(format!("record {:04X} differs from {:04X}", nx.ty, ny.ty));
        }
        map.insert(x, y);
        st.extend(nx.children.iter().copied().zip(ny.children.iter().copied()));
    }
    Ok(map)
}
