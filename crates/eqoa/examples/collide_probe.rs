// Loads TUNARIA's collision world through vi::world and drops CalcFloor-style
// probes: what the game's own collision says the ground is.
use vi::coll::CollSphere;
use vi::fpu::{Ee, F};
use vi::scene::{Filter, FilterKind};
use vi::vect::Vect3;

fn main() {
    let mut iso = iso::Iso::open("run/eqoa.iso").unwrap();
    let mut lib = esf::Library::new();
    let t = std::time::Instant::now();
    let f = lib.add("TUNARIA.ESF", esf::Container::open(iso.read("TUNARIA.ESF").unwrap()).unwrap());
    let read = t.elapsed();
    let mut w = vi::world::World::<Ee>::load(&lib, f).unwrap();
    let rooms: usize = w.zones.iter().map(|z| z.rooms.len()).sum();
    let with_sprite: usize = w.zones.iter().map(|z| z.rooms.iter().filter(|r| r.sprite.is_some()).count()).sum();
    let placed = w.actors.iter().filter(|a| a.flags & 4 == 0).count();
    let occupancy: usize = w.zones.iter().map(|z| z.rooms.iter().map(|r| r.actors.len()).sum::<usize>()).sum();
    println!(
        "read {read:.2?}, built {:.2?}: {} zones, {rooms} rooms ({with_sprite} with a sprite), {} sprites, {} buffers, {} actors ({placed} placed, {occupancy} room links)",
        t.elapsed() - read,
        w.zones.len(),
        w.sprites.len(),
        w.buffers.len(),
        w.actors.len()
    );
    for (x, z) in [(25000.0, 15000.0), (24900.0, 15000.0), (24600.0, 14400.0), (23900.0, 15300.0)] {
        let from = Vect3::<Ee>::of(x, 400.0, z);
        let to = Vect3::<Ee>::of(x, -100.0, z);
        let mut s = CollSphere::default();
        s.init(from, to, F::new(0.001));
        let mut filt = Filter::new(FilterKind::WalkFloor, -1);
        let hit = w.collide_sphere(&mut s, &mut filt);
        let p = s.hit_point();
        println!(
            "({x}, {z}): hit {hit} y {:.3} normal ({:.3}, {:.3}, {:.3}) actor {} env {:#x}",
            p.y.0,
            s.ray.hit_normal.x.0,
            s.ray.hit_normal.y.0,
            s.ray.hit_normal.z.0,
            filt.hit_actor,
            w.env_flags(&p)
        );
    }
}
