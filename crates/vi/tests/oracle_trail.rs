//! `vi::trail` against the decomp's VITrailFx, bit for bit (`Ieee`):
//! eqoa-port's `build/oracle_trail` compiles vitrailfx.cpp over a recording
//! raster, and the same script runs on both sides: weapon swings beginning
//! trails, a segment a frame along an arc, trails ended mid-swing and left
//! to fade, the pool running out, more segments than a trail holds (the
//! oldest recycled), and a frame-time hitch. Every strip (colour, vertex,
//! count, order) must be equal; a planted difference must not be.
//!
//! SKIPs without the oracle (`make oracle` in eqoa-port).

mod common;
use common::Oracle;
use vi::fpu::{F, Ieee};
use vi::particle::Color;
use vi::trail::Trails;
use vi::vect::Vect3;

type M = Ieee;

fn f32s(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn run(plant: bool) -> Option<Result<usize, String>> {
    let mut o = Oracle::open_at("oracle_trail")?;
    o.call(1300, &[]);
    let mut ours = Trails::<M>::default();
    let mut live: Vec<(i32, usize, f32)> = Vec::new();
    let mut now: i64 = 1000;
    let mut compared = 0;
    for frame in 0..900 {
        now += match frame {
            400 => 300,
            f => 16 + (f * 5 % 19) as i64,
        };
        // a swing begins every 7 frames on one of a few weapons: 25 segments
        // and 0.4 s, as ProcessWeaponTrails asks, or other sizes
        if frame % 7 == 0 {
            let (n, life) = [(25, 0.4f32), (25, 0.4), (8, 0.25), (40, 1.0)][frame % 4];
            let mut p = now.to_le_bytes().to_vec();
            p.extend((n as i32).to_le_bytes());
            p.extend(life.to_le_bytes());
            let theirs = i32::from_le_bytes(o.call(1301, &p)[..4].try_into().unwrap());
            let mine = ours.begin(now, n, F::new(life));
            if (theirs >= 0) != mine.is_some() {
                return Some(Err(format!("frame {frame}: begin oracle {theirs} ours {mine:?}")));
            }
            if let Some(t) = mine {
                live.push((theirs, t, frame as f32));
            }
        }
        // each live trail gets a segment along an arc; some end mid-swing
        let mut k = 0;
        while k < live.len() {
            let (theirs, mine, start) = live[k];
            let t = (frame as f32 - start) * 0.21;
            let a = [mine as f32 + t.cos(), 1.0 + t.sin() * 0.5, t * 0.3];
            let b = [a[0] * 1.5, a[1] + 1.2, a[2] - 0.4];
            let ca = [0.25, 0.25, 0.25, 1.0];
            let cb = [0.0, 0.0, 0.0, 1.0];
            let mut p = theirs.to_le_bytes().to_vec();
            p.extend(f32s(&a));
            p.extend(f32s(&b));
            p.extend(f32s(&ca));
            p.extend(f32s(&cb));
            o.call(1302, &p);
            let col = |c: [f32; 4]| Color { r: F::new(c[0]), g: F::new(c[1]), b: F::new(c[2]), a: F::new(c[3]) };
            ours.trails[mine].add_segment(Vect3::of(a[0], a[1], a[2]), Vect3::of(b[0], b[1], b[2]), col(ca), col(cb));
            if (frame as f32 - start) > [12.0, 30.0, 5.0][mine % 3] {
                o.call(1303, &theirs.to_le_bytes());
                ours.end(mine);
                live.remove(k);
            } else {
                k += 1;
            }
        }
        let res = o.call(1304, &now.to_le_bytes());
        let mut strips: Vec<_> = ours.render(now).into_iter().flat_map(|(_, s)| s).collect();
        if plant && frame == 450 && !strips.is_empty() {
            strips[0][0].0.a = strips[0][0].0.a + F::new(1e-6);
        }
        let n = u32::from_le_bytes(res[..4].try_into().unwrap()) as usize;
        if n != strips.len() {
            return Some(Err(format!("frame {frame}: {n} strips in the oracle, {} ours", strips.len())));
        }
        let mut p = 4;
        for (si, s) in strips.iter().enumerate() {
            let count = u32::from_le_bytes(res[p..p + 4].try_into().unwrap()) as usize;
            p += 4;
            if count != s.len() {
                return Some(Err(format!("frame {frame} strip {si}: {count} vertices in the oracle, {} ours", s.len())));
            }
            for (vi, (c, v)) in s.iter().enumerate() {
                let want: Vec<u32> = res[p..p + 28].chunks(4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).collect();
                p += 28;
                let got = [c.r.0, c.g.0, c.b.0, c.a.0, v.x.0, v.y.0, v.z.0].map(f32::to_bits);
                if want != got {
                    return Some(Err(format!("frame {frame} strip {si} vertex {vi}: oracle {want:08x?} ours {got:08x?}")));
                }
                compared += 1;
            }
        }
    }
    Some(Ok(compared))
}

#[test]
fn trails_match_the_decomp() {
    let Some(r) = run(false) else { return };
    let n = r.unwrap();
    eprintln!("{n} trail vertices equal, bit for bit");
    assert!(n > 20_000, "a real load of trails: {n}");
}

#[test]
fn a_planted_difference_fails() {
    if let Some(r) = run(true) {
        assert!(r.is_err(), "a planted alpha went unnoticed");
    }
}
