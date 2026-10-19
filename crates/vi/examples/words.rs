//! Prints WORDS.TXT entries off the disc: `words <iso> <id>...` (column 0).
fn main() {
    let mut args = std::env::args().skip(1);
    let iso = args.next().expect("iso");
    let mut disc = iso::Iso::open(std::path::Path::new(&iso)).expect("disc");
    let t = vi::strtab::StringTable::parse(&disc.read("WORDS.TXT").expect("WORDS.TXT")).expect("table");
    for a in args {
        let id: i32 = a.parse().expect("id");
        println!("{id}: {}", t.get(id, 0));
    }
}
