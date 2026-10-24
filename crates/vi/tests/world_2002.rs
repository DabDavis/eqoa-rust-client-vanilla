//! Tunaria off the 2002 disc (EverQuest Online Adventures, USA, SLUS-20470),
//! the world the client puts in Frontiers' place while the 2002 disc's art is
//! drawn (F9). Its zone proxy table is version 1, each zone's 56 bytes
//! followed by its name, where Frontiers' is version 2: not read (as before),
//! the 2002 world has no zones to stream; read as Frontiers', every zone's
//! bounds after the first are off. The two worlds are the same ground: every
//! zone's bounds agree, and floors probed round the Freeport spawn land at
//! the same heights on both.

use esf::{Container, Library, Window};
use vi::coll::CollSphere;
use vi::fpu::{F, Ieee};
use vi::scene::{Filter, FilterKind};
use vi::vect::Vect3;
use vi::world::World;

/// A record: its children packed at the start of its body, then `extra`.
fn rec(ty: u16, version: i16, children: &[Vec<u8>], extra: &[u8]) -> Vec<u8> {
    let body: Vec<u8> = children.iter().flatten().copied().chain(extra.iter().copied()).collect();
    let mut d = ty.to_le_bytes().to_vec();
    d.extend_from_slice(&version.to_le_bytes());
    d.extend_from_slice(&(body.len() as i32).to_le_bytes());
    d.extend_from_slice(&(children.len() as u32).to_le_bytes());
    d.extend(body);
    d
}

/// An OBJF file of root records.
fn file(records: &[Vec<u8>]) -> Container {
    let mut d = b"FJBO".to_vec();
    d.resize(24, 0);
    d[16..24].copy_from_slice(&24i64.to_le_bytes());
    for r in records {
        d.extend_from_slice(r);
    }
    Container::parse(d).unwrap()
}

/// One proxy as `ParseWorldZoneProxies` reads it: two offsets, a size, a
/// centre, the bounds.
fn proxy(min: [f32; 3], max: [f32; 3]) -> Vec<u8> {
    let mut d = vec![0u8; 20];
    for v in [[0.0; 3], min, max] {
        for x in v {
            d.extend_from_slice(&x.to_le_bytes());
        }
    }
    d
}

/// A proxy table of `boxes`, each named as the 2002 disc names its zones
/// when `named`.
fn table(version: i16, named: bool, boxes: &[([f32; 3], [f32; 3])]) -> Vec<u8> {
    let mut d = (boxes.len() as i32).to_le_bytes().to_vec();
    for (k, (a, b)) in boxes.iter().enumerate() {
        d.extend(proxy(*a, *b));
        if named {
            let name = format!("zone{k:04}.esf");
            d.extend_from_slice(&(name.len() as u16).to_le_bytes());
            d.extend_from_slice(name.as_bytes());
        }
    }
    rec(0x8210, version, &[], &d)
}

#[test]
fn a_version_1_proxy_table_names_each_zone() {
    let boxes = [([0.0, 0.0, 0.0], [10.0, 5.0, 10.0]), ([10.0, 0.0, 0.0], [20.0, 5.0, 10.0]), ([20.0, 1.0, 0.0], [30.0, 6.0, 10.0])];
    let read = |t: Vec<u8>| {
        let c = file(&[rec(0x8200, 2, &[t], &[])]);
        World::<Ieee>::proxies(&c, c.roots[0]).iter().map(|b| [b.min.x.0, b.min.y.0, b.max.x.0, b.max.y.0]).collect::<Vec<_>>()
    };
    let want: Vec<[f32; 4]> = boxes.iter().map(|(a, b)| [a[0], a[1], b[0], b[1]]).collect();
    assert_eq!(read(table(2, false, &boxes)), want, "Frontiers' version 2");
    assert_eq!(read(table(1, true, &boxes)), want, "the 2002 disc's version 1, its names passed over");
    // the control: the names read as bounds (version 1's table taken as
    // version 2's) put every zone after the first off
    let wrong = read(table(2, true, &boxes));
    assert_eq!(wrong[0], want[0]);
    assert_ne!(wrong[1..], want[1..]);
}

/// The two discs: `run/eqoa.iso` (or `$EQOA_ISO`) and `$EQOA_2002_ISO` (or the
/// USA disc under `../decomp-builds/discs/eqoa-usa/`).
fn discs() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let frontiers = std::env::var_os("EQOA_ISO").map(std::path::PathBuf::from).unwrap_or_else(|| root.join("run/eqoa.iso"));
    let y2002 = std::env::var_os("EQOA_2002_ISO").map(std::path::PathBuf::from).or_else(|| {
        std::fs::read_dir(root.join("../decomp-builds/discs/eqoa-usa")).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("iso")))
    });
    match (frontiers.exists(), y2002) {
        (true, Some(y)) => Some((frontiers, y)),
        _ => {
            eprintln!("SKIP: no Frontiers disc or no 2002 disc (set EQOA_ISO and EQOA_2002_ISO)");
            None
        }
    }
}

/// The Freeport spawn (where the server puts a new character).
const SPAWN: (f32, f32) = (25393.7, 15749.5);

/// Tunaria off `disc`, opened with its zones on the disc, the zones within
/// 1,050 of the spawn read in, and its collision world: the zones' bounds,
/// the world, the zones read and the zone the spawn is in.
fn tunaria(disc: &std::path::Path) -> (Vec<[f32; 6]>, World<Ieee>, Vec<usize>, Option<usize>) {
    let iso = iso::Iso::open(disc).unwrap();
    let e = iso.find("TUNARIA.ESF").unwrap().clone();
    let mut src = Window::new(std::fs::File::open(disc).unwrap(), e.lba as u64 * iso::SECTOR, e.size as u64);
    let mut lib = Library::new();
    let f = lib.add("TUNARIA.ESF", Container::open_deferred(&mut src).unwrap());
    let c = lib.file(f);
    let base = c.child(c.roots[0], 0x8100).and_then(|w| c.child(w, 0x8200)).or_else(|| c.child(c.roots[0], 0x8200)).unwrap();
    let bounds = World::<Ieee>::proxies(c, base);
    let here = World::<Ieee>::zone_at(c, SPAWN.0, SPAWN.1);
    let at = Vect3::<Ieee>::of(SPAWN.0, 0.0, SPAWN.1);
    let near: Vec<usize> = (0..bounds.len())
        .filter(|&z| {
            let mut b = bounds[z];
            b.min.y = F::new(-1e9);
            b.max.y = F::new(1e9);
            b.distance_squared(&at).0 < 1050.0 * 1050.0
        })
        .collect();
    for &zi in &near {
        let (off, len) = lib.zone_range(f, zi).unwrap();
        let mut b = vec![0u8; len];
        esf::ReadAt::read_at(&mut src, off, &mut b).unwrap();
        lib.add_zone(f, zi, Container::parse_records(b)).unwrap();
    }
    let w = World::<Ieee>::load(&lib, f).unwrap();
    let boxes = bounds.iter().map(|b| [b.min.x.0, b.min.y.0, b.min.z.0, b.max.x.0, b.max.y.0, b.max.z.0]).collect();
    (boxes, w, near, here)
}

/// The floor straight down at (x, z) from high above, as the walk finds it.
fn floor(w: &mut World<Ieee>, x: f32, z: f32) -> Option<f32> {
    let mut s = CollSphere::<Ieee>::default();
    s.init(Vect3::of(x, 2000.0, z), Vect3::of(x, -2000.0, z), F::new(0.001));
    let mut f = Filter::new(FilterKind::WalkFloor, -1);
    w.collide_sphere(&mut s, &mut f).then(|| s.hit_point().y.0)
}

#[test]
fn tunaria_off_the_2002_disc_is_the_same_ground() {
    let Some((frontiers, y2002)) = discs() else { return };
    let (fb, mut fw, fnear, fhere) = tunaria(&frontiers);
    let (yb, mut yw, ynear, yhere) = tunaria(&y2002);
    assert_eq!((fb.len(), yb.len()), (175, 175), "every zone has its proxy on both discs");
    assert_eq!(fb, yb, "the same zones, the same bounds");
    assert_eq!(fnear, ynear);
    assert!(yhere.is_some() && yhere == fhere, "the spawn's zone: {yhere:?} and {fhere:?}");
    // floors on a grid round the spawn: the ground is the same, and where a
    // prop changed it may not be
    let (mut same, mut n) = (0, 0);
    for i in 0..40 {
        for j in 0..40 {
            let (x, z) = (SPAWN.0 - 400.0 + 20.0 * i as f32, SPAWN.1 - 400.0 + 20.0 * j as f32);
            let (Some(a), Some(b)) = (floor(&mut fw, x, z), floor(&mut yw, x, z)) else { continue };
            n += 1;
            same += ((a - b).abs() < 0.01) as usize;
        }
    }
    eprintln!("{same} of {n} floors round the spawn at the same height on both discs; {} zones in", ynear.len());
    assert!(n > 1000 && same * 100 >= n * 95, "{same} of {n}");
    // the control: a probe off the 2002 world where no zone is in finds nothing
    assert_eq!(floor(&mut yw, SPAWN.0 + 20_000.0, SPAWN.1 + 20_000.0), None);
}
