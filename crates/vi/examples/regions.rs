//! The music regions (streamed-audio boxes) placed in a world file, with
//! where they are: `cargo run --release -p vi --example regions -- TUNARIA.ESF`
use vi::fpu::Ee;
use vi::world::SpriteKind;
fn main() {
    let file = std::env::args().nth(1).unwrap_or("TUNARIA.ESF".into());
    let mut iso = iso::Iso::open(std::env::var("EQOA_ISO").unwrap_or("run/eqoa.iso".into())).unwrap();
    let mut lib = esf::Library::new();
    let f = lib.add(&file, esf::Container::open(iso.read(&file).unwrap()).unwrap());
    let w = vi::world::World::<Ee>::load(&lib, f).unwrap();
    for (a, act) in w.actors.iter().enumerate() {
        let Some(s) = act.sprite else { continue };
        if let SpriteKind::Stream { path, looping, priority } = &w.sprites[s].kind {
            let b = w.sprites[s].bbox;
            println!(
                "actor {a:6} at ({:8.0} {:6.0} {:8.0}) scale {:.2} box {:.0}x{:.0}x{:.0} pri {priority} loop {looping} {path}",
                act.location.x.0, act.location.y.0, act.location.z.0, act.scale.0,
                (b.max.x - b.min.x).0, (b.max.y - b.min.y).0, (b.max.z - b.min.z).0
            );
        }
    }
}
