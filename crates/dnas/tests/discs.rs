//! `unpack` against what the game itself unpacked: each reference is what
//! `VILauncher::UnpackAndVerify` returned for a disc's `.AUT`, called in
//! PCSX2 by the tracer. Every module on hand must come out byte for byte.
//! SKIPs what is absent:
//!
//! - the Frontiers disc (`run/eqoa.iso`, or `$EQOA_ISO`), read through its
//!   ISO 9660 directory, against `run/support.bin` and `run/client.bin`;
//! - the other discs (`$EQOA_BUILDS`, else `../decomp-builds`):
//!   `extracted/<disc>/` holds `UTIL.REL` and the `.AUT`s as the disc has
//!   them, `modules/<disc>/<MODULE>.snr1` the references.
//!
//! The control: one bit flipped anywhere in a `.AUT` (its DNAS header, the
//! encrypted body, the trailer) is refused by the library itself, as a
//! `Dnas` error, where the untouched file unpacks.

use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn iso_path() -> PathBuf {
    std::env::var_os("EQOA_ISO").map(PathBuf::from).unwrap_or_else(|| root().join("run/eqoa.iso"))
}

fn builds() -> PathBuf {
    std::env::var_os("EQOA_BUILDS").map(PathBuf::from).unwrap_or_else(|| root().join("../decomp-builds"))
}

/// The first byte where two images differ, for the failure message.
fn first_difference(a: &[u8], b: &[u8]) -> Option<usize> {
    a.iter().zip(b).position(|(x, y)| x != y).or((a.len() != b.len()).then(|| a.len().min(b.len())))
}

fn check(what: &str, util: &[u8], aut: &[u8], want: &[u8]) {
    let got = dnas::unpack(util, aut).unwrap_or_else(|e| panic!("{what}: {e}"));
    assert!(got.len() == want.len() && got == want, "{what}: {} bytes where the game's are {}, first difference at {:?}", got.len(), want.len(), first_difference(&got, want));
}

#[test]
fn the_frontiers_disc_through_its_iso() {
    let Ok(mut disc) = iso::Iso::open(iso_path()) else {
        eprintln!("SKIP: no disc image at run/eqoa.iso ($EQOA_ISO)");
        return;
    };
    let util = disc.read("UTIL.REL").expect("UTIL.REL");
    let mut done = 0;
    std::thread::scope(|s| {
        for (module, reference) in [("SUPPORT", "run/support.bin"), ("CLIENT", "run/client.bin")] {
            let Ok(want) = std::fs::read(root().join(reference)) else {
                eprintln!("SKIP: {module}: no {reference}");
                continue;
            };
            let aut = disc.read(&format!("{module}.AUT")).expect("the .AUT");
            let util = &util;
            s.spawn(move || check(module, util, &aut, &want));
            done += 1;
        }
    });
    eprintln!("{done} Frontiers modules equal");
}

#[test]
fn every_other_disc_on_hand() {
    let builds = builds();
    let Ok(discs) = std::fs::read_dir(builds.join("extracted")) else {
        eprintln!("SKIP: no discs at {} ($EQOA_BUILDS)", builds.display());
        return;
    };
    let mut discs: Vec<PathBuf> = discs.filter_map(|d| d.ok().map(|d| d.path())).filter(|d| d.join("UTIL.REL").is_file()).collect();
    discs.sort();
    // one thread a disc, its modules in turn
    let counts: Vec<usize> = std::thread::scope(|s| {
        let handles: Vec<_> = discs
            .iter()
            .map(|dir| {
                let refs = builds.join("modules").join(dir.file_name().unwrap());
                s.spawn(move || {
                    let util = std::fs::read(dir.join("UTIL.REL")).unwrap();
                    let mut n = 0;
                    for module in ["SUPPORT", "CLIENT", "SETUP", "STATION", "MCERR"] {
                        let (Ok(aut), Ok(want)) = (std::fs::read(dir.join(format!("{module}.AUT"))), std::fs::read(refs.join(format!("{module}.snr1")))) else { continue };
                        check(&format!("{} {module}", dir.file_name().unwrap().to_string_lossy()), &util, &aut, &want);
                        n += 1;
                    }
                    n
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for (dir, n) in discs.iter().zip(&counts) {
        let name = dir.file_name().unwrap().to_string_lossy();
        if *n == 0 {
            eprintln!("{name}: no .AUT with a reference");
        } else {
            eprintln!("{name}: {n} modules equal");
        }
    }
}

/// `UTIL.REL` and `MCERR.AUT` (the smallest module) off whichever disc is
/// on hand.
fn some_disc() -> Option<(Vec<u8>, Vec<u8>)> {
    if let Ok(mut disc) = iso::Iso::open(iso_path()) {
        return Some((disc.read("UTIL.REL").ok()?, disc.read("MCERR.AUT").ok()?));
    }
    let dir = std::fs::read_dir(builds().join("extracted")).ok()?.filter_map(|d| d.ok().map(|d| d.path())).find(|d| d.join("MCERR.AUT").is_file())?;
    Some((std::fs::read(dir.join("UTIL.REL")).ok()?, std::fs::read(dir.join("MCERR.AUT")).ok()?))
}

#[test]
fn one_flipped_bit_is_refused() {
    let Some((util, aut)) = some_disc() else {
        eprintln!("SKIP: no disc on hand");
        return;
    };
    assert!(dnas::unpack(&util, &aut).is_ok(), "the untouched module unpacks");
    let n = aut.len();
    std::thread::scope(|s| {
        for (what, at) in [("header", 4), ("body", 256 + 1000), ("middle", n / 2), ("trailer", n - 8)] {
            let (util, mut bad) = (&util, aut.clone());
            bad[at] ^= 1;
            s.spawn(move || match dnas::unpack(util, &bad) {
                Err(dnas::Error::Dnas(..)) => {}
                r => panic!("a bit flipped in the {what} (byte {at}): {:?}", r.map(|m| m.len())),
            });
        }
    });
}
