//! Lists the sound records of a container on the disc:
//! `cargo run --release -p esf --example sounds -- DATA/AMBTRACK.ESF`
fn main() {
    let path = std::env::args().nth(1).unwrap_or("DATA/AMBTRACK.ESF".into());
    let iso_path = std::env::var("EQOA_ISO").unwrap_or("run/eqoa.iso".into());
    let mut disc = iso::Iso::open(&iso_path).expect("disc");
    let c = esf::Container::open(disc.read(&path).expect("file")).expect("container");
    let (waves, xms) = esf::sound::all(&c);
    for w in &waves {
        println!("wave {:08x} ch {} blocks {} rate {} vol {} pan {} loop {}", w.id, w.channels, w.blocks, w.rate, w.volume, w.pan, w.looped);
    }
    for x in &xms {
        let m = &x.module;
        let r16 = |o: usize| i16::from_le_bytes([m[o], m[o + 1]]);
        println!(
            "xm {:08x} vol {} pan {} module {} samples {} | len {} ch {} pat {} ins {} flags {} speed {} tempo {}",
            x.id, x.volume, x.pan, m.len(), x.samples.len(), r16(4), r16(8), r16(10), r16(12), r16(14), r16(16), r16(18)
        );
    }
}
