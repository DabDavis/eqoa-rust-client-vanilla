//! Time of day and the terrain-profile grid against the decomp compiled for
//! the host, bit for bit (`Ieee`): `VIScene::InitTimeOfDay`'s 24 entries,
//! `VIWorld::SetProfiles`' 22 profiles, and `VIWorld::CalcProfileBlend` over
//! TUNARIA.ESF's real grid at points inside, on the edge of and outside it.
//!
//! `SetupTimeOfDayLights`/`Fog`/`Sky` are not compared: they write the
//! raster, which the oracle has none of; what they read is checked here.
//!
//! Needs the oracle (see tests/common) and the disc at run/eqoa.iso.

mod common;

use common::*;
use vi::fpu::{F, Ieee};
use vi::tod::{PROFILES, TimeOfDay, profile_color};
use vi::vect::Vect3;
use vi::world::Grid;

fn bits(v: F<Ieee>) -> u32 {
    v.0.to_bits()
}

fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn i32_at(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

/// The 108-byte `VISceneTimeOfDay` the Rust table should produce, laid out
/// as the decomp has it: a[2] (rgba), b, c[2] (rgba), d[2] (rgba). Every
/// alpha is 0 (the table's bytes carry alpha 0).
fn entry(t: &TimeOfDay<Ieee>, h: usize) -> Vec<u32> {
    let rgba = |c: &[F<Ieee>; 3]| [bits(c[0]), bits(c[1]), bits(c[2]), 0];
    let mut v = Vec::new();
    v.extend(rgba(&t.a[h][0]));
    v.extend(rgba(&t.a[h][1]));
    v.extend([bits(t.b[h].x), bits(t.b[h].y), bits(t.b[h].z)]);
    v.extend(rgba(&t.c[h][0]));
    v.extend(rgba(&t.c[h][1]));
    v.extend(rgba(&t.d[h][0]));
    v.extend(rgba(&t.d[h][1]));
    v
}

fn tod_mismatches(o: &[u8], t: &TimeOfDay<Ieee>) -> usize {
    let mut bad = 0;
    for h in 0..24 {
        let want: Vec<u32> = (0..27).map(|k| f32_at(o, h * 108 + k * 4).to_bits()).collect();
        let got = entry(t, h);
        if want != got {
            if bad < 3 {
                eprintln!("hour {h}: decomp {want:08X?}\n          rust   {got:08X?}");
            }
            bad += 1;
        }
    }
    bad
}

#[test]
fn init_time_of_day() {
    let Some(mut o) = Oracle::open() else { return };
    let out = o.call(600, &[]);
    assert_eq!(out.len(), 24 * 108 + 8);
    // InitTimeOfDay starts the clock at 10:00.
    assert_eq!(f32_at(&out, 24 * 108), 10.0);
    let t = TimeOfDay::<Ieee>::new();
    assert_eq!(tod_mismatches(&out, &t), 0, "InitTimeOfDay differs");

    // Negative controls: the table is checked entry by entry, so one wrong
    // channel, and a sun direction off by an hour, each fail it.
    let mut bad = t.clone();
    bad.d[13][1][0] = bad.d[13][1][0] + F::new(1.0 / 1024.0);
    assert_eq!(tod_mismatches(&out, &bad), 1);
    let mut bad = t.clone();
    bad.b.rotate_left(1);
    assert_eq!(tod_mismatches(&out, &bad), 24);
}

fn grid() -> Option<Grid<Ieee>> {
    let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    if !iso_path.exists() {
        eprintln!("SKIP: no disc at {}", iso_path.display());
        return None;
    }
    let mut iso = iso::Iso::open(iso_path).ok()?;
    let c = esf::Container::open(iso.read("TUNARIA.ESF").ok()?).ok()?;
    let base = c.child(*c.roots.first()?, 0x8200)?;
    Grid::read(&c, base)
}

/// Runs op 601 with `g` and the points; returns the decomp's profiles block
/// and one 32-byte blend per point.
fn blends(o: &mut Oracle, g: &Grid<Ieee>, points: &[Vect3<Ieee>]) -> (Vec<u8>, Vec<Vec<u8>>) {
    let mut p = Vec::new();
    for v in [g.rows, g.cols, g.subdiv] {
        p.extend(v.to_le_bytes());
    }
    for v in [g.cell_size.0, g.origin.x.0, g.origin.y.0, g.origin.z.0] {
        p.extend(v.to_le_bytes());
    }
    for &r in &g.regions {
        p.extend((r as i32).to_le_bytes());
    }
    p.extend((points.len() as i32).to_le_bytes());
    for v in points {
        for x in [v.x.0, v.y.0, v.z.0] {
            p.extend(x.to_le_bytes());
        }
    }
    let out = o.call(601, &p);
    assert_eq!(out.len(), 22 * 40 + points.len() * 32);
    let (prof, rest) = out.split_at(22 * 40);
    (prof.to_vec(), rest.chunks(32).map(|c| c.to_vec()).collect())
}

fn blend_mismatches(g: &Grid<Ieee>, points: &[Vect3<Ieee>], want: &[Vec<u8>]) -> usize {
    let mut bad = 0;
    for (v, w) in points.iter().zip(want) {
        let b = g.profile_blend(v);
        let got: Vec<u32> = b.index.iter().map(|&i| i as u32).chain(b.weight.iter().map(|&x| bits(x))).collect();
        let exp: Vec<u32> = (0..8).map(|k| i32_at(w, k * 4) as u32).collect();
        if got != exp {
            if bad < 3 {
                eprintln!("at ({}, {}, {}): decomp {exp:08X?}\n  rust {got:08X?}", v.x.0, v.y.0, v.z.0);
            }
            bad += 1;
        }
    }
    bad
}

#[test]
fn profiles_and_blend() {
    let Some(mut o) = Oracle::open() else { return };
    let Some(g) = grid() else { return };
    let (wide, high) = ((g.cols * g.subdiv) as usize, (g.rows * g.subdiv) as usize);
    assert_eq!(g.regions.len(), wide * high);
    let used: std::collections::BTreeSet<u8> = g.regions.iter().copied().collect();
    eprintln!("TUNARIA grid {}x{} cells of {} by {}, profiles used {used:?}", g.cols, g.rows, g.cell_size.0, g.subdiv);

    // Points over the whole grid and a margin past it (clamped edges), plus
    // ones on exact sub-cell boundaries.
    let (lo_x, lo_z) = (g.origin.x.0 - g.cell_size.0, g.origin.z.0 - g.cell_size.0);
    let (span_x, span_z) = ((g.cols + 2) as f32 * g.cell_size.0, (g.rows + 2) as f32 * g.cell_size.0);
    let mut r = Rng(0x7d0d);
    let mut points: Vec<Vect3<Ieee>> =
        (0..4000).map(|_| Vect3::of(lo_x + r.unit() * span_x, r.range(-100.0, 400.0), lo_z + r.unit() * span_z)).collect();
    let step = g.cell_size.0 / g.subdiv as f32;
    for k in 0..200 {
        points.push(Vect3::of(g.origin.x.0 + (k % 40) as f32 * step, 0.0, g.origin.z.0 + (k / 40) as f32 * step * 3.0));
    }

    let (prof, want) = blends(&mut o, &g, &points);
    for (i, p) in PROFILES.iter().enumerate() {
        let b = &prof[i * 40..];
        let c = profile_color::<Ieee>(i as i32);
        let got = [p.near.to_bits(), p.far.to_bits(), bits(c[0]), bits(c[1]), bits(c[2]), bits(c[3])];
        let exp: Vec<u32> = (0..6).map(|k| f32_at(b, k * 4).to_bits()).collect();
        assert_eq!(exp, got, "profile {i}'s distances and colour");
        assert_eq!(
            (i32_at(b, 24), f32_at(b, 28), i32_at(b, 32), f32_at(b, 36)),
            (p.sky, p.overcast, p.music, p.lavastorm),
            "profile {i}'s sky, overcast, music, lavastorm"
        );
    }
    let bad = blend_mismatches(&g, &points, &want);
    assert_eq!(bad, 0, "{bad} of {} blends differ", points.len());

    // Negative controls: a grid one sub-cell off, and one region changed,
    // each move blends.
    let mut off = g.clone();
    off.origin.x = off.origin.x + F::new(step);
    assert!(blend_mismatches(&off, &points, &want) > 100);
    let mut one = g.clone();
    let k = one.regions.iter().position(|&x| x != 0).unwrap_or(0);
    one.regions[k] ^= 1;
    let near = blend_mismatches(&one, &points, &want);
    let at = Vect3::of(
        g.origin.x.0 + ((k % wide) as f32 + 0.75) * step - g.cell_size.0 * 0.5,
        0.0,
        g.origin.z.0 + ((k / wide) as f32 + 0.75) * step - g.cell_size.0 * 0.5,
    );
    let (_, w1) = blends(&mut o, &g, &[at]);
    assert!(near > 0 || blend_mismatches(&one, &[at], &w1) == 1, "a changed region moved no blend");
}
