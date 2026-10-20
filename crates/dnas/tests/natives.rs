//! The natives (`src/native.rs`) against the interpreter running the
//! library's own code, which is the reference: every call of a whole unpack,
//! then the pure ones on random arguments (`Machine::check_natives`: the
//! registers a call keeps, its result, and every byte either run wrote). A
//! native that is wrong in its result, in memory or in a stray write fails
//! it (the controls). SKIPs without the disc.

use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn disc() -> Option<iso::Iso> {
    let path = std::env::var_os("EQOA_ISO").map(PathBuf::from).unwrap_or_else(|| root().join("run/eqoa.iso"));
    match iso::Iso::open(&path) {
        Ok(d) => Some(d),
        Err(_) => {
            eprintln!("SKIP: no disc image at {}", path.display());
            None
        }
    }
}

fn report(m: &dnas::Machine) {
    let c = m.check.as_ref().unwrap();
    for (name, calls, checked, declined) in &c.counts {
        eprintln!("{name:20} {calls:>8} calls, {checked:>8} checked, {declined:>6} declined");
    }
    for f in c.failures.iter().take(10) {
        eprintln!("  {f}");
    }
}

/// Two whole unpacks, every native call checked: SUPPORT with every native,
/// and CLIENT with the CBC decrypt update declining, so that the block
/// cipher it subsumes is reached on its own. Each native is checked in one
/// of them.
#[test]
fn every_call_of_an_unpack() {
    let Some(mut disc) = disc() else { return };
    let util = disc.read("UTIL.REL").unwrap();
    let mut checked: std::collections::BTreeMap<&str, u64> = Default::default();
    for (module, reference, cbc) in [("SUPPORT", "run/support.bin", true), ("CLIENT", "run/client.bin", false)] {
        let aut = disc.read(&format!("{module}.AUT")).unwrap();
        let mut m = dnas::Machine::new(&util).unwrap();
        assert_eq!(m.natives().len(), 10, "the Frontiers library's functions are the ones proved: {:?}", m.native_checksums());
        if !cbc {
            assert!(m.replace_native("CBC decrypt", |_, _| None));
        }
        m.check_natives(u64::MAX, 1);
        let out = m.unpack(&aut).unwrap();
        eprintln!("{module}{}:", if cbc { "" } else { ", the CBC update interpreted" });
        report(&m);
        let c = m.check.as_ref().unwrap();
        assert!(c.failures.is_empty(), "{module}: {} calls differed", c.failures.len());
        for (name, _, n, _) in &c.counts {
            *checked.entry(name).or_default() += n;
        }
        if let Ok(want) = std::fs::read(root().join(reference)) {
            assert!(out == want, "{module} is not the game's");
        }
    }
    for (name, n) in checked {
        assert!(n > 0, "{name} was never checked");
    }
}

/// A small fast generator (xorshift64*).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

/// Doubles from every class and the ranges the functions branch on.
fn double(r: &mut Rng) -> u64 {
    let pick = r.next();
    let sign = (r.next() & 1) << 63;
    let frac = r.next() & 0x000F_FFFF_FFFF_FFFF;
    match pick % 8 {
        0 => r.next(),                                          // any bits
        1 => sign | ((1023 + (r.next() % 64)) << 52) | frac,    // 1 .. 2^64
        2 => sign | ((1023 - (r.next() % 64)) << 52) | frac,    // 2^-63 .. 1
        3 => sign | ((r.next() % 0x7FF) << 52) | frac,          // any finite exponent
        4 => sign | frac,                                       // zero, denormals
        5 => sign | (0x7FF << 52) | (frac & (r.next() % 2)),     // inf, NaN
        6 => (((r.next() as u32) as i32) as f64).to_bits(),     // whole numbers
        _ => sign | (((1023 + 31) + (r.next() % 3)) << 52) | frac, // near 2^32
    }
}

fn pure_calls(m: &mut dnas::Machine, rng: &mut Rng, n: usize) {
    let f = |m: &dnas::Machine, name: &str| m.export(name).unwrap();
    let (muldi3, dpmul, litodp, dptoul) = (f(m, "__muldi3"), f(m, "dpmul"), f(m, "litodp"), f(m, "dptoul"));
    let edge = [0u64, 1, u64::MAX, 1 << 31, 1 << 32, 1 << 63, (1 << 32) - 1, 0x7FFF_FFFF_FFFF_FFFF];
    for k in 0..n {
        let (a, b) = if k < edge.len() * edge.len() { (edge[k / edge.len()], edge[k % edge.len()]) } else { (rng.next(), rng.next()) };
        m.call_regs(muldi3, &[a, b]).unwrap();
        m.call_regs(dpmul, &[double(rng), double(rng)]).unwrap();
        let w = if k % 4 == 0 { rng.next() } else { rng.next() as u32 as i32 as i64 as u64 };
        m.call_regs(litodp, &[w]).unwrap();
        m.call_regs(dptoul, &[double(rng)]).unwrap();
    }
}

#[test]
fn pure_natives_on_random_arguments() {
    let Some(mut disc) = disc() else { return };
    let util = disc.read("UTIL.REL").unwrap();
    let mut m = dnas::Machine::new(&util).unwrap();
    m.check_natives(u64::MAX, 1);
    pure_calls(&mut m, &mut Rng(0x9E37_79B9_7F4A_7C15), 50_000);
    report(&m);
    let c = m.check.as_ref().unwrap();
    assert!(c.failures.is_empty(), "{} calls differed", c.failures.len());
    // dpmul declines what is not normal; the rest it computes
    for (name, calls, checked, _) in &c.counts {
        if ["__muldi3", "dpmul", "litodp", "dptoul"].contains(name) {
            assert!(*calls >= 50_000 && *checked > 10_000, "{name}: {calls} calls, {checked} checked");
        }
    }
}

type Run = fn(&mut dnas::Cpu, &mut dnas::Mem) -> Option<()>;

#[test]
fn a_wrong_native_is_caught() {
    let Some(mut disc) = disc() else { return };
    let util = disc.read("UTIL.REL").unwrap();
    let aut = disc.read("SUPPORT.AUT").unwrap();
    let controls: [(&str, &str, Run); 3] = [
        // one off in its result
        ("a wrong result", "__muldi3", |cpu, _| {
            cpu.gpr[2] = cpu.gpr[4].wrapping_mul(cpu.gpr[5]).wrapping_add(1);
            Some(())
        }),
        // the right registers, the wrong bytes out (a block of zeros)
        ("wrong bytes", "A_RC6Encrypt_20", |cpu, mem| {
            let out = cpu.gpr[5] as u32;
            (0..4).try_for_each(|k| mem.w32(out + 4 * k, 0))
        }),
        // the right result, and a byte written where the function writes none
        ("a stray write", "__muldi3", |cpu, mem| {
            cpu.gpr[2] = cpu.gpr[4].wrapping_mul(cpu.gpr[5]);
            mem.w8(0x0040_0000, 0x5A)
        }),
    ];
    for (what, name, run) in controls {
        let mut m = dnas::Machine::new(&util).unwrap();
        assert!(m.replace_native(name, run));
        m.check_natives(u64::MAX, 1);
        if name == "A_RC6Encrypt_20" {
            // the unpack itself would fail on the wrong blocks: the check
            // keeps the interpreter's, so it goes on and only reports them
            let _ = m.unpack(&aut);
        } else {
            pure_calls(&mut m, &mut Rng(7), 200);
        }
        let failures = m.check.as_ref().unwrap().failures.len();
        eprintln!("{what} ({name}): {failures} calls differed");
        assert!(failures > 0, "{what} in {name} went unseen");
    }
}
