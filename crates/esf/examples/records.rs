//! Counts a container's records of one type and prints the first few with
//! their children: `cargo run --release -p esf --example records -- DATA/TUNARIA.ESF 0xB100`
fn main() {
    let path = std::env::args().nth(1).unwrap_or("DATA/TUNARIA.ESF".into());
    let ty = u16::from_str_radix(std::env::args().nth(2).unwrap_or("0xB100".into()).trim_start_matches("0x"), 16).unwrap();
    let iso_path = std::env::var("EQOA_ISO").unwrap_or("run/eqoa.iso".into());
    let mut disc = iso::Iso::open(&iso_path).expect("disc");
    let c = esf::Container::open(disc.read(&path).expect("file")).expect("container");
    let all: Vec<usize> = (0..c.nodes.len()).filter(|&i| c.nodes[i].ty == ty).collect();
    println!("{} records of type {ty:#06x}", all.len());
    for &i in all.iter().take(6) {
        let n = c.node(i);
        let b = c.body(i);
        println!("node {i} v{} size {} body {:02x?}", n.version, n.size, &b[..b.len().min(64)]);
        for &k in &n.children {
            let m = c.node(k);
            println!("   child {:#06x} v{} size {} {:02x?}", m.ty, m.version, m.size, &c.body(k)[..m.size.min(24)]);
        }
    }
}
