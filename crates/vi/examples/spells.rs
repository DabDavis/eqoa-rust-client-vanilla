//! The spell effects of SPELLFX.CSF: how many, their event types, the orbit
//! and projectile flags they use, and what their ids name:
//! `cargo run --release -p vi --example spells`
use std::collections::BTreeMap;
use vi::fpu::Ieee;
fn main() {
    let mut iso = iso::Iso::open(std::env::var("EQOA_ISO").unwrap_or("run/eqoa.iso".into())).unwrap();
    let c = esf::Container::open(iso.read("DATA2/SPELLFX.CSF").unwrap()).unwrap();
    let mut types: BTreeMap<i32, usize> = BTreeMap::new();
    let mut orbits = 0;
    let mut flights: BTreeMap<(i32, i32), usize> = BTreeMap::new();
    let mut items = 0;
    let mut kinds: BTreeMap<(i32, String), usize> = BTreeMap::new();
    let mut n = 0;
    for i in 0..c.nodes.len() {
        if c.nodes[i].ty != 0xC200 {
            continue;
        }
        let e = vi::spell::SpellEffect::<Ieee>::parse(&c, i, &mut |k, id| {
            let what = match c.lookup(id as i32) {
                Some(r) => format!("{:#06x}", c.node(r).ty),
                None => "missing".into(),
            };
            *kinds.entry((k, what)).or_default() += 1;
            0
        });
        let Some(e) = e else { continue };
        n += 1;
        for ev in e.lists.iter().flatten() {
            *types.entry(ev.ty).or_default() += 1;
            if ev.ty == 3 && (ev.p0c >> 20) & 3 != 0 {
                orbits += 1;
            }
            if ev.ty == 8 {
                *flights.entry(((ev.p0c >> 8) & 0xF, ev.p0c & 0xFF)).or_default() += 1;
                if (ev.p0c >> 12) & 1 != 0 {
                    items += 1;
                }
            }
        }
    }
    println!("{n} effects; events by type {types:?}");
    println!("{orbits} emitters orbit; projectiles by (flight, source) {flights:?}, {items} carry the hand's item");
    for (k, v) in kinds {
        println!("id kind {} -> {}: {v}", k.0, k.1);
    }
}
