//! Which DictIDs more than one zone of a world registers, and whether their
//! records are the same bytes; how many actors name a sprite their own zone
//! does not hold; and, with a zone index, which of that zone's DictIDs an
//! earlier zone registered with other bytes (what a whole-file lookup shows
//! in its place). `cargo run --release -p esf --example zone_dups [WORLD.ESF [zone]]`
use std::collections::HashMap;
fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "TUNARIA.ESF".into());
    let focus: Option<usize> = std::env::args().nth(2).and_then(|z| z.parse().ok());
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    let disc = iso::Iso::open(&p).unwrap();
    let e = disc.find(&name).unwrap();
    let mut w = esf::Window::new(std::fs::File::open(&p).unwrap(), e.lba as u64 * iso::SECTOR, e.size as u64);
    let def = esf::Container::open_deferred(&mut w).unwrap();
    let mut seen: HashMap<i32, (usize, u16, Vec<u8>)> = HashMap::new();
    let (mut dups, mut differ, mut cross, mut actors) = (0, 0, 0, 0);
    for (zi, &z) in def.zones().iter().enumerate() {
        let (off, len) = def.node(z).deferred.unwrap();
        let mut b = vec![0u8; len];
        esf::ReadAt::read_at(&mut w, off, &mut b).unwrap();
        let c = esf::Container::parse_records(b);
        let mut ids: Vec<(&i32, &usize)> = c.dict.iter().collect();
        ids.sort();
        for (&id, &n) in ids {
            let node = c.node(n);
            let bytes = c.data[node.body - 12..node.body + node.size].to_vec();
            match seen.get(&id) {
                Some((first, ty, old)) => {
                    dups += 1;
                    if *old != bytes {
                        differ += 1;
                        if focus == Some(zi) {
                            println!("zone {zi}: {id:#010x} ({:04X}, {} bytes) first registered by zone {first} ({ty:04X}, {} bytes)", node.ty, bytes.len(), old.len());
                        }
                    }
                }
                None => {
                    seen.insert(id, (zi, node.ty, bytes));
                }
            }
        }
        let root = c.roots[0];
        for a in c.actors(root) {
            actors += 1;
            if !c.dict.contains_key(&a.sprite) {
                cross += 1;
            }
        }
    }
    println!("{name}: {} DictIDs, {dups} registered again by a later zone ({differ} with different bytes); {actors} actors, {cross} name a sprite outside their zone", seen.len());
}
