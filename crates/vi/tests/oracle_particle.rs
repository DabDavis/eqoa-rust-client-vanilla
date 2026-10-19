//! The particle system against the decomp, bit for bit: eqoa-port's
//! `build/oracle_particle` runs SUPPORT's particle TUs over a recording
//! raster with newlib's rand(), and every step here is run on it and on
//! `vi::particle` under `Ieee`. Every billboard (where, which frame, how big,
//! what colour) and every emitter's state after each frame are compared.
//!
//! The game's own definitions drive it: SPELLFX.CSF's (with their motifs,
//! blended in and out) and the ones TUNARIA.ESF places (fires, smoke,
//! fountains), on still and moving emitters, through a frame-time hitch past
//! the 0.1 s cap and a purge.
//!
//! SKIPs without the oracle (`make oracle` in eqoa-port) or the disc.

mod common;
use common::Oracle;
use vi::csprite::Rand;
use vi::fpu::{F, Ieee};
use vi::geom::Matrix44;
use vi::particle::{Definition, Matrix33, System};
use vi::vect::Vect3;

type M = Ieee;

/// A definition object (0xC020) out of a 0xC000 record: (version, body).
fn definition(c: &esf::Container, n: esf::NodeId) -> Option<(i32, Vec<u8>)> {
    let d = c.child(n, 0xC020)?;
    Some((c.node(d).version as i32, c.body(d).to_vec()))
}

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}

fn scenario(planted: bool) -> Option<usize> {
    let mut o = Oracle::open_at("oracle_particle")?;
    let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    let Ok(mut disc) = iso::Iso::open(&iso) else {
        eprintln!("SKIP: no disc");
        return None;
    };
    let fx = esf::Container::open(disc.read("DATA2/SPELLFX.CSF").unwrap()).unwrap();
    let tun = esf::Container::open(disc.read("DATA/TUNARIA.ESF").unwrap()).unwrap();
    let mut defs: Vec<(i32, Vec<u8>)> = Vec::new();
    let fx_defs: Vec<usize> = (0..fx.nodes.len()).filter(|&i| fx.nodes[i].ty == 0xC000).collect();
    for &n in fx_defs.iter().step_by(fx_defs.len() / 12) {
        defs.extend(definition(&fx, n));
    }
    let tun_defs: Vec<usize> = (0..tun.nodes.len()).filter(|&i| tun.nodes[i].ty == 0xC000).collect();
    for &n in tun_defs.iter().step_by(tun_defs.len() / 12) {
        defs.extend(definition(&tun, n));
    }
    assert!(defs.len() >= 20);

    let mut sys = System::<M>::new(0, 2000);
    let mut rand = Rand(1);
    let mut p = 2000i32.to_le_bytes().to_vec();
    p.extend(0i64.to_le_bytes());
    o.call(1100, &p);
    o.call(1101, &4242u32.to_le_bytes());
    rand.0 = 4242;

    for (v, body) in &defs {
        let mut p = v.to_le_bytes().to_vec();
        p.extend(body);
        let k = i32::from_le_bytes(o.call(1102, &p)[..4].try_into().unwrap());
        let d = Definition::<M>::parse(body, *v).expect("a definition");
        assert_eq!(k as usize, sys.add_definition(d));
    }
    let mut emitters = Vec::new();
    for k in 0..defs.len() {
        let e = i32::from_le_bytes(o.call(1103, &(k as i32).to_le_bytes())[..4].try_into().unwrap());
        assert_eq!(e as usize, sys.create_emitter(k));
        emitters.push(e as usize);
    }
    // each one somewhere, facing somewhere, some in a space of their own
    let place = |o: &mut Oracle, sys: &mut System<M>, e: usize, t: f32| {
        let loc = [e as f32 * 3.0 + t.sin() * (e % 3) as f32, 1.0 + (e % 4) as f32, -(e as f32) + t.cos() * (e % 2) as f32];
        let a = e as f32 * 0.7 + t * 0.3;
        let dir = [a.cos(), 0.0, -a.sin(), 0.0, 1.0, 0.0, a.sin(), 0.0, a.cos()];
        let mut space = [0f32; 16];
        space[0] = 1.0;
        space[5] = 1.0;
        space[10] = 1.0;
        space[15] = 1.0;
        if e % 3 == 1 {
            space[12] = 100.0;
            space[13] = -5.0;
            space[14] = 7.5;
        }
        let mut p = (e as i32).to_le_bytes().to_vec();
        for x in loc.iter().chain(&dir).chain(&space) {
            p.extend(x.to_le_bytes());
        }
        o.call(1104, &p);
        let em = sys.emitter(e);
        em.set_location(Vect3::of(loc[0], loc[1], loc[2]));
        em.direction = Matrix33 { m: [Vect3::of(dir[0], dir[1], dir[2]), Vect3::of(dir[3], dir[4], dir[5]), Vect3::of(dir[6], dir[7], dir[8])] };
        let mut m = Matrix44::<M>::identity();
        m.m[3].x = F::new(space[12]);
        m.m[3].y = F::new(space[13]);
        m.m[3].z = F::new(space[14]);
        em.space = m;
    };
    // the motif blenders: the first SPELLFX definitions' first motifs
    let mut blenders = Vec::new();
    for (k, (_, _)) in defs.iter().enumerate().take(6) {
        let d = sys.defs[k].as_ref().unwrap().clone();
        if let Some(m) = d.motifs.first() {
            blenders.push((k, m.name_hash, None::<usize>));
        }
    }
    assert!(!blenders.is_empty(), "SPELLFX's definitions have motifs");

    let mut t = 0i64;
    let mut compared = 0usize;
    for frame in 0..240 {
        // a frame time that wanders, and one hitch past the cap
        t += match frame {
            100 => 400,
            f => 16 + (f * 7 % 37) as i64,
        };
        for &e in &emitters {
            if frame == 0 || e % 2 == 0 {
                place(&mut o, &mut sys, e, frame as f32 * 0.05);
            }
        }
        for b in blenders.iter_mut() {
            let (e, hash) = (b.0, b.1);
            let f = ((frame as f32) * 0.02).sin().abs();
            let mut p = (e as i32).to_le_bytes().to_vec();
            p.extend(hash.to_le_bytes());
            p.extend(f.to_le_bytes());
            o.call(1108, &p);
            let di = sys.emitter(e).def;
            let d = sys.defs[di].as_ref().unwrap().clone();
            let em = sys.emitter(e);
            if b.2.is_none() {
                b.2 = em.link_blender(&d, hash);
            }
            em.set_blender_factor(b.2.unwrap(), F::new(f));
        }
        if frame == 150 {
            o.call(1107, &(emitters[3] as i32).to_le_bytes());
            sys.purge(emitters[3]);
        }
        for &e in &emitters {
            o.call(1105, &(e as i32).to_le_bytes());
            sys.update(e, &mut rand);
        }
        let r = o.call(1106, &t.to_le_bytes());
        let mut ours = sys.render(t);
        if planted && frame == 120 {
            ours[0].1[0].width = ours[0].1[0].width + F::new(1e-6);
        }
        let n = u32::from_le_bytes(r[..4].try_into().unwrap()) as usize;
        let want = &r[4..4 + n * 44];
        let flat: Vec<(usize, &vi::particle::Billboard<M>)> = ours.iter().flat_map(|(e, v)| v.iter().map(move |b| (*e, b))).collect();
        assert_eq!(flat.len(), n, "frame {frame}: billboards");
        for (k, (e, b)) in flat.iter().enumerate() {
            let w = &want[k * 44..k * 44 + 44];
            let we = i32::from_le_bytes(w[..4].try_into().unwrap()) as usize;
            let wf = f32s(&w[4..16]);
            let frame_w = i32::from_le_bytes(w[16..20].try_into().unwrap());
            let rest = f32s(&w[20..44]);
            let got = [b.location.x.0, b.location.y.0, b.location.z.0];
            let got_rest = [b.width.0, b.height.0, b.color.r.0, b.color.g.0, b.color.b.0, b.color.a.0];
            let same = we == *e
                && frame_w == b.frame
                && wf.iter().zip(&got).all(|(a, b)| a.to_bits() == b.to_bits())
                && rest.iter().zip(&got_rest).all(|(a, b)| a.to_bits() == b.to_bits());
            assert!(same, "frame {frame}, billboard {k}: oracle emitter {we} at {wf:?} frame {frame_w} {rest:?}, ours emitter {e} at {got:?} frame {} {got_rest:?}", b.frame);
        }
        compared += n;
        // every emitter's state
        let st = &r[4 + n * 44..];
        let m = u32::from_le_bytes(st[..4].try_into().unwrap()) as usize;
        for k in 0..m {
            let s = &st[4 + k * 64..4 + (k + 1) * 64];
            let e = i32::from_le_bytes(s[..4].try_into().unwrap()) as usize;
            let count = i32::from_le_bytes(s[4..8].try_into().unwrap());
            let v = f32s(&s[8..64]);
            let em = sys.emitter(e);
            assert_eq!(count as usize, em.particles.len(), "frame {frame}: emitter {e}'s particles");
            let ours = [
                em_carry(em),
                em.dyn_bounds.min.x.0,
                em.dyn_bounds.min.y.0,
                em.dyn_bounds.min.z.0,
                em.dyn_bounds.max.x.0,
                em.dyn_bounds.max.y.0,
                em.dyn_bounds.max.z.0,
                em.static_bounds.min.x.0,
                em.static_bounds.min.y.0,
                em.static_bounds.min.z.0,
                em.static_bounds.max.x.0,
                em.static_bounds.max.y.0,
                em.static_bounds.max.z.0,
                sys.delta_time.0,
            ];
            for (i, (a, b)) in v.iter().zip(&ours).enumerate() {
                assert_eq!(a.to_bits(), b.to_bits(), "frame {frame}: emitter {e}'s state word {i}: oracle {a} ours {b}");
            }
        }
    }
    Some(compared)
}

fn em_carry(e: &vi::particle::Emitter<M>) -> f32 {
    e.carry().0
}

#[test]
fn particles_match_the_decomp() {
    let Some(n) = scenario(false) else { return };
    eprintln!("{n} billboards equal, bit for bit");
    assert!(n > 10_000, "a real load of particles: {n}");
}

#[test]
fn a_planted_difference_fails() {
    match std::panic::catch_unwind(|| scenario(true)) {
        Ok(None) => {}
        Ok(Some(_)) => panic!("a planted difference passed"),
        Err(_) => {}
    }
}
