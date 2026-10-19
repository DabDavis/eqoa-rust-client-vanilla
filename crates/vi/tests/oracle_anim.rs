//! The animation system against the decomp compiled for the host, bit for bit
//! (`Ieee`): the human (0x70D898C6) is parsed from CHAR.ESF here, the same data
//! is handed to eqoa-port's oracle, which builds a real VICSprite in a bare
//! VIScene through the guest's API, and a script of (time, locomotion, LOD)
//! steps runs on both: SetLocomotion -> SetAnimation (priorities, 250 ms blends,
//! freezes, the two-layer sync) and VIHSprite::Process (play timing, the blend,
//! the hierarchy). Every node's world matrix is compared at every step.
#![allow(clippy::field_reassign_with_default)]

mod common;

use common::*;
use vi::character::{AnimCache, load_csprite};
use vi::csprite::Rand;
use vi::fpu::{F, Ieee};
use vi::geom::Matrix44;
use vi::hsprite::{AnimScene, Frames};

fn push_i(b: &mut Vec<u8>, v: i32) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn push_f(b: &mut Vec<u8>, v: F<Ieee>) {
    b.extend_from_slice(&v.0.to_le_bytes());
}

#[test]
fn character_animation_matches() {
    let Some(mut o) = Oracle::open() else { return };
    let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    if !iso_path.exists() {
        eprintln!("SKIP: no disc");
        return;
    }
    let mut iso = iso::Iso::open(iso_path).unwrap();
    let mut lib = esf::Library::new();
    lib.add("CHAR.ESF", esf::Container::open(iso.read("CHAR.ESF").unwrap()).unwrap());
    let r = lib.lookup(0x70D898C6u32 as i32).unwrap();
    let mut sc = AnimScene::<Ieee>::default();
    let mut cache = AnimCache::new();
    let mut rand = Rand(1);
    let mut cs = load_csprite(&lib, r, &mut sc, &mut cache, &mut rand).expect("human");
    // The first fidget, pinned on both sides (SetDefaults draws it from rand).
    let anim_time = 30_000;
    cs.anim_time = anim_time;

    // The data, as the oracle reads it.
    let mut b = Vec::new();
    push_i(&mut b, 0);
    let map = &sc.ref_maps[cs.h.ref_map];
    let mut pairs: Vec<(&i32, &i32)> = map.iter().collect();
    pairs.sort();
    push_i(&mut b, pairs.len() as i32);
    for (k, v) in pairs {
        push_i(&mut b, *k);
        push_i(&mut b, *v);
    }
    push_i(&mut b, sc.anims.len() as i32);
    for a in &sc.anims {
        let packed = matches!(a.frames, Frames::Pack(_));
        for v in [packed as i32, a.num_nodes, a.num_frames, a.triggers.len() as i32] {
            push_i(&mut b, v);
        }
        push_f(&mut b, a.play_rate);
        push_f(&mut b, a.play_speed);
        push_i(&mut b, a.playback_type);
        for (j, &(ref_id, first)) in a.nodes.iter().enumerate() {
            push_i(&mut b, ref_id);
            for fi in 0..a.num_frames as usize {
                let idx = first as usize + fi;
                assert_eq!(first, j as i32 * a.num_frames);
                match &a.frames {
                    Frames::Full(v) => {
                        let fr = v[idx];
                        for x in [fr.rot.x, fr.rot.y, fr.rot.z, fr.rot.w, fr.scale, fr.trans.x, fr.trans.y, fr.trans.z] {
                            push_f(&mut b, x);
                        }
                    }
                    Frames::Pack(v) => {
                        for s in v[idx] {
                            b.extend_from_slice(&s.to_le_bytes());
                        }
                    }
                }
            }
        }
        for &(id, fr) in &a.triggers {
            push_i(&mut b, id);
            push_f(&mut b, fr);
        }
    }
    push_i(&mut b, cs.skel_type);
    push_i(&mut b, cs.race);
    push_i(&mut b, cs.sex);
    push_f(&mut b, cs.h.default_scale);
    push_i(&mut b, cs.h.lod_dist.len() as i32);
    for &d in &cs.h.lod_dist {
        push_f(&mut b, d);
    }
    push_i(&mut b, cs.h.nodes.len() as i32);
    for n in &cs.h.nodes {
        push_i(&mut b, n.parent);
        let bd = n.bind;
        for x in [bd.rot.x, bd.rot.y, bd.rot.z, bd.rot.w, bd.scale, bd.trans.x, bd.trans.y, bd.trans.z] {
            push_f(&mut b, x);
        }
        push_i(&mut b, n.lod);
        for x in [n.fix_scale.x, n.fix_scale.y, n.fix_scale.z] {
            push_f(&mut b, x);
        }
    }
    push_i(&mut b, cs.h.triggers.len() as i32);
    for t in &cs.h.triggers {
        push_i(&mut b, t.id);
    }
    // The entries after SetDefaultEntries: running it again on the oracle's
    // side changes nothing (every entry it fills is filled).
    for e in &cs.entries {
        push_i(&mut b, e.animation[0]);
        push_i(&mut b, e.animation[1]);
        push_f(&mut b, e.weight);
        push_i(&mut b, e.sync);
    }
    push_i(&mut b, anim_time as i32);

    // The script: 10 s in 32 ms ticks; locomotion changes, some mid-blend.
    let mut script: Vec<(i32, i32, i32)> = Vec::new();
    for k in 0..320 {
        let t = k * 32;
        let loco = match t {
            0..=999 => 0,
            1000..=2099 => 1,
            2100..=2199 => 3,
            2200..=2229 => 1, // back to walk inside the run's blend
            2230..=3999 => 3,
            4000..=4999 => 4,
            5000..=5999 => 11,
            6000..=6999 => 2,
            // Actions (1000 + id): a swing standing, one mid-blend, a cast.
            7008 => 1023,
            7040 => 1025,
            8000 => 1034,
            _ => 0,
        };
        let lod = match k % 7 {
            3 => 0,
            5 => 1,
            6 => 2,
            _ => -1,
        };
        script.push((t, loco, lod));
    }
    push_i(&mut b, script.len() as i32);
    for &(t, l, lod) in &script {
        push_i(&mut b, t);
        push_i(&mut b, l);
        push_i(&mut b, lod);
    }
    let res = o.call(500, &b);
    assert_eq!(i32::from_le_bytes(res[0..4].try_into().unwrap()), 0, "oracle build failed");

    // The same on our side: SetDefaultEntries/Priorities/Defaults already ran
    // in load_csprite, as they do in ParseCSpriteObj.
    let nn = cs.h.nodes.len();
    let step = 28 + nn * 64;
    let mut changes = 0;
    let mut actions = 0;
    let mut last = [(0, 0); 2];
    for (k, &(t, loco, lod)) in script.iter().enumerate() {
        sc.time = t as i64;
        let handle = if loco >= 1000 {
            actions += 1;
            cs.set_action(loco - 1000, &sc, &mut rand)
        } else {
            cs.set_locomotion(loco, &sc, &mut rand);
            -2
        };
        cs.h.process(lod, &sc);
        let off = 4 + k * step;
        let st: Vec<i32> = (0..7).map(|i| i32::from_le_bytes(res[off + i * 4..off + i * 4 + 4].try_into().unwrap())).collect();
        let ours = [cs.cur[0].anim, cs.cur[0].play, cs.cur[1].anim, cs.cur[1].play, handle, cs.action, cs.action_seq];
        assert_eq!(ours.to_vec(), st, "step {k} (t {t}, loco {loco}): layers (anim, play), handle, action, sequence");
        let now = [(ours[0], ours[1]), (ours[2], ours[3])];
        if now != last {
            changes += 1;
            last = now;
        }
        for j in 0..nn {
            let theirs = from_bytes::<Matrix44<Ieee>>(&res[off + 28 + j * 64..]);
            if let Some((o2, a, b2)) = first_diff(&cs.h.nodes[j].world, &theirs) {
                panic!("step {k} (t {t}, loco {loco}, lod {lod}) node {j}: +0x{o2:02X} ours {a:08X} theirs {b2:08X}");
            }
        }
    }
    eprintln!("{} steps x {nn} nodes equal; {changes} layer changes, {actions} actions", script.len());
    assert!(changes >= 6);
    assert_eq!(actions, 3);
}
