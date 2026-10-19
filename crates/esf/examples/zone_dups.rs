//! Which DictIDs more than one zone of a world registers, and whether their
//! records are the same bytes; and how many actors name a sprite their own
//! zone does not hold. `cargo run --release -p esf --example zone_dups [WORLD.ESF]`
use std::collections::HashMap;
fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "TUNARIA.ESF".into());
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    let disc = iso::Iso::open(&p).unwrap();
    let e = disc.find(&name).unwrap();
    let mut w = esf::Window::new(std::fs::File::open(&p).unwrap(), e.lba as u64 * iso::SECTOR, e.size as u64);
    let def = esf::Container::open_deferred(&mut w).unwrap();
    let mut seen: HashMap<i32, (usize, Vec<u8>)> = HashMap::new();
    let (mut dups, mut differ, mut cross, mut actors) = (0, 0, 0, 0);
    for (zi, &z) in def.zones().iter().enumerate() {
        let (off, len) = def.node(z).deferred.unwrap();
        let mut b = vec![0u8; len];
        esf::ReadAt::read_at(&mut w, off, &mut b).unwrap();
        let c = esf::Container::parse_records(b);
        for (&id, &n) in &c.dict {
            let node = c.node(n);
            let bytes = c.data[node.body - 12..node.body + node.size].to_vec();
            match seen.get(&id) {
                Some((_, old)) => {
                    dups += 1;
                    if *old != bytes {
                        differ += 1;
                    }
                }
                None => {
                    seen.insert(id, (zi, bytes));
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
