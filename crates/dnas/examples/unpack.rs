//! Unpacks one DNAS module off a disc image with the disc's own library.
//!
//!     cargo run --release -p dnas --example unpack -- <iso> SUPPORT [out.snr1] [--trace]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let trace = args.iter().any(|a| a == "--trace");
    let profile = args.iter().any(|a| a == "--profile");
    let args: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let (iso, name) = (args.first().ok_or("usage: unpack <iso> <MODULE> [out]")?, args.get(1).ok_or("module?")?);
    // a disc image, or a directory of its files
    let (util, aut) = if std::path::Path::new(iso.as_str()).is_dir() {
        let d = std::path::Path::new(iso.as_str());
        (std::fs::read(d.join("UTIL.REL"))?, std::fs::read(d.join(format!("{name}.AUT")))?)
    } else {
        let mut disc = iso::Iso::open(iso.as_str())?;
        (disc.read("UTIL.REL")?, disc.read(&format!("{name}.AUT"))?)
    };
    let t = std::time::Instant::now();
    let mut m = dnas::Machine::new(&util)?;
    m.trace = trace;
    if profile {
        m.profile = Some(vec![0; util.len() / 4 + 1]);
    }
    let r = m.unpack(&aut);
    eprintln!("{} instructions in {:.2?}", m.cpu.insns, t.elapsed());
    if trace {
        for (n, c) in &m.calls {
            eprintln!("{c:>9} {n}");
        }
    }
    if let Some(p) = &m.profile {
        // by the nearest export at or before each word
        let ex = m.exports();
        let mut by: std::collections::HashMap<&str, u64> = Default::default();
        for (k, &c) in p.iter().enumerate() {
            if c == 0 {
                continue;
            }
            let a = dnas::UTIL_BASE + 4 * k as u32;
            let i = ex.partition_point(|(e, _)| *e <= a);
            let name = if i == 0 { "?" } else { ex[i - 1].1.as_str() };
            *by.entry(name).or_default() += c;
        }
        let mut v: Vec<_> = by.into_iter().collect();
        v.sort_by_key(|x| std::cmp::Reverse(x.1));
        let total: u64 = v.iter().map(|x| x.1).sum();
        for (n, c) in v.iter().take(40) {
            eprintln!("{:>12} {:>5.1}% {n}", c, 100.0 * *c as f64 / total as f64);
        }
    }
    let module = r?;
    eprintln!("{name}: {} bytes", module.len());
    if let Some(out) = args.get(2) {
        std::fs::write(out.as_str(), &module)?;
    }
    Ok(())
}
