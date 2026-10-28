//! The animation system against the decomp compiled for the host, bit for bit
//! (`Ieee`): a character is parsed from CHAR.ESF here, the same data is handed
//! to eqoa-port's oracle, which builds a real VICSprite in a bare VIScene
//! through the guest's API, and a script of (time, locomotion, LOD) steps runs
//! on both: SetLocomotion -> SetAnimation (priorities, 250 ms blends, freezes,
//! the two-layer sync, the fidget) and VIHSprite::Process (play timing, the
//! blend, the hierarchy). Every node's world matrix is compared at every step.
#![allow(clippy::field_reassign_with_default)]

mod common;

use common::*;
use vi::character::{AnimCache, load_csprite};
use vi::csprite::{CSprite, Rand};
use vi::fpu::{F, Ieee};
use vi::geom::Matrix44;
use vi::hsprite::{AnimScene, Frames};

fn push_i(b: &mut Vec<u8>, v: i32) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn push_f(b: &mut Vec<u8>, v: F<Ieee>) {
    b.extend_from_slice(&v.0.to_le_bytes());
}

/// CHAR.ESF's character `id` as `ParseCSpriteObj` sets it up, its first fidget
/// pinned at `anim_time` (SetDefaults draws it from rand, which the oracle's
/// libc does not share); None without the disc.
fn character(id: u32, anim_time: i64) -> Option<(AnimScene<Ieee>, CSprite<Ieee>, Rand)> {
    let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    if !iso_path.exists() {
        eprintln!("SKIP: no disc");
        return None;
    }
    let mut iso = iso::Iso::open(iso_path).unwrap();
    let mut lib = esf::Library::new();
    lib.add("CHAR.ESF", esf::Container::open(iso.read("CHAR.ESF").unwrap()).unwrap());
    let r = lib.lookup(id as i32).unwrap();
    let mut sc = AnimScene::<Ieee>::default();
    let mut cache = AnimCache::new();
    let mut rand = Rand(1);
    let mut cs = load_csprite(&lib, r, &mut sc, &mut cache, &mut rand).expect("the character");
    cs.anim_time = anim_time;
    Some((sc, cs, rand))
}

/// The character and the script, as the oracle reads them.
fn scenario(sc: &AnimScene<Ieee>, cs: &CSprite<Ieee>, script: &[(i32, i32, i32)]) -> Vec<u8> {
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
    push_i(&mut b, cs.anim_time as i32);
    push_i(&mut b, script.len() as i32);
    for &(t, l, lod) in script {
        push_i(&mut b, t);
        push_i(&mut b, l);
        push_i(&mut b, lod);
    }
    b
}

/// The decomp's state after step `k` of the oracle's result: the layers
/// (anim, play), the handle, the action and the sequence, then every node's
/// world matrix.
fn theirs(res: &[u8], k: usize, nn: usize) -> ([i32; 7], Vec<Matrix44<Ieee>>) {
    let off = 4 + k * (28 + nn * 64);
    let st = std::array::from_fn(|i| i32::from_le_bytes(res[off + i * 4..off + i * 4 + 4].try_into().unwrap()));
    (st, (0..nn).map(|j| from_bytes::<Matrix44<Ieee>>(&res[off + 28 + j * 64..])).collect())
}

/// The script on our side (SetDefaultEntries/Priorities/Defaults already ran
/// in load_csprite, as they do in ParseCSpriteObj), each step against the
/// decomp's: the layers' (anim, play) after every step, or the first
/// difference.
fn replay(sc: &mut AnimScene<Ieee>, cs: &mut CSprite<Ieee>, rand: &mut Rand, script: &[(i32, i32, i32)], res: &[u8]) -> Result<Vec<[(i32, i32); 2]>, String> {
    let nn = cs.h.nodes.len();
    let mut layers = Vec::new();
    for (k, &(t, loco, lod)) in script.iter().enumerate() {
        sc.time = t as i64;
        let handle = if loco >= 2000 {
            cs.set_death(true, loco == 2001, sc, rand, None);
            -2
        } else if loco >= 1000 {
            cs.set_action(loco - 1000, sc, rand)
        } else {
            cs.set_locomotion(loco, sc, rand);
            -2
        };
        cs.h.process(lod, sc);
        let (st, nodes) = theirs(res, k, nn);
        let ours = [cs.cur[0].anim, cs.cur[0].play, cs.cur[1].anim, cs.cur[1].play, handle, cs.action, cs.action_seq];
        if ours != st {
            return Err(format!("step {k} (t {t}, loco {loco}): layers (anim, play), handle, action, sequence: ours {ours:?}, theirs {st:?}"));
        }
        for (j, theirs) in nodes.iter().enumerate() {
            if let Some((o2, a, b2)) = first_diff(&cs.h.nodes[j].world, theirs) {
                return Err(format!("step {k} (t {t}, loco {loco}, lod {lod}) node {j}: +0x{o2:02X} ours {a:08X} theirs {b2:08X}"));
            }
        }
        layers.push([(ours[0], ours[1]), (ours[2], ours[3])]);
    }
    Ok(layers)
}

#[test]
fn character_animation_matches() {
    let Some(mut o) = Oracle::open() else { return };
    // the human; its first fidget out of the script's reach
    let Some((mut sc, mut cs, mut rand)) = character(0x70D898C6, 30_000) else { return };

    // The script: 14 s in 32 ms ticks; locomotion changes, some mid-blend.
    let mut script: Vec<(i32, i32, i32)> = Vec::new();
    for k in 0..440 {
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
            // Swimming (13 still, 12 on the move): a hit stays the hit (34,
            // left out of SetAnimation's swim remap), on the upper body while
            // the legs swim on; a swing becomes 44 on both layers.
            9504 => 1034,
            10528 => 1023,
            11520 => 1034,
            9000..=10999 => 13,
            11000..=12999 => 12,
            // Death (2000 + erase): kept playing, then erased to its last
            // frame; the locomotion after it stays 14.
            13024 => 2000,
            13600 => 2001,
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
    let res = o.call(500, &scenario(&sc, &cs, &script));
    assert_eq!(i32::from_le_bytes(res[0..4].try_into().unwrap()), 0, "oracle build failed");

    let nn = cs.h.nodes.len();
    let layers = replay(&mut sc, &mut cs, &mut rand, &script, &res).unwrap_or_else(|e| panic!("{e}"));
    let mut last = [(0, 0); 2];
    let changes = layers.iter().filter(|&&now| std::mem::replace(&mut last, now) != now).count();
    let actions = script.iter().filter(|s| (1000..2000).contains(&s.1)).count();
    eprintln!("{} steps x {nn} nodes equal; {changes} layer changes, {actions} actions", script.len());
    assert!(changes >= 6);
    assert_eq!(actions, 6);
    // The swimming steps were what the game makes of them: the hit on the
    // upper body over the legs' swim, the swing as the swim's own action.
    let at = |t: i32| script.iter().position(|s| s.0 == t).unwrap();
    for (t, want) in [(9504, [34, 13]), (10528, [44, 44]), (11520, [34, 12])] {
        let (st, _) = theirs(&res, at(t), nn);
        assert_eq!([st[0], st[2]], want, "t {t}: the decomp's layers");
    }
}

/// A character whose only clip is played once: Delwin Stitchfinger's body
/// (0xD7741560, CHAR.ESF 141, the halfling Sandstorm sends for him in
/// Bobble-by-Water). Its play list has one entry, idle, played once (sync 1);
/// its clip lies on the ground, gets up and falls back down, and every other
/// entry is SetDefaultEntries' copy of it. So idle holds the clip's last frame,
/// lying, and each fidget (SetAnimation's 0 -> 15 once m_animTime has passed,
/// every 20 to 40 s) plays it again, then idle's own entry once more after it
/// (F3 20261027-203555: "dropping over dead without dying over and over"). The
/// decomp does exactly that, node for node: it is the console's own. The
/// control: a port that never fidgets (its m_animTime out of reach) holds the
/// last frame, and parts from the decomp at the fidget.
#[test]
fn a_play_once_idle_plays_again_at_each_fidget() {
    let Some(mut o) = Oracle::open() else { return };
    let fidget = 4_000;
    let Some((mut sc, mut cs, mut rand)) = character(0xD7741560, fidget) else { return };
    // the body: a halfling's skeleton of two layers, every entry the one clip
    assert_eq!((cs.race, cs.skel_type), (7, 1));
    let clip = cs.entries[0].animation;
    assert!(clip[0] >= 0 && clip[1] >= 0);
    assert!(cs.entries.iter().all(|e| e.animation == clip || e.animation == [-1, -1]), "every entry is idle's clip");
    assert_eq!(cs.entries[0].sync, 1, "idle is played once");
    assert_eq!(sc.anims[clip[0] as usize].num_frames, 16);

    // 12 s standing (locomotion 0, as UpdateCreatures asks of an NPC in state
    // 0) in 32 ms steps
    let script: Vec<(i32, i32, i32)> = (0..375).map(|k| (k * 32, 0, -1)).collect();
    let data = scenario(&sc, &cs, &script);
    let res = o.call(500, &data);
    assert_eq!(i32::from_le_bytes(res[0..4].try_into().unwrap()), 0, "oracle build failed");
    let nn = cs.h.nodes.len();

    // The control: never fidgeting, the port parts from the decomp at the
    // fidget's step and not before.
    let (mut sc2, mut cs2, mut rand2) = character(0xD7741560, i32::MAX as i64).unwrap();
    let err = replay(&mut sc2, &mut cs2, &mut rand2, &script, &res).expect_err("the control must part from the decomp");
    let first = script.iter().position(|s| s.0 as i64 > fidget).unwrap();
    assert!(err.starts_with(&format!("step {first} ")), "the control parts at the fidget (step {first}): {err}");

    let layers = replay(&mut sc, &mut cs, &mut rand, &script, &res).unwrap_or_else(|e| panic!("{e}"));
    // The decomp's plays on the lower body: idle's, the fidget's (from the
    // first step past m_animTime), idle's again; each a new play of the clip.
    let mut runs: Vec<(usize, (i32, i32))> = Vec::new();
    for (k, l) in layers.iter().enumerate() {
        if runs.last().map(|r| r.1) != Some(l[0]) {
            runs.push((k, l[0]));
        }
    }
    let anims: Vec<i32> = runs.iter().map(|r| r.1.0).collect();
    assert_eq!(anims, [0, 15, 0], "idle, the fidget, idle: {runs:?}");
    assert_eq!(runs[1].0, first, "the fidget from the first step past m_animTime");
    let plays: std::collections::HashSet<i32> = runs.iter().map(|r| r.1.1).collect();
    assert_eq!(plays.len(), 3, "three plays: {runs:?}");
    // Lying, up, lying again: the body's highest node in the decomp's pose.
    let top = |k: usize| theirs(&res, k, nn).1.iter().map(|m| m.m[3].y.0).fold(f32::MIN, f32::max);
    let before = top(first - 1);
    let during = (runs[1].0..runs[2].0).map(top).fold(f32::MIN, f32::max);
    let again = (runs[2].0..script.len()).map(top).fold(f32::MIN, f32::max);
    let end = top(script.len() - 1);
    eprintln!(
        "{} steps x {nn} nodes equal; plays {runs:?}; highest node: lying {before:.3} before the fidget, {during:.3} in it, {again:.3} in idle's after it, {end:.3} at the end",
        script.len()
    );
    assert!(before < 0.5 && end < 0.5, "lying before the fidget and at the end");
    assert!(during > 1.0 && again > 1.0, "up in each replay");
}
